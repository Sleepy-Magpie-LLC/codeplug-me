# Diagnosing "this tool broke the vendor CPS"

A playbook for the class of bug where the radio still works, this tool still
reads and writes it, but the **official AnyTone CPS can no longer read the
radio** after we write to it.

**Read this first: the root cause is known.** The radio erases flash in **32 KB
sectors**. Any write into a sector erases the whole sector, and we rewrite only
the blocks we know about — so ~43 KB of vendor data is destroyed on every write.
See "32 KB flash sector erase" below. If the CPS cannot read a radio this tool
wrote, that is almost certainly why, and no further investigation is needed until
the sector-aware write lands.

The rest of this playbook is for the *next* bug of this class, which will arrive
whenever a qdmr D868UV offset turns out not to hold on the D878UVII Plus, or
AnyTone moves something in a firmware update.

Read the recorded findings before trusting your own reasoning. Two confident,
well-evidenced, completely wrong diagnoses are documented there. Both were wrong
the same way: **a test that had no power to fail.**

## Symptom

The vendor CPS fails on **read**, typically after it has pulled all the data off
the radio and is loading it into the app:

```
GetOptionFromCommData_Error:---2500000
```

Do not trust the address in that message. In the July 2026 case it pointed at
`0x02500000` (the radio-settings block) and the settings block turned out to be
byte-identical to its pre-write state. The CPS reports where it was reading, not
where the bad data is. Treat the number as a hint, never as the diagnosis.

## The seductive wrong insight — read this before reasoning

Here is the argument that wasted an entire session. It is seductive because every
step of it is true:

> `Radio::write_codeplug` copies almost everything verbatim out of the codeplug
> image — the bitmaps and every active record are `data[off..off+len]`. If the
> image came from the radio, those bytes are by definition what the radio already
> had. So **the only bytes a write can get wrong are the ones we compute rather
> than copy.** Restoring a radio's own unmodified backup should be a perfect
> no-op.

Every sentence is correct. The conclusion is fatally incomplete, because it
reasons only about **the bytes we send**. The actual bug was in the bytes we
*don't* send: a write into a 32 KB sector erases the whole sector, destroying
vendor data we never touch, never read, and never modelled.

**A write does not only change what it writes.** Whatever this radio does on a
write — erase a sector, bump a counter, invalidate a cache — is part of the write,
and none of it is visible in the bytes you hand to `write_block`.

So the useful version of the insight is narrower:

- Bytes we *compute* are a good place to look for **wrong content**.
- Bytes we *destroy as a side effect* are where **the damage** lives, and no
  amount of staring at the write path will show them to you. Only the radio will.

When a no-op restore changes nothing observable and still breaks the CPS, stop
theorising about content. The answer is outside what your tooling can see.

## Procedure

Run these in order. Steps 1-3 are read-only and risk nothing.

### 1. Baseline the radio while it is healthy

Get the radio into a known-good state first (vendor CPS write, or a firmware
flash plus a fresh codeplug), and confirm the CPS can read it. Then:

```sh
codeplugme-cli backup docs/reference/codeplug-clean.bin
codeplugme-cli dump-range --address 0x02500000 --length 0x1500 -o docs/reference/settings-clean.bin
```

The backup only covers `REGIONS`; `dump-range` exists to capture anything outside
that map. The settings block is the usual suspect and backups do **not** include
most of it.

Without this baseline you cannot diff, and you cannot undo. Do this *before* any
experiment.

### 2. Prove the round-trip is lossless (offline)

```sh
echo '{}' > /tmp/noop.json
codeplugme-cli edit docs/reference/codeplug-clean.bin /tmp/noop.json -o /tmp/roundtrip.bin
cmp docs/reference/codeplug-clean.bin /tmp/roundtrip.bin
```

If these differ, stop: `parse → serialize` is lossy and you have found the bug
without touching the radio.

Note the trap: this passing does **not** clear the write path. `serialize` and
`write_codeplug` are separate code. In July 2026 the round-trip was byte-perfect
while the write path still sent fabricated tables, because `write_codeplug`
computed them independently of the image.

### 3. Diff what we would write against what the radio has

For every region the write path *computes* rather than copies, compare our bytes
against what is actually on the radio. Any divergence is a candidate.

**Get the comparison bytes from `dump-range`, not from a backup `.bin`.** This is
where the July 2026 investigation went wrong: a throwaway `core/examples/` binary
compared `contact_index_bytes()` against `data[global_offset(CONTACT_INDEX)..]`
from a backup and reported a dramatic mismatch. The backup was zeros there
because the region is not in the read path, so the "mismatch" was against our own
empty buffer and meant nothing. The conclusion drawn from it was wrong.

