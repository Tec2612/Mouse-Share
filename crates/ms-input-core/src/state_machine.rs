use ms_config::{EmergencyHotkey, ScreenLayout};
use ms_protocol::{DeviceId, LogicalKey, Message, Modifiers, MouseButton, ScreenEdge};
use std::collections::HashSet;

/// Which role this device is currently playing with respect to its own
/// physical mouse/keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalState {
    /// Normal: this device's own input goes to this device.
    Idle,
    /// This device's captured input is being forwarded to `target`
    /// instead of reaching local applications.
    Controlling { target: DeviceId },
    /// This device is injecting input received from `source`.
    BeingControlled { source: DeviceId },
}

/// Something that happened, fed into the state machine by
/// `ms-core-service`'s event loop — either from local capture, from the
/// network, or from connection-lifecycle bookkeeping. Kept free of any
/// I/O so the whole transition table is testable without mocking an OS or
/// a socket.
#[derive(Debug, Clone)]
pub enum Event {
    /// Local capture determined the cursor is touching `edge` at
    /// `position` (0.0..1.0 along it). Only actionable while `Idle`.
    LocalCursorAtEdge { edge: ScreenEdge, position: f64 },
    LocalMouseMove { dx: f64, dy: f64 },
    LocalMouseButton { button: MouseButton, pressed: bool },
    LocalMouseWheel { delta_x: f64, delta_y: f64, high_resolution: bool },
    /// `held` is the full set of currently-held keys, checked against the
    /// configured emergency chord on every key event regardless of
    /// current state — the escape hatch must work no matter what.
    LocalKeyEvent { key: LogicalKey, pressed: bool, modifiers: Modifiers, held: HashSet<LogicalKey> },

    /// A peer is handing control of the shared mouse/keyboard to us.
    PeerEdgeEnter { from: DeviceId, edge: ScreenEdge, position: f64 },
    /// The peer we are controlling has released control back to us
    /// because its own cursor reached its configured return edge.
    PeerEdgeRelease { from: DeviceId },
    /// The peer we just sent `EdgeEnter` to refused the hand-off because
    /// it was already busy. Only actionable while `Controlling { target:
    /// from }` — that's the one state this event can arrive in given how
    /// it's produced, but a stale/duplicate delivery is ignored rather
    /// than assumed impossible.
    PeerEdgeEnterRejected { from: DeviceId },
    /// Any protocol message arriving from whichever device is currently
    /// relevant to our state (the one we're controlling, or the one
    /// controlling us) that should simply be injected/forwarded rather
    /// than interpreted as a state transition.
    PeerInputMessage { from: DeviceId, message: Message },

    /// The connection to `device` dropped. Safety-critical: if we were
    /// controlling or being controlled by this device, control must
    /// return to local immediately rather than leave the user unable to
    /// use their own mouse/keyboard.
    ConnectionLost { device: DeviceId },
}

/// What the caller (`ms-core-service`) should actually do in response to
/// an `Event`. The state machine only decides; it performs no I/O itself.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    EnableLocalPassThrough,
    DisableLocalPassThrough,
    SendMessage { to: DeviceId, message: Message },
    InjectLocally(Message),
    WarpLocalCursor { x: f64, y: f64 },
}

pub struct EdgeStateMachine {
    state: LocalState,
    emergency_hotkey: EmergencyHotkey,
    local_device_id: DeviceId,
}

impl EdgeStateMachine {
    pub fn new(local_device_id: DeviceId, emergency_hotkey: EmergencyHotkey) -> Self {
        Self { state: LocalState::Idle, emergency_hotkey, local_device_id }
    }

    pub fn state(&self) -> LocalState {
        self.state
    }

