#![allow(clippy::print_stderr, clippy::print_stdout)] // allowed in this module only

use core::num::NonZeroU32;
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context as _;
use ironrdp::client::rdp::{AutoReconnectDecision, RdpInputEvent, RdpInputSender, RdpOutputEvent};
use ironrdp_daemon::daemon::{Daemon, ResizeError};
use raw_window_handle::{DisplayHandle, HasDisplayHandle as _};
use smallvec::SmallVec;
use tracing::{debug, error, info, trace, warn};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, PhysicalSize};
use winit::event::{self, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::scancode::PhysicalKeyExtScancode as _;
use winit::window::{CursorIcon, CustomCursor, Window, WindowAttributes};

type WindowSurface = (Arc<Window>, softbuffer::Surface<DisplayHandle<'static>, Arc<Window>>);

/// Events delivered from the viewer-hosted RPC server to the window.
pub enum ViewerEvent {
    FrameAvailable,
    Shutdown,
}

/// Where local window input is sent.
enum InputTarget {
    Direct(RdpInputSender),
    Rpc(Arc<Daemon>),
}

/// A viewer application driven by the RDP client's native output events.
pub struct App {
    inner: RpcApp,
}

/// A viewer application driven by a viewer-hosted RPC daemon.
pub struct RpcApp {
    input_target: InputTarget,
    frame_wakeup: Option<Arc<AtomicBool>>,
    context: softbuffer::Context<DisplayHandle<'static>>,
    initial_window_size: PhysicalSize<u32>,
    window: Option<WindowSurface>,
    buffer: Vec<u32>,
    buffer_size: (u16, u16),
    input_database: ironrdp::input::Database,
    last_size: Option<PhysicalSize<u32>>,
    resize_timeout: Option<Instant>,
    /// Keyboard modifiers as last reported by winit, for the local shortcuts.
    modifiers: winit::keyboard::ModifiersState,
    /// The mstsc-style connection bar shown in full screen.
    bar: crate::bar::ConnectionBar,
    /// The "disconnect?" dialog, while it is up.
    close_dialog: Option<crate::modal::CloseDialog>,
    /// Last pointer position, for the dialog's click hit-testing.
    pointer: (f64, f64),
}

/// What the close dialog made of a window event.
enum DialogOutcome {
    NotShown,
    Consumed,
    Confirm,
    Cancel,
}

/// The desktop-file identity shared with the launcher (`StartupWMClass` in
/// `io.winrdp.Next.desktop`), so the shell shows the Win RDP icon for the
/// session window and groups it with the launcher.
const APP_ID: &str = "winrdp-next";

/// Sets the Wayland `app_id` and the X11 `WM_CLASS` to [`APP_ID`].
fn with_app_identity(attributes: WindowAttributes) -> WindowAttributes {
    // Both extension traits set the same attribute, which winit applies as the
    // Wayland `app_id` or the X11 `WM_CLASS` depending on the backend it picks
    // at run time, so one call through either trait covers both.
    #[cfg(all(target_os = "linux", feature = "wayland"))]
    {
        use winit::platform::wayland::WindowAttributesExtWayland as _;
        attributes.with_name(APP_ID, APP_ID)
    }
    #[cfg(all(target_os = "linux", feature = "x11", not(feature = "wayland")))]
    {
        use winit::platform::x11::WindowAttributesExtX11 as _;
        attributes.with_name(APP_ID, APP_ID)
    }
    #[cfg(not(all(target_os = "linux", any(feature = "wayland", feature = "x11"))))]
    {
        attributes
    }
}

/// The launcher's icon, for X11 window managers and taskbars that take the icon
/// from the window rather than from the desktop file.
fn app_icon() -> Option<winit::window::Icon> {
    static PNG: &[u8] = include_bytes!("../../src-tauri/icons/icon.png");
    let mut decoder = png::Decoder::new(std::io::Cursor::new(PNG));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    buffer.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer.chunks_exact(3).flat_map(|px| [px[0], px[1], px[2], 0xFF]).collect(),
        _ => return None,
    };
    winit::window::Icon::from_rgba(rgba, info.width, info.height).ok()
}

/// The session window title: the saved computer name from the launcher, if any.
fn window_title() -> String {
    std::env::var("WINRDP_TITLE")
        .map(|n| format!("{n} - Win RDP"))
        .unwrap_or_else(|_| "Win RDP".to_owned())
}

/// Human-readable transport, as shown in the window title: `UDP v2` when graphics
/// flow over the reliable RDP-UDP tunnel, `TCP` otherwise.
fn transport_label(reliable_udp: bool, udp_version: Option<u16>) -> String {
    match (reliable_udp, udp_version) {
        (true, Some(version)) => format!("UDP v{version}"),
        (true, None) => "UDP".to_owned(),
        (false, _) => "TCP".to_owned(),
    }
}

/// Publish the transport for the launcher, which shows it on the session tab.
/// The file named by `WINRDP_STATUS_FILE` is replaced atomically.
fn write_status_file(reliable_udp: bool, udp_version: Option<u16>) {
    let Some(path) = std::env::var_os("WINRDP_STATUS_FILE") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    let version = udp_version.map_or("null".to_owned(), |v| v.to_string());
    let json = format!(
        "{{\"transport\":\"{}\",\"udpVersion\":{version},\"label\":\"{}\"}}\n",
        if reliable_udp { "udp" } else { "tcp" },
        transport_label(reliable_udp, udp_version)
    );
    let tmp = path.with_extension("tmp");
    if let Err(error) = std::fs::write(&tmp, json).and_then(|()| std::fs::rename(&tmp, &path)) {
        warn!(%error, ?path, "Could not write the session status file");
    }
}

impl App {
    pub fn new(
        event_loop: &EventLoop<RdpOutputEvent>,
        input_event_sender: &RdpInputSender,
        initial_window_size: PhysicalSize<u32>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            inner: RpcApp::new_inner(
                event_loop,
                InputTarget::Direct(input_event_sender.clone()),
                None,
                initial_window_size,
            )?,
        })
    }
}

