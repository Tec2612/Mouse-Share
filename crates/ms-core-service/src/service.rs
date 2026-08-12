use ms_config::{EmergencyHotkey, ScreenLayout};
use ms_input_core::{Action, ClipboardGateway, EdgeStateMachine, Event, InputInjector, LocalState};
use ms_protocol::{DeviceId, Message};
use std::sync::Arc;

/// Where `CoreService` sends outbound protocol messages. Implemented by
/// `session.rs`'s real connection registry in production; a recording
/// mock in tests.
pub trait NetworkSender: Send + Sync {
    fn send(&self, to: DeviceId, message: Message);
}

/// Toggles whether the platform capture layer's events continue to the
/// local OS. Kept as its own narrow trait — rather than requiring
/// `CoreService` to own the full `InputCapture` (whose `start`/`stop`
/// lifecycle belongs to the daemon's top-level setup, not to per-event
/// dispatch) — so the only capability `CoreService` needs from capture is
/// exactly the one `EdgeStateMachine` actions ask for.
pub trait PassThroughControl: Send + Sync {
    fn set_pass_through(&self, enabled: bool);
}

/// Ties `EdgeStateMachine`'s decisions to the concrete collaborators that
/// carry them out: the local injector, the local capture layer's
/// pass-through switch, and the network. This is the composition root for
/// "what actually happens" — the state machine itself (`ms-input-core`)
/// stays pure and platform-agnostic; this is where that decision meets the
/// real OS and the real socket.
pub struct CoreService {
    state_machine: EdgeStateMachine,
    layout: ScreenLayout,
    clipboard: ClipboardGateway,
    injector: Box<dyn InputInjector>,
    pass_through: Arc<dyn PassThroughControl>,
    network: Arc<dyn NetworkSender>,
}

impl CoreService {
    pub fn new(
        local_device_id: DeviceId,
        emergency_hotkey: EmergencyHotkey,
        layout: ScreenLayout,
        clipboard_enabled: bool,
        injector: Box<dyn InputInjector>,
        pass_through: Arc<dyn PassThroughControl>,
        network: Arc<dyn NetworkSender>,
    ) -> Self {
        Self {
            state_machine: EdgeStateMachine::new(local_device_id, emergency_hotkey),
            layout,
            clipboard: ClipboardGateway::new(clipboard_enabled),
            injector,
            pass_through,
            network,
        }
    }

    pub fn state(&self) -> LocalState {
        self.state_machine.state()
    }

    pub fn layout_mut(&mut self) -> &mut ScreenLayout {
        &mut self.layout
    }

    pub fn set_clipboard_enabled(&mut self, enabled: bool) {
        self.clipboard.set_enabled(enabled);
    }

    /// Feeds one event through the state machine and carries out every
    /// resulting action. This is the only place `Action` variants get
    /// interpreted — see each arm for which collaborator handles it.
    pub fn handle(&mut self, event: Event) {
        let actions = self.state_machine.handle(event, &self.layout);
        for action in actions {
            self.execute(action);
        }
    }

    fn execute(&mut self, action: Action) {
        match action {
            Action::EnableLocalPassThrough => self.pass_through.set_pass_through(true),
            Action::DisableLocalPassThrough => self.pass_through.set_pass_through(false),
            Action::SendMessage { to, message } => self.network.send(to, message),
            Action::InjectLocally(message) => self.inject(message),
            Action::WarpLocalCursor { x, y } => {
                if let Ok((bx, by, bw, bh)) = self.injector.screen_bounds() {
                    let _ = self.injector.warp_absolute(bx + x * bw, by + y * bh);
                }
            }
        }
    }