```sh
# Right: ask the radio.
codeplugme-cli dump-range --address 0x02600000 --length 0x400 -o /tmp/actual.bin

# Wrong, unless you have verified the region is in the read path:
#   compare against docs/reference/codeplug-clean.bin
```

A region is only trustworthy in a backup if `is_full_transfer_region` accepts it
or `active_transfer_segments` emits it. Check `core/src/device.rs` before
believing anything.

### 4. The no-op restore

If steps 2-3 are clean and the CPS still breaks, write the radio's own unmodified
backup back to it:

```sh
codeplugme-cli restore docs/reference/codeplug-clean.bin --force
```

Nothing about the content changed, so if the CPS now fails, the *act of writing*
is the trigger and content is irrelevant. This is the cheapest write that still
reproduces the failure.

Expect the radio to reboot and drop off USB for ~15 seconds afterwards. That is
normal. Wait for `/dev/cu.usbmodem*` to reappear before the next command.

### 5. Diff the radio against the baseline — and pick the range correctly

```sh
codeplugme-cli dump-range --address <addr> --length <len> -o /tmp/after.bin
cmp /tmp/before.bin /tmp/after.bin
```

**Choose a range our write actually touches, or the test proves nothing.** This
is precisely how the sector-erase mechanism was "ruled out" in July 2026: the
before/after dump used the settings block at `0x02500000`, which came back
byte-identical — because `0x02500000..0x02508000` is the one sector our write
never enters. The test could not have failed no matter what was true.

Before running any before/after comparison, ask: *if my hypothesis is right,
would this specific range change?* If the answer is no, or you are not sure, pick
a different range. A confirmed hypothesis from a powerless test is worse than no
test, because you will stop looking.

Good targets are addresses the vendor CPS writes but we do not, inside a 32 KB
sector we do write — e.g. `0x024c4000` (in the same sector as the bitmaps at
`0x024c1300`).

### 6. Compare a vendor CPS write trace against a vendor CPS read trace