impl RpcApp {
    pub fn new(
        event_loop: &EventLoop<ViewerEvent>,
        daemon: Arc<Daemon>,
        frame_wakeup: Arc<AtomicBool>,
        initial_window_size: PhysicalSize<u32>,
    ) -> anyhow::Result<Self> {
        Self::new_inner(
            event_loop,
            InputTarget::Rpc(daemon),
            Some(frame_wakeup),
            initial_window_size,
        )
    }

    fn new_inner<T: 'static>(
        event_loop: &EventLoop<T>,
        input_target: InputTarget,
        frame_wakeup: Option<Arc<AtomicBool>>,
        initial_window_size: PhysicalSize<u32>,
    ) -> anyhow::Result<Self> {
        // SAFETY: We drop the softbuffer context right before the event loop is stopped, thus making this safe.
        // FIXME: This is not a sufficient proof and the API is actually unsound as-is.
        let display_handle = unsafe {
            core::mem::transmute::<DisplayHandle<'_>, DisplayHandle<'static>>(
                event_loop.display_handle().context("get display handle")?,
            )
        };
        let context = softbuffer::Context::new(display_handle)
            .map_err(|e| anyhow::anyhow!("unable to initialize softbuffer context: {e}"))?;

        let input_database = ironrdp::input::Database::new();
        Ok(Self {
            input_target,
            frame_wakeup,
            context,
            initial_window_size,
            window: None,
            buffer: Vec::new(),
            buffer_size: (0, 0),
            input_database,
            last_size: None,
            resize_timeout: None,
            modifiers: winit::keyboard::ModifiersState::empty(),
            bar: crate::bar::ConnectionBar::new(std::env::var("WINRDP_TITLE").unwrap_or_else(|_| "Win RDP".to_owned())),
            close_dialog: None,
            pointer: (0.0, 0.0),
        })
    }

    fn send_resize_event(&mut self) {
        let Some(size) = self.last_size else {
            return;
        };
        let Some((window, _)) = self.window.as_mut() else {
            return;
        };
        #[expect(clippy::as_conversions, reason = "casting f64 to u32")]
        let scale_factor = (window.scale_factor() * 100.0) as u32;

        let width = u16::try_from(size.width).expect("reasonable width");
        let height = u16::try_from(size.height).expect("reasonable height");

        match &self.input_target {
            InputTarget::Direct(input_event_sender) => match input_event_sender.try_send(RdpInputEvent::Resize {
                width,
                height,
                scale_factor,
                // TODO: it should be possible to get the physical size here, however winit doesn't make it straightforward.
                // FreeRDP does it based on DPI reading grabbed via [`SDL_GetDisplayDPI`](https://wiki.libsdl.org/SDL2/SDL_GetDisplayDPI):
                // https://github.com/FreeRDP/FreeRDP/blob/ba8cf8cf2158018fb7abbedb51ab245f369be813/client/SDL/sdl_monitor.cpp#L250-L262
                // See also: https://github.com/rust-windowing/winit/issues/826
                physical_size: None,
            }) {
                Ok(()) => self.last_size = None,
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                    self.resize_timeout = Some(Instant::now() + Duration::from_millis(10));
                }
                Err(_) => {
                    self.last_size = None;
                    warn!("Unable to enqueue resize event because the RDP session is closed");
                }
            },
            InputTarget::Rpc(daemon) => match daemon.try_resize(width, height) {
                Ok(()) => self.last_size = None,
                Err(ResizeError::Full) => self.resize_timeout = Some(Instant::now() + Duration::from_millis(10)),
                Err(error) => {
                    self.last_size = None;
                    warn!(?error, "Unable to resize the RPC-backed RDP session");
                }
            },
        }
    }

    fn update_rpc_frame(&mut self) {
        let InputTarget::Rpc(daemon) = &self.input_target else {
            return;
        };
        let Some(frame) = daemon.current_frame() else {
            return;
        };
        let (Some(width), Some(height)) = (
            NonZeroU32::new(u32::from(frame.width)),
            NonZeroU32::new(u32::from(frame.height)),
        ) else {
            return;
        };
        self.update_frame(frame.pixels, width, height);
    }

    fn update_frame(&mut self, buffer: Vec<u32>, width: NonZeroU32, height: NonZeroU32) {
        let Some((window, _surface)) = self.window.as_mut() else {
            return;
        };
        trace!(?width, ?height, "Received RPC-backed image");
        self.buffer_size = (
            u16::try_from(width.get()).expect("frame width fits in u16"),
            u16::try_from(height.get()).expect("frame height fits in u16"),
        );
        self.buffer = buffer;
        window.request_redraw();
    }

    fn draw(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        let Some((window, surface)) = self.window.as_mut() else {
            return;
        };
        // Present into the window's current size, not the remote desktop's. While a
        // resize is pending the two differ; blitting the old frame at the old stride
        // into a larger window skews every row, so copy row by row and clip, leaving
        // the uncovered area black until the server sends the new-size frame.
        let size = window.inner_size();
        let (Some(width), Some(height)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        surface.resize(width, height).expect("surface resize");
        let mut sb_buffer = surface.buffer_mut().expect("surface buffer");
        let (buffer_width, buffer_height) = (usize::from(self.buffer_size.0), usize::from(self.buffer_size.1));
        let (window_width, window_height) = (size.width as usize, size.height as usize);
        if (buffer_width, buffer_height) == (window_width, window_height) && self.buffer.len() == sb_buffer.len() {
            sb_buffer.copy_from_slice(self.buffer.as_slice());
        } else {
            let copy_width = buffer_width.min(window_width);
            for y in 0..window_height {
                let row = &mut sb_buffer[y * window_width..(y + 1) * window_width];
                if y < buffer_height {
                    let source = &self.buffer[y * buffer_width..y * buffer_width + copy_width];
                    row[..copy_width].copy_from_slice(source);
                    row[copy_width..].fill(0);
                } else {
                    row.fill(0);
                }
            }
        }
        if window.fullscreen().is_some() && self.bar.is_shown() {
            self.bar.paint(&mut sb_buffer, size);
        }
        if let Some(dialog) = self.close_dialog.as_ref() {
            dialog.paint(&mut sb_buffer, size);
        }
        sb_buffer.present().expect("buffer present");
    }

    /// Leave or enter borderless full screen; the bar and Ctrl+Alt+Enter both land here.
    fn toggle_fullscreen(&mut self) {
        let Some((window, _)) = self.window.as_mut() else {
            return;
        };
        if window.fullscreen().is_some() {
            window.set_fullscreen(None);
            window.set_maximized(false);
        } else {
            window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
        }
        self.bar.reset();
        window.request_redraw();
    }

    /// The window's close button and the bar's X land here: ask first, unless
    /// there is no desktop on screen yet to ask over.
    fn request_close(&mut self, event_loop: &ActiveEventLoop) {
        if self.close_dialog.is_some() {
            return;
        }
        if self.buffer.is_empty() {
            self.confirm_close(event_loop);
            return;
        }
        let computer = std::env::var("WINRDP_TITLE").unwrap_or_else(|_| "this computer".to_owned());
        self.close_dialog = Some(crate::modal::CloseDialog::new(computer));
        if let Some((window, _)) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    /// Route a window event to the close dialog while it is up. Everything but
    /// resize, redraw, and the dialog's own controls is swallowed so the remote
    /// desktop sees no stray input.
    fn dialog_event(&mut self, event: &WindowEvent) -> DialogOutcome {
        if let WindowEvent::CursorMoved { position, .. } = event {
            self.pointer = (position.x, position.y);
        }
        let Some(dialog) = self.close_dialog.as_mut() else {
            return DialogOutcome::NotShown;
        };
        let Some((window, _)) = self.window.as_ref() else {
            return DialogOutcome::NotShown;
        };
        let size = window.inner_size();
        match event {
            WindowEvent::Resized(_)
            | WindowEvent::RedrawRequested
            | WindowEvent::CloseRequested
            | WindowEvent::Destroyed
            | WindowEvent::Focused(_)
            | WindowEvent::Moved(_)
            | WindowEvent::ScaleFactorChanged { .. } => DialogOutcome::NotShown,
            WindowEvent::CursorMoved { position, .. } => {
                if dialog.pointer_moved(size, position.x, position.y) {
                    window.request_redraw();
                }
                DialogOutcome::Consumed
            }
            WindowEvent::MouseInput {
                state: event::ElementState::Pressed,
                button: event::MouseButton::Left,
                ..
            } => match dialog.click(size, self.pointer.0, self.pointer.1) {
                Some(crate::modal::Choice::Disconnect) => DialogOutcome::Confirm,
                Some(crate::modal::Choice::Cancel) => DialogOutcome::Cancel,
                None => DialogOutcome::Consumed,
            },
            WindowEvent::KeyboardInput { event: key, .. } if key.state == event::ElementState::Pressed => {
                match key.physical_key {
                    PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space) => DialogOutcome::Confirm,
                    PhysicalKey::Code(KeyCode::Escape) => DialogOutcome::Cancel,
                    _ => DialogOutcome::Consumed,
                }
            }
            _ => DialogOutcome::Consumed,
        }
    }

    /// Close the session the way the window's close button does.
    fn confirm_close(&mut self, event_loop: &ActiveEventLoop) {
        match &self.input_target {
            InputTarget::Direct(input_event_sender) => input_event_sender.request_graceful_close(),
            InputTarget::Rpc(daemon) => {
                let _ = daemon.disconnect();
                daemon.shutdown();
                event_loop.exit();
            }
        }
    }
    fn on_about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if self.resize_timeout.is_some_and(|timeout| timeout <= now) {
            self.resize_timeout = None;
            self.send_resize_event();
        }
        if self.bar.tick(now) {
            if let Some((window, _)) = self.window.as_ref() {
                window.request_redraw();
            }
        }
        let next = [self.resize_timeout, self.bar.deadline()].into_iter().flatten().min();
        event_loop.set_control_flow(match next {
            Some(deadline) => ControlFlow::wait_duration(deadline.saturating_duration_since(now)),
            None => ControlFlow::Wait,
        });
    }

    fn on_resumed(&mut self, event_loop: &ActiveEventLoop) {
        let fullscreen = std::env::var("WINRDP_FULLSCREEN").is_ok_and(|v| v == "1");
        let window_attributes = WindowAttributes::default()
            .with_title(window_title())
            .with_window_icon(app_icon())
            .with_inner_size(self.initial_window_size)
            .with_fullscreen(fullscreen.then_some(winit::window::Fullscreen::Borderless(None)));
        let window_attributes = with_app_identity(window_attributes);
        match event_loop.create_window(window_attributes) {
            Ok(window) => {
                let window = Arc::new(window);
                if matches!(&self.input_target, InputTarget::Rpc(_)) {
                    window.set_cursor_visible(false);
                }
                let surface = softbuffer::Surface::new(&self.context, Arc::clone(&window)).expect("surface");
                self.window = Some((window, surface));
            }
            Err(error) => {
                error!(%error, "Failed to create window");
                event_loop.exit();
            }
        }
    }

    fn on_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(id) = self.window.as_ref().map(|(window, _)| window.id()) else {
            return;
        };
        if window_id != id {
            return;
        }
        match self.dialog_event(&event) {
            DialogOutcome::NotShown => {}
            DialogOutcome::Consumed => return,
            DialogOutcome::Confirm => {
                self.close_dialog = None;
                self.confirm_close(event_loop);
                return;
            }
            DialogOutcome::Cancel => {
                self.close_dialog = None;
                if let Some((window, _)) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }
        }
        let Some((window, _)) = self.window.as_mut() else {
            return;
        };

        match event {
            WindowEvent::Resized(size) => {
                // Maximizing from the title bar means "fill the screen": go borderless full
                // screen with the connection bar instead of keeping the desktop's title bar.
                if window.is_maximized() && window.fullscreen().is_none() {
                    window.set_maximized(false);
                    window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                    self.bar.reset();
                    return;
                }
                self.last_size = Some(size);
                // Coalesce the burst of events a drag produces, but ask the server for
                // the new size quickly; every millisecond here is spent showing a
                // clipped stale frame.
                self.resize_timeout = Some(Instant::now() + Duration::from_millis(150));
                window.request_redraw();
            }
            WindowEvent::CloseRequested => self.request_close(event_loop),
            WindowEvent::DroppedFile(_) => {
                // TODO(#110): File upload
            }
            // WindowEvent::ReceivedCharacter(_) => {
            // Sadly, we can't use this winit event to send RDP unicode events because
            // of the several reasons:
            // 1. `ReceivedCharacter` event doesn't provide a way to distinguish between
            //    key press and key release, therefore the only way to use it is to send
            //    a key press + release events sequentially, which will not allow to
            //    handle long press and key repeat events.
            // 2. This event do not fire for non-printable keys (e.g. Control, Alt, etc.)
            // 3. This event fies BEFORE `KeyboardInput` event, so we can't make a
            //    reasonable workaround for `1` and `2` by collecting physical key press
            //    information first via `KeyboardInput` before processing `ReceivedCharacter`.
            //
            // However, all of these issues can be solved by updating `winit` to the
            // newer version.
            //
            // TODO(#376): Update winit
            // TODO(#376): Implement unicode input in native client
            // }
            WindowEvent::KeyboardInput { event, .. }
                if self.modifiers.control_key()
                    && self.modifiers.alt_key()
                    && matches!(event.physical_key, PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter)) =>
            {
                // Ctrl+Alt+Enter toggles full screen locally, as in mstsc and the launcher.
                if event.state == event::ElementState::Pressed && !event.repeat {
                    self.toggle_fullscreen();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // `winit` scan codes are platform-specific, but RDP expects PC/AT set-1 scan codes.
                // Override the navigation keys reported in #535 before using the existing fallback.
                let mapped_scancode = match event.physical_key {
                    // Send each physical modifier in the same stream as letters. A
                    // later aggregate ModifiersChanged event cannot establish the
                    // ordering needed by quick Ctrl+C / Ctrl+V combinations.
                    PhysicalKey::Code(KeyCode::ShiftLeft) => Some(ironrdp::input::Scancode::from_u8(false, 0x2A)),
                    PhysicalKey::Code(KeyCode::ShiftRight) => Some(ironrdp::input::Scancode::from_u8(false, 0x36)),
                    PhysicalKey::Code(KeyCode::ControlLeft) => Some(ironrdp::input::Scancode::from_u8(false, 0x1D)),
                    PhysicalKey::Code(KeyCode::ControlRight) => Some(ironrdp::input::Scancode::from_u8(true, 0x1D)),
                    PhysicalKey::Code(KeyCode::AltLeft) => Some(ironrdp::input::Scancode::from_u8(false, 0x38)),
                    PhysicalKey::Code(KeyCode::AltRight) => Some(ironrdp::input::Scancode::from_u8(true, 0x38)),
                    PhysicalKey::Code(KeyCode::SuperLeft) => Some(ironrdp::input::Scancode::from_u8(true, 0x5B)),
                    PhysicalKey::Code(KeyCode::SuperRight) => Some(ironrdp::input::Scancode::from_u8(true, 0x5C)),
                    PhysicalKey::Code(KeyCode::Home) => Some(ironrdp::input::Scancode::from_u8(true, 0x47)),
                    PhysicalKey::Code(KeyCode::ArrowUp) => Some(ironrdp::input::Scancode::from_u8(true, 0x48)),
                    PhysicalKey::Code(KeyCode::PageUp) => Some(ironrdp::input::Scancode::from_u8(true, 0x49)),
                    PhysicalKey::Code(KeyCode::ArrowLeft) => Some(ironrdp::input::Scancode::from_u8(true, 0x4B)),
                    PhysicalKey::Code(KeyCode::ArrowRight) => Some(ironrdp::input::Scancode::from_u8(true, 0x4D)),
                    PhysicalKey::Code(KeyCode::End) => Some(ironrdp::input::Scancode::from_u8(true, 0x4F)),
                    PhysicalKey::Code(KeyCode::ArrowDown) => Some(ironrdp::input::Scancode::from_u8(true, 0x50)),
                    PhysicalKey::Code(KeyCode::PageDown) => Some(ironrdp::input::Scancode::from_u8(true, 0x51)),
                    PhysicalKey::Code(KeyCode::Insert) => Some(ironrdp::input::Scancode::from_u8(true, 0x52)),
                    PhysicalKey::Code(KeyCode::Delete) => Some(ironrdp::input::Scancode::from_u8(true, 0x53)),
                    _ => None,
                };

                let scancode = if let Some(scancode) = mapped_scancode {
                    scancode
                } else {
                    let Some(scancode) = event.physical_key.to_scancode() else {
                        return;
                    };
                    let Ok(scancode) = u16::try_from(scancode) else {
                        warn!("Unsupported scancode: `{scancode:#X}`; ignored");
                        return;
                    };

                    ironrdp::input::Scancode::from_u16(scancode)
                };

                let operation = match event.state {
                    event::ElementState::Pressed => ironrdp::input::Operation::KeyPressed(scancode),
                    event::ElementState::Released => ironrdp::input::Operation::KeyReleased(scancode),
                };

                apply_and_send_fast_path_events(
                    &self.input_target,
                    &mut self.input_database,
                    core::iter::once(operation),
                );
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::Focused(false) => {
                self.modifiers = winit::keyboard::ModifiersState::empty();
                // The key-up may be delivered to a different local window after
                // Alt+Tab. Release held inputs so the remote is not left with Ctrl
                // or Alt stuck down when this window regains focus.
                let keys = self.input_database.keyboard_state();
                let operations = keys.iter_ones().map(|index| {
                    ironrdp::input::Operation::KeyReleased(ironrdp::input::Scancode::from_u8(index >= 256, index as u8))
                }).collect::<Vec<_>>();
                apply_and_send_fast_path_events(&self.input_target, &mut self.input_database, operations);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let win_size = window.inner_size();
                if window.fullscreen().is_some() {
                    let (captured, redraw) = self.bar.pointer_moved(win_size, position.x, position.y, Instant::now());
                    if redraw {
                        window.request_redraw();
                    }
                    if captured {
                        // The bar owns the pointer; the remote desktop does not see it.
                        return;
                    }
                }
                #[expect(clippy::as_conversions, reason = "casting f64 to u16")]
                let x = (position.x / f64::from(win_size.width) * f64::from(self.buffer_size.0)) as u16;
                #[expect(clippy::as_conversions, reason = "casting f64 to u16")]
                let y = (position.y / f64::from(win_size.height) * f64::from(self.buffer_size.1)) as u16;
                let operation = ironrdp::input::Operation::MouseMove(ironrdp::input::MousePosition { x, y });

                apply_and_send_fast_path_events(
                    &self.input_target,
                    &mut self.input_database,
                    core::iter::once(operation),
                );
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let mut operations = SmallVec::<[ironrdp::input::Operation; 2]>::new();

                match delta {
                    event::MouseScrollDelta::LineDelta(delta_x, delta_y) => {
                        if delta_x.abs() > 0.001 {
                            operations.push(ironrdp::input::Operation::WheelRotations(
                                ironrdp::input::WheelRotations {
                                    is_vertical: false,
                                    #[expect(clippy::as_conversions, reason = "casting f32 to i16")]
                                    rotation_units: (delta_x * 100.) as i16,
                                },
                            ));
                        }

                        if delta_y.abs() > 0.001 {
                            operations.push(ironrdp::input::Operation::WheelRotations(
                                ironrdp::input::WheelRotations {
                                    is_vertical: true,
                                    #[expect(clippy::as_conversions, reason = "casting f32 to i16")]
                                    rotation_units: (delta_y * 100.) as i16,
                                },
                            ));
                        }
                    }
                    event::MouseScrollDelta::PixelDelta(delta) => {
                        if delta.x.abs() > 0.001 {
                            operations.push(ironrdp::input::Operation::WheelRotations(
                                ironrdp::input::WheelRotations {
                                    is_vertical: false,
                                    #[expect(clippy::as_conversions, reason = "casting f64 to i16")]
                                    rotation_units: delta.x as i16,
                                },
                            ));
                        }

                        if delta.y.abs() > 0.001 {
                            operations.push(ironrdp::input::Operation::WheelRotations(
                                ironrdp::input::WheelRotations {
                                    is_vertical: true,
                                    #[expect(clippy::as_conversions, reason = "casting f64 to i16")]
                                    rotation_units: delta.y as i16,
                                },
                            ));
                        }
                    }
                };

                apply_and_send_fast_path_events(&self.input_target, &mut self.input_database, operations);
            }
            WindowEvent::MouseInput { state, button, .. } if self.bar.is_shown() && self.bar.hovered() => {
                if button == event::MouseButton::Left && state == event::ElementState::Pressed {
                    match self.bar.click(Instant::now()) {
                        Some(crate::bar::Hit::Restore) => self.toggle_fullscreen(),
                        Some(crate::bar::Hit::Close) => self.request_close(event_loop),
                        Some(crate::bar::Hit::Pin) => window.request_redraw(),
                        Some(crate::bar::Hit::Body) | None => {}
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let mouse_button = match button {
                    event::MouseButton::Left => ironrdp::input::MouseButton::Left,
                    event::MouseButton::Right => ironrdp::input::MouseButton::Right,
                    event::MouseButton::Middle => ironrdp::input::MouseButton::Middle,
                    event::MouseButton::Back => ironrdp::input::MouseButton::X1,
                    event::MouseButton::Forward => ironrdp::input::MouseButton::X2,
                    event::MouseButton::Other(native_button) => {
                        if let Some(button) = ironrdp::input::MouseButton::from_native_button(native_button) {
                            button
                        } else {
                            return;
                        }
                    }
                };

                let operation = match state {
                    event::ElementState::Pressed => ironrdp::input::Operation::MouseButtonPressed(mouse_button),
                    event::ElementState::Released => ironrdp::input::Operation::MouseButtonReleased(mouse_button),
                };

                apply_and_send_fast_path_events(
                    &self.input_target,
                    &mut self.input_database,
                    core::iter::once(operation),
                );
            }
            WindowEvent::CursorLeft { .. } => {
                if self.bar.pointer_left(Instant::now()) {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                self.draw();
            }
            WindowEvent::ActivationTokenDone { .. }
            | WindowEvent::Moved(_)
            | WindowEvent::Destroyed
            | WindowEvent::HoveredFile(_)
            | WindowEvent::HoveredFileCancelled
            | WindowEvent::Focused(true)
            | WindowEvent::Ime(_)
            | WindowEvent::CursorEntered { .. }
            | WindowEvent::PinchGesture { .. }
            | WindowEvent::PanGesture { .. }
            | WindowEvent::DoubleTapGesture { .. }
            | WindowEvent::RotationGesture { .. }
            | WindowEvent::TouchpadPressure { .. }
            | WindowEvent::AxisMotion { .. }
            | WindowEvent::Touch(_)
            | WindowEvent::ScaleFactorChanged { .. }
            | WindowEvent::ThemeChanged(_)
            | WindowEvent::Occluded(_) => {
                // ignore
            }
        }
    }

    fn handle_viewer_event(&mut self, event_loop: &ActiveEventLoop, event: ViewerEvent) {
        match event {
            ViewerEvent::FrameAvailable => {
                if let Some(frame_wakeup) = &self.frame_wakeup {
                    frame_wakeup.store(false, Ordering::Release);
                }
                self.update_rpc_frame();
            }
            ViewerEvent::Shutdown => event_loop.exit(),
        }
    }

    fn handle_output_event(&mut self, event_loop: &ActiveEventLoop, event: RdpOutputEvent) {
        let Some((window, _surface)) = self.window.as_mut() else {
            return;
        };
        match event {
            RdpOutputEvent::Connected => info!("RDP session connected"),
            RdpOutputEvent::MonitorLayout(monitors) => {
                debug!(monitor_count = monitors.len(), "Received remote monitor layout");
            }
            RdpOutputEvent::LoginComplete => info!("RDP login complete"),
            RdpOutputEvent::PostLogonDisplayRedraw => info!("Requested post-logon display redraw"),
            RdpOutputEvent::MalformedBitmapDisplayRedraw => {
                warn!("Requested display redraw after discarding a malformed bitmap update");
            }
            RdpOutputEvent::Image { buffer, width, height } => {
                trace!(width = ?width, height = ?height, "Received image with size");
                trace!(window_physical_size = ?window.inner_size(), "Drawing image to the window with size");
                self.buffer_size = (width.get(), height.get());
                self.buffer = buffer;
                window.request_redraw();
            }
            RdpOutputEvent::ConnectionFailure(error) => {
                error!(?error);
                eprintln!("Connection error: {}", error.report().with_locations());
                // TODO set proc_exit::sysexits::PROTOCOL_ERR.as_raw());
                event_loop.exit();
            }
            RdpOutputEvent::Terminated(result) => {
                let _exit_code = match result {
                    Ok(reason) => {
                        println!("Terminated gracefully: {reason}");
                        proc_exit::sysexits::OK
                    }
                    Err(error) => {
                        error!(?error);
                        eprintln!("Active session error: {}", error.report().with_locations());
                        proc_exit::sysexits::PROTOCOL_ERR
                    }
                };
                // TODO set exit_code.as_raw());
                event_loop.exit();
            }
            RdpOutputEvent::PointerHidden => {
                window.set_cursor_visible(false);
            }
            RdpOutputEvent::PointerDefault => {
                window.set_cursor(CursorIcon::default());
                window.set_cursor_visible(true);
            }
            RdpOutputEvent::PointerPosition { x, y } => {
                if let Err(error) = window.set_cursor_position(LogicalPosition::new(x, y)) {
                    error!(?error, "Failed to set cursor position");
                }
            }
            RdpOutputEvent::PointerBitmap(pointer) => {
                debug!(width = ?pointer.width, height = ?pointer.height, "Received pointer bitmap");
                match CustomCursor::from_rgba(
                    pointer.bitmap_data.clone(),
                    pointer.width,
                    pointer.height,
                    pointer.hotspot_x,
                    pointer.hotspot_y,
                ) {
                    Ok(cursor) => window.set_cursor(event_loop.create_custom_cursor(cursor)),
                    Err(error) => error!(?error, "Failed to set cursor bitmap"),
                }
                window.set_cursor_visible(true);
            }
            RdpOutputEvent::DisplayResizeFallback(reason) => {
                warn!(
                    ?reason,
                    "Reconnecting because dynamic display resize could not complete"
                );
            }
            RdpOutputEvent::AutoReconnecting {
                attempt,
                maximum_attempts,
                response,
                ..
            } => {
                warn!(attempt, maximum_attempts, "Stopping unsupported automatic reconnect");
                let _ = response.send(AutoReconnectDecision::Stop);
            }
            RdpOutputEvent::AutoReconnected => {
                info!("RDP session automatically reconnected");
            }
            RdpOutputEvent::RailHandshake {
                handshake_ex_flags,
                initialization_message_count,
                queued_execute_count,
            } => {
                debug!(
                    ?handshake_ex_flags,
                    initialization_message_count, queued_execute_count, "RAIL static channel initialized"
                );
            }
            RdpOutputEvent::RailDesktopSynchronized { released_execute_count } => {
                debug!(
                    released_execute_count,
                    "RAIL queued input released after desktop synchronization"
                );
            }
            RdpOutputEvent::RailPostHandshakeQueueReleased { released_execute_count } => {
                debug!(
                    released_execute_count,
                    "RAIL queued input released after handshake fallback"
                );
            }
            RdpOutputEvent::RailExecuteResult(result) => {
                debug!(?result, "RAIL execute completed");
            }
            RdpOutputEvent::RailExecuteFailed { flags, reason, .. } => {
                warn!(flags, ?reason, "RAIL execute could not be processed");
            }
            RdpOutputEvent::RailApplicationId {
                window_id,
                application_id,
                process_id,
                process_image_name,
            } => {
                debug!(
                    window_id,
                    %application_id,
                    ?process_id,
                    ?process_image_name,
                    "RAIL application identity received"
                );
            }
            RdpOutputEvent::RailControl(control) => {
                debug!(?control, "RAIL control received");
            }
            RdpOutputEvent::Transport {
                reliable_udp,
                udp_version,
            } => {
                let label = transport_label(reliable_udp, udp_version);
                info!(%label, "Session transport");
                window.set_title(&format!("{} ({label})", window_title()));
                self.bar.set_transport(&label);
                window.request_redraw();
                write_status_file(reliable_udp, udp_version);
            }
            RdpOutputEvent::WindowingOrders(_) => {}
        }
    }
}

impl ApplicationHandler<ViewerEvent> for RpcApp {
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.on_about_to_wait(event_loop);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.on_resumed(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: winit::window::WindowId, event: WindowEvent) {
        self.on_window_event(event_loop, window_id, event);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: ViewerEvent) {
        self.handle_viewer_event(event_loop, event);
    }
}

impl ApplicationHandler<RdpOutputEvent> for App {
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.on_about_to_wait(event_loop);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.on_resumed(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: winit::window::WindowId, event: WindowEvent) {
        self.inner.on_window_event(event_loop, window_id, event);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: RdpOutputEvent) {
        self.inner.handle_output_event(event_loop, event);
    }
}

fn apply_and_send_fast_path_events(
    input_target: &InputTarget,
    input_database: &mut ironrdp::input::Database,
    operations: impl IntoIterator<Item = ironrdp::input::Operation>,
) {
    match input_target {
        InputTarget::Direct(input_event_sender) => {
            let Ok(permit) = input_event_sender.try_reserve() else {
                return;
            };
            let input_events = input_database.apply(operations);
            if !input_events.is_empty() {
                permit.send(RdpInputEvent::FastPath(input_events));
            }
        }
        InputTarget::Rpc(daemon) => {
            let _ = daemon.input_operations(operations);
        }
    }
}
