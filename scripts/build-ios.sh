#!/usr/bin/env bash
# Build the FFCore.xcframework consumed by ios/uniffi.
#
# Requirements (run on macOS with Xcode CLT):
#   rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
#   xcode-select --install
#
# Outputs:
#   ios/uniffi/Frameworks/FFCore.xcframework
#   ios/uniffi/Generated/FFCore.swift               (Swift bindings)
#   ios/uniffi/Generated/FFCoreFFI.modulemap
#
# Then `cd ios/uniffi && xcodegen generate && open GroksFantasyCore.xcodeproj`.

set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "This script must run on macOS." >&2
    exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PACKAGE=ff-ffi
LIBNAME=libff_ffi
SWIFTNAME=FFCore

OUT="$ROOT/ios/uniffi"
mkdir -p "$OUT/Frameworks" "$OUT/Generated"

echo "==> Building Rust staticlib for iOS device + simulator targets"
cargo build --release --package "$PACKAGE" --target aarch64-apple-ios
cargo build --release --package "$PACKAGE" --target aarch64-apple-ios-sim
cargo build --release --package "$PACKAGE" --target x86_64-apple-ios

DEVICE_LIB="target/aarch64-apple-ios/release/${LIBNAME}.a"
SIM_ARM="target/aarch64-apple-ios-sim/release/${LIBNAME}.a"
SIM_X86="target/x86_64-apple-ios/release/${LIBNAME}.a"

SIM_OUT="target/ios-sim-fat"
mkdir -p "$SIM_OUT"
SIM_LIB="$SIM_OUT/${LIBNAME}.a"
echo "==> lipo simulator slices into $SIM_LIB"
lipo -create "$SIM_ARM" "$SIM_X86" -output "$SIM_LIB"

echo "==> Generating Swift bindings via uniffi-bindgen"
HEADER_DIR="$(mktemp -d)"
cargo run --release --package "$PACKAGE" --bin uniffi-bindgen -- generate \
    --library "$DEVICE_LIB" \
    --language swift \
    --out-dir "$HEADER_DIR"

# UniFFI generates: <module>.swift, <module>FFI.h, <module>FFI.modulemap
cp "$HEADER_DIR"/*.swift     "$OUT/Generated/"
cp "$HEADER_DIR"/*FFI.h      "$OUT/Generated/" 2>/dev/null || true
cp "$HEADER_DIR"/*.modulemap "$OUT/Generated/" 2>/dev/null || true

# Build the framework directories that xcodebuild -create-xcframework expects.
echo "==> Assembling XCFramework"
FRAMEWORK="$OUT/Frameworks/${SWIFTNAME}.xcframework"
rm -rf "$FRAMEWORK"

# Each "library" gets its own headers directory containing the modulemap.
DEVICE_HEADERS="$(mktemp -d)"
SIM_HEADERS="$(mktemp -d)"
cp "$HEADER_DIR"/*FFI.h      "$DEVICE_HEADERS/" 2>/dev/null || true
cp "$HEADER_DIR"/*.modulemap "$DEVICE_HEADERS/module.modulemap" 2>/dev/null || true
cp "$HEADER_DIR"/*FFI.h      "$SIM_HEADERS/"    2>/dev/null || true
cp "$HEADER_DIR"/*.modulemap "$SIM_HEADERS/module.modulemap"    2>/dev/null || true

xcodebuild -create-xcframework \
    -library "$DEVICE_LIB" -headers "$DEVICE_HEADERS" \
    -library "$SIM_LIB"    -headers "$SIM_HEADERS" \
    -output "$FRAMEWORK"

echo
echo "Built:"
echo "  $FRAMEWORK"
echo "  $OUT/Generated/*.swift"
echo
echo "Next: cd ios/uniffi && xcodegen generate && open GroksFantasyCore.xcodeproj"
