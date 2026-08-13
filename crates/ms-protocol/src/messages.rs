use serde::{Deserialize, Serialize};

/// Current wire protocol version. Bumped on any breaking change to
/// `Message`. `Hello` exchanges this so mismatched builds fail fast with a
/// clear error instead of silently misinterpreting bytes.
pub const PROTOCOL_VERSION: u16 = 1;

pub type SessionId = uuid::Uuid;
pub type DeviceId = uuid::Uuid;

/// Every value that travels the wire between two paired devices. Kept as a
/// single flat enum (rather than per-channel types) because the transport
/// is one ordered, reliable, encrypted TCP stream and messages are cheap to
/// discriminate on the receiving end.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Message {
    /// First message sent by the connecting side after the TLS handshake
    /// completes. Confirms both ends speak the same protocol version and
    /// identifies the sending device (already authenticated at the TLS
    /// layer via the client certificate; this is metadata, not auth).
    Hello(Hello),
    HelloAck(HelloAck),

    Heartbeat { seq: u32, sent_at_ms: u64 },
    HeartbeatAck { seq: u32 },

    /// Coalesced relative mouse movement. `dx`/`dy` are accumulated deltas
    /// in the sender's logical (DPI-normalized) pixel space; the receiver
    /// rescales into its own pixel space using the negotiated DPI ratio in
    /// `EdgeEnter`.
    MouseMove { dx: f64, dy: f64, seq: u32 },

    /// Absolute warp, for a mid-session cursor re-sync. `x`/`y` are
    /// normalized 0.0..1.0 within the *receiver's* screen (the sender
    /// can't know the receiver's actual pixel dimensions) — the same
    /// convention `EdgeEnter.position` uses. Not currently sent anywhere:
    /// initial placement on hand-off is driven entirely by `EdgeEnter`'s
    /// `position` field instead (see `ms-input-core`'s
    /// `EdgeStateMachine::on_local_cursor_at_edge`); this variant exists
    /// for a future mid-session re-sync need.
    MouseWarp { x: f64, y: f64 },

    MouseButton { button: MouseButton, pressed: bool },

    MouseWheel {
        delta_x: f64,
        delta_y: f64,
        /// True if the deltas are high-resolution (sub-notch) values as
        /// reported by precision trackpads / high-res mice, false if they
        /// are quantized to standard notch increments.
        high_resolution: bool,
    },

    KeyEvent {
        key: crate::LogicalKey,
        pressed: bool,
        modifiers: Modifiers,
        /// Monotonically increasing per-session counter. Used by the
        /// receiver to detect and drop duplicate deliveries after a
        /// reconnect replays the sender's outbound queue.
        seq: u32,
    },

    /// Sent when the mouse crosses the configured edge and control should
    /// move to the receiving device.
    EdgeEnter {
        edge: ScreenEdge,
        /// Position along the edge, normalized 0.0..1.0, so entry lines up
        /// vertically/horizontally regardless of resolution differences.
        position: f64,
        /// Sender's screen DPI scale factor (96 DPI = 1.0), so the
        /// receiver can convert incoming relative deltas correctly.
        sender_scale: f64,
    },
    /// Sent when control returns to the sender (mouse reached the
    /// configured return edge on the receiving device).
    EdgeRelease,
    /// Sent back to an `EdgeEnter` sender when the receiver can't accept
    /// the hand-off because it is already controlling or being controlled
    /// by someone else. Lets the sender recover instead of being stuck
    /// `Controlling` forever waiting for a hand-off the peer silently
    /// dropped — this is what breaks the race where both sides cross
    /// their linked edge at nearly the same instant and each refuses the
    /// other's `EdgeEnter` because it's already busy handling its own.
    EdgeEnterRejected,

    ClipboardOffer { formats: Vec<ClipboardFormat> },
    ClipboardRequest { format: ClipboardFormat },
    ClipboardData { format: ClipboardFormat, data: Vec<u8> },

    Disconnect { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hello {
    pub protocol_version: u16,
    pub device_id: DeviceId,
    pub device_name: String,
    pub os: OperatingSystem,
    pub session_id: SessionId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelloAck {
    pub protocol_version: u16,
    pub accepted: bool,
    /// Populated when `accepted` is false, e.g. protocol version mismatch.
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum OperatingSystem {
    Windows,
    MacOs,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u8),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ScreenEdge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ClipboardFormat {
    PlainTextUtf8,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub meta: bool, // Windows key / Command key
    pub caps_lock: bool,
}
