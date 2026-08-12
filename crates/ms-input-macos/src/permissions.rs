//! Accessibility and Input Monitoring permission checks.
//!
//! Neither `core-graphics` nor `core-foundation` wrap these — they're
//! ApplicationServices/IOKit APIs with no safe Rust binding in this
//! ecosystem as of writing — so this module declares the C functions
//! directly against their documented signatures:
//! - `AXIsProcessTrusted` / `AXIsProcessTrustedWithOptions`:
//!   `developer.apple.com/documentation/applicationservices/1462075-axisprocesstrusted`
//! - `IOHIDCheckAccess`: `developer.apple.com/documentation/iokit/1652685-iohidcheckaccess`
//!
//! Neither permission can be granted programmatically — the OS always
//! requires an explicit user action in System Settings. These functions
//! exist so `ms-daemon`'s onboarding flow can detect the current state and
//! deep-link/guide the user there, and so it can poll for the grant taking
//! effect (macOS does not notify processes when TCC permissions change).
use core_foundation::base::TCFType;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use std::os::raw::c_void;

#[allow(non_snake_case)]
#[cfg_attr(target_os = "macos", link(name = "ApplicationServices", kind = "framework"))]
extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef) -> u8;
    static kAXTrustedCheckOptionPrompt: core_foundation::string::CFStringRef;
}

/// Whether this process currently has Accessibility access, required to
/// create an *active* `CGEventTap` (one that can suppress/modify events,
/// not just observe them) and to post synthetic input via `CGEventPost`.
pub fn accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

/// Triggers the system's native "MouseShare would like to control this
/// computer" Accessibility prompt if not already trusted (a no-op,
/// returning the current state, if already granted). Apple's API for this
/// intentionally only prompts once per app per install; after a user
/// denies it, only the System Settings pane itself can re-grant it, which
/// is why the onboarding flow also needs a direct deep link (see
/// `ms-daemon`'s onboarding screen).
pub fn request_accessibility_access() -> bool {
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let value = core_foundation::boolean::CFBoolean::true_value();
        let options: CFDictionary<CFString, core_foundation::boolean::CFBoolean> =
            CFDictionary::from_CFType_pairs(&[(key, value)]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef() as *const _ as *mut c_void as _) != 0
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IOHIDRequestType {
    ListenEvent = 1,
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMonitoringStatus {
    Granted = 0,
    Denied = 1,
    Unknown = 2,
}

impl From<i32> for InputMonitoringStatus {
    fn from(value: i32) -> Self {
        match value {
            0 => InputMonitoringStatus::Granted,
            1 => InputMonitoringStatus::Denied,
            _ => InputMonitoringStatus::Unknown,
        }
    }
}

#[cfg_attr(target_os = "macos", link(name = "IOKit", kind = "framework"))]
extern "C" {
    fn IOHIDCheckAccess(request_type: i32) -> i32;
}

/// Whether this process has Input Monitoring access, required (since
/// macOS 10.15) to receive keyboard event *content* — not just timing —
/// through a system-wide `CGEventTap`. `Unknown` means the user has never
/// been asked, which macOS treats differently from an explicit denial in
/// its own Privacy & Security UI, so the onboarding flow should surface it
/// as "grant access" rather than "re-enable access".
pub fn input_monitoring_status() -> InputMonitoringStatus {
    unsafe { IOHIDCheckAccess(IOHIDRequestType::ListenEvent as i32).into() }
}
