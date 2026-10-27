# CPS Feature-Parity Plan

Goal: bring this tool's channel / scan-list / zone / APRS settings toward parity
with the official AnyTone CPS, keeping read/write to the radio working and safe.
Reference screenshots live in `temp/` (channl.png, scan list.png, zone.png,
aprs config.png).

## Decisions (from the operator, 2026-07-16)

1. **APRS config and Zone A/B channel are READ-ONLY for now.** Both map into the
   radio-settings block `0x02500000–0x025014FF` that previously bricked a radio
   when written (power-on password lock + Chinese menus). Build the tabs to
   display values from a backup, with a visible "read-only until safe write is
   worked out" note. No writes into that block.
2. **Scan lists: full read + write.** They are not currently in the read path at
   all; add the region, model, write support, and UI. They live in their own
   sector (`0x01080000`), same risk profile as channels/zones already written.
3. **Hardware is available** to verify writes (read a backup, diff, test
   write + vendor-CPS re-read).
4. **Channels: add every field with a confirmed qdmr offset** inside the 64-byte
   channel record (its own safe sector). Skip any offset that can't be confirmed.

## Data flow (do not break)

- Read: `Codeplug::to_json()` → `CodeplugJson` → JSON → Swift `Models.swift`.
- Write: Swift builds `EditSpec` JSON → `codeplugme_apply_edits` → re-parse + verify
  → serialize. Sector read-modify-write preserves untouched bytes.

## Streams (run core sequentially — shared hub files)

- [x] **S1 Channels** — 20 qdmr-sourced fields added to `channel.rs` + `edits.rs`;
  verified (offsets non-overlapping, tests pass).
- [x] **S2 Scan lists** — `codeplug/scan_list.rs` + region/bitmap in `device.rs`,
  parse/serialize/add/remove/move + channel↔scan-list cross-ref remapping in
  `mod.rs`, scan-list family in `edits.rs`. 91 tests pass incl. read/write parity
  exercising an active scan list. NOTE: `codeplug_size()` grew — old `.bin`
  backups no longer parse.
- [x] **S3 APRS + Zone A/B (read-only)** — `codeplug/aprs.rs` (parse-only,
  focused identity subset: call signs/SSIDs, symbol, TX intervals, FM freq/power);
  APRS region `0x02501000/0x490` added to READ path only (never written); zone
  `a_channel`/`b_channel` read from the already-read zone-channel-list; `to_json`
  + tests (incl. APRS bytes untouched on serialize). 93 tests pass.
- [x] **S4 Swift UI** — done in-house (subagents kept dying on infra). Channel
  editor gained all 20 new fields (analog signaling, flags, TX permit, scan-list
  assignment); new **Scan Lists** tab (full CRUD + editor with member transfer +
  priority/timing); read-only **APRS** tab; zone A/B available in the dump model.
  App builds; 56 Swift tests pass (fixtures updated for the new size + keys).
- [~] **S5 Docs/verify** — README + memory map updated. REMAINING: verify
  scan-list + channel-field writes on real hardware (read backup → edit → write →
  vendor-CPS re-read), and confirm APRS/zone-A-B read values against the CPS.
