#!/usr/bin/env bash
# Generates the UniFFI Kotlin bindings for the Android app:
#   android/app/src/main/java/uniffi/codeplugme_core/codeplugme_core.kt
#
# Builds the codeplugme-core cdylib first and runs uniffi-bindgen in library mode
# against it, so the bindings always match the compiled surface.
#
# The macOS app is NOT generated from here. It links the hand-written C ABI in
# core/src/ffi.rs against core/include/codeplugme_core.h, and is untouched by this
# script.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

KOTLIN_OUT="$ROOT/android/app/src/main/java"

case "$(uname -s)" in
    Darwin) LIB="$ROOT/target/release/libcodeplugme_core.dylib" ;;
    *)      LIB="$ROOT/target/release/libcodeplugme_core.so" ;;
esac

# --features cli implies uniffi-api and adds uniffi's bindgen CLI. The desktop
# feature stays on so the host build is the same one the CLI and tests use.
echo "==> building codeplugme-core cdylib (release)"
cargo build -p codeplugme-core --release --features cli

if [[ ! -f "$LIB" ]]; then
    echo "error: expected cdylib at $LIB after build" >&2
    exit 1
fi

mkdir -p "$KOTLIN_OUT"

echo "==> generating Kotlin bindings into $KOTLIN_OUT"
cargo run -p codeplugme-core --release --features cli --bin uniffi-bindgen -- \
    generate --library "$LIB" --language kotlin --out-dir "$KOTLIN_OUT"

GENERATED="$KOTLIN_OUT/uniffi/codeplugme_core/codeplugme_core.kt"
if [[ ! -f "$GENERATED" ]]; then
    echo "error: expected generated file missing: $GENERATED" >&2
    exit 1
fi

echo "==> bindings generated:"
ls -1 "$KOTLIN_OUT/uniffi/codeplugme_core"
