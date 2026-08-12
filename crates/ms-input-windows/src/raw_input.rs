use crate::hook_state;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::{
    GetRawInputData, RegisterRawInputDevices, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER,
    RID_INPUT, RIDEV_INPUTSINK, RIM_TYPEMOUSE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassW, CW_USEDEFAULT, HWND_MESSAGE, WINDOW_EX_STYLE,
    WM_INPUT, WNDCLASSW, WS_OVERLAPPED,
};

const HID_USAGE_PAGE_GENERIC: u16 = 0x01;
const HID_USAGE_GENERIC_MOUSE: u16 = 0x02;

/// A hidden, message-only window that exists solely to receive `WM_INPUT`
/// notifications after `RegisterRawInputDevices`. Raw Input delivery
/// requires a real window handle to target; message-only windows
/// (`HWND_MESSAGE` parent) are the documented way to receive them without
/// creating any visible UI, at
/// `learn.microsoft.com/windows/win32/winmsg/window-features#message-only-windows`.
/// Kept alive only for its `Drop`-free lifetime scoping (the window and
/// its raw input registration must outlive the hook thread's message
/// loop); nothing currently needs to read the handle back out.
pub struct RawInputWindow {
    _hwnd: HWND,
}

impl RawInputWindow {
    pub fn create() -> windows::core::Result<Self> {
        let class_name: PCWSTR = w!("MouseShareRawInputWindow");
        unsafe {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wnd_proc),
                lpszClassName: class_name,
                ..Default::default()
            };
            RegisterClassW(&wc);

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class_name,
                PCWSTR::null(),
                WS_OVERLAPPED,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                HWND_MESSAGE,
                None,
                None,
                None,
            )?;

            let device = RAWINPUTDEVICE {
                usUsagePage: HID_USAGE_PAGE_GENERIC,
                usUsage: HID_USAGE_GENERIC_MOUSE,
                dwFlags: RIDEV_INPUTSINK,
                hwndTarget: hwnd,
            };
            RegisterRawInputDevices(&[device], std::mem::size_of::<RAWINPUTDEVICE>() as u32)?;

            Ok(Self { _hwnd: hwnd })
        }
    }
}

/// Window procedure for the hidden Raw Input window. Only handles
/// `WM_INPUT`; everything else goes to `DefWindowProcW`. Mouse button and
/// wheel events are intentionally *not* read from Raw Input here — those
/// keep coming from the `WH_MOUSE_LL` hook (`capture.rs`), which already
/// decodes them unambiguously via the documented `MSLLHOOKSTRUCT.mouseData`
/// field; only relative movement deltas need Raw Input's unclamped values.
unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_INPUT {
        handle_raw_input(lparam);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe fn handle_raw_input(lparam: LPARAM) {
    let mut size: u32 = 0;
    GetRawInputData(
        HRAWINPUT(lparam.0 as *mut _),
        RID_INPUT,
        None,
        &mut size,
        std::mem::size_of::<RAWINPUTHEADER>() as u32,
    );
    if size == 0 {
        return;
    }

    let mut buffer = vec![0u8; size as usize];
    let read = GetRawInputData(
        HRAWINPUT(lparam.0 as *mut _),
        RID_INPUT,
        Some(buffer.as_mut_ptr() as *mut _),
        &mut size,
        std::mem::size_of::<RAWINPUTHEADER>() as u32,
    );
    if read != size {
        return;
    }

    let raw = &*(buffer.as_ptr() as *const RAWINPUT);
    if raw.header.dwType != RIM_TYPEMOUSE.0 {
        return;
    }

    let mouse = raw.data.mouse;
    // MOUSE_MOVE_RELATIVE == 0; absolute-mode raw input (some virtualized
    // / remote-desktop input paths report absolute) is out of scope here
    // since we're only pulling this feed while actively forwarding to a
    // peer, at which point the local physical mouse is a normal relative
    // HID device.
    if mouse.usFlags.0 & 0x01 == 0 {
        let dx = mouse.lLastX as f64;
        let dy = mouse.lLastY as f64;
        if dx != 0.0 || dy != 0.0 {
            hook_state::with_sink(|sink| sink.on_mouse_move(dx, dy));
        }
    }
}
