//! Bounded input delivery without losing key or button releases under backpressure.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ironrdp::client::rdp::{RdpInputEvent, RdpInputSender};
use ironrdp::input::{Database, MouseButton, Operation, Scancode};
use ironrdp_daemon::daemon::Daemon;

pub(crate) enum InputTarget {
    Direct(RdpInputSender),
    Rpc(Arc<Daemon>),
}

/// The database reflects accepted input only. Pending operations retain ordering;
/// adjacent pointer motion can be replaced by the newest position.
pub(crate) struct InputQueue {
    database: Database,
    pending: VecDeque<Operation>,
    retry_at: Option<Instant>,
}

impl Default for InputQueue {
    fn default() -> Self {
        Self {
            database: Database::new(),
            pending: VecDeque::new(),
            retry_at: None,
        }
    }
}

impl InputQueue {
    // More than the 512 scan codes and five buttons that release_all may queue.
    const LIMIT: usize = 1024;
    const RETRY: Duration = Duration::from_millis(10);

    pub fn deadline(&self) -> Option<Instant> {
        self.retry_at
    }

    pub fn send(&mut self, target: &InputTarget, operations: impl IntoIterator<Item = Operation>) {
        for operation in operations {
            if matches!(operation, Operation::MouseMove(_))
                && let Some(last @ Operation::MouseMove(_)) = self.pending.back_mut()
            {
                *last = operation;
                continue;
            }
            if self.pending.len() == Self::LIMIT {
                // A stalled connection must not grow memory indefinitely. Abandon
                // unsent input and release every accepted hold before continuing.
                tracing::warn!("Session input queue saturated; releasing held inputs");
                self.queue_releases();
                break;
            }
            self.pending.push_back(operation);
        }
        self.flush(target);
    }

    fn queue_releases(&mut self) {
        self.pending.clear();
        self.pending.extend(
            self.database
                .keyboard_state()
                .iter_ones()
                .map(|index| Operation::KeyReleased(Scancode::from_u8(index >= 256, (index % 256) as u8))),
        );
        self.pending.extend(
            self.database
                .mouse_buttons_state()
                .iter_ones()
                .filter_map(MouseButton::from_idx)
                .map(Operation::MouseButtonReleased),
        );
    }

    pub fn release_all(&mut self, target: &InputTarget) {
        self.queue_releases();
        self.flush(target);
    }

    pub fn release_buttons(&mut self, target: &InputTarget) {
        self.pending.retain(|operation| {
            !matches!(
                operation,
                Operation::MouseMove(_)
                    | Operation::MouseButtonPressed(_)
                    | Operation::MouseButtonReleased(_)
                    | Operation::WheelRotations(_)
            )
        });
        let releases = self
            .database
            .mouse_buttons_state()
            .iter_ones()
            .filter_map(MouseButton::from_idx)
            .map(Operation::MouseButtonReleased)
            .collect::<Vec<_>>();
        // Button releases precede any queued keys and do not grow the bounded queue.
        if self.pending.len() + releases.len() > Self::LIMIT {
            self.queue_releases();
        } else {
            for release in releases.into_iter().rev() {
                self.pending.push_front(release);
            }
        }
        self.flush(target);
    }