    pub fn handle(&mut self, event: Event, layout: &ScreenLayout) -> Vec<Action> {
        match event {
            Event::LocalCursorAtEdge { edge, position } => self.on_local_cursor_at_edge(edge, position, layout),
            Event::LocalMouseMove { dx, dy } => self.forward_if_controlling(|| Message::MouseMove { dx, dy, seq: 0 }),
            Event::LocalMouseButton { button, pressed } => {
                self.forward_if_controlling(|| Message::MouseButton { button, pressed })
            }
            Event::LocalMouseWheel { delta_x, delta_y, high_resolution } => {
                self.forward_if_controlling(|| Message::MouseWheel { delta_x, delta_y, high_resolution })
            }
            Event::LocalKeyEvent { key, pressed, modifiers, held } => {
                self.on_local_key_event(key, pressed, modifiers, held)
            }
            Event::PeerEdgeEnter { from, edge, position } => self.on_peer_edge_enter(from, edge, position),
            Event::PeerEdgeRelease { from } => self.on_peer_edge_release(from),
            Event::PeerEdgeEnterRejected { from } => self.on_peer_edge_enter_rejected(from),
            Event::PeerInputMessage { from, message } => self.on_peer_input_message(from, message),
            Event::ConnectionLost { device } => self.on_connection_lost(device),
        }
    }

    fn forward_if_controlling(&self, make_message: impl FnOnce() -> Message) -> Vec<Action> {
        match self.state {
            LocalState::Controlling { target } => vec![Action::SendMessage { to: target, message: make_message() }],
            _ => vec![],
        }
    }

    fn on_local_cursor_at_edge(&mut self, edge: ScreenEdge, position: f64, layout: &ScreenLayout) -> Vec<Action> {
        // Only Idle -> Controlling transitions originate from local edge
        // detection. While already Controlling, our own OS cursor is
        // pinned (pass-through is off) so this shouldn't fire; while
        // BeingControlled, it is the *injected* cursor crossing an edge,
        // which the platform layer reports through the same event since
        // ms-input-core doesn't distinguish the cursor's cause — that
        // case is handled below via the BeingControlled arm.
        match self.state {
            LocalState::Idle => {
                let Some(link) = layout.resolve(self.local_device_id, edge, position as f32) else {
                    return vec![];
                };
                self.state = LocalState::Controlling { target: link.to };
                vec![
                    Action::DisableLocalPassThrough,
                    Action::SendMessage {
                        to: link.to,
                        message: Message::EdgeEnter { edge: link.to_edge, position, sender_scale: 1.0 },
                    },
                ]
            }
            LocalState::BeingControlled { source } => {
                let Some(link) = layout.resolve(self.local_device_id, edge, position as f32) else { return vec![] };
                if link.to != source {
                    // The injected cursor reached an edge that doesn't
                    // lead back to whoever is controlling us; ignore it
                    // rather than release to the wrong place.
                    return vec![];
                }
                self.state = LocalState::Idle;
                vec![Action::SendMessage { to: source, message: Message::EdgeRelease }]
            }
            LocalState::Controlling { .. } => vec![],
        }
    }

    fn on_local_key_event(
        &mut self,
        key: LogicalKey,
        pressed: bool,
        modifiers: Modifiers,
        held: HashSet<LogicalKey>,
    ) -> Vec<Action> {
        if pressed && self.emergency_hotkey.keys.iter().all(|k| held.contains(k)) {
            return self.force_release();
        }
        self.forward_if_controlling(|| Message::KeyEvent { key, pressed, modifiers, seq: 0 })
    }

    fn on_peer_edge_enter(&mut self, from: DeviceId, edge: ScreenEdge, position: f64) -> Vec<Action> {
        if self.state != LocalState::Idle {
            // Already busy (already controlling someone, or already being
            // controlled by someone else); refuse the hand-off rather than
            // silently overwrite state mid-session. Tell the sender so it
            // doesn't stay stuck `Controlling` with local input disabled,
            // waiting for a release that will never come — this is what
            // happens if both sides cross their linked edge at nearly the
            // same instant and each ends up refusing the other's
            // `EdgeEnter` here.
            return vec![Action::SendMessage { to: from, message: Message::EdgeEnterRejected }];
        }
        self.state = LocalState::BeingControlled { source: from };
        vec![
            Action::WarpLocalCursor { x: edge_entry_x(edge, position), y: edge_entry_y(edge, position) },
        ]
    }

    fn on_peer_edge_enter_rejected(&mut self, from: DeviceId) -> Vec<Action> {
        match self.state {
            LocalState::Controlling { target } if target == from => {
                self.state = LocalState::Idle;
                vec![Action::EnableLocalPassThrough]
            }
            _ => vec![], // stale/unexpected rejection; ignore
        }
    }

