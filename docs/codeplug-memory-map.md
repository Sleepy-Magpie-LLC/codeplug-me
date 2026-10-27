# AT-D878UV(II) Codeplug Memory Map (Phase 2 reference)

Collected from reald/anytone-flash-tools and to be cross-checked against qdmr's
`D878UVCodeplug` class during Phase 2 implementation. **These are D878UV offsets;
the D878UVII may differ in a few regions — verify each against a real backup read
from the target radio before trusting it for writes.** All addresses are the radio
memory addresses used with the `R`/`W` block protocol (see PLAN.md §2).

## ⚠️ The radio erases flash in 32 KB sectors

**Any write into a `0x8000`-aligned sector erases the entire sector.** The radio
does not do read-modify-write for you.

Measured against firmware `D878UV2V101` by comparing a vendor CPS write trace
against a vendor CPS read trace: an earlier version that rewrote only the modelled
blocks destroyed **43568 bytes** of vendor data per write. Writing the 2-block
group-list bitmap at `0x025c0b10` alone wiped 390 blocks; the three bitmaps at
`0x024c1300`/`0x024c1320`/`0x024c1500` wiped 1231 blocks including 5-tone, 2-tone
and the 16 KB region at `0x024c4000`. `0x8000` is the smallest sector size that
explains every erased block (`0x4000` accounts for only 1699 of 2723). Full
analysis: [`diagnosing-cps-breakage.md`](diagnosing-cps-breakage.md).

`Radio::write_codeplug` now handles this: it reads every 32 KB sector it will
touch, overlays the modelled blocks, and writes the sector back, preserving
everything else. `SECTOR_SIZE` in `core/src/device.rs` carries the derivation.
(Implemented 2026-07-16; not yet confirmed on hardware with a vendor CPS read.)

**Consequence for this map:** a region's address and length are not enough. Any
new write must reproduce every byte of every 32 KB sector the region falls in. The
addresses below are necessary but not sufficient on their own.

The zone channel list (read-only below) and the contact ID table were both wrong
qdmr D868UV offsets for the II Plus. The ID table is now written to the vendor's
**`0x04800000`** (was `0x04340000`); our per-entry encoding was already
byte-identical to the vendor's.

## Regions

| Region | Base address | Layout |
|--------|--------------|--------|
| Channels | 0x00800000 – 0x00FC0000 | 32 banks × 8192 B (last 2176 B); **64 B per channel** |
| Zones | 0x01000000 | 250 zones × 512 B; channel IDs are 2-byte entries |
| Roaming channels | 0x01040000 | 32 B each, max 250; used-flags bitfield @ 0x01042000 |
| Roaming zones | 0x01043000 | 128 B each, max 64; used-flags @ 0x01042080 |
| Scan lists | 0x01080000 – 0x01441200 | 250 × 144 B at irregular addrs (formula below) |
| SMS management | 0x01640000 | 100 × 16 B; used-flags @ 0x01640800 |
| SMS text | 0x02140000 – 0x02440300 | 100 × 208 B |
| FM (broadcast) channels | 0x02480000 | 100 × 4 B; VFO @ 0x02480200; flags @ 0x02480210 |
| 5-Tone | 0x024C0000 | 100 × 32 B; used-flags @ 0x024C0C80 |
| 2-Tone encode | 0x024C1100 | 24 × 16 B |
| 2-Tone decode | 0x024C2400 | 24 × 32 B |
| Radio settings | 0x02500000 – 0x025014FF | power-on/display/audio/key bindings/etc. |
| APRS config | 0x02501000 – 0x02501490 | general, TX text, GPS templates, freqs |

## DMR regions (implemented; offsets from qdmr `d868uv_codeplug.hh` / `anytone_codeplug.hh`)

These are the offsets the model in `core/src/codeplug/` actually uses. Element
field layouts are in each module's header doc comment.

