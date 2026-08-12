use ms_protocol::{LogicalKey, Modifiers, MouseButton};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("failed to install input capture hook: {0}")]
    HookInstall(String),
    #[error("required OS permission not granted: {0}")]
    PermissionDenied(String),
}

/// Receives raw local input events from a platform `InputCapture`
/// implementation. Implemented by `ms-core-service`'s event loop, which
/// feeds each callback into `EdgeStateMachine`. Split out as a trait
/// (rather than a channel) so the hot path — every mouse-move sample —
/// never allocates or crosses a channel just to reach the state machine.
pub trait CaptureSink: Send + Sync {
    fn on_mouse_move(&self, dx: f64, dy: f64);
    fn on_mouse_button(&self, button: MouseButton, pressed: bool);
    fn on_mouse_wheel(&self, delta_x: f64, delta_y: f64, high_resolution: bool);
    fn on_key_event(&self, key: LogicalKey, pressed: bool, modifiers: Modifiers);
    /// Called by the platform layer when it has determined (via its own
    /// monitor-geometry APIs) that the local cursor's physical position is
    /// touching `edge` at normalized `position` along it. The platform
    /// layer owns *detecting* this (it requires per-OS screen-geometry
    /// APIs); `EdgeStateMachine` owns *deciding* what it means.
    fn on_cursor_at_edge(&self, edge: ms_protocol::ScreenEdge, position: f64);
}

/// Platform-specific low-level input capture. On Windows this is backed by
/// `SetWindowsHookEx(WH_MOUSE_LL/WH_KEYBOARD_LL)`; on macOS by a
/// `CGEventTap`. Both require capturing at a level below normal
/// application input so events can be observed (and, once a remote
/// session is active, suppressed from reaching any local window) even
/// when Mouse Share itself doesn't have focus.
pub trait InputCapture: Send {
    fn start(&mut self, sink: std::sync::Arc<dyn CaptureSink>) -> Result<(), CaptureError>;
    fn stop(&mut self) -> Result<(), CaptureError>;

    /// Controls whether captured events continue on to the local OS
    /// (`true`, the default/idle state) or are consumed by Mouse Share
    /// instead (`false`, set while this device is controlling a peer so
    /// local apps don't *also* react to the same physical mouse/keyboard).
    fn set_pass_through(&mut self, pass_through: bool) -> Result<(), CaptureError>;
}