    pub fn flush(&mut self, target: &InputTarget) {
        self.retry_at = None;
        while let Some(operation) = self.pending.front() {
            match target {
                InputTarget::Direct(sender) => {
                    let permit = match sender.try_reserve() {
                        Ok(permit) => permit,
                        Err(tokio::sync::mpsc::error::TrySendError::Full(())) => {
                            self.retry_at = Some(Instant::now() + Self::RETRY);
                            return;
                        }
                        Err(tokio::sync::mpsc::error::TrySendError::Closed(())) => {
                            self.pending.clear();
                            self.database = Database::new();
                            return;
                        }
                    };
                    let events = self.database.apply([operation.clone()]);
                    if !events.is_empty() {
                        permit.send(RdpInputEvent::FastPath(events));
                    }
                }
                InputTarget::Rpc(daemon) => {
                    if !daemon.input_operations([operation.clone()]).is_ok() {
                        // The RPC API does not distinguish full and closed queues or
                        // expose a session identity. Preserve one-shot delivery rather
                        // than replaying old input into a later session.
                        self.pending.clear();
                        self.database = Database::new();
                        return;
                    }
                    // The daemon owns its own database; mirror accepted holds so
                    // local focus changes and overlays can release them as well.
                    let _ = self.database.apply([operation.clone()]);
                }
            }
            let _ = self.pending.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ironrdp::input::MousePosition;

    #[test]
    fn a_release_waits_for_channel_capacity_and_is_retried() {
        let (sender, mut receiver) = RdpInputSender::channel(1);
        let target = InputTarget::Direct(sender);
        let mut input = InputQueue::default();
        let key = Scancode::from_u8(false, 0x1D);
        input.send(&target, [Operation::KeyPressed(key)]);
        input.send(&target, [Operation::KeyReleased(key)]);
        assert!(input.database.keyboard_state().any());
        assert!(input.deadline().is_some());
        let _ = receiver.try_recv().expect("an accepted input event is queued");
        input.flush(&target);
        assert!(!input.database.keyboard_state().any());
        assert!(receiver.try_recv().is_ok(), "the release was delivered");
        assert!(input.pending.is_empty());
        assert!(input.deadline().is_none());
    }

    #[test]
    fn adjacent_motion_is_coalesced_without_reordering_button_edges() {
        let (sender, mut receiver) = RdpInputSender::channel(1);
        let target = InputTarget::Direct(sender);
        let mut input = InputQueue::default();
        input.send(&target, [Operation::MouseButtonPressed(MouseButton::Left)]);
        for x in 1..1000 {
            input.send(&target, [Operation::MouseMove(MousePosition { x, y: 1 })]);
        }
        input.send(&target, [Operation::MouseButtonReleased(MouseButton::Left)]);
        assert_eq!(input.pending.len(), 2);
        assert!(matches!(
            input.pending[0],
            Operation::MouseMove(MousePosition { x: 999, y: 1 })
        ));
        let _ = receiver.try_recv().expect("an accepted input event is queued");
        input.flush(&target);
        let _ = receiver.try_recv().expect("an accepted input event is queued");
        input.flush(&target);
        assert!(!input.database.mouse_buttons_state().any());
        assert!(receiver.try_recv().is_ok());
    }

    #[test]
    fn focus_or_modal_capture_releases_buttons_and_keys_even_when_full() {
        let (sender, mut receiver) = RdpInputSender::channel(1);
        let target = InputTarget::Direct(sender);
        let mut input = InputQueue::default();
        input.send(&target, [Operation::MouseButtonPressed(MouseButton::Left)]);
        let _ = receiver.try_recv().expect("an accepted input event is queued");
        input.send(&target, [Operation::KeyPressed(Scancode::from_u8(false, 0x1D))]);
        input.release_all(&target);
        while !input.pending.is_empty() {
            let _ = receiver.try_recv().expect("an accepted input event is queued");
            input.flush(&target);
        }
        assert!(!input.database.keyboard_state().any());
        assert!(!input.database.mouse_buttons_state().any());
    }

    #[test]
    fn saturation_is_bounded_and_preserves_releases_for_accepted_holds() {
        let (sender, mut receiver) = RdpInputSender::channel(1);
        let target = InputTarget::Direct(sender);
        let mut input = InputQueue::default();
        let key = Scancode::from_u8(false, 0x1D);
        input.send(&target, [Operation::KeyPressed(key)]);
        for _ in 0..=InputQueue::LIMIT {
            input.send(&target, [Operation::KeyPressed(key)]);
        }
        assert_eq!(input.pending.len(), 1);
        assert!(matches!(input.pending[0], Operation::KeyReleased(_)));
        let _ = receiver.try_recv().expect("an accepted input event is queued");
        input.flush(&target);
        assert!(!input.database.keyboard_state().any());
        assert!(receiver.try_recv().is_ok());
    }

    #[test]
    fn a_closed_channel_discards_pending_input_and_local_holds() {
        let (sender, receiver) = RdpInputSender::channel(1);
        let target = InputTarget::Direct(sender);
        let mut input = InputQueue::default();
        input.send(&target, [Operation::KeyPressed(Scancode::from_u8(false, 0x1D))]);
        drop(receiver);
        input.release_all(&target);
        assert!(input.pending.is_empty());
        assert!(input.deadline().is_none());
        assert!(!input.database.keyboard_state().any());
    }

    #[test]
    fn an_unavailable_rpc_session_does_not_retry_or_replay_input() {
        let daemon = Daemon::with_overlay(ironrdp_propertyset::PropertySet::new());
        let target = InputTarget::Rpc(Arc::new(daemon));
        let mut input = InputQueue::default();
        input.send(&target, [Operation::KeyPressed(Scancode::from_u8(false, 0x1D))]);
        assert!(input.pending.is_empty());
        assert!(input.deadline().is_none());
        assert!(!input.database.keyboard_state().any());
    }
}
