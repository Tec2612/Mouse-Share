//! macOS Accessibility/Input Monitoring onboarding commands. On any other
//! platform these report "granted" unconditionally since no such gate
//! exists there — see `docs/user-guide.md`'s onboarding section.

#[derive(serde::Serialize)]
pub struct PermissionStatus {
    pub accessibility_granted: bool,
    pub input_monitoring_granted: bool,
}

#[tauri::command]
pub fn check_permissions() -> PermissionStatus {
    #[cfg(target_os = "macos")]
    {
        PermissionStatus {
            accessibility_granted: ms_input_macos::accessibility_trusted(),
            input_monitoring_granted: matches!(
                ms_input_macos::input_monitoring_status(),
                ms_input_macos::InputMonitoringStatus::Granted
            ),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        PermissionStatus { accessibility_granted: true, input_monitoring_granted: true }
    }
}

/// Triggers the native system prompt (first run only — see
/// `ms_input_macos::request_accessibility_access`'s doc comment for why a
/// direct deep link to System Settings is also needed after a denial).
#[tauri::command]
pub fn request_accessibility() -> bool {
    #[cfg(target_os = "macos")]
    {
        ms_input_macos::request_accessibility_access()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}
