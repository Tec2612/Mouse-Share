//! macOS virtual keycode (`CGKeyCode`, aka the Carbon `kVK_*` constants)
//! <-> `LogicalKey` translation.
//!
//! These codes are positional (they identify a physical key on the
//! standard ANSI keyboard, not the character it currently produces under
//! the active input source), which is exactly the semantics `LogicalKey`
//! wants. Reference: `HIToolbox/Events.h` `kVK_*` constants.
//!
//! Media keys (Play/Pause, volume, etc.) are **not** delivered as normal
//! key-down/up events with a `CGKeyCode` on macOS — they arrive as
//! `NX_SYSDEFINED` (`kCGEventSystemDefined`) events carrying an
//! `NX_KEYTYPE_*` value in their subtype-specific data. `ms-input-macos`
//! decodes those separately and maps them into the same `LogicalKey`
//! media variants using the `NX_KEYTYPE_*` constants re-exported here.
use ms_protocol::LogicalKey;

pub fn vk_to_logical(vk: u16) -> LogicalKey {
    use LogicalKey::*;
    match vk {
        0x00 => A, 0x01 => S, 0x02 => D, 0x03 => F, 0x04 => H, 0x05 => G,
        0x06 => Z, 0x07 => X, 0x08 => C, 0x09 => V, 0x0B => B, 0x0C => Q,
        0x0D => W, 0x0E => E, 0x0F => R, 0x10 => Y, 0x11 => T,
        0x1F => O, 0x20 => U, 0x22 => I, 0x23 => P, 0x25 => L, 0x26 => J,
        0x28 => K, 0x2D => N, 0x2E => M,

        0x12 => Digit1, 0x13 => Digit2, 0x14 => Digit3, 0x15 => Digit4,
        0x17 => Digit5, 0x16 => Digit6, 0x1A => Digit7, 0x1C => Digit8,
        0x19 => Digit9, 0x1D => Digit0,

        0x7A => F1, 0x78 => F2, 0x63 => F3, 0x76 => F4, 0x60 => F5,
        0x61 => F6, 0x62 => F7, 0x64 => F8, 0x65 => F9, 0x6D => F10,
        0x67 => F11, 0x6F => F12, 0x69 => F13, 0x6B => F14, 0x71 => F15,
        0x6A => F16, 0x40 => F17, 0x4F => F18, 0x50 => F19, 0x5A => F20,

        0x35 => Escape,
        0x30 => Tab,
        0x39 => CapsLock,
        0x31 => Space,
        0x33 => Backspace,
        0x24 => Enter,
        0x4C => NumpadEnter,

        0x38 => ShiftLeft, 0x3C => ShiftRight,
        0x3B => ControlLeft, 0x3E => ControlRight,
        0x3A => AltLeft, 0x3D => AltRight,
        0x37 => MetaLeft, 0x36 => MetaRight,

        0x7E => ArrowUp, 0x7D => ArrowDown, 0x7B => ArrowLeft, 0x7C => ArrowRight,
        0x73 => Home, 0x77 => End, 0x74 => PageUp, 0x79 => PageDown,
        0x72 => Insert, // kVK_Help; closest physical equivalent to PC Insert on ANSI Mac keyboards
        0x75 => Delete, // kVK_ForwardDelete

        0x47 => NumLock, // kVK_ANSI_KeypadClear, conventionally treated as NumLock's Mac analogue
        0x52 => Numpad0, 0x53 => Numpad1, 0x54 => Numpad2, 0x55 => Numpad3,
        0x56 => Numpad4, 0x57 => Numpad5, 0x58 => Numpad6, 0x59 => Numpad7,
        0x5B => Numpad8, 0x5C => Numpad9,
        0x45 => NumpadAdd, 0x4E => NumpadSubtract,
        0x43 => NumpadMultiply, 0x4B => NumpadDivide, 0x41 => NumpadDecimal,

        0x1B => Minus, 0x18 => Equal,
        0x21 => BracketLeft, 0x1E => BracketRight, 0x2A => Backslash,
        0x29 => Semicolon, 0x27 => Quote,
        0x2B => Comma, 0x2F => Period, 0x2C => Slash, 0x32 => Backquote,

        0x6E => ContextMenu, // kVK_ContextualMenu on extended keyboards

        other => Unmapped(other as u32),
    }
}

