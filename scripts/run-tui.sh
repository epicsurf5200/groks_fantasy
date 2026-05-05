#!/usr/bin/env bash
# Build (release) and launch the terminal UI on macOS / Linux.
set -euo pipefail
cargo build --release --bin ff
exec ./target/release/ff "$@"