**This is what actually solved it, and it should be step 1 next time.** Capture
both with [USBPcap](https://desowin.org/usbpcap/) +
[Wireshark](https://www.wireshark.org/), decode with the framing in
`core/src/protocol.rs`, then ask the one question that matters:

> For every address the CPS **wrote**, does the radio **hand back the same bytes**
> when the CPS reads?

Anything that comes back different — especially `0xff` — is memory something
destroyed. Correlate the destroyed addresses against the addresses *we* write and
the mechanism falls out immediately. Fitting sector sizes against that set gave
an exact answer in one pass:

```
sector 0x04000: 1699/2723 erased blocks explained
sector 0x08000: 2723/2723 erased blocks explained   <- exact fit
```

No hardware experiments, no guessing, no broken radios. Two captures and a
decoder.

### 7. Confirm with the CPS

Only the vendor CPS can tell you the bug is actually fixed. A green test suite
cannot; every one of these bugs passed our tests.

## What we know about a healthy radio

Measured 2026-07-15 against a D878UVII Plus, identify string `D878UV2V101`,
freshly programmed by the vendor CPS with 275 channels / 6 zones / 130 contacts /
1 radio ID.

| Region | Address | State on a CPS-programmed radio |
|--------|---------|--------------------------------|
| Radio settings | `0x02500000` | real data; APRS config readable as ASCII at `0x02501000`; unchanged by anything this tool writes |
| Zone channel list (claimed) | `0x02500100` | *not* a zone channel list; padded `0x0000`, not the `0xffff` an index table would use |
| Contact index | `0x02600000` | dense `0, 1, 2 …` u32-le run to the contact count, then `0xffffffff`. Empty state is `0xff`, not `0x00`. Identical on working and broken radios |
| Contact ID table | `0x04340000` | `id=3→idx 0, id=5→idx 1 …` (`(bcd_le(number)<<1)\|group_flag`), sorted by number. Identical on working and broken radios |

### The trap that cost hours: a backup is not the radio

`read_codeplug` reads **sparsely**. It starts from `vec![0u8; codeplug_size()]`
and only fills the regions in its transfer list — so **any region not in that
list reads back as zeros that came from our own allocation, not from the radio.**
A backup silently invents data for anything it doesn't fetch.

This is no longer true of the contact tables (they are read in full as of
2026-07-15, after this trap cost an entire session), but the shape of the trap is
permanent. `REGIONS` still omits roaming, scan lists, SMS, FM, 5-tone, 2-tone,
APRS, and most of the settings block, and the DMR families are read
bitmap-driven, so a record the bitmaps call inactive is never fetched either.

Before treating any byte in a backup as fact, confirm the region is actually
read: it must satisfy `is_full_transfer_region` or be emitted by
`active_transfer_segments` in `core/src/device.rs`. When in doubt, ask the radio
directly:

```sh
# What the radio really has, no interpretation layer:
codeplugme-cli dump-range --address 0x02600000 --length 0x400
```

`dump-range` is the only thing in this repo that reads exactly what you ask for.
Prefer it over a backup for any region whose read coverage you have not
personally verified.

## Findings so far

### July 2026: 32 KB flash sector erase — **SOLVED, root cause**

**The radio erases flash in 32 KB (`0x8000`) sectors. Any write into a sector
erases the entire sector. We rewrite only the blocks we know about, so every
other byte in that sector is destroyed and reads back `0xff`.**

A single no-op restore destroys **43568 bytes** of vendor data:

| Sector | We write | We destroy |
|---|---|---|
| `0x024c0000..0x024c8000` | **36 blk** (zone/radio-ID/channel bitmaps) | **1231 blk** — 5-tone, 2-tone, the 16 KB region at `0x024c4000` |
| `0x025c0000..0x025c8000` | **2 blk** (group-list bitmap) | **390 blk** |
| `0x00800000..0x00808000` | 368 blk | 368 blk (upper half of channel bank 0) |
| `0x00840000..0x00848000` | 512 blk | 512 blk |
| `0x00880000..0x00888000` | 220 blk | 220 blk |
| `0x02580000..0x02588000` | 2 blk (radio ID) | 2 blk at `0x02582000` |

Writing a **two-block** bitmap at `0x025c0b10` wipes 390 blocks. That is the
whole bug.

#### How it was proven

Two vendor CPS USB captures, decoded with the framing in
`core/src/protocol.rs`:

1. A CPS **write** session: 7895 `W` blocks in 67 ranges.
2. A CPS **read** session afterwards: 7933 `R` blocks.

Comparing what the CPS *wrote* against what the radio *handed back*: of 5358
shared addresses, **2723 read back as `0xff`** where the CPS had written real
data. Fitting candidate sector sizes against the set of addresses *we* write:

```
sector 0x04000: 1699/2723 erased blocks explained
sector 0x08000: 2723/2723 erased blocks explained   <- exact fit
```

`0x8000` is the smallest size that accounts for every erased block, and nothing
the CPS wrote outside our sectors was harmed.

#### Why every earlier test missed it

- **The backup diff said "identical".** We only read the regions we know about —
  which are exactly the regions we correctly rewrite. The 43 KB we destroy lives
  in regions our read path never touches. Our own tooling cannot see the damage
  it does.
- **The sector-erase theory was "ruled out"** early by dumping the settings block
  at `0x02500000` before and after a write and finding it byte-identical.
  `0x02500000..0x02508000` is *the one sector our write never enters*, because
  the settings block is deliberately skipped. The test picked the single region
  immune to the mechanism and concluded the mechanism did not exist. A check with
  no power to fail proves nothing.
- **The contact tables looked guilty** because they were the only bytes the write
  path computes rather than copies. True, and irrelevant — the damage was never
  in the bytes we send, it was in the bytes the erase takes with them.

#### The fix (implemented 2026-07-16, hardware-verified 2026-09-04)

`Radio::write_codeplug` now does **sector read-modify-write**. For every 32 KB
sector any modelled segment falls in, it reads the sector off the radio, overlays
the modelled blocks onto the radio's current contents, and writes back only the
non-`0xff` blocks (the erase provides the `0xff` fill, keeping write volume near
what the vendor CPS emits). Reading before writing is what preserves the ~43 KB
we do not model. `SECTOR_SIZE` in `core/src/device.rs` carries the derivation.

Locked in by tests against a `MockTransport` that now reproduces the sector
erase (`mock_erases_a_whole_sector_on_first_write`,
`write_codeplug_preserves_bytes_outside_the_model`). The preservation test fails
against the old direct-segment write — verified by reverting it — so it has real
power.

**Still required: hardware verification.** Per rule 7, a green suite proves
nothing here. The acceptance test is unchanged:

1. Vendor CPS writes a known-good codeplug; confirm the CPS reads it back.
2. `codeplugme-cli backup` → `restore` unmodified.
3. **Vendor CPS read must succeed.** This is the only acceptance criterion.
4. Re-capture a CPS read trace and diff the vendor's written bytes against the
   radio's read-back — zero `0xff` regressions means genuinely fixed, not just
   "the CPS did not complain".

### July 2026: contact index and contact ID table — CLEARED, and a lesson

This is a **worked example of a confident wrong diagnosis**, kept because the way
it went wrong is more instructive than the answer.

**The reasoning was sound.** Every write segment except the derived contact tables
is copied verbatim from the image, so those tables are the only bytes a no-op
restore *can* change. That argument is still true. It just wasn't enough.

**The evidence was garbage.** A backup `.bin` showed 40000 and 80000 bytes of
zeros at 0x02600000 / 0x04340000, read as "the vendor CPS leaves these empty, so
we're writing invented data into regions it never touches." Confident, specific,
documented — and wrong. Those zeros came from `read_codeplug`'s own zero-filled
buffer, because the regions were not in the read path. **The backup was making a
claim about hardware it had never asked.**

**What actually settled it**, and would have settled it in five minutes at the
start:

- `dump-range` both regions on a **CPS-failing** radio, then again after a vendor
  CPS write makes it **readable**. Result: byte-identical, 4096 bytes of index
  and 2048 of ID table, zero differences. A region that is identical in a working
  and a broken state cannot be what distinguishes them.
- Our reconstruction is sitting on a working radio right now (index reads
  `0, 1, 2 … 129` then `0xffffffff`; ID table reads `id=3→idx 0, id=5→idx 1`,
  matching qdmr's `(bcd_le(number)<<1)|group_flag`) and the CPS reads it fine.
- Skipping the writes entirely was tested against the CPS. It did not help.

**Resolution:** the skip was reverted — leaving the tables stale after a contact
edit would break caller-name resolution for no benefit. Both regions are now
**read in full**, which is the fix that should have existed all along: a backup
now matches the radio there (verified on hardware), so a no-op restore is a
genuine no-op and the next investigation has something to diff.
`every_written_range_is_also_read` fails if anyone reintroduces the gap.

**Facts worth keeping:**

- A vendor CPS codeplug write does not touch either region.
- Their empty state is `0xff` (erased flash), not `0x00`. The repair that was
  nearly attempted — zeroing them — would have corrupted rather than restored.

### Earlier: zone channel list

Writing `0x02500100` (qdmr `Offset::zoneChannelList`, a D878UV offset) landed
inside the radio-settings block and locked the radio behind a power-on password
nobody had set, with menus switched to Chinese. See
[codeplug-memory-map.md](codeplug-memory-map.md).

## Rules that came out of this

1. **Never write into a 32 KB sector you cannot reproduce in full.** This is the
   root-cause rule. The unit of damage is the *sector*, not the byte or the
   region. Writing 2 blocks of bitmap at `0x025c0b10` destroys 390 blocks around
   it. `every_written_range_is_also_read` currently checks regions; it needs to
   check sectors.
2. **A write does not only change what it writes.** Erase, counters, caches — the
   side effects are part of the write and are invisible in the bytes you hand to
   `write_block`. Reasoning about the write path can only ever find *wrong
   content*, never *collateral damage*.
3. **Design tests that can fail.** Both wrong diagnoses came from tests with no
   power: a before/after dump of the one sector our write never touches, and an
   alias check between two addresses that hold identical bytes under either
   hypothesis. Before running a check, ask "if I'm wrong, what do I see?" If
   there is no answer, the test is theatre.
4. **A backup `.bin` is not the radio.** The read path is sparse and the buffer
   starts zeroed, so unread regions silently read as zeros. Confirm a region is
   in the read path before believing a byte of it, or use `dump-range`, which
   reads exactly what you ask for.
5. **Prefer a vendor trace over a hardware experiment.** Two USB captures answered
   in one pass what a night of before/after dumps could not, cost nothing, and
   broke no radios. Sniff first.
6. **Verify an offset against a CPS-programmed radio before writing it**, not
   against qdmr's D868UV constants. Confirmed wrong for the II Plus so far: the
   zone channel list, and the contact ID table (`0x04340000` — the vendor uses
   `0x04800000`).
7. **A passing test suite proves nothing here.** The mock transport happily
   accepts writes a real radio mishandles — it has no concept of sector erase.
   Only a CPS read confirms a fix.
8. **Baseline before experimenting**, with `dump-range`, over every *sector* you
   might write. Both recoveries needed a firmware flash precisely because the
   original bytes were never captured.
9. **When a region turns out to be unsafe, keep it in `REGIONS`** so backups still
   capture it and old `.bin` files still parse; just skip it in the write path
   and pin that with a test that fails if someone re-adds it.
