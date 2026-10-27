#!/usr/bin/env bash
# Builds, tests, and installs the Codeplug Android app.
#
# One command produces a complete APK: the Gradle build's cargoNdk task
# cross-compiles codeplugme-core into jniLibs before anything else, so there is no
# separate Rust step to remember.
#
# Usage:
#   ./build.sh                 debug APK (default)
#   ./build.sh release         release APK (unsigned)
#   ./build.sh test            host JVM unit tests, no device or radio needed
#   ./build.sh install         build the debug APK and adb install it
#   ./build.sh doctor          compare the APK's demands against the attached device
#   ./build.sh bindings        regenerate the Kotlin bindings from core/src/api.rs
#   ./build.sh clean           remove build outputs
#
# Anything else is passed straight through to gradlew as a task name.

set -euo pipefail

INVOKE_DIR="$PWD"
cd "$(dirname "$0")"
ROOT="$(cd .. && pwd)"

die() { echo "error: $*" >&2; exit 1; }

# ---------------------------------------------------------------- toolchain

# Gradle needs a JDK 17+. Prefer whatever the user already has, then the JDK
# Android Studio bundles, which is present on any machine that can build this
# at all and saves a separate install.
find_java_home() {
    if [ -n "${JAVA_HOME:-}" ] && [ -x "$JAVA_HOME/bin/java" ]; then
        echo "$JAVA_HOME"; return
    fi
    if [ -x /usr/libexec/java_home ] && /usr/libexec/java_home -v 17+ >/dev/null 2>&1; then
        /usr/libexec/java_home -v 17+; return
    fi
    for jbr in \
        "/Applications/Android Studio.app/Contents/jbr/Contents/Home" \
        "$HOME/Applications/Android Studio.app/Contents/jbr/Contents/Home"
    do
        [ -x "$jbr/bin/java" ] && { echo "$jbr"; return; }
    done
    return 1
}

JAVA_HOME="$(find_java_home)" || die \
    "no JDK 17+ found. Install one (brew install openjdk@17) or install Android Studio."
export JAVA_HOME
export PATH="$JAVA_HOME/bin:$HOME/.cargo/bin:$PATH"

# The SDK path Gradle reads from local.properties, which is gitignored, so a
# fresh clone has none and fails with an unhelpful "SDK location not found".
ANDROID_HOME="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}"
[ -d "$ANDROID_HOME" ] || die \
    "Android SDK not found at $ANDROID_HOME. Set ANDROID_HOME to your SDK location."
export ANDROID_HOME
if [ ! -f local.properties ]; then
    echo "sdk.dir=$ANDROID_HOME" > local.properties
    echo "==> wrote local.properties (sdk.dir=$ANDROID_HOME)"
fi

# ------------------------------------------------------- rust cross-compile

