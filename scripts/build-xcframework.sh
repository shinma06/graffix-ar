#!/usr/bin/env bash
#
# Build the graffix-core XCFramework + Swift bindings for the iOS app.
#
# Produces:
#   core/build/GraffixCore.xcframework   (static-lib slices: ios-arm64 + ios-sim-arm64)
#   Graffix-AR/Generated/graffix_core.swift   (UniFFI Swift bindings, app target)
#
# Prerequisites (macOS only — this needs Xcode + Rust):
#   rustup target add aarch64-apple-ios aarch64-apple-ios-sim
#   Xcode 26+ command line tools selected (xcode-select)
#
# Usage: ./scripts/build-xcframework.sh [--debug]

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CORE="$ROOT/core"
BUILD_DIR="$CORE/build"
BINDINGS_DIR="$BUILD_DIR/bindings"
GEN_DIR="$ROOT/Graffix-AR/Generated"
LIB_NAME="libgraffix_core.a"
PROFILE="release"
CARGO_FLAGS="--release"

if [[ "${1:-}" == "--debug" ]]; then
    PROFILE="debug"
    CARGO_FLAGS=""
fi

TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim)

echo "==> Building graffix-core ($PROFILE)"
for target in "${TARGETS[@]}"; do
    cargo build $CARGO_FLAGS --target "$target" --manifest-path "$CORE/Cargo.toml"
done

echo "==> Generating Swift bindings"
mkdir -p "$BINDINGS_DIR" "$GEN_DIR"
# Any slice carries identical UniFFI metadata; use the device build.
# uniffi-bindgen (since 0.30) resolves `cargo metadata` from the current
# working directory, so run it from inside the core crate.
(cd "$CORE" && cargo run --quiet \
    --features cli --bin uniffi-bindgen -- \
    generate --library --language swift \
    --out-dir "$BINDINGS_DIR" \
    "$CORE/target/aarch64-apple-ios/$PROFILE/$LIB_NAME")

# UniFFI emits: <ns>.swift, <ns>FFI.h, <ns>FFI.modulemap
HEADERS_DIR="$BUILD_DIR/headers"
mkdir -p "$HEADERS_DIR"
cp "$BINDINGS_DIR/graffix_coreFFI.h" "$HEADERS_DIR/"
cp "$BINDINGS_DIR/graffix_coreFFI.modulemap" "$HEADERS_DIR/module.modulemap"

echo "==> Assembling GraffixCore.xcframework"
rm -rf "$BUILD_DIR/GraffixCore.xcframework"
xcodebuild -create-xcframework \
    -library "$CORE/target/aarch64-apple-ios/$PROFILE/$LIB_NAME" -headers "$HEADERS_DIR" \
    -library "$CORE/target/aarch64-apple-ios-sim/$PROFILE/$LIB_NAME" -headers "$HEADERS_DIR" \
    -output "$BUILD_DIR/GraffixCore.xcframework"

# Keep the generated Swift bindings next to the app sources so the Xcode
# project (synchronized folder group) picks them up automatically.
cp "$BINDINGS_DIR/graffix_core.swift" "$GEN_DIR/graffix_core.swift"

echo "==> Done"
echo "    XCFramework : $BUILD_DIR/GraffixCore.xcframework"
echo "    Bindings    : $GEN_DIR/graffix_core.swift"