| Region | Base / bitmap | Layout |
|--------|---------------|--------|
| Digital contacts | banks 0x02680000 (10 × 0x40000, 1000 × 0x64 each) | `ContactElement` 0x64: type@0x00, name@0x01 (16), number@0x23 (BCD8-be), alert@0x27 |
| Contact bitmap | 0x02640000, 0x500 | **inverted** (`InvertedBitmapElement`): active = bit *clear* |
| Contact index | 0x02600000, 10000 × u32-le | dense list of active contact indices; read in full, rebuilt on write. Empty state is 0xff, not 0x00 |
| Contact ID table | **0x04800000** (vendor address; was 0x04340000) | `ContactMapElement`: id = (bcd_le(number)<<1)\|group_flag @0x00, index @0x04; sorted ascending by number |
| RX group lists | 0x02980000, 250 × 0x200 (0x120 element) | members@0x00 = 64 × u32-le contact indices (0xffffffff empty), name@0x100 (16) |
| Group-list bitmap | 0x025C0B10, 0x20 | normal |
| Radio IDs | 0x02580000, 250 × 0x20 | number@0x00 (BCD8-be), name@0x05 (16) |
| Radio-ID bitmap | 0x024C1320, 0x20 | normal |
| Zone channel list (**read-only**) | 0x02500100, 0x400 | `ZoneChannelListElement`: VFO A@0x000, VFO B@0x200, each 250 × u16-le channel index; 0xffff = unset |

## Never write the radio-settings block (0x02500000 – 0x025014FF)

The zone channel list address above is a **D878UV** offset that lands inside the
radio-settings block, which also holds the power-on password flag and the menu
language. It was briefly added to the write path so zone up/down would follow
zone membership. On a real D878UVII that write locked the radio: it came up
demanding a power-on password nobody had set, with its menus switched to
Chinese. So either the offset is wrong for this model, the element is a
different size here, or neighbouring bytes carry meaning the model does not
capture.

The region stays in `REGIONS` so backups capture the bytes and existing `.bin`
files still parse, but `Radio::write_codeplug` skips it, `serialize` passes it
through untouched, and two tests pin that down
(`serialize_never_touches_the_radio_settings_block`,
`write_codeplug_never_writes_the_radio_settings_block`).

**Read-only projections now decoded from this block (never written):**
- **Zone A/B channel** — `Zone::a_channel`/`b_channel` are read from the zone
  channel list (`0x02500100`) for display. No setter, never serialized.
- **APRS settings** — `core/src/codeplug/aprs.rs` parses a focused subset
  (call signs/SSIDs, symbol, TX intervals, FM freq/power) from the APRS block
  (`0x02501000/0x490`, now in the READ path). Display/backup only; offsets from
  qdmr `D878UVCodeplug::APRSSettingsElement` and should be CPS-verified. Adding
  this region (and the scan-list banks) grew `codeplug_size()`, so pre-change
  `.bin` backups no longer parse — re-read from the radio.

Nothing in this range may be added to the write path until the layout is
confirmed against a real D878UVII backup — a wrong byte here locks the operator
out of the radio, and the tool cannot undo it because it never captured the
original settings.

Channel record DMR fields (64-byte `ChannelElement`): contactIndex@0x14 (u32-le),
radioIdIndex@0x18 (u8), scanListIndex@0x1b (u8), groupListIndex@0x1c (u8),
colorCode@0x20 (u8), timeSlot@{0x21,0}.

## The contact index (0x02600000) and contact ID table (0x04800000) — CLEARED

