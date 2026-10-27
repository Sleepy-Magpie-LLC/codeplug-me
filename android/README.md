# Codeplug for Android

Reads, edits, and writes ham radio codeplugs from an Android phone, over the
radio's USB programming cable. Same radios and same codeplug logic as the macOS
app in this repo: both are thin UI shells over the shared Rust core in `core/`.

Supported radios:

- **AnyTone AT-D878UVII Plus** (read and write both hardware-verified)
- **QYT KT-5000** (read and write both hardware-verified)

## What you need

- A phone that supports **USB host mode (OTG)**. Most do; some budget phones do not.
- A **USB OTG adapter** between the phone and the radio's programming cable.
- Android 10 (API 29) or newer.

The phone powers the cable, so a phone with a nearly flat battery may fail to
enumerate the adapter at all.

## Using it

The app opens on the **Radio** screen, which is the whole of the common job:

1. Plug the radio in. If it is an AnyTone, the app recognises it on sight.
2. If it is a **KT-5000, you have to pick the model yourself.** This is not a
   bug and it will never be fixed: that radio's programming cable is a stock
   Prolific PL2303 (`067b:2303`), the same chip in thousands of unrelated USB
   serial adapters. Claiming that USB id would tell every PL2303 owner they had
   a radio attached, and would let a codeplug be aimed at whatever else happened
   to be plugged in.
3. **Read from Radio** pulls the codeplug off and files it in your codeplugs.
4. **Write** next to any codeplug sends it back, after a confirmation that says
   where that codeplug came from, and on a KT-5000 reads the radio back
   afterwards to check what it kept.

Codeplugs live in the app's own storage rather than being picked from the file
system each time, so writing one is a couple of taps. Import and export move
`.bin` files in and out through the system file picker.

**Config** edits the open codeplug's radio settings, and **Channels** its
channels, including bulk edits across a multi-select. Everything else (zones,
scan lists, contacts, group lists, radio IDs) is under **More**.

**More > Add nearby repeaters** adds repeater listings around a place to the
open codeplug. It asks for coarse location to prefill where you are (deny it
and type a city, ZIP, or `lat, lon` instead), then queries the sources you turn
on: [Hearham](https://hearham.com), RadioID's DMR list (AnyTone only, since the
KT-5000 has no DMR), and [RepeaterBook](https://www.repeaterbook.com), which
needs your own app token and contact email, set from the gear icon and kept in
the app's private preferences.

- **Picking:** nothing is picked until you pick it, and the counter at the
  bottom says how many fit.
- **Merge or Clean:** Merge (the default) adds to what is there. Clean keeps
  settings and contacts but replaces every channel, zone, and scan list.
- **Add to codeplug** (the default) stages the picks into the open codeplug as
  one unsaved change, which Undo or Discard takes back.
- **Save as new** files the codeplug plus the picks as a new codeplug and opens
  it, leaving the open one alone.
- **Session state:** the screen keeps its search and results for as long as the
  app is running, with rows you've added marked "Already in codeplug". If you
  open a different codeplug, the results are cleared but the search is kept.

The app's only network use is this screen (`INTERNET`).

A channel row is a slot number in the left gutter, the name and both
frequencies beside it, and that channel's settings on the line below. On the
KT-5000 those settings lead with scan membership, and the filter above the list
narrows it to only the scanned channels or only the skipped ones. A channel
whose RX, TX and mode match another channel's (plus RX / TX tones for analog
modes, and color code and time slot for DMR modes) is tinted, with a "Duplicate of channel …" line naming the others. The
"Duplicates only" chip shows just those channels, sorted by RX then TX so each
group of twins sits together.

Edits are staged and do not touch the stored codeplug until you save. **Save**
is the disk icon on the Channels screen, or the button on the Radio screen's
"Editing" card, which also offers **Save As…** (files the staged bytes as a new
codeplug and leaves the one you started from alone) and **Discard changes**.

Writing to the radio sends the staged bytes, so a change you have not saved yet
still reaches the radio, which makes "tweak a channel, write it, try it" one
loop. Saving is a separate thing from writing: you can keep your edits with no
radio attached.

## Three things to know before you write to a radio

**Take a backup before your first write.** Both write paths are hardware-verified
now, the AnyTone's as of 2026-09-04 and the KT-5000's since 2026-08-06, but this
is still reverse-engineered software writing directly to a radio's flash. A
backup costs one tap and is the difference between an annoyance and a trip to
the vendor CPS.

**Do not unplug during a transfer.** A full transfer runs for minutes. It is held
by a foreground service so backgrounding the app will not kill it, but pulling
the cable part-way through a write leaves the radio holding a partly written
codeplug. The app tells you when that has happened; write a known good codeplug
before using the radio on the air.

