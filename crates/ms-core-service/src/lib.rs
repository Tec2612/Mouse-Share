//! Orchestration daemon core: ties the edge state machine
//! (`ms-input-core`) to real network sessions, reconnection with backoff,
//! heartbeat-based liveness detection, and duplicate/replay rejection.
//! Platform input capture/injection are injected as trait objects
//! (`ms-input-windows`/`ms-input-macos` implement them) so this crate has
//! no OS-specific code and is fully unit-testable on any host.

mod dedup;
mod heartbeat;
mod reconnect;
mod service;
mod session;

pub use dedup::SequenceGuard;
pub use heartbeat::HeartbeatMonitor;
pub use reconnect::ReconnectBackoff;
pub use service::{CoreService, NetworkSender, PassThroughControl};
pub use session::{handshake, heartbeat_loop, read_loop, write_loop, SessionError};