The contact index + ID table are derived reverse-lookup structures the radio uses
to show a caller's name by DMR number. The model rebuilds them from the active
contacts (matching qdmr's `encodeContacts`) when contacts change, and
`write_codeplug` sends them sized to the contact count.

**They are written, and that is correct.** They were briefly suspected of
breaking the vendor CPS and were skipped on write for a few hours on 2026-07-15.
That was wrong twice over, and both mistakes are worth remembering:

1. **The evidence was an artifact.** The diagnosis rested on a backup `.bin`
   showing 40000 and 80000 bytes of zeros, read as "the CPS leaves these empty".
   Those zeros came from `read_codeplug`'s own zero-filled buffer — the regions
   were not in the read path, so the radio was never asked. A backup was
   asserting something about hardware it had never looked at.
2. **The regions are demonstrably innocent.** Both are **byte-identical**
   between a radio the CPS cannot read and the same radio after a vendor CPS
   write makes it readable again (4096 bytes of index, 2048 of ID table, zero
   differences). Our reconstruction is sitting on a working radio right now and
   the CPS reads it fine. Skipping the writes also did not fix the CPS when
   tested directly.

Both regions are now **read in full** (`is_sparse_data_region` no longer excludes
them, costing ~7500 blocks / ~4 s per backup). Verified on hardware: a backup's
bytes at 0x02600000 now match a `dump-range` of the same radio exactly, where the
pre-fix backup held all zeros. `every_written_range_is_also_read` in
`core/src/device.rs` fails if anyone removes them from the read path again.

Useful facts measured along the way:

- The empty state of these regions is `0xff` (erased flash), **not** `0x00`. A
  repair that zeroed them would corrupt rather than restore.
- A vendor CPS codeplug write does **not** touch either region.
- The real cause of the CPS breakage was the 32 KB sector erase, above. See
  [`diagnosing-cps-breakage.md`](diagnosing-cps-breakage.md).

## Scan lists (IMPLEMENTED — read + write)

Modeled in `core/src/codeplug/scan_list.rs` and wired into the read/write path.
Offsets from qdmr `AnytoneCodeplug::ScanListElement` (`lib/anytone_codeplug.hh`,
`size()` = 0x90) and `D868UVCodeplug::Offset`/`::Limit`
(`lib/d868uv_codeplug.hh`).

| Region | Base / bitmap | Layout |
|--------|---------------|--------|
| Scan lists | banks 0x01080000 (16 banks × 0x40000, 16 × 0x200 each) | `ScanListElement` 0x90 |
| Scan-list bitmap | 0x024c1340, 0x20 | normal (active = bit set) |

`numScanLists` = 250, `numScanListsPerBank` = 16, `betweenScanLists` = 0x200,
`betweenScanListBanks` = 0x40000.

ScanListElement fields: priorityChannel@0x01 (u8: 0 off / 1 primary / 2 secondary
/ 3 both), primaryChannel@0x02 (u16-le, 0xffff none), secondaryChannel@0x04
(u16-le), lookBackTimeA@0x06, lookBackTimeB@0x08, dropOutDelay@0x0a,
dwellTime@0x0c (all u16-le; qdmr reads ×10 deci-seconds — we keep the raw stored
value), revertChannel@0x0e (u8), name@0x0f (16 B Latin1), members@0x20 (up to 50
× u16-le channel indices, 0xffff = unused).

**Backup size changed.** Adding the scan-list banks + bitmap to `REGIONS` grows
`codeplug_size()`, so `.bin` backups written before this change no longer parse
(exact-size check). Re-read a fresh backup from the radio after upgrading.

### Scan list address formula
```
addr(pos) = 0x01080000 + floor((pos-1)/16) * 0x40000 + ((pos-1) mod 16) * 0x200
```
(pos is 1-based, 1..=250, each record 144 B; the code uses 0-based `index` = pos-1.)

## Notes for the parser
- Read is done in 16-byte blocks; the codeplug model should operate on assembled
  region byte-slices, not raw blocks.
- Many regions have parallel "used-flags" bitfields — the model must honor these
  when deciding which records are active, and keep them consistent on serialize.
- Round-trip fidelity is the acceptance test: parse → serialize must be byte-
  identical to a codeplug read from the radio (only intentionally edited bytes change).

## Sources
- https://github.com/reald/anytone-flash-tools/blob/master/at-d878uv_memory.md
- https://static.dm3mat.de/qdmr/libdmrconf/classD878UVCodeplug.html
