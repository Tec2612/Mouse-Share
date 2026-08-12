# End-to-End Test Plan

Automated tests (100+, `cargo test --workspace`) cover every
platform-independent behavior: protocol framing/coalescing, key mapping
(including cross-platform translation and modifier remapping), pairing
and mTLS (against real TLS handshakes over loopback), discovery (against
real mDNS multicast), screen layout resolution, the edge-crossing state
machine (16 scenarios including emergency release and connection loss),
reconnection backoff, heartbeat liveness, and replay protection. See each
crate's `src/*.rs` test modules and `tests/` directories.

What automated tests **cannot** cover, because they require the real OS
input pipeline and real hardware, is this plan's subject: **at least one
physical Windows 10/11 machine and one physical Mac (13+), on the same
LAN**, going through the product as a real user would.

## Required hardware/setup

- 1× Windows 10 or 11 machine.
- 1× Mac running macOS 13 or later.
- Both on the same LAN segment with multicast (mDNS) permitted (a second
  network with client isolation enabled is useful for exercising the
  manual-connect fallback — see 5.2).
- At least one of the two machines with a second physical monitor
  attached, for the multi-monitor/DPI scenarios (6.x).
- A keyboard with a non-US layout available for 7.4, if practical.

## 1. Installation & first run

| # | Steps | Expected result |
|---|---|---|
| 1.1 | Install on Windows via the built installer | Installs, background service starts, tray icon appears |
| 1.2 | Install on macOS via the built `.dmg`/`.app` | Installs, menu-bar icon appears |
| 1.3 | First launch on macOS before granting any permission | App detects missing Accessibility/Input Monitoring access and shows the onboarding flow rather than silently failing or crashing |
| 1.4 | Grant Accessibility, then Input Monitoring, following onboarding prompts | App detects each grant without requiring a restart; onboarding completes |
| 1.5 | Deny a permission, then grant it later via System Settings directly | App detects the change on its own (polling), no restart required |

## 2. Discovery & pairing

| # | Steps | Expected result |
|---|---|---|
| 2.1 | With both machines on the same LAN and auto-discovery on, open the Dashboard on each | Each machine sees the other listed, with correct name, OS, IP, and "not paired" status |
| 2.2 | Initiate pairing from the Windows side | Both machines show a 6-digit code |
| 2.3 | Compare the codes | **They match.** Confirm on both sides |
| 2.4 | Reboot both machines, relaunch the app | Devices remain paired with no re-entry of any code (long-term auth is certificate-based, see [security.md](security.md)) |
| 2.5 | On a network with client isolation enabled (or simulate by blocking multicast), repeat 2.1 | Devices do **not** appear via auto-discovery; use **Add Computer → Connect by IP/Hostname** with the peer's LAN IP instead — pairing proceeds identically from there |
| 2.6 | Attempt to pair a third, uninvolved device to either Windows or Mac by entering a **mismatched** confirmation (simulate by having a person read the wrong code) | Pairing must **not** complete; no trust is established |
| 2.7 | Remove/revoke the Mac from the Windows machine's paired list, then have the Mac attempt to reconnect | Connection is rejected immediately (not after some delay) |

## 3. Core mouse/keyboard sharing: Windows → macOS

