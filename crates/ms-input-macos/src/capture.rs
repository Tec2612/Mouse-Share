use crate::{hook_state, screen};
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CallbackResult, EventField,
};
use ms_input_core::{CaptureError, CaptureSink, InputCapture};
use ms_protocol::{Modifiers, MouseButton};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};

fn run_loop_handle() -> &'static Mutex<Option<CFRunLoop>> {
    static HANDLE: OnceLock<Mutex<Option<CFRunLoop>>> = OnceLock::new();
    HANDLE.get_or_init(|| Mutex::new(None))
}

pub struct MacOsInputCapture {
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Default for MacOsInputCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl MacOsInputCapture {
    pub fn new() -> Self {
        Self { thread: None }
    }
}

impl InputCapture for MacOsInputCapture {
    fn start(&mut self, sink: Arc<dyn CaptureSink>) -> Result<(), CaptureError> {
        if !crate::permissions::accessibility_trusted() {
            return Err(CaptureError::PermissionDenied(
                "Accessibility access is required to capture input system-wide; grant it in System Settings -> Privacy & Security -> Accessibility".into(),
            ));
        }
        hook_state::set_sink(Some(sink));

        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let handle = std::thread::Builder::new()
            .name("mouse-share-input-tap".into())
            .spawn(move || run_capture_thread(ready_tx))
            .map_err(|e| CaptureError::HookInstall(e.to_string()))?;

        ready_rx
            .recv()
            .map_err(|e| CaptureError::HookInstall(format!("capture thread died before starting: {e}")))?
            .map_err(CaptureError::HookInstall)?;

        self.thread = Some(handle);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        if let Some(run_loop) = run_loop_handle().lock().expect("run loop mutex poisoned").take() {
            run_loop.stop();
        }
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
        hook_state::set_sink(None);
        Ok(())
    }

