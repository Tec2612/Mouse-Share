use crate::screen;
use ms_input_core::{InjectError, InputInjector};
use ms_protocol::{LogicalKey, Modifiers, MouseButton};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN,
    MOUSEEVENTF_XUP, MOUSEINPUT, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetCursorPos, WHEEL_DELTA, XBUTTON1, XBUTTON2};

#[derive(Default)]
pub struct WindowsInputInjector;

impl WindowsInputInjector {
    pub fn new() -> Self {
        Self
    }

    fn send(&self, input: INPUT) -> Result<(), InjectError> {
        // SAFETY: SendInput's contract is just "a valid array of INPUT
        // structs of the given length"; we always pass exactly one.
        let sent = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
        if sent != 1 {
            return Err(InjectError::Synthesis(
                "SendInput reported 0 events accepted (often means another process has a lower-level hook blocking synthetic input, or the desktop is a secure desktop such as a UAC prompt)".into(),
            ));
        }
        Ok(())
    }
}

impl InputInjector for WindowsInputInjector {
    fn move_relative(&mut self, dx: f64, dy: f64) -> Result<(), InjectError> {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: dx.round() as i32,
                    dy: dy.round() as i32,
                    mouseData: 0,
                    dwFlags: MOUSEEVENTF_MOVE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        self.send(input)
    }

    fn warp_absolute(&mut self, x: f64, y: f64) -> Result<(), InjectError> {
        // SetCursorPos (rather than SendInput's absolute mode, which
        // requires normalizing into 0..65535 virtual-desktop coordinates)
        // is the simpler, equally-documented API for the one-time "place
        // the cursor here" used exactly once per hand-off, at
        // `learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setcursorpos`.
        unsafe { SetCursorPos(x.round() as i32, y.round() as i32) }
            .map_err(|e| InjectError::Synthesis(e.to_string()))
    }

    fn mouse_button(&mut self, button: MouseButton, pressed: bool) -> Result<(), InjectError> {
        let (flags, mouse_data) = match (button, pressed) {
            (MouseButton::Left, true) => (MOUSEEVENTF_LEFTDOWN, 0),
            (MouseButton::Left, false) => (MOUSEEVENTF_LEFTUP, 0),
            (MouseButton::Right, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
            (MouseButton::Right, false) => (MOUSEEVENTF_RIGHTUP, 0),
            (MouseButton::Middle, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
            (MouseButton::Middle, false) => (MOUSEEVENTF_MIDDLEUP, 0),
            (MouseButton::Back, true) => (MOUSEEVENTF_XDOWN, XBUTTON1 as u32),
            (MouseButton::Back, false) => (MOUSEEVENTF_XUP, XBUTTON1 as u32),
            (MouseButton::Forward, true) => (MOUSEEVENTF_XDOWN, XBUTTON2 as u32),
            (MouseButton::Forward, false) => (MOUSEEVENTF_XUP, XBUTTON2 as u32),
            (MouseButton::Other(_), _) => {
                return Err(InjectError::Synthesis("unsupported mouse button on Windows".into()))
            }
        };
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: mouse_data, dwFlags: flags, time: 0, dwExtraInfo: 0 },
            },
        };
        self.send(input)
    }

    fn mouse_wheel(&mut self, delta_x: f64, delta_y: f64, _high_resolution: bool) -> Result<(), InjectError> {
        // Windows doesn't distinguish "high-resolution" wheel input at the
        // SendInput layer the way some HID reports do at capture time; we
        // scale into WHEEL_DELTA units either way, which is what
        // MOUSEEVENTF_WHEEL/HWHEEL expect regardless of the source
        // device's native resolution.
        if delta_y != 0.0 {
            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: (delta_y * WHEEL_DELTA as f64) as i32 as u32,
                        dwFlags: MOUSEEVENTF_WHEEL,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            self.send(input)?;
        }
        if delta_x != 0.0 {
            let input = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx: 0,
                        dy: 0,
                        mouseData: (delta_x * WHEEL_DELTA as f64) as i32 as u32,
                        dwFlags: MOUSEEVENTF_HWHEEL,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            self.send(input)?;
        }
        Ok(())
    }

    fn key_event(&mut self, key: LogicalKey, pressed: bool, _modifiers: Modifiers) -> Result<(), InjectError> {
        // Modifiers accompanying a non-modifier key (e.g. Ctrl+C) are not
        // separately injected here: the individual modifier KeyEvents
        // themselves (ControlLeft down, then C down) already arrive over
        // the wire and get injected as their own key_event calls, exactly
        // matching how physical keyboards report combinations. The
        // `modifiers` field exists for the receiver's own bookkeeping
        // (e.g. `ModifierPolicy` remapping upstream in `ms-keymap`), not
        // as a second source of truth to re-inject from.
        let Some((vk, extended)) = ms_keymap::windows::logical_to_vk(key) else {
            return Err(InjectError::Synthesis(format!("no Windows VK mapping for {key:?}")));
        };
        let mut flags = if pressed { Default::default() } else { KEYEVENTF_KEYUP };
        if extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 },
            },
        };
        self.send(input)
    }

    fn cursor_position(&self) -> Result<(f64, f64), InjectError> {
        let mut point = POINT::default();
        unsafe { GetCursorPos(&mut point) }.map_err(|e| InjectError::Synthesis(e.to_string()))?;
        Ok((point.x as f64, point.y as f64))
    }

    fn screen_bounds(&self) -> Result<(f64, f64, f64, f64), InjectError> {
        let b = screen::virtual_screen_bounds();
        Ok((b.x as f64, b.y as f64, b.width as f64, b.height as f64))
    }
}

/// Exposed for `ms-core-service`, which needs the local virtual-screen
/// bounds (not just the current position) to resolve edge hand-offs.
pub fn virtual_screen_bounds() -> screen::VirtualScreenBounds {
    screen::virtual_screen_bounds()
}