| # | Steps | Expected result |
|---|---|---|
| 3.1 | Arrange layout: Windows PC on the left, Mac on the right; link Windows' right edge to Mac's left edge | Layout saves and both machines show the same arrangement |
| 3.2 | Move the mouse to the Windows machine's right edge | Control transfers to the Mac; the Mac's cursor appears at the corresponding vertical position |
| 3.3 | Move the mouse around on the Mac (now controlled from the Windows machine's physical mouse) | Movement feels smooth and directly responsive, no stutter, no perceptible added lag under normal LAN conditions |
| 3.4 | Click left/right/middle buttons; use any extra (back/forward) buttons your mouse has | All register correctly on the Mac |
| 3.5 | Scroll vertically and horizontally; if available, test a precision trackpad or high-res mouse wheel | Scrolling direction and magnitude feel correct on the Mac; high-resolution scrolling is smooth, not chunked into large steps |
| 3.6 | Type a sentence including letters, numbers, and punctuation | All characters appear correctly on the Mac |
| 3.7 | Test modifier keys: Shift, Ctrl, Alt, the Windows key, Caps Lock, Num Lock | Each behaves as its mapped macOS equivalent (Windows key → Cmd by default; see [user-guide.md](user-guide.md#6-configure-keyboard-mapping-optional)) |
| 3.8 | Test function keys F1–F12 (and F13+ if your keyboard has them), arrows, Home/End/Page Up/Page Down, Insert/Delete | All register the correct action on the Mac |
| 3.9 | Test a common shortcut, e.g. Ctrl+C / Ctrl+V (or Cmd+C/V under the optional Ctrl↔Cmd swap setting) | Copy/paste works as expected on the Mac |
| 3.10 | Test media keys (volume, play/pause) if present on the keyboard | Correct action on the Mac, if the keyboard/OS combination supports it |
| 3.11 | Move the mouse back across the Mac's left edge | Control returns to the Windows machine immediately; Windows cursor is where it was left |

## 4. Core mouse/keyboard sharing: macOS → Windows

Repeat all of section 3 with roles reversed (Mac controlling, Windows
controlled). Pay particular attention to:

| # | Steps | Expected result |
|---|---|---|
| 4.1 | Cmd, Option, Control from the Mac's physical keyboard | Map correctly to Windows key / Alt / Ctrl by default |
| 4.2 | macOS-specific keys with no direct Windows equivalent (e.g. the Globe/Fn key) | Does not crash or send garbage input; either ignored or mapped to its closest documented equivalent |

## 5. Windows ↔ Windows and macOS ↔ macOS

Repeat the relevant subset of section 3 for same-OS pairs, if a second
machine of either OS is available. Same-OS pairs should need **no**
keyboard mapping configuration to feel completely native (Ctrl is Ctrl,
Alt is Alt, the modifier swap setting must have no effect here — see
`ms-keymap`'s `ModifierPolicy` tests, which assert this at the unit level;
this section confirms it holds true end-to-end).

## 6. Multiple monitors & DPI/scaling

| # | Steps | Expected result |
|---|---|---|
| 6.1 | With a second monitor attached to one machine, verify the configured edge boundary is measured against the *combined* virtual desktop, not a single monitor | Moving the mouse to the outer edge of the multi-monitor arrangement triggers the hand-off; the seam between the two local monitors does not |
| 6.2 | Set one machine's display scaling to something other than 100% (e.g. 125% or 150% on Windows, a scaled resolution on macOS) | Edge detection and cursor placement on hand-off remain accurate — no drift or off-by-some-pixels misplacement |
| 6.3 | Pair two machines with *different* DPI scale factors | `EdgeEnter.sender_scale` correctly compensates; movement doesn't feel unnaturally fast/slow immediately after a hand-off |

## 7. Keyboard layouts

| # | Steps | Expected result |
|---|---|---|
| 7.1 | Set the *controlled* machine's active keyboard layout to something other than US (e.g. a European QWERTY/AZERTY variant) while the controller stays on a US layout | Because `LogicalKey` is position-based (see [architecture.md](architecture.md)), the physically-pressed key produces whatever character the **controlled** machine's layout says it should — matching how a physical KVM switch behaves |
| 7.2 | Type a character that requires a dead-key/compose sequence on the controlled machine's layout | Composes correctly, the same as typing it locally on that machine |

## 8. Clipboard sharing

| # | Steps | Expected result |
|---|---|---|
| 8.1 | With clipboard sharing **off** (default), copy text on the controller while controlling the other machine, then paste on the controlled machine | Paste does **not** produce the copied text (no data was ever sent) |
| 8.2 | Enable clipboard sharing on both machines, repeat | Paste succeeds with the correct text |
| 8.3 | Copy non-ASCII/Unicode text (emoji, non-Latin script) | Round-trips correctly |
| 8.4 | Disable clipboard sharing on one side only | No content syncs in either direction (verify the setting is honored per-device, not just from one side's perspective) |

## 9. Reliability & reconnection

| # | Steps | Expected result |
|---|---|---|
| 9.1 | While actively controlling the other machine, disable Wi-Fi (or unplug Ethernet) on either machine | Local mouse/keyboard control returns to whichever machine you're physically using, within a few seconds, automatically |
| 9.2 | Re-enable the network | Devices reconnect automatically without user action; pairing/trust is unaffected |
| 9.3 | Repeat 9.1 but specifically while the *controlled* machine (not the controller) loses network | Same result: the controller regains local input promptly, does not hang |
| 9.4 | Switch one machine from Wi-Fi to Ethernet (or vice versa) mid-session | Connection either survives the transition or reconnects automatically; no stuck/frozen input state |
| 9.5 | Put the controlled machine to sleep, then wake it | Connection is detected as lost while asleep and reconnects cleanly on wake, or is cleanly re-established when the user next interacts |
| 9.6 | Force-quit the app on the controlled machine while it's being controlled | Controller regains local input within a few seconds |

## 10. Emergency release

| # | Steps | Expected result |
|---|---|---|
| 10.1 | While controlling the other machine, press the configured emergency hotkey (default Ctrl+Alt+Esc) on the controller | Local input returns immediately, regardless of network state |
| 10.2 | Repeat while the network connection is healthy | Works identically (not just as a failure fallback) |
| 10.3 | Use the tray/menu-bar "Stop Sharing" action instead of the hotkey | Same result |
| 10.4 | Reconfigure the emergency hotkey to a custom combination, then test it | New combination works; old default no longer triggers it |

## 11. Multiple computers

| # | Steps | Expected result |
|---|---|---|
| 11.1 | With a third machine available (or simulated via a second instance/VM), arrange a hub layout: Mac in the middle, Windows PC on each side | Moving the mouse left from the Mac reaches PC #1; moving right reaches PC #2; each independently returns control correctly |
| 11.2 | Disable one of the three devices in the layout (without unpairing) | Its edge no longer triggers a hand-off; the other two continue working |

## 12. Diagnostics

| # | Steps | Expected result |
|---|---|---|
| 12.1 | Open **Settings → Logs and Diagnostics** during an active session | Shows live latency, event rate, connection status, and peer address that plausibly match observed behavior |
| 12.2 | Trigger a deliberate error (e.g. attempt to connect to a revoked device) | Appears in "Recent errors" with a clear, non-technical-jargon description |
| 12.3 | Inspect the log file/viewer after typing a password into some other application while a session was active | Confirm no actual keystroke content, clipboard contents, or key material appears anywhere in the logs (see [security.md](security.md#logging)) |

## Sign-off

This plan is considered passed for a release candidate when every row
above has been executed on real hardware, on both platform pairings (at
minimum Windows↔macOS in both control directions), with results recorded
against the build/commit tested.
