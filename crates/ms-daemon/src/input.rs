use ms_core_service::PassThroughControl;
use ms_input_core::{CaptureSink, Event, InputCapture, InputInjector};
use ms_protocol::{LogicalKey, ScreenEdge};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;

/// Bridges platform capture callbacks (called synchronously from the
/// OS-owned hook/event-tap thread — see `ms-input-windows`/
/// `ms-input-macos`) into `ms_input_core::Event`s on the shared channel
/// `CoreService`'s owning task reads from. `UnboundedSender::send` is a
/// plain synchronous method, so no async bridging thread is needed here.
pub struct CaptureAdapter {
    tx: UnboundedSender<Event>,
    held: Mutex<HashSet<LogicalKey>>,
}

impl CaptureAdapter {
    pub fn new(tx: UnboundedSender<Event>) -> Arc<Self> {
        Arc::new(Self { tx, held: Mutex::new(HashSet::new()) })
    }
}

impl CaptureSink for CaptureAdapter {
    fn on_mouse_move(&self, dx: f64, dy: f64) {
        let _ = self.tx.send(Event::LocalMouseMove { dx, dy });
    }

    fn on_mouse_button(&self, button: ms_protocol::MouseButton, pressed: bool) {
        let _ = self.tx.send(Event::LocalMouseButton { button, pressed });
    }

    fn on_mouse_wheel(&self, delta_x: f64, delta_y: f64, high_resolution: bool) {
        let _ = self.tx.send(Event::LocalMouseWheel { delta_x, delta_y, high_resolution });
    }

    fn on_key_event(&self, key: LogicalKey, pressed: bool, modifiers: ms_protocol::Modifiers) {
        let held = {
            let mut held = self.held.lock().expect("poisoned");
            if pressed {
                held.insert(key);
            } else {
                held.remove(&key);
            }
            held.clone()
        };
        let _ = self.tx.send(Event::LocalKeyEvent { key, pressed, modifiers, held });
    }

    fn on_cursor_at_edge(&self, edge: ScreenEdge, position: f64) {
        let _ = self.tx.send(Event::LocalCursorAtEdge { edge, position });
    }
}

/// Adapts the platform `InputCapture` (which owns start/stop lifecycle,
/// managed once at daemon startup — see `main.rs`) to the narrower
/// `PassThroughControl` interface `CoreService` needs for per-event
/// dispatch.
pub struct PassThroughAdapter {
    capture: Mutex<Box<dyn InputCapture>>,
}

impl PassThroughAdapter {
    pub fn new(capture: Box<dyn InputCapture>) -> Self {
        Self { capture: Mutex::new(capture) }
    }
}

impl PassThroughControl for PassThroughAdapter {
    fn set_pass_through(&self, enabled: bool) {
        if let Err(e) = self.capture.lock().expect("poisoned").set_pass_through(enabled) {
            tracing::error!(error = %e, "failed to toggle input pass-through");
        }
    }
}

#[cfg(windows)]
pub fn build_platform_input() -> anyhow::Result<(Box<dyn InputCapture>, Box<dyn InputInjector>)> {
    Ok((Box::new(ms_input_windows::WindowsInputCapture::new()), Box::new(ms_input_windows::WindowsInputInjector::new())))
}

#[cfg(target_os = "macos")]
pub fn build_platform_input() -> anyhow::Result<(Box<dyn InputCapture>, Box<dyn InputInjector>)> {
    if !ms_input_macos::accessibility_trusted() {
        anyhow::bail!(
            "Accessibility access not granted; run onboarding first (System Settings -> Privacy & Security -> Accessibility)"
        );
    }
    Ok((Box::new(ms_input_macos::MacOsInputCapture::new()), Box::new(ms_input_macos::MacOsInputInjector::new())))
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn build_platform_input() -> anyhow::Result<(Box<dyn InputCapture>, Box<dyn InputInjector>)> {
    anyhow::bail!("no input capture/injection backend is available on this platform (Mouse Share ships Windows and macOS support only)")
}
