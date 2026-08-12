use serde::{Deserialize, Serialize};

/// Platform-independent key identity transmitted on the wire. Deliberately
/// modeled on *physical key position* (like a USB HID usage code) rather
/// than the character it produces, because the sender and receiver may use
/// different keyboard layouts — position-based codes let the receiving OS
/// apply its own layout to decide what character results, matching how
/// physical KVM switches behave and avoiding double-translation bugs.
///
/// Platform-specific scan/virtual-key codes are mapped to/from this enum by
/// `ms-keymap`; nothing in this crate knows about VK_* or macOS keycodes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LogicalKey {
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Digit0, Digit1, Digit2, Digit3, Digit4,
    Digit5, Digit6, Digit7, Digit8, Digit9,

    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    F13, F14, F15, F16, F17, F18, F19, F20,

    Escape, Tab, CapsLock, Space, Backspace, Enter,
    ShiftLeft, ShiftRight,
    ControlLeft, ControlRight,
    AltLeft, AltRight,
    MetaLeft, MetaRight, // Windows key / Command key

    ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
    Home, End, PageUp, PageDown, Insert, Delete,

    NumLock,
    Numpad0, Numpad1, Numpad2, Numpad3, Numpad4,
    Numpad5, Numpad6, Numpad7, Numpad8, Numpad9,
    NumpadAdd, NumpadSubtract, NumpadMultiply, NumpadDivide,
    NumpadDecimal, NumpadEnter,

    Minus, Equal, BracketLeft, BracketRight, Backslash,
    Semicolon, Quote, Comma, Period, Slash, Backquote,

    PrintScreen, ScrollLock, Pause, ContextMenu,

    MediaPlayPause, MediaNext, MediaPrevious, MediaStop,
    VolumeUp, VolumeDown, VolumeMute,

    /// Escape hatch for platform key codes with no mapping yet. Carries the
    /// raw platform code purely for logging/diagnostics; receivers should
    /// ignore keys they don't recognize rather than guess.
    Unmapped(u32),
}
