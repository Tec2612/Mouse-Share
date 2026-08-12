# User Guide

## What Mouse Share does

Once two or more computers are paired and arranged, moving your mouse to
the edge of one computer's screen moves control — mouse and keyboard — to
the neighboring computer, as if they shared one desktop. Moving back to
the original edge on the other machine hands control back.

## First-time setup

### 1. Install and launch

Install on each computer you want to include (see
[build-instructions.md](build-instructions.md) or use a built installer
from `installers/`). Mouse Share runs as a background service with a
system tray icon (Windows) or menu-bar icon (macOS); the main window
(Dashboard) opens from there.

### 2. Grant macOS permissions (macOS only)

macOS requires two permissions before Mouse Share can capture or inject
input system-wide. The onboarding flow walks you through both:

- **Accessibility** — required to capture input events and to post
  synthetic events. Without it, Mouse Share cannot see your mouse/keyboard
  at all.
- **Input Monitoring** — required (since macOS 10.15) to read keyboard
  event *content* through a system-wide event tap, not just timing.

Both are granted in **System Settings → Privacy & Security**. Mouse Share
can open the correct pane directly and will detect the grant taking
effect automatically (macOS does not notify apps when a permission
changes, so the app polls). If you accidentally deny either prompt, you
must re-enable it manually in System Settings — macOS only shows the
system prompt once per install.

Windows requires no special permission grant for standard use; some
antivirus/EDR software may prompt about a low-level keyboard/mouse hook
being installed the first time, which is expected (see
[security.md](security.md)).

### 3. Discover or manually add a computer

Computers on the same network appear automatically on the **Dashboard**
under "Discovered Devices" (via mDNS/Bonjour), showing name, OS, IP
address, and pairing status. If a computer doesn't appear — common on
networks with client/AP isolation, some VPNs, or guest Wi-Fi — use **Add
Computer → Connect by IP/Hostname** and enter its address.

### 4. Pair

Select a discovered (or manually added) computer and choose **Pair**. Both
computers will display a 6-digit code. **Compare them on both screens** —
if they match, confirm on both sides. If they don't match, do not confirm;
this indicates a potential network attacker, not a normal error, and
pairing should be retried only after investigating (see
[security.md](security.md#pairing-trust-on-first-use-with-a-cryptographic-not-decorative-pin)
for why this comparison is the actual security check, not decoration).

Once paired, no further password or code entry is needed to reconnect —
your two devices now trust each other's cryptographic identity directly.

### 5. Arrange your screens

Open **Computer Setup → Screen Layout**. Drag each computer's icon to
match its physical position relative to the others (left/right/above/
below). This determines which edge on which computer hands off to which
other computer. You can:

- Restrict a hand-off to only part of an edge (useful when a laptop screen
  is shorter than an external monitor it sits beside).
- Enable/disable a paired computer without unpairing it (temporarily
  excludes it from the layout, e.g. when it's asleep or you don't want it
  reachable right now).
- Set up more than two computers — see [Multiple
  computers](#multiple-computers) below.

### 6. Configure keyboard mapping (optional)

By default, modifier keys map **positionally**: the Windows key and
Command key (same physical position on most keyboards) are treated as the
same logical key, and likewise Ctrl↔Ctrl, Alt↔Option. If you'd rather your
Windows keyboard's Ctrl act as the Mac's Cmd (so Ctrl+C/V behave like the
Mac's native Cmd+C/V when controlling it), enable **Settings → Keyboard →
Swap Ctrl/Command for cross-platform pairs**. This only affects traffic
between different operating systems — a Windows-to-Windows or
Mac-to-Mac pair is unaffected either way.

## Everyday use

Move your mouse to a configured edge; control transfers automatically.
Move it back across the corresponding edge on the other machine to
return. Keyboard input, clicks, and scrolling (including horizontal and
high-resolution/precision scrolling) all follow the active computer.

### Clipboard sharing

Off by default. Enable in **Settings → Clipboard** to sync plain-text
copy/paste between computers while one is controlling the other. Only
plain/Unicode text is synced; nothing is sent unless you explicitly turn
this on.

### Emergency release

If control ever seems stuck, or you simply want your local mouse/keyboard
back immediately regardless of network state: press the **emergency
hotkey** (default: **Ctrl+Alt+Esc**, configurable in **Settings →
Emergency Hotkey**), or click **Stop Sharing** in the tray/menu-bar menu.
This always works locally and instantly, even if the network connection to
the other device has failed — see
[security.md](security.md#emergency-release).

If a connection simply drops (Wi-Fi hiccup, the other computer sleeps,
etc.), control returns to you automatically within a few seconds — you
never need to do anything.

### Multiple computers

Mouse Share supports arranging more than two computers, including a hub
topology (e.g. one Mac in the middle with a Windows PC on each side and a
MacBook above it — control flows from any edge to whichever computer is
configured on the other side of it). Configure each pairwise edge link
independently in **Computer Setup → Screen Layout**; there's no limit on
how many computers can be in one layout beyond what's practical to arrange
on screen.

## Settings reference

| Setting | Default | Notes |
|---|---|---|
| Start on startup | Off | Launches the background service at login. |
| Auto-discovery | On | Toggles mDNS advertise/browse; turn off on networks where you only ever connect by IP. |
| Clipboard sharing | Off | See above. |
| Keyboard mapping | Positional | See [Configure keyboard mapping](#6-configure-keyboard-mapping-optional). |
| Mouse sensitivity | 1.0× | Multiplier applied to inbound movement. |
| Respect receiver acceleration | On | Leaves the receiving OS's own pointer-acceleration curve active on injected movement (closest to "feels local"); turn off if you notice the pointer feels inconsistent between machines. |
| Natural scrolling | Off | |
| Pairing port | 45678 | Used only during the pairing handshake itself. If your firewall prompts when Mouse Share first starts, allow it — otherwise other computers can't pair with this one. Change only if it conflicts with something else on your network. |
| Session port | 45677 | Reserved for shared-control traffic once a pairing is active. |
| Emergency hotkey | Ctrl+Alt+Esc | |
| Theme | System | Light/Dark/System. |
| Language | Matches system | |
| Logging level | Info | See **Settings → Logs and Diagnostics** for the live log viewer and connection/latency stats. |

## Diagnostics

**Settings → Logs and Diagnostics** shows, per connection: current
latency, event/packet rate, connection status, the peer's network
address, local permission status (macOS), and recent errors. Logs never
contain the actual characters you type, clipboard contents, or any
password/key material — see [security.md](security.md#logging).

## Removing or revoking a computer

**Computer Setup → select a computer → Remove.** This immediately revokes
its certificate from your trust store; it can no longer connect to this
device without pairing again from scratch (a new pairing, with a new SAS
comparison — its old identity is not silently re-trusted).
