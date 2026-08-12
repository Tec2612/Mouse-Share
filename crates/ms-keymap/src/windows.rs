//! Windows virtual-key (VK_*) code <-> `LogicalKey` translation.
//!
//! Reference: Win32 `Winuser.h` virtual-key constants. Capture comes from a
//! `WH_KEYBOARD_LL` hook, which reports `(vkCode, scanCode, flags)`; the
//! `LLKHF_EXTENDED` bit distinguishes keys that share a VK code between the
//! main keyboard and the numpad (e.g. Enter vs. numpad Enter). Injection
//! via `SendInput` needs the inverse: a VK code plus whether
//! `KEYEVENTF_EXTENDEDKEY` must be set.
use ms_protocol::LogicalKey;

/// Converts a captured `(vk_code, extended)` pair into a `LogicalKey`.
/// `extended` is the `LLKHF_EXTENDED` flag from the low-level keyboard hook.
pub fn vk_to_logical(vk: u16, extended: bool) -> LogicalKey {
    use LogicalKey::*;
    match vk {
        0x41 => A, 0x42 => B, 0x43 => C, 0x44 => D, 0x45 => E, 0x46 => F,
        0x47 => G, 0x48 => H, 0x49 => I, 0x4A => J, 0x4B => K, 0x4C => L,
        0x4D => M, 0x4E => N, 0x4F => O, 0x50 => P, 0x51 => Q, 0x52 => R,
        0x53 => S, 0x54 => T, 0x55 => U, 0x56 => V, 0x57 => W, 0x58 => X,
        0x59 => Y, 0x5A => Z,
        0x30 => Digit0, 0x31 => Digit1, 0x32 => Digit2, 0x33 => Digit3,
        0x34 => Digit4, 0x35 => Digit5, 0x36 => Digit6, 0x37 => Digit7,
        0x38 => Digit8, 0x39 => Digit9,

        0x70 => F1, 0x71 => F2, 0x72 => F3, 0x73 => F4, 0x74 => F5,
        0x75 => F6, 0x76 => F7, 0x77 => F8, 0x78 => F9, 0x79 => F10,
        0x7A => F11, 0x7B => F12, 0x7C => F13, 0x7D => F14, 0x7E => F15,
        0x7F => F16, 0x80 => F17, 0x81 => F18, 0x82 => F19, 0x83 => F20,

        0x1B => Escape,
        0x09 => Tab,
        0x14 => CapsLock,
        0x20 => Space,
        0x08 => Backspace,
        // VK_RETURN is shared between the main Enter key and the numpad
        // Enter key; the numpad variant sets the extended flag.
        0x0D => if extended { NumpadEnter } else { Enter },

        0xA0 => ShiftLeft, 0xA1 => ShiftRight,
        0xA2 => ControlLeft, 0xA3 => ControlRight,
        0xA4 => AltLeft, 0xA5 => AltRight,
        0x5B => MetaLeft, 0x5C => MetaRight,

        // The navigation cluster and the numpad-without-NumLock cluster
        // share VK codes on Windows; the dedicated keys set the extended
        // flag, the numpad-emulated ones do not.
        0x26 if extended => ArrowUp,
        0x28 if extended => ArrowDown,
        0x25 if extended => ArrowLeft,
        0x27 if extended => ArrowRight,
        0x24 if extended => Home,
        0x23 if extended => End,
        0x21 if extended => PageUp,
        0x22 if extended => PageDown,
        0x2D if extended => Insert,
        0x2E if extended => Delete,
        0x26 => Numpad8, 0x28 => Numpad2, 0x25 => Numpad4, 0x27 => Numpad6,
        0x24 => Numpad7, 0x23 => Numpad1, 0x21 => Numpad9, 0x22 => Numpad3,
        0x2D => Numpad0, 0x2E => NumpadDecimal,

        0x90 => NumLock,
        0x60 => Numpad0, 0x61 => Numpad1, 0x62 => Numpad2, 0x63 => Numpad3,
        0x64 => Numpad4, 0x65 => Numpad5, 0x66 => Numpad6, 0x67 => Numpad7,
        0x68 => Numpad8, 0x69 => Numpad9,
        0x6B => NumpadAdd, 0x6D => NumpadSubtract,
        0x6A => NumpadMultiply, 0x6F => NumpadDivide, 0x6E => NumpadDecimal,

        0xBD => Minus, 0xBB => Equal,
        0xDB => BracketLeft, 0xDD => BracketRight, 0xDC => Backslash,
        0xBA => Semicolon, 0xDE => Quote,
        0xBC => Comma, 0xBE => Period, 0xBF => Slash, 0xC0 => Backquote,

        0x2C => PrintScreen, 0x91 => ScrollLock, 0x13 => Pause, 0x5D => ContextMenu,

        0xB3 => MediaPlayPause, 0xB0 => MediaNext, 0xB1 => MediaPrevious, 0xB2 => MediaStop,
        0xAF => VolumeUp, 0xAE => VolumeDown, 0xAD => VolumeMute,

        other => Unmapped(other as u32),
    }
}

