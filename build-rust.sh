#!/bin/bash
# Builds the codeplugme-core Rust static library (release) and copies it next to
# the Xcode project so the CodeplugMe target can link it. Runs standalone or
# as an Xcode pre-build phase (where Xcode sets SRCROOT/ARCHS).
#
# One slice is built per requested arch and the slices are lipo'd together, so a
# universal build (ARCHS="arm64 x86_64") yields a universal static library.

set -euo pipefail

SRCROOT="${SRCROOT:-$(cd "$(dirname "$0")" && pwd)}"
export PATH="$HOME/.cargo/bin:$PATH"

# Requested archs come from Xcode's ARCHS when present, else the host arch.
ARCH_LIST="${ARCHS:-$(uname -m)}"

# Maps an Xcode arch name to its Rust target triple.
rust_target_for_arch() {
    case "$1" in
        arm64|aarch64) echo "aarch64-apple-darwin" ;;
        x86_64) echo "x86_64-apple-darwin" ;;
        *) echo "error: unsupported arch '$1'" >&2; return 1 ;;
    esac
}

cd "$SRCROOT"

SLICES=()
for arch in $ARCH_LIST; do
    target="$(rust_target_for_arch "$arch")"
    if ! rustup target list --installed 2>/dev/null | grep -qx "$target"; then
        echo "Installing Rust target $target..."
        rustup target add "$target"
    fi
    echo "Building codeplugme-core (release) for $target..."
    cargo build --release -p codeplugme-core --target "$target"
    SLICES+=("target/$target/release/libcodeplugme_core.a")
done

if [ "${#SLICES[@]}" -eq 1 ]; then
    cp "${SLICES[0]}" "$SRCROOT/libcodeplugme_core.a"
else
    lipo -create "${SLICES[@]}" -output "$SRCROOT/libcodeplugme_core.a"
fi
echo "Copied libcodeplugme_core.a ($(lipo -archs "$SRCROOT/libcodeplugme_core.a")) to $SRCROOT"
