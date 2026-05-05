#!/usr/bin/env bash
# install.sh — macOS / Linux installer.
#
# Installs the Rust toolchain (if missing) and builds release binaries
# `ff` (CLI/TUI) and `ff-gui` (desktop GUI) into ./target/release/.
set -euo pipefail

if ! command -v cargo >/dev/null 2>&1; then
    echo "Rust toolchain not found — installing via rustup."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    # shellcheck disable=SC1090
    source "$HOME/.cargo/env"
fi

OS="$(uname -s)"
case "$OS" in
    Darwin)
        echo "macOS detected — no extra system packages needed for the GUI."
        ;;
    Linux)
        echo "Linux detected — make sure libxkbcommon, libwayland, libGL and"
        echo "libxcb development headers are installed for the GUI."
        echo "  Debian/Ubuntu: sudo apt install -y build-essential libxkbcommon-dev libwayland-dev libgl1-mesa-dev libxcb1-dev pkg-config"
        echo "  Fedora:        sudo dnf install -y gcc libxkbcommon-devel wayland-devel mesa-libGL-devel libxcb-devel pkgconf-pkg-config"
        echo "  Arch:          sudo pacman -S --needed base-devel libxkbcommon wayland mesa libxcb pkgconf"
        ;;
    *)
        echo "Unrecognized OS '$OS' — proceeding optimistically."
        ;;
esac

echo "Building release binaries…"
cargo build --release --bins

echo
echo "Built:"
ls -1 target/release/ff target/release/ff-gui 2>/dev/null
echo
echo "Next steps:"
echo "  ./target/release/ff init sleeper       # or: espn / yahoo"
echo "  \$EDITOR config.yaml"
echo "  export ANTHROPIC_API_KEY=sk-ant-..."
echo "  ./target/release/ff-gui                # launch desktop GUI"
echo "  ./target/release/ff                    # launch terminal UI"
