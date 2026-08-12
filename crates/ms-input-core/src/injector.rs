use ms_protocol::{LogicalKey, Modifiers, MouseButton};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InjectError {
    #[error("failed to synthesize input: {0}")]
    Synthesis(String),
    #[error("required OS permission not granted: {0}")]
    PermissionDenied(String),
}

/// Platform-specific synthetic input injection: Win32 `SendInput` on
/// Windows, `CGEventPost` on macOS. Everything here operates in terms of
/// `LogicalKey`/`MouseButton` from `ms-protocol` — cross-platform key
/// translation already happened in `ms-keymap` before a `Message` reaches
/// this trait, so implementations only need to know their own platform's
/// codes.
pub trait InputInjector: Send {
    fn move_relative(&mut self, dx: f64, dy: f64) -> Result<(), InjectError>;
    /// Used exactly once when a remote session begins, to place the local
    /// cursor at the correct point along the shared edge.
    fn warp_absolute(&mut self, x: f64, y: f64) -> Result<(), InjectError>;
    fn mouse_button(&mut self, button: MouseButton, pressed: bool) -> Result<(), InjectError>;
    fn mouse_wheel(&mut self, delta_x: f64, delta_y: f64, high_resolution: bool) -> Result<(), InjectError>;
    fn key_event(&mut self, key: LogicalKey, pressed: bool, modifiers: Modifiers) -> Result<(), InjectError>;

    /// Returns the current cursor position and the local virtual screen's
    /// bounds, so the caller can detect the cursor reaching a configured
    /// return edge after a burst of injected movement.
    fn cursor_position(&self) -> Result<(f64, f64), InjectError>;
}
