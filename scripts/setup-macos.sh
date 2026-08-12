#!/usr/bin/env bash
# One-shot bootstrap for building Mouse Share on macOS: detects every
# missing prerequisite (Xcode Command Line Tools, Homebrew, Rust, Node.js)
# and installs each one automatically, then builds the daemon and the UI.
# Safe to re-run — every step first checks whether it's already satisfied.
#
# Usage (from any directory):
#   bash scripts/setup-macos.sh
# or double-click scripts/setup-macos.command in Finder.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

step() { printf "\n\033[36m==> %s\033[0m\n" "$1"; }
ok()   { printf "    \033[32mOK: %s\033[0m\n" "$1"; }
skip() { printf "    \033[90malready present: %s\033[0m\n" "$1"; }

step "Checking for Xcode Command Line Tools"
if xcode-select -p >/dev/null 2>&1; then
  skip "$(xcode-select -p)"
else
  echo "    Not found. Triggering the install — a system dialog will pop up;"
  echo "    click \"Install\" there and wait for it to finish (Apple doesn't"
  echo "    provide a way to do this fully unattended without an enrolled"
  echo "    Apple Developer / MDM setup)."
  xcode-select --install || true
  echo "    Waiting for the Command Line Tools install to complete..."
  until xcode-select -p >/dev/null 2>&1; do
    sleep 5
  done
  ok "Xcode Command Line Tools installed"
fi

step "Checking for Homebrew"
if command -v brew >/dev/null 2>&1; then
  skip "$(command -v brew)"
else
  echo "    Not found; installing Homebrew non-interactively..."
  NONINTERACTIVE=1 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
  # Homebrew installs to a different prefix on Apple Silicon vs Intel;
  # pick whichever exists so `brew` is usable for the rest of this script.
  if [ -x /opt/homebrew/bin/brew ]; then
    eval "$(/opt/homebrew/bin/brew shellenv)"
  elif [ -x /usr/local/bin/brew ]; then
    eval "$(/usr/local/bin/brew shellenv)"
  fi
  ok "Homebrew installed"
fi

step "Checking for Rust (cargo/rustc)"
if command -v cargo >/dev/null 2>&1; then
  skip "$(command -v cargo)"
else
  echo "    Not found; installing via rustup (non-interactive, default profile)..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
  ok "Rust installed"
fi

step "Checking for Node.js/npm"
if command -v npm >/dev/null 2>&1; then
  skip "$(command -v npm)"
else
  echo "    Not found; installing via Homebrew..."
  brew install node
  ok "Node.js installed"
fi

step "Building the Rust core + daemon (cargo build --release --workspace)"
(cd "$REPO_ROOT" && cargo build --release --workspace)
ok "Rust build complete"

step "Building the desktop UI (npm install + tauri build)"
(cd "$REPO_ROOT/app" && npm install && npm run tauri build)

step "Done"
BUNDLE_DIR="$REPO_ROOT/app/src-tauri/target/release/bundle"
echo "Installer(s)/app bundle should be under:"
echo "  $BUNDLE_DIR"
if [ -d "$BUNDLE_DIR" ]; then
  find "$BUNDLE_DIR" \( -name "*.dmg" -o -name "*.app" \) -maxdepth 3 -print | sed 's/^/  - /'
fi
