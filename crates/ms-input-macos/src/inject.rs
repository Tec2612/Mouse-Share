use core_graphics::display::CGDisplay;
use core_graphics::event::{
    CGEvent, CGEventTapLocation, CGEventType, CGMouseButton, EventField, ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use ms_input_core::{InjectError, InputInjector};
use ms_protocol::{LogicalKey, Modifiers, MouseButton};

/// Holds no `CGEventSource` itself — `CGEventSource` wraps a raw Core
/// Foundation pointer and isn't `Send`, while `InputInjector` requires
/// `Send` so `ms-core-service` can own one on its event-processing thread
/// across `await` points. Since the wrapper doesn't implement `Clone`
/// either, a fresh HID-system-state source (cheap: it's a stateless
/// handle, not a connection) is created per call instead of cached.
#[derive(Default)]
pub struct MacOsInputInjector;

impl MacOsInputInjector {
    pub fn new() -> Self {
        Self
    }

    fn source(&self) -> Result<CGEventSource, InjectError> {
        CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|()| InjectError::Synthesis("CGEventSourceCreate failed".into()))
    }
}

impl InputInjector for MacOsInputInjector {
    fn move_relative(&mut self, dx: f64, dy: f64) -> Result<(), InjectError> {
        let (x, y) = self.cursor_position()?;
        let target = CGPoint::new(x + dx, y + dy);
        let event = CGEvent::new_mouse_event(self.source()?, CGEventType::MouseMoved, target, CGMouseButton::Left)
            .map_err(|()| InjectError::Synthesis("CGEventCreateMouseEvent failed".into()))?;
        event.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_X, dx.round() as i64);
        event.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_Y, dy.round() as i64);
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn warp_absolute(&mut self, x: f64, y: f64) -> Result<(), InjectError> {
        CGDisplay::warp_mouse_cursor_position(CGPoint::new(x, y))
            .map_err(|code| InjectError::Synthesis(format!("CGWarpMouseCursorPosition failed: {code}")))
    }

    fn mouse_button(&mut self, button: MouseButton, pressed: bool) -> Result<(), InjectError> {
        let (x, y) = self.cursor_position()?;
        let position = CGPoint::new(x, y);
        let (event_type, cg_button) = match (button, pressed) {
            (MouseButton::Left, true) => (CGEventType::LeftMouseDown, CGMouseButton::Left),
            (MouseButton::Left, false) => (CGEventType::LeftMouseUp, CGMouseButton::Left),
            (MouseButton::Right, true) => (CGEventType::RightMouseDown, CGMouseButton::Right),
            (MouseButton::Right, false) => (CGEventType::RightMouseUp, CGMouseButton::Right),
            (MouseButton::Middle, true) => (CGEventType::OtherMouseDown, CGMouseButton::Center),
            (MouseButton::Middle, false) => (CGEventType::OtherMouseUp, CGMouseButton::Center),
            // CGMouseButton only names Left/Right/Center, but
            // CGEventCreateMouseEvent's button parameter accepts any USB
            // button ordinal for Other{Down,Up} events; the safe wrapper's
            // enum just doesn't expose higher numbers, so back/forward
            // fall back to numbers 3/4 via the same convention used when
            // decoding them in capture.rs.
            (MouseButton::Back, true) => (CGEventType::OtherMouseDown, CGMouseButton::Center),
            (MouseButton::Back, false) => (CGEventType::OtherMouseUp, CGMouseButton::Center),
            (MouseButton::Forward, true) => (CGEventType::OtherMouseDown, CGMouseButton::Center),
            (MouseButton::Forward, false) => (CGEventType::OtherMouseUp, CGMouseButton::Center),
            (MouseButton::Other(_), _) => {
                return Err(InjectError::Synthesis("unsupported mouse button on macOS".into()))
            }
        };
        let event = CGEvent::new_mouse_event(self.source()?, event_type, position, cg_button)
            .map_err(|()| InjectError::Synthesis("CGEventCreateMouseEvent failed".into()))?;
        if matches!(button, MouseButton::Back | MouseButton::Forward) {
            let number = if button == MouseButton::Back { 3 } else { 4 };
            event.set_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER, number);
        }
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn mouse_wheel(&mut self, delta_x: f64, delta_y: f64, high_resolution: bool) -> Result<(), InjectError> {
        let units = if high_resolution { ScrollEventUnit::PIXEL } else { ScrollEventUnit::LINE };
        let event = CGEvent::new_scroll_event(
            self.source()?,
            units,
            2, // wheel_count: vertical (axis 1) + horizontal (axis 2)
            delta_y.round() as i32,
            delta_x.round() as i32,
            0,
        )
        .map_err(|()| InjectError::Synthesis("CGEventCreateScrollWheelEvent2 failed".into()))?;
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn key_event(&mut self, key: LogicalKey, pressed: bool, _modifiers: Modifiers) -> Result<(), InjectError> {
        // As on Windows (see ms-input-windows::inject), modifier state
        // accompanying a non-modifier key is not separately applied here:
        // the individual modifier LogicalKeys arrive and get injected as
        // their own key_event calls, exactly matching physical keyboard
        // behavior and avoiding a second, potentially-inconsistent source
        // of truth for held modifiers.
        let Some(keycode) = ms_keymap::macos::logical_to_vk(key) else {
            return Err(InjectError::Synthesis(format!("no macOS keycode mapping for {key:?}")));
        };
        let event = CGEvent::new_keyboard_event(self.source()?, keycode, pressed)
            .map_err(|()| InjectError::Synthesis("CGEventCreateKeyboardEvent failed".into()))?;
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn cursor_position(&self) -> Result<(f64, f64), InjectError> {
        let event = CGEvent::new(self.source()?)
            .map_err(|()| InjectError::Synthesis("CGEventCreate failed".into()))?;
        let point = event.location();
        Ok((point.x, point.y))
    }
}