    fn set_pass_through(&mut self, pass_through: bool) -> Result<(), CaptureError> {
        hook_state::set_pass_through(pass_through);
        Ok(())
    }
}

fn run_capture_thread(ready_tx: mpsc::Sender<Result<(), String>>) {
    let events_of_interest = vec![
        CGEventType::MouseMoved,
        CGEventType::LeftMouseDown,
        CGEventType::LeftMouseUp,
        CGEventType::LeftMouseDragged,
        CGEventType::RightMouseDown,
        CGEventType::RightMouseUp,
        CGEventType::RightMouseDragged,
        CGEventType::OtherMouseDown,
        CGEventType::OtherMouseUp,
        CGEventType::OtherMouseDragged,
        CGEventType::ScrollWheel,
        CGEventType::KeyDown,
        CGEventType::KeyUp,
        CGEventType::FlagsChanged,
    ];

    let tap = CGEventTap::new(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        events_of_interest,
        move |_proxy, event_type, event| {
            handle_event(event_type, event);
            if hook_state::pass_through() {
                CallbackResult::Keep
            } else {
                CallbackResult::Drop
            }
        },
    );

    let tap = match tap {
        Ok(t) => t,
        Err(()) => {
            let _ = ready_tx.send(Err(
                "CGEventTapCreate failed; verify Accessibility and Input Monitoring access".into(),
            ));
            return;
        }
    };

    let Ok(source) = tap.mach_port().create_runloop_source(0) else {
        let _ = ready_tx.send(Err("failed to create a run loop source for the event tap".into()));
        return;
    };
    let run_loop = CFRunLoop::get_current();
    run_loop.add_source(&source, unsafe { kCFRunLoopCommonModes });
    tap.enable();

    *run_loop_handle().lock().expect("run loop mutex poisoned") = Some(run_loop);
    let _ = ready_tx.send(Ok(()));

    CFRunLoop::run_current();
    // Returns once `CFRunLoop::stop()` is called from `stop()` above; `tap`
    // is dropped here, which disables and invalidates the mach port.
}

fn handle_event(event_type: CGEventType, event: &core_graphics::event::CGEvent) {
    match event_type {
        CGEventType::MouseMoved | CGEventType::LeftMouseDragged | CGEventType::RightMouseDragged | CGEventType::OtherMouseDragged => {
            let dx = event.get_integer_value_field(EventField::MOUSE_EVENT_DELTA_X) as f64;
            let dy = event.get_integer_value_field(EventField::MOUSE_EVENT_DELTA_Y) as f64;
            if dx != 0.0 || dy != 0.0 {
                hook_state::with_sink(|sink| sink.on_mouse_move(dx, dy));
            }
            let location = event.location();
            let bounds = screen::virtual_screen_bounds();
            if let Some((edge, position)) = screen::detect_edge(location.x, location.y, bounds) {
                hook_state::with_sink(|sink| sink.on_cursor_at_edge(edge, position));
            }
        }
        CGEventType::LeftMouseDown => hook_state::with_sink(|s| s.on_mouse_button(MouseButton::Left, true)),
        CGEventType::LeftMouseUp => hook_state::with_sink(|s| s.on_mouse_button(MouseButton::Left, false)),
        CGEventType::RightMouseDown => hook_state::with_sink(|s| s.on_mouse_button(MouseButton::Right, true)),
        CGEventType::RightMouseUp => hook_state::with_sink(|s| s.on_mouse_button(MouseButton::Right, false)),
        CGEventType::OtherMouseDown | CGEventType::OtherMouseUp => {
            let pressed = matches!(event_type, CGEventType::OtherMouseDown);
            let button_number = event.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER);
            // Button 2 is the middle button by macOS convention; higher
            // numbers vary by vendor, so 3/4 are mapped to back/forward as
            // the most common third-party mouse convention.
            let button = match button_number {
                2 => MouseButton::Middle,
                3 => MouseButton::Back,
                4 => MouseButton::Forward,
                other => MouseButton::Other(other as u8),
            };
            hook_state::with_sink(|s| s.on_mouse_button(button, pressed));
        }
        CGEventType::ScrollWheel => {
            let is_continuous = event.get_integer_value_field(EventField::SCROLL_WHEEL_EVENT_IS_CONTINUOUS) != 0;
            let dy = event.get_integer_value_field(EventField::SCROLL_WHEEL_EVENT_DELTA_AXIS_1) as f64;
            let dx = event.get_integer_value_field(EventField::SCROLL_WHEEL_EVENT_DELTA_AXIS_2) as f64;
            if dx != 0.0 || dy != 0.0 {
                hook_state::with_sink(|s| s.on_mouse_wheel(dx, dy, is_continuous));
            }
        }
        CGEventType::KeyDown | CGEventType::KeyUp => {
            let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
            let logical = ms_keymap::macos::vk_to_logical(keycode);
            let pressed = matches!(event_type, CGEventType::KeyDown);
            let modifiers = modifiers_from_flags(event.get_flags());
            hook_state::with_sink(|s| s.on_key_event(logical, pressed, modifiers));
        }
        CGEventType::FlagsChanged => {
            let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
            let logical = ms_keymap::macos::vk_to_logical(keycode);
            let flags = event.get_flags();
            // FlagsChanged doesn't carry an explicit press/release bit like
            // KeyDown/KeyUp; the OS-documented technique is to infer it
            // from whether this modifier's aggregate flag bit is currently
            // set. This can't distinguish "still held" from "just pressed"
            // if two keys sharing a bit are involved (there is no separate
            // left/right bit for Shift/Control/Option in CGEventFlags),
            // which is a known platform limitation, not a bug here.
            let pressed = modifier_bit_for_keycode(keycode).is_some_and(|bit| flags.contains(bit));
            let modifiers = modifiers_from_flags(flags);
            hook_state::with_sink(|s| s.on_key_event(logical, pressed, modifiers));
        }
        _ => {}
    }
}

fn modifiers_from_flags(flags: core_graphics::event::CGEventFlags) -> Modifiers {
    use core_graphics::event::CGEventFlags;
    Modifiers {
        shift: flags.contains(CGEventFlags::CGEventFlagShift),
        control: flags.contains(CGEventFlags::CGEventFlagControl),
        alt: flags.contains(CGEventFlags::CGEventFlagAlternate),
        meta: flags.contains(CGEventFlags::CGEventFlagCommand),
        caps_lock: flags.contains(CGEventFlags::CGEventFlagAlphaShift),
    }
}

fn modifier_bit_for_keycode(keycode: u16) -> Option<core_graphics::event::CGEventFlags> {
    use core_graphics::event::{CGEventFlags, KeyCode};
    match keycode {
        KeyCode::SHIFT | KeyCode::RIGHT_SHIFT => Some(CGEventFlags::CGEventFlagShift),
        KeyCode::CONTROL | KeyCode::RIGHT_CONTROL => Some(CGEventFlags::CGEventFlagControl),
        KeyCode::OPTION | KeyCode::RIGHT_OPTION => Some(CGEventFlags::CGEventFlagAlternate),
        KeyCode::COMMAND | KeyCode::RIGHT_COMMAND => Some(CGEventFlags::CGEventFlagCommand),
        KeyCode::CAPS_LOCK => Some(CGEventFlags::CGEventFlagAlphaShift),
        _ => None,
    }
}