    fn on_peer_edge_release(&mut self, from: DeviceId) -> Vec<Action> {
        match self.state {
            LocalState::Controlling { target } if target == from => {
                self.state = LocalState::Idle;
                vec![Action::EnableLocalPassThrough]
            }
            _ => vec![], // stale/unexpected release; ignore
        }
    }

    fn on_peer_input_message(&mut self, from: DeviceId, message: Message) -> Vec<Action> {
        match self.state {
            LocalState::BeingControlled { source } if source == from => vec![Action::InjectLocally(message)],
            _ => vec![], // input from a device that isn't currently controlling us
        }
    }

    fn on_connection_lost(&mut self, device: DeviceId) -> Vec<Action> {
        match self.state {
            LocalState::Controlling { target } if target == device => {
                self.state = LocalState::Idle;
                vec![Action::EnableLocalPassThrough]
            }
            LocalState::BeingControlled { source } if source == device => {
                self.state = LocalState::Idle;
                vec![] // nothing to send; the peer is the one that's gone
            }
            _ => vec![],
        }
    }

    /// Forces an immediate return to local control regardless of network
    /// state — the emergency hotkey and connection loss both route through
    /// this so there is exactly one code path that can never leave the
    /// user without their own mouse/keyboard.
    fn force_release(&mut self) -> Vec<Action> {
        match self.state {
            LocalState::Controlling { target } => {
                self.state = LocalState::Idle;
                vec![
                    Action::EnableLocalPassThrough,
                    Action::SendMessage { to: target, message: Message::EdgeRelease },
                ]
            }
            LocalState::BeingControlled { .. } => {
                self.state = LocalState::Idle;
                vec![]
            }
            LocalState::Idle => vec![],
        }
    }
}

fn edge_entry_x(edge: ScreenEdge, position: f64) -> f64 {
    match edge {
        ScreenEdge::Left => 0.0,
        ScreenEdge::Right => 1.0,
        ScreenEdge::Top | ScreenEdge::Bottom => position,
    }
}

