# CodeplugMe: a Programming Tool for the AnyTone AT-D878UVII Plus and QYT KT-5000

> **Status note.** This document was written for the AnyTone on macOS, which was
> the whole project for most of its life. Two things have been added since:
> QYT KT-5000 support, and an Android app. Section 3 (Architecture) has been
> rewritten to describe the per-model structure and the two host surfaces that
> shipped. The phased plan in section 4 is AnyTone-on-macOS history and is kept
> as written. For the KT-5000's protocol and memory layout see
> [`docs/qyt-kt5000-memory-map.md`](docs/qyt-kt5000-memory-map.md); for the
> Android app see [`android/README.md`](android/README.md).

**Codeplug** is a native macOS app to **create, download, and upload codeplugs**
for the AnyTone AT-D878UVII Plus over the USB programming cable — replacing the
Windows-only CPS.

> Terminology: AnyTone calls the config a **codeplug** (the user said "CUPs"; the
> real term is codeplug / CPS = Customer Programming Software). This document uses
> "codeplug".

Status: **shipping** · Last updated: 2026-09-04

Progress: Phase 0 ✅ · Phase 1 ✅ (protocol + backup/restore) · Phase 2 ✅
(model + offline `dump`/`edit` for channels, zones, scan lists, DMR contacts /
talk groups, RX group lists, and radio IDs, full add / remove / update, with the
contact reverse-lookup tables rebuilt on contact changes) · Phase 3 ✅ (SwiftUI
macOS GUI over the C FFI) · Phase 4 ⏳ (CSV / `.rdt` import-export, still not
started).

Since the phased plan was written, two things landed that it never anticipated:
QYT KT-5000 support, and an Android app (see [`android/README.md`](android/README.md)).

**Hardware validation is done for both radios.** The KT-5000's read and write
paths were verified on 2026-08-06 and the AnyTone's on 2026-09-04, the latter
across repeated real writes. That closes the track this document spent most of
its life waiting on: the Phase 2 offsets and the sector read-modify-write fix
are confirmed against real firmware rather than only against a mock.

One remaining track: **breadth**, meaning CSV / `.rdt` import-export (Phase 4).
AnyTone global radio settings stay deliberately out of scope; that region is
read-only for the reason recorded in the README.

---

## 1. Goal

- Read ("download") the full codeplug from the radio to a file on the Mac.
- Write ("upload") a codeplug from the Mac back to the radio.
- Create / edit codeplug contents (channels, zones, contacts/talkgroups, scan
  lists, radio settings) in a native macOS GUI.
- Never require Windows or the vendor CPS.

## 2. Key research findings (verified from 2+ independent sources)

### USB / transport
- The radio enumerates as a **standard USB CDC-ACM serial device**. On macOS it
  appears as `/dev/cu.usbmodemXXXX` — **no driver or kext required** (macOS ships
  the CDC-ACM driver). This is the single biggest de-risking fact.
