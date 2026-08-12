//! Wire protocol for Mouse Share: framing, message types, and mouse-move
//! coalescing. Transport-agnostic; see `ms-security` for the mTLS session
//! this protocol is expected to run over, and `docs/protocol-spec.md` for
//! the full specification.

mod coalesce;
mod framing;
mod logical_key;
mod messages;

pub use coalesce::MouseMoveCoalescer;
pub use framing::{decode_message, encode_message, read_message, write_message, FramingError};
pub use logical_key::LogicalKey;
pub use messages::*;
