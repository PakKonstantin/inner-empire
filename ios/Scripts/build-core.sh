#!/usr/bin/env bash
#
# Build the shared core for iOS and regenerate the Swift bindings.
#
#   ./ios/Scripts/build-core.sh              release, device + both simulators
#   ./ios/Scripts/build-core.sh --debug      faster, for iterating
#   ./ios/Scripts/build-core.sh --bindings   bindings only; runs anywhere
#
# Everything but --bindings needs macOS with Xcode: `ie-ffi` pulls in
# `libsqlite3-sys`, which compiles SQLite from C and therefore needs the iOS
# SDK. The bindings step is pure Rust and runs on any host, which is how the
# generated Swift stays reviewable without a Mac.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CARGO_ROOT="$REPO_ROOT/src-tauri"
# Overridable so a freshness check can generate into a temporary directory and
# compare, rather than overwriting the files it is meant to be checking.
GENERATED="${IE_BINDINGS_OUT:-$REPO_ROOT/ios/Generated}"
FRAMEWORKS="$REPO_ROOT/ios/Frameworks"
XCFRAMEWORK="$FRAMEWORKS/InnerEmpireCore.xcframework"

PROFILE="release"
PROFILE_FLAG="--release"
BINDINGS_ONLY=0

for arg in "$@"; do
  case "$arg" in
    --debug)    PROFILE="debug"; PROFILE_FLAG="" ;;
    --bindings) BINDINGS_ONLY=1 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

TARGETS=(
  aarch64-apple-ios          # device
  aarch64-apple-ios-sim      # simulator on Apple silicon
  x86_64-apple-ios           # simulator on Intel
)

# ---------------------------------------------------------------- bindings --
#
# Generated from a host-target cdylib. The bindings describe the interface,
# which is the same for every target, so they do not need a cross build — and
# generating them here means a change to the Rust API shows up in a diff even
# when nobody has a Mac to hand.

# Always the debug cdylib, whatever profile the library is built with. The
# generator reads UniFFI's metadata out of the binary, and the release profile
# sets `strip = true` — which is right for a shipped binary and fatal here.
# The interface is identical either way; only the machine code differs.
echo "==> building the host cdylib for binding generation"
cargo build --manifest-path "$CARGO_ROOT/Cargo.toml" -p ie-ffi

HOST_LIB=""
for candidate in \
  "$CARGO_ROOT/target/debug/libie_ffi.dylib" \
  "$CARGO_ROOT/target/debug/libie_ffi.so"
do
  [ -f "$candidate" ] && HOST_LIB="$candidate" && break
done
[ -n "$HOST_LIB" ] || { echo "no host cdylib found under target/debug" >&2; exit 1; }

echo "==> generating Swift bindings"
mkdir -p "$GENERATED"
# From the cargo root: uniffi-bindgen shells out to `cargo metadata` in the
# working directory to find the crate the library belongs to, and the repository
# root has no manifest.
#
# --no-format because swift-format is part of Xcode. The committed bindings are
# read as generated output, not as hand-written source; on a Mac the generator
# formats them and the result is identical either way.
( cd "$CARGO_ROOT" && \
  cargo run --manifest-path "$CARGO_ROOT/Cargo.toml" -p ie-ffi --bin uniffi-bindgen -- \
    generate "$HOST_LIB" --language swift --out-dir "$GENERATED" --no-format )

# The module map names the header; Xcode imports the module, not the header.
echo "==> bindings written to ios/Generated"
ls -1 "$GENERATED"

if [ "$BINDINGS_ONLY" -eq 1 ]; then
  echo "==> bindings only; stopping here"
  exit 0
fi

# ------------------------------------------------------------- the library --

if [ "$(uname -s)" != "Darwin" ]; then
  cat >&2 <<'MSG'

Stopping: building for iOS needs macOS with Xcode installed. `ie-ffi` depends on
`rusqlite` with the `bundled` feature, which compiles SQLite from C and so needs
the iOS SDK's sysroot; there is no way around that on another host.

Run with --bindings to regenerate the Swift only.
MSG
  exit 1
fi

for target in "${TARGETS[@]}"; do
  echo "==> cargo build --target $target"
  rustup target add "$target" >/dev/null 2>&1 || true
  cargo build --manifest-path "$CARGO_ROOT/Cargo.toml" -p ie-ffi $PROFILE_FLAG --target "$target"
done

# The two simulator slices share a platform, so they must be one fat library or
# `create-xcframework` refuses them.
SIM_DIR="$CARGO_ROOT/target/ios-sim-fat/$PROFILE"
mkdir -p "$SIM_DIR"
lipo -create \
  "$CARGO_ROOT/target/aarch64-apple-ios-sim/$PROFILE/libie_ffi.a" \
  "$CARGO_ROOT/target/x86_64-apple-ios/$PROFILE/libie_ffi.a" \
  -output "$SIM_DIR/libie_ffi.a"

# The framework carries the header and module map, so the app target needs no
# search paths of its own.
HEADERS="$CARGO_ROOT/target/ios-headers"
rm -rf "$HEADERS" && mkdir -p "$HEADERS"
cp "$GENERATED"/*.h "$HEADERS/"
cp "$GENERATED"/*.modulemap "$HEADERS/module.modulemap"

rm -rf "$XCFRAMEWORK"
mkdir -p "$FRAMEWORKS"
xcodebuild -create-xcframework \
  -library "$CARGO_ROOT/target/aarch64-apple-ios/$PROFILE/libie_ffi.a" -headers "$HEADERS" \
  -library "$SIM_DIR/libie_ffi.a" -headers "$HEADERS" \
  -output "$XCFRAMEWORK"

echo "==> $XCFRAMEWORK"