**Where a codeplug came from matters on the KT-5000.** That radio's write sends
the image as it is, byte for byte, so an imported file replaces regions this app
does not model with whatever the file has in them. A codeplug read off the radio
in front of you carries that radio's own bytes there; anything else carries
someone else's. The write confirmation says which one you are about to send, and
warns when the codeplug is missing something the radio normally holds.

After a KT-5000 write the app reads the radio back and compares it against what
it sent, because that radio acknowledges a block on receipt and commits it later:
an acknowledgement is not proof the byte stuck. If anything did not land, the
dialog names it by map region rather than by offset. This takes about fifteen
seconds and costs the radio one extra reboot. The AnyTone is not read back: its
write already preserves everything it does not model, and a full read of it takes
minutes.

AnyTone global settings (the region holding menu options and the power-on
password) are shown read-only and are never written. Writing that region once
locked a radio behind an unknown power-on password with Chinese menus.

## Building

Needs the Rust toolchain, `cargo-ndk`, the Android SDK, and an NDK:

```sh
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk
```

Then everything goes through one script:

```sh
./build.sh              # debug APK (default), prints the path and an adb line
./build.sh release      # release APK, unsigned
./build.sh test         # host JVM tests, no device or radio needed
./build.sh install      # build the debug APK and adb install it
./build.sh doctor       # compare the APK's demands against the attached device
./build.sh bindings     # regenerate the Kotlin bindings from core/src/api.rs
./build.sh clean        # remove build outputs and cross-compiled libraries
```

Anything else is passed through to `gradlew` as a task name.

You do not need to set `JAVA_HOME` or `ANDROID_HOME`. The script finds a JDK 17+
(yours, then `/usr/libexec/java_home`, then the one Android Studio bundles),
locates the SDK, and writes `local.properties` on a fresh clone where it is
missing. It also checks for `cargo-ndk`, the Rust targets, and an NDK up front,
so a missing tool gives you the install command instead of a Gradle stack trace.

`build.sh` cross-compiles `codeplugme-core` into `app/src/main/jniLibs` via the
`cargoNdk` Gradle task before anything else, so one command produces a complete
APK. `./build.sh test` skips the cross-compile and builds a host library
instead, so the tests run against the real core through JNA.

### If the install fails

A phone that refuses an APK usually says only "App not installed", with no
reason. `./build.sh install` goes through `adb` rather than Gradle precisely so
the real `INSTALL_FAILED_*` code is printed, and it explains the common ones.
`./build.sh doctor` prints what the APK asks for beside what the attached device
offers, which is where the mismatch normally shows up.

The three that actually happen:

- **An older build is already installed, signed with a different debug key.**
  Every machine has its own debug keystore, so an APK built elsewhere will not
  upgrade one built here. Fix: `adb uninstall me.faulk.codeplugme`.
- **The phone blocks USB installs.** Many phones need "Install via USB" turned
  on in Developer options; MIUI and HyperOS also need "Verify apps over USB"
  turned off.
- **Android older than 10.** `minSdk` is 29.

Note that this app does *not* require the `android.hardware.usb.host` feature,
deliberately. Plenty of phones run OTG fine without declaring it, and requiring
it makes the installer reject the app on those phones with no explanation. The
app checks at runtime instead and says so on the Radio screen.

## How it fits together

```
core/                     Rust: protocols, memory maps, codeplug records
  src/transport.rs        the two-method seam both platforms plug into
  src/ffi.rs              hand-written C ABI, used by the macOS app only
  src/api.rs              UniFFI surface, used by this app only
android/app/src/main/java/me/faulk/codeplugme/
  usb/                    USB host transport, device attribution, transfer service
  library/                the codeplugs stored on the phone
  state/                  staging, undo, recovery, and the edit builders
  model/                  kotlinx.serialization types for the core's JSON
  ui/                     Compose screens
```

The Rust core does every byte of protocol and codeplug work. Kotlin supplies the
transport (Android has no `serialport`, since that needs libudev) and the UI.
Radio capabilities, which sections exist, channel capacities, and the USB
identity table all come from the core, so adding a radio there needs no change
here.

The link rate comes from the core's `Model::baud`, not from a constant here. It
is a property of the radio and getting it wrong fails silently: the port opens,
commands go out, and the radio never answers. The KT-5000 runs at 9600, which
only surfaced once this app started configuring the bridge itself. When a
transfer fails without the radio saying a word, the app handshakes across both
known rates with the control lines each way and puts the results in the error,
so the next radio with a surprising rate takes one run to find rather than a
week.

One thing about that transport is worth knowing before you touch it. A serial
port hands out a byte stream, but a USB bulk endpoint hands out whole packets,
and Android's `bulkTransfer` throws away whatever does not fit the buffer you
asked with. Asking for one byte of a 69-byte reply consumes the radio's entire
first packet and gives you one byte of it. So `UsbRadioTransport` never lets the
core's read size reach USB: it always reads a full packet, keeps the remainder,
and serves the next read from that. Without it the KT-5000 fails on its very
first block read, because that read starts by peeking a single lead byte.