/// Converts a `LogicalKey` into `(vk_code, needs_extended_flag)` for use
/// with `SendInput`'s `KEYBDINPUT.wVk` / `KEYEVENTF_EXTENDEDKEY`. Returns
/// `None` for keys with no Windows equivalent (there are none in the
/// current `LogicalKey` set, but callers on the receiving side may still
/// see `Unmapped` values from a peer's future protocol version).
pub fn logical_to_vk(key: LogicalKey) -> Option<(u16, bool)> {
    use LogicalKey::*;
    Some(match key {
        A => (0x41, false), B => (0x42, false), C => (0x43, false), D => (0x44, false),
        E => (0x45, false), F => (0x46, false), G => (0x47, false), H => (0x48, false),
        I => (0x49, false), J => (0x4A, false), K => (0x4B, false), L => (0x4C, false),
        M => (0x4D, false), N => (0x4E, false), O => (0x4F, false), P => (0x50, false),
        Q => (0x51, false), R => (0x52, false), S => (0x53, false), T => (0x54, false),
        U => (0x55, false), V => (0x56, false), W => (0x57, false), X => (0x58, false),
        Y => (0x59, false), Z => (0x5A, false),
        Digit0 => (0x30, false), Digit1 => (0x31, false), Digit2 => (0x32, false),
        Digit3 => (0x33, false), Digit4 => (0x34, false), Digit5 => (0x35, false),
        Digit6 => (0x36, false), Digit7 => (0x37, false), Digit8 => (0x38, false),
        Digit9 => (0x39, false),

        F1 => (0x70, false), F2 => (0x71, false), F3 => (0x72, false), F4 => (0x73, false),
        F5 => (0x74, false), F6 => (0x75, false), F7 => (0x76, false), F8 => (0x77, false),
        F9 => (0x78, false), F10 => (0x79, false), F11 => (0x7A, false), F12 => (0x7B, false),
        F13 => (0x7C, false), F14 => (0x7D, false), F15 => (0x7E, false), F16 => (0x7F, false),
        F17 => (0x80, false), F18 => (0x81, false), F19 => (0x82, false), F20 => (0x83, false),

        Escape => (0x1B, false),
        Tab => (0x09, false),
        CapsLock => (0x14, false),
        Space => (0x20, false),
        Backspace => (0x08, false),
        Enter => (0x0D, false),
        NumpadEnter => (0x0D, true),

        ShiftLeft => (0xA0, false), ShiftRight => (0xA1, false),
        ControlLeft => (0xA2, false), ControlRight => (0xA3, true),
        AltLeft => (0xA4, false), AltRight => (0xA5, true),
        MetaLeft => (0x5B, true), MetaRight => (0x5C, true),

        ArrowUp => (0x26, true), ArrowDown => (0x28, true),
        ArrowLeft => (0x25, true), ArrowRight => (0x27, true),
        Home => (0x24, true), End => (0x23, true),
        PageUp => (0x21, true), PageDown => (0x22, true),
        Insert => (0x2D, true), Delete => (0x2E, true),

        NumLock => (0x90, true),
        Numpad0 => (0x60, false), Numpad1 => (0x61, false), Numpad2 => (0x62, false),
        Numpad3 => (0x63, false), Numpad4 => (0x64, false), Numpad5 => (0x65, false),
        Numpad6 => (0x66, false), Numpad7 => (0x67, false), Numpad8 => (0x68, false),
        Numpad9 => (0x69, false),
        NumpadAdd => (0x6B, false), NumpadSubtract => (0x6D, false),
        NumpadMultiply => (0x6A, false), NumpadDivide => (0x6F, true), NumpadDecimal => (0x6E, false),

        Minus => (0xBD, false), Equal => (0xBB, false),
        BracketLeft => (0xDB, false), BracketRight => (0xDD, false), Backslash => (0xDC, false),
        Semicolon => (0xBA, false), Quote => (0xDE, false),
        Comma => (0xBC, false), Period => (0xBE, false), Slash => (0xBF, false), Backquote => (0xC0, false),

        PrintScreen => (0x2C, true), ScrollLock => (0x91, false), Pause => (0x13, false),
        ContextMenu => (0x5D, true),

        MediaPlayPause => (0xB3, true), MediaNext => (0xB0, true),
        MediaPrevious => (0xB1, true), MediaStop => (0xB2, true),
        VolumeUp => (0xAF, true), VolumeDown => (0xAE, true), VolumeMute => (0xAD, true),

        Unmapped(_) => return None,
        // LogicalKey is #[non_exhaustive]; unknown future variants have no
        // known VK code yet.
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use LogicalKey::*;

    #[test]
    fn round_trips_every_mapped_logical_key_through_vk_codes() {
        let samples = [
            A, Z, Digit0, Digit9, F1, F20, Escape, Tab, CapsLock, Space, Backspace,
            Enter, NumpadEnter, ShiftLeft, ShiftRight, ControlLeft, ControlRight,
            AltLeft, AltRight, MetaLeft, MetaRight, ArrowUp, ArrowDown, ArrowLeft,
            ArrowRight, Home, End, PageUp, PageDown, Insert, Delete, NumLock,
            Numpad5, NumpadAdd, NumpadDivide, Minus, Equal, BracketLeft, Backslash,
            Semicolon, Quote, Comma, Period, Slash, Backquote, PrintScreen,
            ScrollLock, Pause, ContextMenu, MediaPlayPause, VolumeUp,
        ];
        for key in samples {
            let (vk, extended) = logical_to_vk(key).unwrap_or_else(|| panic!("no VK for {key:?}"));
            let back = vk_to_logical(vk, extended);
            assert_eq!(back, key, "round trip failed for {key:?} (vk=0x{vk:02X}, ext={extended})");
        }
    }

    #[test]
    fn shared_vk_return_disambiguates_via_extended_flag() {
        assert_eq!(vk_to_logical(0x0D, false), Enter);
        assert_eq!(vk_to_logical(0x0D, true), NumpadEnter);
    }

    #[test]
    fn shared_vk_navigation_cluster_disambiguates_via_extended_flag() {
        assert_eq!(vk_to_logical(0x24, true), Home);
        assert_eq!(vk_to_logical(0x24, false), Numpad7);
    }

    #[test]
    fn unknown_vk_code_becomes_unmapped_instead_of_panicking() {
        assert_eq!(vk_to_logical(0xFE, false), Unmapped(0xFE));
    }
}