# Only checked for tasks that actually cross-compile. `test` builds a host
# dylib instead and needs none of this.
check_rust_toolchain() {
    command -v cargo >/dev/null 2>&1 || die "cargo not found. Install Rust from https://rustup.rs"
    command -v cargo-ndk >/dev/null 2>&1 || die \
        "cargo-ndk not found. Install it with: cargo install cargo-ndk"

    local missing=()
    for target in aarch64-linux-android x86_64-linux-android; do
        rustup target list --installed 2>/dev/null | grep -qx "$target" || missing+=("$target")
    done
    [ ${#missing[@]} -eq 0 ] || die \
        "missing Rust targets. Install with: rustup target add ${missing[*]}"

    [ -n "${ANDROID_NDK_HOME:-}" ] || [ -d "$ANDROID_HOME/ndk" ] || die \
        "no NDK found under $ANDROID_HOME/ndk. Install one from Android Studio's SDK Manager."
}

# ------------------------------------------------------------------- tasks

# Path of the newest APK for a build type, or empty.
newest_apk() {
    find "app/build/outputs/apk/$1" -name '*.apk' -type f -print0 2>/dev/null \
        | xargs -0 ls -t 2>/dev/null | head -n1 || true
}

# Print an APK path relative to where the user invoked us, so it pastes.
report_apk() {
    local apk="$1"
    [ -n "$apk" ] && [ -f "$apk" ] || die "build finished but no APK was produced."
    # Relative to where the user invoked us so it pastes straight into an adb
    # command, but absolute when that would mean climbing out of the tree.
    local rel
    rel="$(python3 -c '
import os, sys
rel = os.path.relpath(sys.argv[1], sys.argv[2])
print(rel if not rel.startswith("..") else sys.argv[1])
' "$PWD/$apk" "$INVOKE_DIR")"
    local size
    size="$(python3 -c 'import os,sys; print(f"{os.path.getsize(sys.argv[1])/1048576:.1f} MB")' "$apk")"
    echo ""
    echo "==> Build succeeded: $rel ($size)"
    echo "    Install with:  adb install -r \"$rel\""
}

CMD="${1:-debug}"

case "$CMD" in
    debug)
        check_rust_toolchain
        echo "==> ./gradlew assembleDebug"
        ./gradlew assembleDebug
        report_apk "$(newest_apk debug)"
        ;;

    release)
        check_rust_toolchain
        echo "==> ./gradlew assembleRelease"
        ./gradlew assembleRelease
        report_apk "$(newest_apk release)"
        if [ ! -f key.properties ]; then
            echo ""
            echo "    Note: unsigned (no android/key.properties). Sign it before"
            echo "    installing on a device that does not allow debug builds."
        fi
        ;;

    test)
        # No cross-compile: the Gradle hostCoreLib task builds codeplugme-core for
        # this machine instead, so the tests exercise the real core through JNA.
        echo "==> ./gradlew testDebugUnitTest"
        ./gradlew testDebugUnitTest
        echo ""
        echo "==> Report: android/app/build/reports/tests/testDebugUnitTest/index.html"
        ;;

    install)
        # adb rather than `gradlew installDebug`, because adb prints the actual
        # INSTALL_FAILED_* reason. A silent "install failed" is the single most
        # common thing to get stuck on here, so the reason is worth surfacing
        # and, where it is a known one, explaining.
        ADB="${ADB:-$(command -v adb || true)}"
        [ -n "$ADB" ] && [ -x "$ADB" ] || ADB="$ANDROID_HOME/platform-tools/adb"
        [ -x "$ADB" ] || die "adb not found. Add \$ANDROID_HOME/platform-tools to PATH."

        check_rust_toolchain
        echo "==> ./gradlew assembleDebug"
        ./gradlew assembleDebug
        apk="$(newest_apk debug)"
        [ -n "$apk" ] || die "build finished but no APK was produced."

        if [ -z "$("$ADB" devices | sed '1d' | grep -w device || true)" ]; then
            die "no device is connected and authorised. Check: $ADB devices"
        fi

        echo "==> adb install -r"
        out="$("$ADB" install -r "$apk" 2>&1)" && status=0 || status=$?
        echo "$out"
        if [ "$status" -eq 0 ] && ! echo "$out" | grep -q "INSTALL_FAILED"; then
            echo ""
            echo "==> Installed. Plug the radio in through a USB OTG adapter to use it."
            exit 0
        fi

        echo ""
        case "$out" in
            *INSTALL_FAILED_UPDATE_INCOMPATIBLE*|*INSTALL_FAILED_VERSION_DOWNGRADE*|*signatures\ do\ not\ match*)
                echo "    A different build of me.faulk.codeplugme is already installed and was"
                echo "    signed with a different debug key. Uninstall it and try again:"
                echo "        $ADB uninstall me.faulk.codeplugme"
                ;;
            *INSTALL_FAILED_NO_MATCHING_ABIS*)
                echo "    This APK carries arm64-v8a and x86_64 only. Your device is a"
                echo "    different architecture and is not supported."
                ;;
            *INSTALL_FAILED_OLDER_SDK*)
                echo "    This app needs Android 10 (API 29) or newer."
                ;;
            *INSTALL_FAILED_INSUFFICIENT_STORAGE*)
                echo "    Not enough free space on the device."
                ;;
            *INSTALL_FAILED_USER_RESTRICTED*)
                echo "    The device refused the install. On many phones USB installs need"
                echo "    'Install via USB' enabled in Developer options, and MIUI/HyperOS"
                echo "    also requires turning off 'Verify apps over USB'."
                ;;
            *)
                echo "    Install failed. If the reason above is empty, try installing by"
                echo "    hand to see the device's own message:"
                echo "        $ADB install -r \"$apk\""
                ;;
        esac
        exit 1
        ;;

    doctor)
        # Compares what the APK demands against what the attached device
        # offers. Exists because an install refused by the device's package
        # installer often reports nothing at all, and the mismatch is usually
        # one of these four lines.
        ADB="${ADB:-$(command -v adb || true)}"
        [ -n "$ADB" ] && [ -x "$ADB" ] || ADB="$ANDROID_HOME/platform-tools/adb"

        apk="$(newest_apk debug)"
        if [ -n "$apk" ]; then
            bt="$(/bin/ls "$ANDROID_HOME/build-tools" 2>/dev/null | sort -V | tail -1)"
            aapt="$ANDROID_HOME/build-tools/$bt/aapt2"
            echo "APK: $apk"
            if [ -x "$aapt" ]; then
                "$aapt" dump badging "$apk" 2>/dev/null \
                    | grep -E "^package:|^minSdkVersion|^native-code|uses-feature" \
                    | sed 's/^/  /'
            fi
        else
            echo "APK: none built yet (run ./build.sh first)"
        fi

        echo ""
        if [ ! -x "$ADB" ]; then
            echo "Device: adb not found, cannot check."
            exit 0
        fi
        if [ -z "$("$ADB" devices | sed '1d' | grep -w device || true)" ]; then
            echo "Device: none connected and authorised."
            echo "  Check the cable, and confirm USB debugging is on and this"
            echo "  computer is authorised on the phone."
            exit 0
        fi

        echo "Device:"
        printf '  model:      %s\n' "$("$ADB" shell getprop ro.product.model | tr -d '\r')"
        printf '  android:    %s (API %s)\n' \
            "$("$ADB" shell getprop ro.build.version.release | tr -d '\r')" \
            "$("$ADB" shell getprop ro.build.version.sdk | tr -d '\r')"
        printf '  abis:       %s\n' "$("$ADB" shell getprop ro.product.cpu.abilist | tr -d '\r')"
        if "$ADB" shell pm list features 2>/dev/null | tr -d '\r' | grep -qx "feature:android.hardware.usb.host"; then
            printf '  usb host:   declared\n'
        else
            printf '  usb host:   NOT declared (OTG may still work; the app no longer requires it)\n'
        fi
        existing="$("$ADB" shell pm list packages me.faulk.codeplugme 2>/dev/null | tr -d '\r')"
        if [ -n "$existing" ]; then
            printf '  installed:  yes, already present. If installs fail, uninstall first:\n'
            printf '                %s uninstall me.faulk.codeplugme\n' "$ADB"
        else
            printf '  installed:  no\n'
        fi
        ;;

    bindings)
        echo "==> ../scripts/gen-bindings.sh"
        "$ROOT/scripts/gen-bindings.sh"
        ;;

    clean)
        echo "==> ./gradlew clean"
        ./gradlew clean
        rm -rf app/src/main/jniLibs
        echo "==> Removed build outputs and cross-compiled libraries."
        ;;

    *)
        echo "==> ./gradlew $CMD"
        ./gradlew "$CMD"
        ;;
esac
