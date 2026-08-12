#!/usr/bin/env bash
# Double-click this file in Finder to run the automated setup in a new
# Terminal window. (Finder runs .command files as shell scripts directly;
# this thin wrapper just locates and runs the real script next to it.)
cd "$(dirname "${BASH_SOURCE[0]}")"
bash setup-macos.sh
echo
read -r -p "Press Enter to close this window..."
