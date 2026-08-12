use ms_protocol::{LogicalKey, Modifiers, OperatingSystem};
use serde::{Deserialize, Serialize};

/// How modifier keys are translated when the controlling and controlled
/// computers run different operating systems. Configured per device pair
/// in settings (see `ms-config`); irrelevant for same-OS pairs, where the
/// mapping is always the identity (both sides already agree on what
/// Ctrl/Alt/Meta mean).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ModifierPolicy {
    /// Meta<->Meta, Control<->Control, Alt<->Alt. Since `LogicalKey`
    /// already represents keys by physical position, this is a no-op: the
    /// Windows key and Command key occupy the same position and both map
    /// to `MetaLeft`/`MetaRight` already.
    #[default]
    Positional,
    /// Swaps Control and Meta so that, e.g., a Windows keyboard's Ctrl+C
    /// acts as macOS's Cmd+C. Only takes effect between different OSes;
    /// same-OS traffic is left untouched even under this policy.
    CtrlCommandSwap,
}

/// Applies `policy` to a single key, given which OS captured it and which
/// OS will receive it. No-op for keys that aren't modifiers, and for
/// same-OS pairs under any policy.
pub fn remap_modifiers(
    key: LogicalKey,
    source_os: OperatingSystem,
    target_os: OperatingSystem,
    policy: ModifierPolicy,
) -> LogicalKey {
    if source_os == target_os || policy == ModifierPolicy::Positional {
        return key;
    }
    use LogicalKey::*;
    match key {
        ControlLeft => MetaLeft,
        ControlRight => MetaRight,
        MetaLeft => ControlLeft,
        MetaRight => ControlRight,
        other => other,
    }
}

/// Applies the same swap to a `Modifiers` bitset (used for the modifier
/// state accompanying non-modifier key events, e.g. Ctrl+C where `C` is
/// the key and `control` is set in `Modifiers`).
pub fn remap_modifier_flags(
    modifiers: Modifiers,
    source_os: OperatingSystem,
    target_os: OperatingSystem,
    policy: ModifierPolicy,
) -> Modifiers {
    if source_os == target_os || policy == ModifierPolicy::Positional {
        return modifiers;
    }
    Modifiers {
        control: modifiers.meta,
        meta: modifiers.control,
        ..modifiers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use OperatingSystem::*;

    #[test]
    fn positional_policy_never_swaps_regardless_of_os_pair() {
        assert_eq!(
            remap_modifiers(LogicalKey::ControlLeft, Windows, MacOs, ModifierPolicy::Positional),
            LogicalKey::ControlLeft
        );
    }

    #[test]
    fn ctrl_command_swap_only_applies_across_different_operating_systems() {
        assert_eq!(
            remap_modifiers(LogicalKey::ControlLeft, Windows, Windows, ModifierPolicy::CtrlCommandSwap),
            LogicalKey::ControlLeft,
            "same-OS traffic must never be swapped"
        );
        assert_eq!(
            remap_modifiers(LogicalKey::ControlLeft, Windows, MacOs, ModifierPolicy::CtrlCommandSwap),
            LogicalKey::MetaLeft
        );
        assert_eq!(
            remap_modifiers(LogicalKey::MetaLeft, MacOs, Windows, ModifierPolicy::CtrlCommandSwap),
            LogicalKey::ControlLeft
        );
    }

    #[test]
    fn non_modifier_keys_are_never_touched() {
        assert_eq!(
            remap_modifiers(LogicalKey::A, Windows, MacOs, ModifierPolicy::CtrlCommandSwap),
            LogicalKey::A
        );
    }

    #[test]
    fn modifier_flags_swap_consistently_with_key_swap() {
        let mods = Modifiers { control: true, meta: false, ..Default::default() };
        let swapped = remap_modifier_flags(mods, Windows, MacOs, ModifierPolicy::CtrlCommandSwap);
        assert!(swapped.meta && !swapped.control);
    }
}