- USB IDs seen in the wild: GD32 variant `VID 0x28e9 / PID 0x018a`, STM32 variant
  `VID 0x2e3c / PID 0x5740`. Detect by matching either, then confirm via the
  identify command below (don't hard-fail on VID/PID alone).
- Serial framing: 8-N-1. Baud rate is effectively ignored by the CDC driver but
  set a sane value (e.g. 115200) anyway.

### Programming protocol (byte-level, confirmed by qdmr, dmrconfig, and reald docs)
All multi-byte addresses are **big-endian**.

| Step | Host sends | Radio replies |
|------|-----------|---------------|
| Enter program mode | `"PROGRAM"` (0x50 52 4F 47 52 41 4D) | `"QX"` + `0x06` |
| Identify | `0x02` | `"I"` + model string (e.g. `ID878UV...`) + version/band + `0x06` |
| Read block | `'R'` + addr[4] + len(0x10) | `'W'` + addr[4] + `0x10` + data[16] + checksum[1] + `0x06` |
| Write block | `'W'` + addr[4] + `0x10` + data[16] + checksum[1] + `0x06` | `0x06` |
| Exit program mode | `"END"` | `0x06` |

- **Checksum** = 1-byte sum of (addr[4] + len + data) — i.e. every byte after the
  command byte and before the checksum, mod 256. Excludes the `'R'`/`'W'` byte.
- Block size is 16 bytes for the codeplug data path. (Firmware update uses a
  different 32-byte path with `"UPDATE"` — **out of scope**, do not implement,
  it's the brick-prone path.)
- The codeplug is read/written as a set of memory regions; qdmr's
  `D878UVCodeplug` documents the region list and per-section offsets.

### Prior art to reference (not to bundle)
- **qdmr / libdmrconf** (C++/Qt, GPLv3) — most complete; documents `D878UVCodeplug`
  offsets and the `anytone_interface` protocol. Reference for memory layout.
- **dmrconfig** (C, OpenRTX) — CLI that reads/writes and emits an editable text
  config. Good cross-check for offsets and protocol.
- **reald/anytone-flash-tools** — independent protocol + memory-map writeup.

Links collected in §8.

## 3. Architecture

Follows this repo's existing Swift-GUI-over-Rust-core pattern (see `spell-i/`).

The crate is organised **per radio model**. Everything specific to one radio, its
wire protocol, memory layout, and the fake radio its tests drive, lives in that
model's module. Model-agnostic pieces sit at the top level.

```
codeplug-me/
├── core/                    # Rust: codeplugme-core
│   ├── src/
│   │   ├── transport.rs     # Transport trait + macOS CDC-ACM impl (model-agnostic)
│   │   ├── bytes.rs         # shared codecs: BCD (BE + LE), Latin1, bitmaps
│   │   ├── model.rs         # enum Model, struct Capabilities, detect()
│   │   ├── driver.rs        # trait RadioDriver, driver_for(model)
│   │   ├── document.rs      # enum Document, the tagged JSON envelope
│   │   ├── ports.rs         # port discovery, (VID, PID, Model) attribution
│   │   ├── ffi.rs           # C ABI for the Swift app (hand-written header)
│   │   ├── anytone/         # AnyTone AT-D878UVII Plus
│   │   │   ├── protocol.rs  # PROGRAM/identify/R/W/END, checksum, framing
│   │   │   ├── device.rs    # REGIONS, sector read-modify-write, codeplug I/O
│   │   │   ├── driver.rs    # AnyToneDriver: RadioDriver
│   │   │   ├── mock.rs      # in-memory fake radio (AnyTone grammar)
│   │   │   └── codeplug/    # channels, zones, contacts, scan lists, group lists…
│   │   └── qyt/             # QYT KT-5000
│   │       ├── protocol.rs  # handshake, 'S'/'X', 16-bit BE addrs, no checksum
│   │       ├── device.rs    # flat 12.8 KB image read/write
│   │       ├── driver.rs    # QytDriver: RadioDriver
│   │       ├── mock.rs      # in-memory fake radio (QYT grammar)
│   │       └── codeplug/    # channels, settings, VFOs
│   └── tests/qyt_capture.rs # golden tests vs the real vendor USB captures
├── cli/                     # Rust: codeplugme-cli (backup/restore/dump/info)
├── CodeplugMe/              # SwiftUI app (XcodeGen project.yml + build.sh)
├── docs/
│   ├── codeplug-memory-map.md
│   └── qyt-kt5000-memory-map.md
├── PLAN.md
└── README.md
```

### Multi-model design decisions

- **Wire protocol: object-safe `trait RadioDriver`**, one impl per model. The two
  protocols share nothing but "push bytes / pull bytes", so a trait keeps the FFI
  free of `match`.
- **Region/memory map: deliberately not abstracted.** The AnyTone needs `REGIONS`
  plus bitmap-driven sparse transfers plus 32 KB sector read-modify-write; the QYT
  needs a flat `step_by(0x10)` loop. A trait over one instance each is not an
  abstraction. Each driver owns its transfer loop.
- **Entity model: a 2-variant `enum Document`**, not a trait. Only four operations
  are shared (parse / serialize / to_json / apply_edits) and the entity families
  are disjoint.
- **JSON: a tagged envelope**, `{model, display_name, capabilities, <model-key>: {...}}`,
  rather than a flattened union. Nesting keeps the AnyTone payload's strict
  non-optional decoding, which makes a dropped entity family fail loudly instead of
  silently rendering as empty. An edits object with no model key is re-parsed as a
  bare AnyTone spec, so older edit files and the CLI keep working.
- **Capabilities live in the core**, not a Swift table, so Rust stays the single
  source of truth and the two cannot drift.
- **Cross-model write guard.** `restore` detects the model from the *file*, then
  refuses to write unless the connected radio's identity matches. Tested in both
  directions, asserting zero write bytes are emitted.
- **Separate Swift panes per model, not conditional columns.** macOS 13 is the
  deployment target and `TableColumnBuilder.buildIf` is macOS 14.4+, so conditional
  columns will not compile. This is also semantically correct: `rxCtcss` is a tone
  table index on the AnyTone and 0.1 Hz units on the QYT, and a shared union whose
  meaning depends on a sibling `model` field is how frequency-corruption bugs happen.
  This does not scale past about three radios; at radio #3, move to a
  capabilities-driven schema renderer.

Rationale:
- **Rust core** matches repo/global conventions, is fully unit-testable without
  hardware (via `MockTransport`), and isolates the byte-twiddling.
- **CLI first** delivers the core user value (download/upload a codeplug file)
  and is the safest way to validate against real hardware before any GUI exists.
- **SwiftUI GUI** via C-FFI is native and reuses the `spell-i` build approach.

### Two host surfaces over one core

The core has two front doors, and they are siblings rather than layers:

- **`core/src/ffi.rs`** is a hand-written C ABI for the macOS Swift app. It takes
  filesystem paths and opens the serial port itself through `SerialTransport`.
- **`core/src/api.rs`** is a UniFFI surface for the Android app. It takes and
  returns byte vectors, because Android reaches files through the Storage Access
  Framework as content URIs rather than paths, and the transport is supplied by
  Kotlin: `serialport` needs libudev, which Android does not have.

Both are thin wrappers over the same `driver_for` and `Document` layers and hold
no protocol or codeplug logic. The cost is that a new radio needs adding in two
places; the benefit is that the shipping macOS app was not touched to add
Android. Revisit if a third radio lands, which is also the point at which the
per-model Swift panes should become a capabilities-driven renderer.

Feature-gating keeps them apart. `desktop` (default) pulls in `serialport`,
`SerialTransport`, `ports`, and the C ABI. `uniffi-api` pulls in `api.rs`. The
USB identity table sits in `usb_ids.rs`, outside both, so macOS and Android
attribute radios from one table rather than two that can drift.

### What the Android app adds beyond a port

Two things are genuinely new rather than translated:

- **A codeplug library.** The macOS app is single-document because a laptop makes
  a file picker cheap. On a phone that would put a Storage Access Framework round
  trip in front of every write, which is the wrong cost on the primary use case,
  so codeplugs live in app storage and SAF is used only to import and export.
- **A foreground service owning each transfer.** A full transfer runs for
  minutes, and Android will kill a backgrounded activity's work. Losing a read
  costs time; losing a write part-way leaves a radio holding half a codeplug.

Three hardware constraints shaped the USB layer rather than being discovered
late: permission is requested before the session opens because a session is one
uninterrupted open, the KT-5000 accepts exactly one handshake per open link and
reboots when the port closes, and a mid-write detach is reported as possibly
having written rather than as a clean failure.

## 4. Phased plan

### Phase 0 — Scaffolding & hardware discovery
- Create `core/` (lib) + `cli/` (bin) Cargo workspace; strict clippy.
- Detect candidate serial ports on macOS (`/dev/cu.usbmodem*` + VID/PID enum via
  `serialport` crate). `codeplugme-cli ports` lists them.
- **Deliverable:** `codeplugme-cli ports` shows the radio when plugged in.

### Phase 1 — Protocol + full codeplug backup/restore (MVP, highest value)
- Implement `protocol.rs`: enter, identify, read/write block, exit, checksum.
- `MockTransport` that emulates the radio for unit tests (round-trips reads/writes,
  validates checksums, asserts the PROGRAM/END handshake).
- `device.rs`: `read_codeplug()` streams all regions to a `.bin`; `write_codeplug()`
  writes a `.bin` back block-by-block with **read-back verification**.
- CLI: `info` (identify), `backup <file>`, `restore <file>`.
- **Safety gates (mandatory):** refuse `restore` unless a `backup` of the current
  radio state was taken this session or `--force`; verify model string matches the
  file; per-block read-after-write compare; never touch the firmware/UPDATE path.
- **Deliverable:** download a codeplug from the radio and write it back byte-identical.

### Phase 2 — Codeplug binary model (parse → edit)
- Port the region map + section offsets for D878UVII from qdmr's `D878UVCodeplug`.
- Start **read-only**: parse channels, zones, contacts/talkgroups, scan lists,
  radio ID(s); `codeplugme-cli dump --json`. Validate by parsing a codeplug read from
  the real radio and diffing against what the vendor CPS shows.
- Then **editable**: mutate model → re-serialize → byte-diff vs original to prove
  round-trip fidelity (only intended bytes change).
- Extensive unit tests with golden fixtures (a real backup `.bin`, gitignored if
  it contains personal data; ship a synthetic fixture for CI).
- **Deliverable:** parse + edit + re-serialize channels/zones/contacts losslessly.

### Phase 3 — SwiftUI macOS GUI
- `CodeplugMe` via XcodeGen + `build.sh`; link the Rust staticlib through cbindgen
  C header (mirror `spell-i`).
- Screens: Device (detect/identify/backup/restore with progress + big warnings),
  Channels table editor, Zones, Contacts/Talkgroups, Scan lists, Settings.
- Read from radio → edit → write to radio, all with the Phase-1 safety gates.
- **Deliverable:** end-to-end GUI flow: read, edit a channel, write, verify on radio.

### Phase 4 — Import/export & polish
- CSV import/export for channels & contacts (RadioID/BrandMeister talkgroup lists).
- Best-effort import of vendor `.rdt` codeplug files if the container is tractable.
- README (setup/build/test), error handling hardening, release build + notarization
  notes.

## 5. Risks & mitigations
- **Bricking on write.** Mitigate: mandatory backup-before-write, read-back verify,
  model/firmware guard, hard-exclude the firmware `UPDATE` path, restore is the
  first thing proven to work.
- **Codeplug layout drift across firmware versions.** Capture the identify version
  string; warn when it differs from the layout we target (D878UVII, current fw).
- **No hardware in CI.** All protocol/model logic tested via `MockTransport` +
  golden fixtures; hardware steps are explicit manual checkpoints.
- **Licensing.** qdmr is GPLv3 — reference its documented offsets, don't copy code;
  keep this a clean Rust implementation. reald docs + dmrconfig corroborate offsets.
- **Personal data.** Real codeplug backups may contain callsign/contacts; gitignore
  `*.bin` fixtures, commit only synthetic ones.

## 6. Testing strategy
- `cargo test` in `core/`: checksum vectors, framing, MockTransport round-trips,
  codeplug parse/serialize golden round-trips.
- Manual hardware checkpoints at the end of Phase 1, 2, 3 (documented in README).
- `/verify` before any commit that touches protocol or serialization.

## 7. Open decisions (default chosen, revisit if needed)
- GUI = **SwiftUI** (native, matches repo) vs a Rust TUI/egui. Default SwiftUI.
- Editable scope for v1 = channels, zones, contacts/talkgroups, scan lists, radio
  ID. Deeper menus (roaming, APRS, digital-monitor) deferred to post-v1.

## 8. References
- qdmr: https://github.com/hmatuschek/qdmr · docs https://dm3mat.darc.de/qdmr/
- libdmrconf D878UVCodeplug: https://static.dm3mat.de/qdmr/libdmrconf/classD878UVCodeplug.html
- dmrconfig: https://github.com/OpenRTX/dmrconfig
- reald protocol: https://github.com/reald/anytone-flash-tools/blob/master/at-d878uv_protocol.md
- reald memory map: https://github.com/reald/anytone-flash-tools/blob/master/at-d878uv_memory.md
- ANYTONE serial/factory mode: https://do1alx.de/2022/anytone-factory-settings-mode/
