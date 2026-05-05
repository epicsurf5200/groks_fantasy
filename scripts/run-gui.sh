#!/usr/bin/env bash
# Build (release) and launch the desktop GUI on macOS / Linux.
set -euo pipefail
cargo build --release --bin ff-gui
exec ./target/release/ff-gui "$@"