pub fn logical_to_vk(key: LogicalKey) -> Option<u16> {
    use LogicalKey::*;
    Some(match key {
        A => 0x00, S => 0x01, D => 0x02, F => 0x03, H => 0x04, G => 0x05,
        Z => 0x06, X => 0x07, C => 0x08, V => 0x09, B => 0x0B, Q => 0x0C,
        W => 0x0D, E => 0x0E, R => 0x0F, Y => 0x10, T => 0x11,
        O => 0x1F, U => 0x20, I => 0x22, P => 0x23, L => 0x25, J => 0x26,
        K => 0x28, N => 0x2D, M => 0x2E,

        Digit1 => 0x12, Digit2 => 0x13, Digit3 => 0x14, Digit4 => 0x15,
        Digit5 => 0x17, Digit6 => 0x16, Digit7 => 0x1A, Digit8 => 0x1C,
        Digit9 => 0x19, Digit0 => 0x1D,

        F1 => 0x7A, F2 => 0x78, F3 => 0x63, F4 => 0x76, F5 => 0x60,
        F6 => 0x61, F7 => 0x62, F8 => 0x64, F9 => 0x65, F10 => 0x6D,
        F11 => 0x67, F12 => 0x6F, F13 => 0x69, F14 => 0x6B, F15 => 0x71,
        F16 => 0x6A, F17 => 0x40, F18 => 0x4F, F19 => 0x50, F20 => 0x5A,

        Escape => 0x35,
        Tab => 0x30,
        CapsLock => 0x39,
        Space => 0x31,
        Backspace => 0x33,
        Enter => 0x24,
        NumpadEnter => 0x4C,

        ShiftLeft => 0x38, ShiftRight => 0x3C,
        ControlLeft => 0x3B, ControlRight => 0x3E,
        AltLeft => 0x3A, AltRight => 0x3D,
        MetaLeft => 0x37, MetaRight => 0x36,

        ArrowUp => 0x7E, ArrowDown => 0x7D, ArrowLeft => 0x7B, ArrowRight => 0x7C,
        Home => 0x73, End => 0x77, PageUp => 0x74, PageDown => 0x79,
        Insert => 0x72, Delete => 0x75,

        NumLock => 0x47,
        Numpad0 => 0x52, Numpad1 => 0x53, Numpad2 => 0x54, Numpad3 => 0x55,
        Numpad4 => 0x56, Numpad5 => 0x57, Numpad6 => 0x58, Numpad7 => 0x59,
        Numpad8 => 0x5B, Numpad9 => 0x5C,
        NumpadAdd => 0x45, NumpadSubtract => 0x4E,
        NumpadMultiply => 0x43, NumpadDivide => 0x4B, NumpadDecimal => 0x41,

        Minus => 0x1B, Equal => 0x18,
        BracketLeft => 0x21, BracketRight => 0x1E, Backslash => 0x2A,
        Semicolon => 0x29, Quote => 0x27,
        Comma => 0x2B, Period => 0x2F, Slash => 0x2C, Backquote => 0x32,

        ContextMenu => 0x6E,

        // No CGKeyCode exists for these; ms-input-macos synthesizes them
        // via NX_SYSDEFINED media-key events instead of CGEventPost with a
        // keycode.
        PrintScreen | ScrollLock | Pause | MediaPlayPause | MediaNext
        | MediaPrevious | MediaStop | VolumeUp | VolumeDown | VolumeMute
        | Unmapped(_) => return None,
        // LogicalKey is #[non_exhaustive]; unknown future variants have no
        // known CGKeyCode yet.
        _ => return None,
    })
}

/// `NX_KEYTYPE_*` values from `IOKit/hidsystem/ev_keymap.h`, carried in the
/// subtype-specific data of `NX_SYSDEFINED` events. Used by
/// `ms-input-macos` to translate media-key hardware events to/from
/// `LogicalKey`.
pub mod media_key_type {
    pub const SOUND_UP: i32 = 0;
    pub const SOUND_DOWN: i32 = 1;
    pub const MUTE: i32 = 7;
    pub const PLAY: i32 = 16;
    pub const NEXT: i32 = 17;
    pub const PREVIOUS: i32 = 18;
}

#[cfg(test)]
mod tests {
    use super::*;
    use LogicalKey::*;

    #[test]
    fn round_trips_every_mapped_logical_key_through_cgkeycodes() {
        let samples = [
            A, Z, Digit0, Digit9, F1, F20, Escape, Tab, CapsLock, Space, Backspace,
            Enter, NumpadEnter, ShiftLeft, ShiftRight, ControlLeft, ControlRight,
            AltLeft, AltRight, MetaLeft, MetaRight, ArrowUp, ArrowDown, ArrowLeft,
            ArrowRight, Home, End, PageUp, PageDown, Insert, Delete, NumLock,
            Numpad5, NumpadAdd, NumpadDivide, Minus, Equal, BracketLeft, Backslash,
            Semicolon, Quote, Comma, Period, Slash, Backquote, ContextMenu,
        ];
        for key in samples {
            let vk = logical_to_vk(key).unwrap_or_else(|| panic!("no CGKeyCode for {key:?}"));
            let back = vk_to_logical(vk);
            assert_eq!(back, key, "round trip failed for {key:?} (vk=0x{vk:02X})");
        }
    }

    #[test]
    fn media_keys_have_no_cgkeycode_and_route_through_nx_syskeys_instead() {
        assert_eq!(logical_to_vk(MediaPlayPause), None);
        assert_eq!(logical_to_vk(VolumeUp), None);
    }

    #[test]
    fn unknown_vk_code_becomes_unmapped_instead_of_panicking() {
        assert_eq!(vk_to_logical(0xFF), Unmapped(0xFF));
    }
}
