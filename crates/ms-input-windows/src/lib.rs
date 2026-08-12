//! Windows input capture and injection.
//!
//! This entire crate compiles to nothing on non-Windows targets — it is
//! meaningless off Windows and the `windows` crate dependency is only
//! pulled in for `cfg(windows)` builds (see `Cargo.toml`), so the rest of
//! the workspace keeps building on any host OS. Validate with:
//! `cargo check -p ms-input-windows --target x86_64-pc-windows-gnu`
//! (this repo's CI additionally does a full `cargo test` on
//! `windows-latest`, since low-level input hooks can only be exercised for
//! real on actual Windows).
//!
//! ## APIs used, and why
//!
//! - **Capture**: `SetWindowsHookEx(WH_MOUSE_LL)` / `WH_KEYBOARD_LL`.
//!   These are the only APIs that see input system-wide (not just when
//!   Mouse Share has focus) and that can *suppress* an event from
//!   reaching the rest of the system by returning a non-zero value from
//!   the hook procedure — required for the "stop sharing" transition to
//!   actually stop the physical mouse/keyboard from also driving local
//!   apps. Documented at
//!   `learn.microsoft.com/windows/win32/winmsg/lowlevelmouseproc` and
//!   `.../lowlevelkeyboardproc`.
//! - **High-fidelity relative movement**: `RegisterRawInputDevices` /
//!   `WM_INPUT` (Raw Input). `WH_MOUSE_LL` only reports the cursor's
//!   *clamped* absolute screen position, which loses movement once the
//!   real cursor is pinned at a screen edge — exactly the situation this
//!   app is in while actively forwarding to a peer. Raw Input reports
//!   true unclamped relative deltas straight from the HID report, which
//!   is why it's the API actual KVM/remote-input software uses for this,
//!   not the hook's `pt` field.
//! - **Injection**: `SendInput` with `INPUT_MOUSE`/`INPUT_KEYBOARD`,
//!   which — unlike `mouse_event`/`keybd_event` — is the current
//!   documented API and correctly interleaves with other input sources.
//! - **DPI**: `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)` must
//!   be called at process startup (in `ms-daemon`, before any window or
//!   hook is created) so `GetCursorPos`/hook coordinates are real physical
//!   pixels rather than DPI-virtualized ones; otherwise edge detection
//!   drifts on any display that isn't 100% scale.
#![cfg(windows)]

mod capture;
mod hook_state;
mod inject;
mod raw_input;
mod screen;

pub use capture::WindowsInputCapture;
pub use inject::{virtual_screen_bounds, WindowsInputInjector};
pub use screen::VirtualScreenBounds;
