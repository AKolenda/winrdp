#![allow(clippy::print_stderr, clippy::print_stdout)] // allowed in this module only

use core::num::{NonZeroU16, NonZeroU32};
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use std::sync::Arc;
use std::time::Instant;

use ironrdp::client::framebuffer::SharedFramebuffer;
use ironrdp::client::rdp::{AutoReconnectDecision, RdpInputEvent, RdpInputSender, RdpOutputEvent};
use ironrdp_daemon::daemon::{Daemon, ResizeError};
use smallvec::SmallVec;
use tracing::{debug, error, info, trace, warn};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, PhysicalSize};
use winit::event::{self, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorIcon, CustomCursor, Window, WindowAttributes};

use crate::damage::{self, Damage, Layout, Rect};
use crate::status::{
    Failure, connect_failure, transport_label, write_closed_status, write_failure_status, write_status_file,
};

pub use crate::status::{STARTUP_FAILURE_EXIT, report_startup_failure};

type WindowSurface = (Arc<Window>, softbuffer::Surface<OwnedDisplayHandle, Arc<Window>>);

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
    context: softbuffer::Context<OwnedDisplayHandle>,
    initial_window_size: PhysicalSize<u32>,
    window: Option<WindowSurface>,
    /// The remote desktop: written by the RDP session, or by [`Self::update_frame`] for
    /// full frames, and presented from here.
    frame: SharedFramebuffer,
    /// The desktop size as of the last look at `frame`, so pointer input needs no lock.
    /// Zero until the first frame.
    desktop_size: (u16, u16),
    /// What recent presents changed, to bring an older softbuffer buffer up to date.
    history: damage::History,
    /// The window areas the connection bar and the close dialog covered in the last present.
    overlays: Damage,
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
    /// What the process should exit with once the event loop stops, so a caller
    /// can tell a refused connection from a desktop the user closed.
    exit_code: i32,
}

/// How much of the window a draw presents.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Present {
    /// What changed: a new frame area, and the overlays painted over it.
    Changes,
    /// The whole window: a redraw request from the window system or for a changed overlay.
    Everything,
}

/// What the close dialog made of a window event.
enum DialogOutcome {
    NotShown,
    Consumed,
    Confirm,
    Cancel,
}

/// Which close signal the RDP thread needs.
#[derive(Debug, PartialEq, Eq)]
enum CloseKind {
    /// Cancel a connection attempt outright. The graceful signal is only read
    /// once the session is active, so it cannot end a connect that is still in
    /// flight.
    Cancel,
    /// Ask an active session to shut down over RDP.
    Graceful,
}

/// A window with no desktop drawn in it yet is still connecting.
fn close_kind(has_desktop: bool) -> CloseKind {
    if has_desktop {
        CloseKind::Graceful
    } else {
        CloseKind::Cancel
    }
}

/// Where a pointer position in the window lands on the remote desktop.
///
/// [`RpcApp::present_frame`] blits the remote frame 1:1 from the top-left and pads the rest
/// of the window black, so the pointer maps the same way. Scaling the position
/// by the window size instead puts clicks in the wrong place for as long as the
/// window and the desktop disagree, which is every pending resize and the whole
/// session on a host that refuses to resize. `None` means the pointer is over
/// the padding rather than over the desktop.
fn remote_pointer(position: (f64, f64), desktop: (u16, u16)) -> Option<(u16, u16)> {
    let (x, y) = position;
    if x < 0.0 || y < 0.0 || x >= f64::from(desktop.0) || y >= f64::from(desktop.1) {
        return None;
    }
    #[expect(clippy::as_conversions, reason = "bounded above by the desktop size")]
    Some((x as u16, y as u16))
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
    static PNG: &[u8] = include_bytes!("../../packaging/icons/256.png");
    let mut decoder = png::Decoder::new(std::io::Cursor::new(PNG));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    buffer.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|px| [px[0], px[1], px[2], 0xFF])
            .collect(),
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

impl App {
    /// What the process should exit with once the window is gone: non-zero when
    /// the connection was refused or dropped on an error.
    pub fn exit_code(&self) -> i32 {
        self.inner.exit_code
    }