fn edge_entry_y(edge: ScreenEdge, position: f64) -> f64 {
    match edge {
        ScreenEdge::Top => 0.0,
        ScreenEdge::Bottom => 1.0,
        ScreenEdge::Left | ScreenEdge::Right => position,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ms_config::ComputerNode;

    fn node(id: DeviceId) -> ComputerNode {
        ComputerNode { device_id: id, display_name: "test".into(), enabled: true, canvas_x: 0.0, canvas_y: 0.0 }
    }

    fn linked_layout(a: DeviceId, b: DeviceId) -> ScreenLayout {
        let mut layout = ScreenLayout::new();
        layout.add_device(node(a));
        layout.add_device(node(b));
        layout.link_edges(a, ScreenEdge::Right, b).unwrap();
        layout
    }

    fn no_hotkey() -> EmergencyHotkey {
        EmergencyHotkey { keys: vec![LogicalKey::ControlLeft, LogicalKey::AltLeft, LogicalKey::Escape] }
    }

    #[test]
    fn reaching_a_linked_edge_starts_controlling_the_neighbor() {
        let (me, neighbor) = (DeviceId::new_v4(), DeviceId::new_v4());
        let layout = linked_layout(me, neighbor);
        let mut sm = EdgeStateMachine::new(me, no_hotkey());

        let actions = sm.handle(Event::LocalCursorAtEdge { edge: ScreenEdge::Right, position: 0.5 }, &layout);

        assert_eq!(sm.state(), LocalState::Controlling { target: neighbor });
        assert!(actions.contains(&Action::DisableLocalPassThrough));
        assert!(actions.iter().any(|a| matches!(a, Action::SendMessage { to, message: Message::EdgeEnter { .. } } if *to == neighbor)));
    }

    #[test]
    fn reaching_an_unlinked_edge_does_nothing() {
        let (me, neighbor) = (DeviceId::new_v4(), DeviceId::new_v4());
        let layout = linked_layout(me, neighbor);
        let mut sm = EdgeStateMachine::new(me, no_hotkey());

        let actions = sm.handle(Event::LocalCursorAtEdge { edge: ScreenEdge::Left, position: 0.5 }, &layout);

        assert_eq!(sm.state(), LocalState::Idle);
        assert!(actions.is_empty());
    }

    #[test]
    fn while_controlling_raw_mouse_and_key_events_are_forwarded_to_the_target() {
        let (me, neighbor) = (DeviceId::new_v4(), DeviceId::new_v4());
        let layout = linked_layout(me, neighbor);
        let mut sm = EdgeStateMachine::new(me, no_hotkey());
        sm.handle(Event::LocalCursorAtEdge { edge: ScreenEdge::Right, position: 0.5 }, &layout);

        let actions = sm.handle(Event::LocalMouseMove { dx: 4.0, dy: -2.0 }, &layout);
        assert_eq!(actions, vec![Action::SendMessage { to: neighbor, message: Message::MouseMove { dx: 4.0, dy: -2.0, seq: 0 } }]);

        let actions = sm.handle(
            Event::LocalKeyEvent { key: LogicalKey::A, pressed: true, modifiers: Modifiers::default(), held: HashSet::new() },
            &layout,
        );
        assert_eq!(
            actions,
            vec![Action::SendMessage { to: neighbor, message: Message::KeyEvent { key: LogicalKey::A, pressed: true, modifiers: Modifiers::default(), seq: 0 } }]
        );
    }

    #[test]
    fn while_idle_raw_events_are_never_forwarded_anywhere() {
        let (me, neighbor) = (DeviceId::new_v4(), DeviceId::new_v4());
        let layout = linked_layout(me, neighbor);
        let mut sm = EdgeStateMachine::new(me, no_hotkey());

        let actions = sm.handle(Event::LocalMouseMove { dx: 1.0, dy: 1.0 }, &layout);
        assert!(actions.is_empty());
    }

    #[test]
    fn peer_edge_release_returns_control_and_reenables_pass_through() {
        let (me, neighbor) = (DeviceId::new_v4(), DeviceId::new_v4());
        let layout = linked_layout(me, neighbor);
        let mut sm = EdgeStateMachine::new(me, no_hotkey());
        sm.handle(Event::LocalCursorAtEdge { edge: ScreenEdge::Right, position: 0.5 }, &layout);
        assert_eq!(sm.state(), LocalState::Controlling { target: neighbor });

        let actions = sm.handle(Event::PeerEdgeRelease { from: neighbor }, &layout);
        assert_eq!(sm.state(), LocalState::Idle);
        assert_eq!(actions, vec![Action::EnableLocalPassThrough]);
    }

    #[test]
    fn edge_release_from_the_wrong_device_is_ignored() {
        let (me, neighbor, stranger) = (DeviceId::new_v4(), DeviceId::new_v4(), DeviceId::new_v4());
        let layout = linked_layout(me, neighbor);
        let mut sm = EdgeStateMachine::new(me, no_hotkey());
        sm.handle(Event::LocalCursorAtEdge { edge: ScreenEdge::Right, position: 0.5 }, &layout);

        let actions = sm.handle(Event::PeerEdgeRelease { from: stranger }, &layout);
        assert_eq!(sm.state(), LocalState::Controlling { target: neighbor }, "an unrelated device cannot release a session it doesn't own");
        assert!(actions.is_empty());
    }

    #[test]
    fn peer_edge_enter_starts_being_controlled_and_warps_cursor_to_the_entry_point() {
        let (me, controller) = (DeviceId::new_v4(), DeviceId::new_v4());
        let layout = ScreenLayout::new();
        let mut sm = EdgeStateMachine::new(me, no_hotkey());

        let actions = sm.handle(Event::PeerEdgeEnter { from: controller, edge: ScreenEdge::Left, position: 0.75 }, &layout);

        assert_eq!(sm.state(), LocalState::BeingControlled { source: controller });
        assert_eq!(actions, vec![Action::WarpLocalCursor { x: 0.0, y: 0.75 }]);
    }

    #[test]
    fn peer_input_messages_are_injected_only_while_being_controlled_by_that_peer() {
        let (me, controller, stranger) = (DeviceId::new_v4(), DeviceId::new_v4(), DeviceId::new_v4());
        let layout = ScreenLayout::new();
        let mut sm = EdgeStateMachine::new(me, no_hotkey());
        sm.handle(Event::PeerEdgeEnter { from: controller, edge: ScreenEdge::Left, position: 0.5 }, &layout);

        let msg = Message::MouseButton { button: MouseButton::Left, pressed: true };
        let actions = sm.handle(Event::PeerInputMessage { from: controller, message: msg.clone() }, &layout);
        assert_eq!(actions, vec![Action::InjectLocally(msg.clone())]);

        let actions = sm.handle(Event::PeerInputMessage { from: stranger, message: msg }, &layout);
        assert!(actions.is_empty(), "input from a device that isn't controlling us must never be injected");
    }

    #[test]
    fn injected_cursor_reaching_the_return_edge_releases_back_to_the_controller() {
        let (me, controller) = (DeviceId::new_v4(), DeviceId::new_v4());
        let layout = linked_layout(controller, me); // controller's Right links to me's Left
        let mut sm = EdgeStateMachine::new(me, no_hotkey());
        sm.handle(Event::PeerEdgeEnter { from: controller, edge: ScreenEdge::Left, position: 0.5 }, &layout);

        let actions = sm.handle(Event::LocalCursorAtEdge { edge: ScreenEdge::Left, position: 0.5 }, &layout);
        assert_eq!(sm.state(), LocalState::Idle);
        assert_eq!(actions, vec![Action::SendMessage { to: controller, message: Message::EdgeRelease }]);
    }

    #[test]
    fn connection_lost_while_controlling_forces_local_release() {
        let (me, neighbor) = (DeviceId::new_v4(), DeviceId::new_v4());
        let layout = linked_layout(me, neighbor);
        let mut sm = EdgeStateMachine::new(me, no_hotkey());
        sm.handle(Event::LocalCursorAtEdge { edge: ScreenEdge::Right, position: 0.5 }, &layout);

        let actions = sm.handle(Event::ConnectionLost { device: neighbor }, &layout);
        assert_eq!(sm.state(), LocalState::Idle, "losing the connection must never leave the user without local input");
        assert_eq!(actions, vec![Action::EnableLocalPassThrough]);
    }

    #[test]
    fn connection_lost_while_being_controlled_returns_to_idle() {
        let (me, controller) = (DeviceId::new_v4(), DeviceId::new_v4());
        let layout = ScreenLayout::new();
        let mut sm = EdgeStateMachine::new(me, no_hotkey());
        sm.handle(Event::PeerEdgeEnter { from: controller, edge: ScreenEdge::Left, position: 0.5 }, &layout);

        sm.handle(Event::ConnectionLost { device: controller }, &layout);
        assert_eq!(sm.state(), LocalState::Idle);
    }

    #[test]
    fn emergency_hotkey_forces_release_even_mid_session() {
        let (me, neighbor) = (DeviceId::new_v4(), DeviceId::new_v4());
        let layout = linked_layout(me, neighbor);
        let hotkey = EmergencyHotkey { keys: vec![LogicalKey::ControlLeft, LogicalKey::AltLeft, LogicalKey::Escape] };
        let mut sm = EdgeStateMachine::new(me, hotkey);
        sm.handle(Event::LocalCursorAtEdge { edge: ScreenEdge::Right, position: 0.5 }, &layout);
        assert_eq!(sm.state(), LocalState::Controlling { target: neighbor });

        let held: HashSet<_> = [LogicalKey::ControlLeft, LogicalKey::AltLeft, LogicalKey::Escape].into_iter().collect();
        let actions = sm.handle(
            Event::LocalKeyEvent { key: LogicalKey::Escape, pressed: true, modifiers: Modifiers::default(), held },
            &layout,
        );

        assert_eq!(sm.state(), LocalState::Idle);
        assert!(actions.contains(&Action::EnableLocalPassThrough));
        assert!(actions.iter().any(|a| matches!(a, Action::SendMessage { to, message: Message::EdgeRelease } if *to == neighbor)));
    }

    #[test]
    fn emergency_hotkey_partial_chord_does_not_trigger_release() {
        let (me, neighbor) = (DeviceId::new_v4(), DeviceId::new_v4());
        let layout = linked_layout(me, neighbor);
        let hotkey = EmergencyHotkey { keys: vec![LogicalKey::ControlLeft, LogicalKey::AltLeft, LogicalKey::Escape] };
        let mut sm = EdgeStateMachine::new(me, hotkey);
        sm.handle(Event::LocalCursorAtEdge { edge: ScreenEdge::Right, position: 0.5 }, &layout);

        let held: HashSet<_> = [LogicalKey::ControlLeft].into_iter().collect();
        sm.handle(
            Event::LocalKeyEvent { key: LogicalKey::ControlLeft, pressed: true, modifiers: Modifiers::default(), held },
            &layout,
        );

        assert_eq!(sm.state(), LocalState::Controlling { target: neighbor }, "a partial chord must not release control");
    }

    #[test]
    fn a_second_peer_cannot_hijack_control_while_already_being_controlled() {
        let (me, controller, second) = (DeviceId::new_v4(), DeviceId::new_v4(), DeviceId::new_v4());
        let layout = ScreenLayout::new();
        let mut sm = EdgeStateMachine::new(me, no_hotkey());
        sm.handle(Event::PeerEdgeEnter { from: controller, edge: ScreenEdge::Left, position: 0.5 }, &layout);

        let actions = sm.handle(Event::PeerEdgeEnter { from: second, edge: ScreenEdge::Right, position: 0.5 }, &layout);

        assert_eq!(sm.state(), LocalState::BeingControlled { source: controller }, "the original controller must not be silently displaced");
        assert_eq!(
            actions,
            vec![Action::SendMessage { to: second, message: Message::EdgeEnterRejected }],
            "the refused peer must be told, or it's stuck Controlling with its own input disabled forever"
        );
    }

    #[test]
    fn simultaneous_edge_crossings_reject_each_other_and_both_recover_to_idle() {
        // Both devices' physical cursors cross their shared linked edge at
        // nearly the same instant: each independently goes Idle ->
        // Controlling (disabling local pass-through) and sends EdgeEnter to
        // the other *before* either has received the other's EdgeEnter.
        // When those EdgeEnters do arrive, both sides are already busy and
        // each refuses the other's hand-off. Without EdgeEnterRejected
        // wiring back into a recovery transition, this is a real deadlock:
        // both machines end up Controlling forever with local input
        // disabled and no local mouse ever entering BeingControlled to
        // receive the forwarded input either place — matching the reported
        // "both mice disappear, only Ctrl+Alt+Del recovers" symptom.
        let (a, b) = (DeviceId::new_v4(), DeviceId::new_v4());
        let layout = linked_layout(a, b);
        let mut sm_a = EdgeStateMachine::new(a, no_hotkey());
        let mut sm_b = EdgeStateMachine::new(b, no_hotkey());

        sm_a.handle(Event::LocalCursorAtEdge { edge: ScreenEdge::Right, position: 0.5 }, &layout);
        sm_b.handle(Event::LocalCursorAtEdge { edge: ScreenEdge::Left, position: 0.5 }, &layout);
        assert_eq!(sm_a.state(), LocalState::Controlling { target: b });
        assert_eq!(sm_b.state(), LocalState::Controlling { target: a });

        // Each side's EdgeEnter now arrives at the other, which is already
        // Controlling and refuses it.
        let rejected_by_b = sm_b.handle(Event::PeerEdgeEnter { from: a, edge: ScreenEdge::Left, position: 0.5 }, &layout);
        let rejected_by_a = sm_a.handle(Event::PeerEdgeEnter { from: b, edge: ScreenEdge::Right, position: 0.5 }, &layout);
        assert_eq!(rejected_by_b, vec![Action::SendMessage { to: a, message: Message::EdgeEnterRejected }]);
        assert_eq!(rejected_by_a, vec![Action::SendMessage { to: b, message: Message::EdgeEnterRejected }]);

        // Each rejection reaches the side that's still Controlling and
        // waiting; both must recover to Idle with pass-through restored.
        let recovery_a = sm_a.handle(Event::PeerEdgeEnterRejected { from: b }, &layout);
        let recovery_b = sm_b.handle(Event::PeerEdgeEnterRejected { from: a }, &layout);

        assert_eq!(sm_a.state(), LocalState::Idle);
        assert_eq!(sm_b.state(), LocalState::Idle);
        assert_eq!(recovery_a, vec![Action::EnableLocalPassThrough]);
        assert_eq!(recovery_b, vec![Action::EnableLocalPassThrough]);
    }
}
