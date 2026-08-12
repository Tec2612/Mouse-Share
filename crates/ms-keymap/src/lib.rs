//! Translates between platform-native key codes and the wire-level
//! `LogicalKey`, and applies configurable cross-platform modifier
//! remapping (e.g. Windows Ctrl <-> macOS Command).
//!
//! `LogicalKey` is defined *by physical key position*, so translating
//! within one platform (Windows -> Windows, macOS -> macOS) and across
//! platforms both flow through the same table; there is no special-cased
//! Windows<->macOS path.

pub mod macos;
pub mod modifiers;
pub mod windows;

pub use modifiers::{remap_modifiers, ModifierPolicy};
pub use ms_protocol::{LogicalKey, Modifiers, OperatingSystem};