    /// `frame` is the framebuffer the RDP client was given with
    /// `RdpClient::with_shared_framebuffer`.
    pub fn new(
        event_loop: &EventLoop<RdpOutputEvent>,
        input_event_sender: &RdpInputSender,
        frame: SharedFramebuffer,
        initial_window_size: PhysicalSize<u32>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            inner: RpcApp::new_inner(
                event_loop,
                InputTarget::Direct(input_event_sender.clone()),
                None,
                frame,
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
            SharedFramebuffer::new(),
            initial_window_size,
        )
    }

    fn new_inner<T: 'static>(
        event_loop: &EventLoop<T>,
        input_target: InputTarget,
        frame_wakeup: Option<Arc<AtomicBool>>,
        frame: SharedFramebuffer,
        initial_window_size: PhysicalSize<u32>,
    ) -> anyhow::Result<Self> {
        let context = softbuffer::Context::new(event_loop.owned_display_handle())
            .map_err(|e| anyhow::anyhow!("unable to initialize softbuffer context: {e}"))?;

        let input_database = ironrdp::input::Database::new();
        Ok(Self {
            input_target,
            frame_wakeup,
            context,
            initial_window_size,
            window: None,
            frame,
            desktop_size: (0, 0),
            history: damage::History::default(),
            overlays: Damage::new(),
            input_database,
            last_size: None,
            resize_timeout: None,
            modifiers: winit::keyboard::ModifiersState::empty(),
            bar: crate::bar::ConnectionBar::new(std::env::var("WINRDP_TITLE").unwrap_or_else(|_| "Win RDP".to_owned())),
            close_dialog: None,
            pointer: (0.0, 0.0),
            exit_code: proc_exit::sysexits::OK.as_raw(),
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
            InputTarget::Direct(input_event_sender) => {
                match input_event_sender.try_send(RdpInputEvent::Resize {
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
                }
            }
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

    fn update_rpc_frame(&mut self, event_loop: &ActiveEventLoop) {
        let InputTarget::Rpc(daemon) = &self.input_target else {
            return;
        };
        let Some(frame) = daemon.current_frame() else {
            return;
        };
        let (Some(width), Some(height)) = (NonZeroU16::new(frame.width), NonZeroU16::new(frame.height)) else {
            return;
        };
        trace!(?width, ?height, "Received RPC-backed image");
        self.update_frame(event_loop, frame.pixels, width, height);
    }

    /// Shows a full frame, as the RPC daemon and `RdpOutputEvent::Image` deliver them.
    fn update_frame(&mut self, event_loop: &ActiveEventLoop, buffer: Vec<u32>, width: NonZeroU16, height: NonZeroU16) {
        self.frame.lock().replace(buffer, width, height);
        self.draw(event_loop, Present::Changes);
    }

    /// Whether a remote desktop has been received yet.
    fn has_desktop(&self) -> bool {
        self.desktop_size != (0, 0)
    }

    fn draw(&mut self, event_loop: &ActiveEventLoop, present: Present) {
        if let Err(error) = self.present_frame(present) {
            error!(%error, "Failed to present the remote desktop");
            write_failure_status(&Failure {
                reason: "display",
                message: "The remote desktop window could not be updated.".to_owned(),
                detail: error.to_string(),
            });
            self.exit_code = proc_exit::sysexits::OS_ERR.as_raw();
            event_loop.exit();
        }
    }

    /// Paints the remote desktop, the connection bar and the close dialog into the window.
    ///
    /// Only what changed is copied and presented, as far as the buffer softbuffer hands out
    /// allows (see [`damage::History`]). The frame is drawn 1:1 from the top-left corner
    /// into the window's current size, not the desktop's: while a resize is pending the two
    /// differ, and the uncovered area stays black until the server sends the new-size frame.
    fn present_frame(&mut self, present: Present) -> Result<(), softbuffer::SoftBufferError> {
        let Some((window, surface)) = self.window.as_mut() else {
            return Ok(());
        };
        let started = Instant::now();
        let size = window.inner_size();
        let (Some(width), Some(height)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return Ok(());
        };

        // These calls can wait for the compositor or an X11 shared-memory transfer.
        // Acquire the backbuffer before locking the frame so RDP updates can continue.
        surface.resize(width, height)?;
        let mut buffer = surface.buffer_mut()?;

        let fullscreen = window.fullscreen().is_some();
        let bar_shown = fullscreen && self.bar.is_shown();
        let mut overlays = Damage::new();
        if bar_shown {
            let (left, top, width, height) = self.bar.rect(size);
            overlays.extend(
                Rect {
                    x: left as u32,
                    y: top as u32,
                    width: width as u32,
                    height: height as u32,
                }
                .clip(size),
            );
        }
        if self.close_dialog.is_some() {
            // It dims the whole desktop.
            overlays.push(Rect::covering(size));
        }
        let mut frame = self.frame.lock();
        self.desktop_size = (frame.width(), frame.height());
        if self.desktop_size == (0, 0) {
            // Nothing received yet.
            return Ok(());
        }
        // A frame update with nothing left to paint: an earlier draw already took it.
        let dirty = frame.take_dirty();
        if present == Present::Changes && dirty.is_none() {
            return Ok(());
        }

        // An overlay is repainted whole over the frame, and where one was painted last
        // time the frame has to show again.
        let mut changed: Damage = dirty
            .and_then(|dirty| Rect::from(&dirty).clip(size))
            .into_iter()
            .collect();
        changed.extend(overlays.iter().chain(&self.overlays).copied());

        let layout = Layout {
            window: size,
            desktop: self.desktop_size,
        };
        let plan = self
            .history
            .plan(buffer.age(), layout, &changed, present == Present::Everything);
        let frame_size = (usize::from(self.desktop_size.0), usize::from(self.desktop_size.1));
        for rect in &plan.copy {
            damage::blit(&mut buffer, size.width as usize, frame.pixels(), frame_size, *rect);
        }
        // The session waits on the frame to apply its next update; the rest needs none of it.
        drop(frame);

        if bar_shown {
            self.bar.paint(&mut buffer, size);
        }
        if let Some(dialog) = self.close_dialog.as_ref() {
            dialog.paint(&mut buffer, size);
        }
        let rects: SmallVec<[softbuffer::Rect; 4]> =
            plan.present.iter().filter_map(|rect| rect.to_softbuffer()).collect();
        buffer.present_with_damage(&rects)?;
        self.history.record(layout, plan.present);
        self.overlays = overlays;
        self.frame.lock().record_present(started.elapsed());
        Ok(())
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
        if !self.has_desktop() {
            self.confirm_close(event_loop);
            return;
        }
        // A window too small for the panel would swallow every event for a dialog
        // nothing can draw, so close directly instead of asking.
        let Some((window, _)) = self.window.as_ref() else {
            self.confirm_close(event_loop);
            return;
        };
        if !crate::modal::CloseDialog::fits(window.inner_size()) {
            self.confirm_close(event_loop);
            return;
        }
        let computer = std::env::var("WINRDP_TITLE").unwrap_or_else(|_| "this computer".to_owned());
        self.close_dialog = Some(crate::modal::CloseDialog::new(computer));
        window.request_redraw();
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
            InputTarget::Direct(input_event_sender) => match close_kind(self.has_desktop()) {
                CloseKind::Cancel => input_event_sender.request_close(),
                CloseKind::Graceful => input_event_sender.request_graceful_close(),
            },
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
        if self.bar.tick(now)
            && let Some((window, _)) = self.window.as_ref()
        {
            window.request_redraw();
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
                let surface = match softbuffer::Surface::new(&self.context, Arc::clone(&window)) {
                    Ok(surface) => surface,
                    Err(error) => {
                        error!(%error, "Failed to create the remote desktop surface");
                        self.exit_code = proc_exit::sysexits::OS_ERR.as_raw();
                        event_loop.exit();
                        return;
                    }
                };
                self.window = Some((window, surface));
            }
            Err(error) => {
                error!(%error, "Failed to create window");
                self.exit_code = proc_exit::sysexits::OS_ERR.as_raw();
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
                    && matches!(
                        event.physical_key,
                        PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter)
                    ) =>
            {
                // Ctrl+Alt+Enter toggles full screen locally, as in mstsc and the launcher.
                if event.state == event::ElementState::Pressed && !event.repeat {
                    self.toggle_fullscreen();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let key_code = match event.physical_key {
                    PhysicalKey::Code(key_code) => key_code,
                    PhysicalKey::Unidentified(native_key_code) => {
                        warn!(?native_key_code, "Unsupported physical key; ignored");
                        return;
                    }
                };
                let Some((scancode, release_only)) = crate::keymap::map_key_code(key_code) else {
                    warn!(?key_code, "Unsupported physical key; ignored");
                    return;
                };
                let operations: SmallVec<[ironrdp::input::Operation; 2]> = match event.state {
                    event::ElementState::Pressed => {
                        smallvec::smallvec![ironrdp::input::Operation::KeyPressed(scancode)]
                    }
                    event::ElementState::Released if release_only => smallvec::smallvec![
                        ironrdp::input::Operation::KeyPressed(scancode),
                        ironrdp::input::Operation::KeyReleased(scancode),
                    ],
                    event::ElementState::Released => {
                        smallvec::smallvec![ironrdp::input::Operation::KeyReleased(scancode)]
                    }
                };

                apply_and_send_fast_path_events(&self.input_target, &mut self.input_database, operations);
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
                let operations = keys
                    .iter_ones()
                    .map(|index| {
                        ironrdp::input::Operation::KeyReleased(ironrdp::input::Scancode::from_u8(
                            index >= 256,
                            index as u8,
                        ))
                    })
                    .collect::<Vec<_>>();
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
                let Some((x, y)) = remote_pointer((position.x, position.y), self.desktop_size) else {
                    // Over the black padding beside a desktop smaller than the window.
                    return;
                };
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
                self.draw(event_loop, Present::Everything);
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
                self.update_rpc_frame(event_loop);
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
                self.update_frame(event_loop, buffer, width, height);
            }
            // Drawn straight away rather than through a redraw request, which on X11 is also
            // how an exposed window asks to be repainted whole: see `Present::Everything`.
            RdpOutputEvent::FramebufferUpdated => self.draw(event_loop, Present::Changes),
            RdpOutputEvent::ConnectionFailure(error) => {
                error!(?error);
                eprintln!("Connection error: {}", error.report().with_locations());
                // The window never reached a desktop, so the launcher is the only
                // place left to tell the user why.
                let failure = connect_failure(&error);
                info!(reason = failure.reason, message = %failure.message, "Connection refused");
                write_failure_status(&failure);
                self.exit_code = proc_exit::sysexits::PROTOCOL_ERR.as_raw();
                event_loop.exit();
            }
            RdpOutputEvent::Terminated(result) => {
                self.exit_code = match result {
                    Ok(reason) => {
                        println!("Terminated gracefully: {reason}");
                        write_closed_status();
                        proc_exit::sysexits::OK.as_raw()
                    }
                    Err(error) => {
                        error!(?error);
                        eprintln!("Active session error: {}", error.report().with_locations());
                        write_failure_status(&Failure {
                            reason: "session",
                            message: "The connection to the Windows computer stopped.".to_owned(),
                            detail: error.report().to_string(),
                        });
                        proc_exit::sysexits::PROTOCOL_ERR.as_raw()
                    }
                };
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

#[cfg(test)]
mod tests {
    use super::{CloseKind, close_kind, remote_pointer};

    #[test]
    fn closing_before_the_desktop_arrives_cancels_the_connection_attempt() {
        assert_eq!(close_kind(false), CloseKind::Cancel);
        assert_eq!(close_kind(true), CloseKind::Graceful);
    }

    #[test]
    fn the_pointer_maps_onto_the_desktop_the_same_way_the_frame_is_drawn() {
        // 1:1 from the top-left, exactly as draw() blits the frame.
        assert_eq!(remote_pointer((0.0, 0.0), (1280, 720)), Some((0, 0)));
        assert_eq!(remote_pointer((640.9, 360.2), (1280, 720)), Some((640, 360)));
        // A window wider than the desktop pads the rest black; that is not the desktop.
        assert_eq!(remote_pointer((1400.0, 100.0), (1280, 720)), None);
        assert_eq!(remote_pointer((100.0, 800.0), (1280, 720)), None);
        assert_eq!(remote_pointer((1280.0, 719.0), (1280, 720)), None);
        assert_eq!(remote_pointer((-1.0, 10.0), (1280, 720)), None);
        // A window smaller than the desktop clips the frame; every position is on it.
        assert_eq!(remote_pointer((799.0, 599.0), (1280, 720)), Some((799, 599)));
    }
}
