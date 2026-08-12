//! Cross-platform input orchestration core: capture/injector
//! abstractions implemented per-platform in `ms-input-windows` /
//! `ms-input-macos`, the edge-crossing state machine that decides when
//! control hands off between devices, and the clipboard sync gate.

mod capture;
mod clipboard;
mod injector;
mod state_machine;

pub use capture::{CaptureError, CaptureSink, InputCapture};
pub use clipboard::ClipboardGateway;
pub use injector::{InjectError, InputInjector};
pub use state_machine::{Action, EdgeStateMachine, Event, LocalState};
