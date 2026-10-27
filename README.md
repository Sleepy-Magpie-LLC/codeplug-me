# CodeplugMe

CodeplugMe lets you read, edit, and write the codeplug (the channel and settings
memory) of your ham radio over its USB programming cable, without the
manufacturer's Windows-only CPS.

It comes as a macOS app, an Android app, and a command-line tool, all built on
the same Rust core.

## Supported radios

| Radio | What you can edit |
|---|---|
| **AnyTone AT-D878UVII Plus** | DMR and analog. Channels, zones, scan lists, contacts and talk groups, RX group lists, radio IDs. APRS and zone A/B are shown read-only. |
| **QYT KT-5000** | Analog. 200 channels (name, RX/TX, tones, scan skip), VFO channels, common settings like squelch and beep, and the message on the radio's display. |

Both have been tested end to end on real radios: read, edit, write, and read back.

## Before you start

This is an unofficial, reverse-engineered tool. It isn't affiliated with or
supported by AnyTone or QYT, and it comes with no warranty. A bad write or a
cable pulled mid-transfer can leave a radio needing recovery.

- **Back up first.** Read your radio and keep that `.bin` somewhere safe before
  you write anything.
- **Check what you transmit.** Nothing here checks that your frequencies, power,
  or modes are legal for your license or region.

## Install

For now you build it from source.

1. Install Rust with [rustup](https://rustup.rs).
2. For the macOS app, install Xcode and [XcodeGen](https://github.com/yonaskolb/XcodeGen)
   (`brew install xcodegen`), then run `./build.sh`. The app lands in
   `build/Debug/CodeplugMe.app`.
3. For the command-line tool, run `cargo build --release`. The binary is
   `target/release/codeplugme-cli`.
4. For the Android app, see [`android/README.md`](android/README.md).
5. To build both release deliverables at once, run `./build-all.sh`. It puts an
   Apple silicon DMG and the Android release APK in `dist/`. Pass `macos` or
   `android` to build just one. The APK is signed when `android/key.properties`
   exists (`storeFile`, `storePassword`, `keyAlias`, `keyPassword`), otherwise
   it is unsigned.

No drivers are needed on macOS. The AnyTone shows up as `/dev/cu.usbmodem*` and
the KT-5000's cable as `/dev/cu.usbserial*`.

## Using the macOS app

1. **Connect the radio** and open the **Device** pane. Your radio appears in the
   list. Press **Identify** to confirm it's talking.
2. **Read from Radio…** saves the full codeplug to a `.bin` file and offers to
   open it. This is also your backup.
3. **Edit.** The sidebar shows the sections for that radio (channels, zones,
   contacts, and so on). Double-click any row to edit it. Tables sort by column
   and filter from the search field. Undo and redo work as usual (⌘Z, ⇧⌘Z).
4. **Save** (⌘S). Your `.bin` is never touched until you save. Unsaved work is
   kept in a recovery file, so a crash won't lose it.
5. **Write to Radio.** Use the button in the Device pane, or the dropdown in the
   Codeplug toolbar to write what you're looking at, unsaved edits included.
   You'll get a confirmation first, and a progress overlay while it runs.

A few things that help:

- **Duplicates.** Channels that match another channel's frequencies and mode get
  a warning mark. Tick "Duplicates only" to see just those.
- **Zones and group lists** use a two-column picker. Select several on one side
  and move them across in one go, or double-click to move one.
- **Bulk Edit** appears when you select several channels (⌘-click or
  shift-click).
- **Move to Slot…** in a row's context menu moves an entry to another slot and
  updates every zone or list that pointed at it.
- **Save As…** (⇧⌘S) writes to a new file, handy for trying something without
  risking a setup that works.

## Adding nearby repeaters

Both apps can fill a codeplug with repeaters near a location, from
[Hearham](https://hearham.com/repeaters), [RadioID](https://radioid.net) (DMR),
and [RepeaterBook](https://www.repeaterbook.com) (needs your own token). You pick
exactly which ones go in, and a counter stops you picking more than the radio can
hold. **Merge** adds to your codeplug, **Clean** replaces its channels, zones and
scan lists.

- macOS: the **Add Repeaters** toolbar button.
- Android: **More > Add nearby repeaters**.

The KT-5000 only gets analog repeaters. More in
[`docs/repeater-import.md`](docs/repeater-import.md).

## Radio-specific notes

### QYT KT-5000

- **Pick the model by hand.** The KT-5000 isn't auto-detected yet, so choose it
  in the Device pane before reading or writing. Everything else works normally.
- **It reboots after every read or write.** Reconnect before the next step.
- **Display message.** The settings pane's Display message is what the radio
  shows on its screen. Stock radios say `Model:KT-5000`. You can put your
  callsign or anything up to 13 ASCII characters there.
- Power, bandwidth and some other channel flags are shown read-only for now and
  are kept exactly as the radio had them.

### AnyTone AT-D878UVII Plus

- The radio's general settings region is never written, by design. That's why
  APRS and zone A/B are read-only.
- Backups made with older versions of this tool may no longer open. Read a fresh
  one from the radio.

## Command line

```sh
codeplugme-cli ports                         # list ports; radios are marked
codeplugme-cli info                          # identify the connected radio
codeplugme-cli backup my-codeplug.bin        # read the radio to a file
codeplugme-cli restore my-codeplug.bin --force   # write a file to the radio
codeplugme-cli dump my-codeplug.bin          # show a codeplug as JSON
codeplugme-cli edit my-codeplug.bin edits.json -o edited.bin   # apply JSON edits
```

Add `--port /dev/cu.usbmodemXXXX` if more than one radio is connected.
`backup`, `dump`, `dump-range` and `edit` never write to the radio. `restore`
won't run without `--force`.

An `edits.json` lists changes per section: `channels`, `zones`, `contacts`,
`group_lists` and `radio_ids` to update by index, plus `add_*` and `remove_*`
arrays. This adds a talk group and a DMR channel that uses it:

```json
{
  "add_contacts": [{"name": "LOCAL 2", "number": 2, "call_type": "group"}],
  "add_channels": [{"name": "TG2 TS1", "rx_frequency_hz": 439000000,
                    "mode": "digital", "color_code": 1, "time_slot": 1,
                    "contact_index": 0}]
}
```

### Rescuing a KT-5000 stuck in a reboot loop

A normal write needs about 18 seconds of steady connection. A radio stuck
rebooting won't give you that, so the `rescue` commands keep reconnecting and
pick up where they left off.

```sh
codeplugme-cli rescue watch --port /dev/cu.usbserial-10 --seconds 30  # listen only
codeplugme-cli rescue knock --port /dev/cu.usbserial-10   # how long the link stays up
codeplugme-cli rescue verify kt5000-backup.bin            # compare radio to a backup
codeplugme-cli rescue write kt5000-backup.bin --address 0x0000 --length 0x3100 --force
```

Always run `rescue verify` after a `rescue write`. A reset mid-transfer can
silently drop a block, and the read-back is how you catch it. Only
`rescue write` writes to the radio.

## Getting help when a write goes wrong

If the vendor CPS can't read a radio after this tool wrote to it, follow
[`docs/diagnosing-cps-breakage.md`](docs/diagnosing-cps-breakage.md).

## For developers

Building, testing, code layout, release signing, and the protocol details that
matter are in [`docs/developing.md`](docs/developing.md). The design and roadmap
are in [`PLAN.md`](PLAN.md). Memory maps:
[AnyTone](docs/codeplug-memory-map.md), [KT-5000](docs/qyt-kt5000-memory-map.md).
