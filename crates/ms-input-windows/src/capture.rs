use crate::{hook_state, raw_input::RawInputWindow, screen};
use ms_input_core::{CaptureError, CaptureSink, InputCapture};
use ms_protocol::MouseButton;
use std::sync::mpsc;
use std::sync::Arc;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, LLKHF_EXTENDED,
    MSG, MSLLHOOKSTRUCT, WHEEL_DELTA, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_APP, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_XBUTTONDOWN,
    WM_XBUTTONUP,
};

/// Custom thread message used to ask the hook thread's message loop to
/// exit cleanly (`PostThreadMessageW` + `GetMessageW` is the standard way
/// to signal a thread blocked in a Win32 message loop).
const WM_APP_QUIT: u32 = WM_APP + 1;

pub struct WindowsInputCapture {
    hook_thread: Option<std::thread::JoinHandle<()>>,
    hook_thread_id: Option<u32>,
}

impl Default for WindowsInputCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsInputCapture {
    pub fn new() -> Self {
        Self { hook_thread: None, hook_thread_id: None }
    }
}

impl InputCapture for WindowsInputCapture {
    fn start(&mut self, sink: Arc<dyn CaptureSink>) -> Result<(), CaptureError> {
        hook_state::set_sink(Some(sink));

        let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, String>>();
        let handle = std::thread::Builder::new()
            .name("mouse-share-input-hook".into())
            .spawn(move || run_hook_thread(ready_tx))
            .map_err(|e| CaptureError::HookInstall(e.to_string()))?;

        let thread_id = ready_rx
            .recv()
            .map_err(|e| CaptureError::HookInstall(format!("hook thread died before starting: {e}")))?
            .map_err(CaptureError::HookInstall)?;

        self.hook_thread = Some(handle);
        self.hook_thread_id = Some(thread_id);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        if let Some(thread_id) = self.hook_thread_id.take() {
            unsafe {
                let _ = PostThreadMessageW(thread_id, WM_APP_QUIT, WPARAM(0), LPARAM(0));
            }
        }
        if let Some(handle) = self.hook_thread.take() {
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

fn run_hook_thread(ready_tx: mpsc::Sender<Result<u32, String>>) {
    // SAFETY: hooks and the raw input window must be created and torn
    // down on the same thread that runs their message loop; Win32
    // dispatches hook callbacks and window messages to the thread that
    // registered them.
    unsafe {
        let thread_id = windows::Win32::System::Threading::GetCurrentThreadId();

        let raw_input_window = match RawInputWindow::create() {
            Ok(w) => w,
            Err(e) => {
                let _ = ready_tx.send(Err(format!("failed to create raw input window: {e}")));
                return;
            }
        };
        let _ = raw_input_window; // kept alive for the duration of this thread

        let mouse_hook = match SetWindowsHookExW(WH_MOUSE_LL, Some(low_level_mouse_proc), None, 0) {
            Ok(h) => h,
            Err(e) => {
                let _ = ready_tx.send(Err(format!(
                    "SetWindowsHookExW(WH_MOUSE_LL) failed: {e} (Input Monitoring / accessibility-equivalent permission may be required depending on Windows version and security software)"
                )));
                return;
            }
        };
        let keyboard_hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), None, 0) {
            Ok(h) => h,
            Err(e) => {
                let _ = UnhookWindowsHookEx(mouse_hook);
                let _ = ready_tx.send(Err(format!("SetWindowsHookExW(WH_KEYBOARD_LL) failed: {e}")));
                return;
            }
        };

        let _ = ready_tx.send(Ok(thread_id));

        let mut msg = MSG::default();
        loop {
            let result = GetMessageW(&mut msg, None, 0, 0);
            if result.0 <= 0 {
                break; // WM_QUIT or an error; either way, stop pumping.
            }
            if msg.message == WM_APP_QUIT {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = UnhookWindowsHookEx(mouse_hook);
        let _ = UnhookWindowsHookEx(keyboard_hook);
    }
}

unsafe extern "system" fn low_level_mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let data = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        let (x, y) = (data.pt.x, data.pt.y);

        match wparam.0 as u32 {
            WM_MOUSEMOVE => {
                let bounds = screen::virtual_screen_bounds();
                if let Some((edge, position)) = screen::detect_edge(x, y, bounds) {
                    hook_state::with_sink(|sink| sink.on_cursor_at_edge(edge, position));
                }
                // Relative deltas themselves come from Raw Input
                // (raw_input.rs), not from this absolute `pt` — see the
                // crate-level doc comment for why.
            }
            WM_LBUTTONDOWN => hook_state::with_sink(|s| s.on_mouse_button(MouseButton::Left, true)),
            WM_LBUTTONUP => hook_state::with_sink(|s| s.on_mouse_button(MouseButton::Left, false)),
            WM_RBUTTONDOWN => hook_state::with_sink(|s| s.on_mouse_button(MouseButton::Right, true)),
            WM_RBUTTONUP => hook_state::with_sink(|s| s.on_mouse_button(MouseButton::Right, false)),
            WM_MBUTTONDOWN => hook_state::with_sink(|s| s.on_mouse_button(MouseButton::Middle, true)),
            WM_MBUTTONUP => hook_state::with_sink(|s| s.on_mouse_button(MouseButton::Middle, false)),
            WM_XBUTTONDOWN | WM_XBUTTONUP => {
                let pressed = wparam.0 as u32 == WM_XBUTTONDOWN;
                // High word of mouseData distinguishes XBUTTON1/XBUTTON2.
                let x_button = (data.mouseData >> 16) & 0xFFFF;
                let button = if x_button == 1 { MouseButton::Back } else { MouseButton::Forward };
                hook_state::with_sink(|s| s.on_mouse_button(button, pressed));
            }
            WM_MOUSEWHEEL => {
                let delta = ((data.mouseData >> 16) as i16) as f64 / WHEEL_DELTA as f64;
                hook_state::with_sink(|s| s.on_mouse_wheel(0.0, delta, false));
            }
            WM_MOUSEHWHEEL => {
                let delta = ((data.mouseData >> 16) as i16) as f64 / WHEEL_DELTA as f64;
                hook_state::with_sink(|s| s.on_mouse_wheel(delta, 0.0, false));
            }
            _ => {}
        }
    }

    if code >= 0 && !hook_state::pass_through() {
        // Non-zero return value from a WH_MOUSE_LL hook prevents the event
        // from reaching the rest of the system — this is what stops the
        // physical mouse from *also* driving local windows while it's
        // controlling a peer.
        return LRESULT(1);
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn low_level_keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let data = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let extended = (data.flags.0 & LLKHF_EXTENDED.0) != 0;
        let pressed = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        let logical = ms_keymap::windows::vk_to_logical(data.vkCode as u16, extended);
        let modifiers = current_modifiers();
        hook_state::with_sink(|s| s.on_key_event(logical, pressed, modifiers));
    }

    if code >= 0 && !hook_state::pass_through() {
        return LRESULT(1);
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// Snapshots held-modifier state via `GetAsyncKeyState` at the moment of a
/// key event. Reading modifier state this way (rather than tracking every
/// individual modifier key-up/down ourselves) matches what the receiving
/// OS's own input pipeline does and avoids the state drifting out of sync
/// if a modifier-up event is ever missed.
fn current_modifiers() -> ms_protocol::Modifiers {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        VK_CAPITAL, VK_CONTROL, VK_LWIN, VK_RWIN, VK_SHIFT,
    };
    unsafe fn is_down(vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY) -> bool {
        (GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000) != 0
    }
    unsafe {
        ms_protocol::Modifiers {
            shift: is_down(VK_SHIFT),
            control: is_down(VK_CONTROL),
            alt: is_down(VK_MENU),
            meta: is_down(VK_LWIN) || is_down(VK_RWIN),
            caps_lock: (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(VK_CAPITAL.0 as i32) & 1) != 0,
        }
    }
}
