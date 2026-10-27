#!/bin/bash
# Unified build for CodeplugMe: builds the codeplugme-core Rust staticlib
# (release), regenerates the Xcode project with XcodeGen, and builds the app
# with xcodebuild.
#
# Usage:
#   ./build.sh              Local Debug build, host arch only, ad-hoc signed.
#   ./build.sh release      Universal (arm64 + x86_64) Release build (set
#                           ARCHS=arm64 for Apple silicon only), signed
#                           with a Developer ID cert if one is available,
#                           packaged as a DMG in dist/, notarized when a
#                           notarytool keychain profile is provided.
#
# Release environment variables:
#   SIGNING_IDENTITY  Codesign identity. Defaults to the first "Developer ID
#                     Application" cert in the keychain, or "-" (ad-hoc) if
#                     there is none.
#   NOTARY_PROFILE    notarytool keychain profile name. When set, the DMG is
#                     submitted to Apple and stapled. Create one with:
#                       xcrun notarytool store-credentials <name> \
#                         --apple-id <id> --team-id <team> --password <app-pw>

set -euo pipefail

PROJECT_NAME="CodeplugMe"
COMMAND="${1:-debug}"

cd "$(dirname "$0")"
export SRCROOT="$(pwd)"
BUILD_DIR="$SRCROOT/build"
DIST_DIR="$SRCROOT/dist"
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

case "$COMMAND" in
    debug)
        CONFIGURATION="${CONFIGURATION:-Debug}"
        BUILD_ARCHS="$(uname -m)"
        ONLY_ACTIVE="YES"
        ;;
    release)
        CONFIGURATION="Release"
        BUILD_ARCHS="${ARCHS:-arm64 x86_64}"
        ONLY_ACTIVE="NO"
        ;;
    *)
        echo "usage: $0 [debug|release]" >&2
        exit 1
        ;;
esac

# Codesign identity: explicit override, else a Developer ID cert, else ad-hoc.
if [ "$COMMAND" = "release" ]; then
    if [ -z "${SIGNING_IDENTITY:-}" ]; then
        SIGNING_IDENTITY=$(security find-identity -v -p codesigning \
            | grep "Developer ID Application" \
            | head -n 1 \
            | sed -E 's/.*"(.*)"/\1/')
    fi
    if [ -z "$SIGNING_IDENTITY" ]; then
        echo "warning: no Developer ID Application cert found; signing ad-hoc." >&2
        echo "         The app will be Gatekeeper-blocked on other Macs." >&2
        SIGNING_IDENTITY="-"
    fi
else
    SIGNING_IDENTITY="-"
fi

# 1. Rust static library (one slice per arch, lipo'd when universal).
ARCHS="$BUILD_ARCHS" ./build-rust.sh

# 2. Xcode project from project.yml.
if ! command -v xcodegen >/dev/null 2>&1; then
    echo "error: xcodegen not found (brew install xcodegen)" >&2
    exit 1
fi
xcodegen generate --spec project.yml

# 3. Swift app.
echo "Building $PROJECT_NAME ($CONFIGURATION, $BUILD_ARCHS) with xcodebuild..."
if ! xcodebuild \
    -project "$PROJECT_NAME.xcodeproj" \
    -scheme "$PROJECT_NAME" \
    -configuration "$CONFIGURATION" \
    -derivedDataPath "$BUILD_DIR" \
    ARCHS="$BUILD_ARCHS" \
    ONLY_ACTIVE_ARCH="$ONLY_ACTIVE" \
    CODE_SIGN_IDENTITY="-" \
    build > build_output.log 2>&1; then
    echo "xcodebuild failed; last 30 lines of build_output.log:" >&2
    tail -n 30 build_output.log >&2
    exit 1
fi

echo "Build succeeded."
# SYMROOT puts products in build/<configuration>; fall back to a search if a
# future project.yml change moves them.
APP_PATH="$BUILD_DIR/$CONFIGURATION/$PROJECT_NAME.app"
if [ ! -d "$APP_PATH" ]; then
    APP_PATH=$(find "$BUILD_DIR" -name "$PROJECT_NAME.app" -type d | head -n 1)
fi
echo "App: $APP_PATH"

if [ "$COMMAND" != "release" ]; then
    exit 0
fi

# 4. Re-sign for distribution: hardened runtime, secure timestamp. Required for
#    notarization; skipped when falling back to an ad-hoc signature.
VERSION=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" \
    "$APP_PATH/Contents/Info.plist")
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"
STAGED_APP="$DIST_DIR/$PROJECT_NAME.app"
cp -R "$APP_PATH" "$STAGED_APP"

if [ "$SIGNING_IDENTITY" = "-" ]; then
    codesign --force --sign - "$STAGED_APP"
else
    echo "Signing with: $SIGNING_IDENTITY"
    codesign --force --options runtime --timestamp \
        --sign "$SIGNING_IDENTITY" "$STAGED_APP"
    codesign --verify --strict --verbose=2 "$STAGED_APP"
fi

# 5. DMG.
DMG_PATH="$DIST_DIR/$PROJECT_NAME-$VERSION.dmg"
STAGE_DIR="$DIST_DIR/dmg-stage"
mkdir -p "$STAGE_DIR"
cp -R "$STAGED_APP" "$STAGE_DIR/"
ln -s /Applications "$STAGE_DIR/Applications"
hdiutil create -volname "$PROJECT_NAME" -srcfolder "$STAGE_DIR" \
    -ov -format UDZO "$DMG_PATH" > /dev/null
rm -rf "$STAGE_DIR"
echo "DMG: $DMG_PATH"

# 6. Notarize and staple, if credentials were provided.
if [ -n "${NOTARY_PROFILE:-}" ] && [ "$SIGNING_IDENTITY" != "-" ]; then
    echo "Submitting to Apple for notarization (this can take a few minutes)..."
    xcrun notarytool submit "$DMG_PATH" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$DMG_PATH"
    xcrun stapler staple "$STAGED_APP"
    echo "Notarized and stapled."
elif [ "$SIGNING_IDENTITY" != "-" ]; then
    echo "Not notarized (NOTARY_PROFILE unset). The DMG will trip Gatekeeper on"
    echo "other Macs until it is notarized."
fi

echo "Release artifacts in $DIST_DIR"
