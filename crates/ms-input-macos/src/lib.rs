//! macOS input capture and injection.
//!
//! Like `ms-input-windows`, this whole crate compiles to nothing on
//! non-macOS targets (see `#![cfg(...)]` below and the target-specific
//! dependency table in `Cargo.toml`), so the rest of the workspace keeps
//! building anywhere. Validate with:
//! `cargo check -p ms-input-macos --target x86_64-apple-darwin`
//! (framework linking only happens at final link time, so `cargo check`
//! succeeds without an actual macOS SDK present; this repo's CI runs the
//! full `cargo test` on `macos-latest` for the real thing).
//!
//! ## APIs used, and why
//!
//! - **Capture + suppression**: `CGEventTapCreate` (via the `core-graphics`
//!   crate's safe `CGEventTap` wrapper) at `kCGHIDEventTap` — the tap
//!   location closest to the hardware, before window-server-level
//!   transforms — with `CGEventTapOptions::Default` so the callback can
//!   return `CallbackResult::Drop` to swallow an event, which is what
//!   stops the physical mouse/keyboard from also driving local apps once
//!   this device is controlling a peer.
//! - **Relative movement without edge-clamping**: unlike Windows'
//!   `WH_MOUSE_LL` (see `ms-input-windows`'s doc comment for why that
//!   needed a separate Raw Input path), a `CGEventTap`'s `MOUSE_EVENT_DELTA_X`/
//!   `_Y` fields already report the *unclamped* HID delta even when
//!   `location()` is pinned at a screen edge — so on macOS the tap alone
//!   is sufficient; no analogue of Raw Input is needed.
//! - **Injection**: `CGEventCreateMouseEvent` / `CGEventCreateKeyboardEvent`
//!   / `CGEventPost` at `kCGHIDEventTap`.
//! - **Permissions**: creating an event tap that can intercept (not just
//!   passively listen to) system-wide input requires **Accessibility**
//!   access (`AXIsProcessTrusted`); reading keyboard content system-wide
//!   additionally requires **Input Monitoring** access (`IOHIDCheckAccess`,
//!   added in macOS 10.15). Both are user-granted via System
//!   Settings -> Privacy & Security, and cannot be granted
//!   programmatically — `permissions.rs` provides the checks
//!   `ms-daemon`'s onboarding flow polls to guide the user there.
#![cfg(target_os = "macos")]

mod capture;
mod hook_state;
mod inject;
mod permissions;
mod screen;

pub use capture::MacOsInputCapture;
pub use inject::MacOsInputInjector;
pub use permissions::{
    accessibility_trusted, input_monitoring_status, request_accessibility_access,
    InputMonitoringStatus,
};
pub use screen::VirtualScreenBounds;