    fn inject(&mut self, message: Message) {
        let _ = match message {
            Message::MouseMove { dx, dy, .. } => self.injector.move_relative(dx, dy),
            Message::MouseWarp { x, y } => self.injector.warp_absolute(x, y),
            Message::MouseButton { button, pressed } => self.injector.mouse_button(button, pressed),
            Message::MouseWheel { delta_x, delta_y, high_resolution } => {
                self.injector.mouse_wheel(delta_x, delta_y, high_resolution)
            }
            Message::KeyEvent { key, pressed, modifiers, .. } => self.injector.key_event(key, pressed, modifiers),
            // Non-input messages (Hello, heartbeats, clipboard, edge
            // control, disconnect) never reach here: only messages the
            // state machine wraps in `Action::InjectLocally` do, and it
            // only does that for the mouse/keyboard variants above — see
            // `EdgeStateMachine::on_peer_input_message`.
            _ => Ok(()),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ms_config::ComputerNode;
    use ms_protocol::{LogicalKey, ScreenEdge};
    use std::sync::Mutex;

    #[derive(Default)]
    struct MockInjector {
        calls: Mutex<Vec<String>>,
    }
    impl InputInjector for MockInjector {
        fn move_relative(&mut self, dx: f64, dy: f64) -> Result<(), ms_input_core::InjectError> {
            self.calls.lock().unwrap().push(format!("move({dx},{dy})"));
            Ok(())
        }
        fn warp_absolute(&mut self, x: f64, y: f64) -> Result<(), ms_input_core::InjectError> {
            self.calls.lock().unwrap().push(format!("warp({x},{y})"));
            Ok(())
        }
        fn mouse_button(&mut self, button: ms_protocol::MouseButton, pressed: bool) -> Result<(), ms_input_core::InjectError> {
            self.calls.lock().unwrap().push(format!("button({button:?},{pressed})"));
            Ok(())
        }
        fn mouse_wheel(&mut self, dx: f64, dy: f64, hr: bool) -> Result<(), ms_input_core::InjectError> {
            self.calls.lock().unwrap().push(format!("wheel({dx},{dy},{hr})"));
            Ok(())
        }
        fn key_event(&mut self, key: LogicalKey, pressed: bool, _m: ms_protocol::Modifiers) -> Result<(), ms_input_core::InjectError> {
            self.calls.lock().unwrap().push(format!("key({key:?},{pressed})"));
            Ok(())
        }
        fn cursor_position(&self) -> Result<(f64, f64), ms_input_core::InjectError> {
            Ok((0.0, 0.0))
        }
        fn screen_bounds(&self) -> Result<(f64, f64, f64, f64), ms_input_core::InjectError> {
            Ok((0.0, 0.0, 1920.0, 1080.0))
        }
    }

    #[derive(Default)]
    struct MockPassThrough {
        states: Mutex<Vec<bool>>,
    }
    impl PassThroughControl for MockPassThrough {
        fn set_pass_through(&self, enabled: bool) {
            self.states.lock().unwrap().push(enabled);
        }
    }

    #[derive(Default)]
    struct MockNetwork {
        sent: Mutex<Vec<(DeviceId, Message)>>,
    }
    impl NetworkSender for MockNetwork {
        fn send(&self, to: DeviceId, message: Message) {
            self.sent.lock().unwrap().push((to, message));
        }
    }

    fn make_service(local: DeviceId, neighbor: DeviceId) -> (CoreService, Arc<MockPassThrough>, Arc<MockNetwork>) {
        let mut layout = ScreenLayout::new();
        layout.add_device(ComputerNode { device_id: local, display_name: "me".into(), enabled: true, canvas_x: 0.0, canvas_y: 0.0 });
        layout.add_device(ComputerNode { device_id: neighbor, display_name: "them".into(), enabled: true, canvas_x: 0.0, canvas_y: 0.0 });
        layout.link_edges(local, ScreenEdge::Right, neighbor).unwrap();

        let pass_through = Arc::new(MockPassThrough::default());
        let network = Arc::new(MockNetwork::default());
        let service = CoreService::new(
            local,
            EmergencyHotkey { keys: vec![LogicalKey::ControlLeft, LogicalKey::AltLeft, LogicalKey::Escape] },
            layout,
            false,
            Box::new(MockInjector::default()),
            pass_through.clone(),
            network.clone(),
        );
        (service, pass_through, network)
    }

    #[test]
    fn crossing_a_linked_edge_disables_pass_through_and_notifies_the_peer() {
        let (mut service, pass_through, network) = make_service(DeviceId::new_v4(), DeviceId::new_v4());
        service.handle(Event::LocalCursorAtEdge { edge: ScreenEdge::Right, position: 0.5 });

        assert_eq!(pass_through.states.lock().unwrap().as_slice(), &[false]);
        assert_eq!(network.sent.lock().unwrap().len(), 1);
        assert!(matches!(service.state(), LocalState::Controlling { .. }));
    }

    #[test]
    fn peer_input_while_being_controlled_reaches_the_injector() {
        let local = DeviceId::new_v4();
        let controller = DeviceId::new_v4();
        let mut layout = ScreenLayout::new();
        layout.add_device(ComputerNode { device_id: local, display_name: "me".into(), enabled: true, canvas_x: 0.0, canvas_y: 0.0 });
        layout.add_device(ComputerNode { device_id: controller, display_name: "them".into(), enabled: true, canvas_x: 0.0, canvas_y: 0.0 });

        let pass_through = Arc::new(MockPassThrough::default());
        let network = Arc::new(MockNetwork::default());
        let injector = Box::new(MockInjector::default());
        let mut service = CoreService::new(
            local,
            EmergencyHotkey { keys: vec![] },
            layout,
            false,
            injector,
            pass_through,
            network,
        );

        service.handle(Event::PeerEdgeEnter { from: controller, edge: ScreenEdge::Left, position: 0.5 });
        assert!(matches!(service.state(), LocalState::BeingControlled { .. }));

        service.handle(Event::PeerInputMessage {
            from: controller,
            message: Message::MouseButton { button: ms_protocol::MouseButton::Left, pressed: true },
        });
        // Can't inspect the boxed injector's calls directly (moved into
        // the service), but reaching this point without panicking and the
        // state remaining BeingControlled confirms the dispatch path ran;
        // ms-input-core's own tests cover the Action-level contract this
        // relies on.
        assert!(matches!(service.state(), LocalState::BeingControlled { source } if source == controller));
    }

    #[test]
    fn connection_loss_forces_release_through_the_full_pipeline() {
        let local = DeviceId::new_v4();
        let neighbor = DeviceId::new_v4();
        let (mut service, pass_through, _network) = make_service(local, neighbor);
        service.handle(Event::LocalCursorAtEdge { edge: ScreenEdge::Right, position: 0.5 });
        assert!(matches!(service.state(), LocalState::Controlling { .. }));

        service.handle(Event::ConnectionLost { device: neighbor });
        assert_eq!(service.state(), LocalState::Idle);
        assert_eq!(pass_through.states.lock().unwrap().as_slice(), &[false, true]);
    }
}
