#!/bin/bash
# Builds both release deliverables into dist/: the Apple silicon (arm64) macOS
# DMG and the Android release APK. Each half is the platform's own build script,
# so signing and notarization behave exactly as documented there:
#
#   macOS    ./build.sh release   (SIGNING_IDENTITY, NOTARY_PROFILE)
#   Android  android/build.sh release   (signed when android/key.properties exists)
#
# Usage:
#   ./build-all.sh            both
#   ./build-all.sh macos      macOS DMG only
#   ./build-all.sh android    Android APK only

set -euo pipefail

cd "$(dirname "$0")"
DIST_DIR="$(pwd)/dist"
TARGET="${1:-all}"

# Runs the macOS release build for arm64 only. build.sh release wipes dist/,
# so this must run before the APK is copied in.
build_macos() {
    echo "==> macOS (arm64)"
    ARCHS=arm64 ./build.sh release
}

# Runs the Android release build and copies the newest APK into dist/.
build_android() {
    echo "==> Android"
    android/build.sh release
    local apk
    apk="$(ls -t android/app/build/outputs/apk/release/*.apk 2>/dev/null | head -n 1)"
    if [ -z "$apk" ]; then
        echo "error: Android build finished but no release APK was found." >&2
        exit 1
    fi
    local version
    version="$(sed -nE 's/.*versionName = "(.*)".*/\1/p' android/app/build.gradle.kts)"
    mkdir -p "$DIST_DIR"
    cp "$apk" "$DIST_DIR/CodeplugMe-$version.apk"
}

case "$TARGET" in
    all)     build_macos; build_android ;;
    macos)   build_macos ;;
    android) build_android ;;
    *)       echo "usage: $0 [all|macos|android]" >&2; exit 1 ;;
esac

echo ""
echo "Artifacts in $DIST_DIR:"
ls -1 "$DIST_DIR" | grep -E '\.(dmg|apk)$'
