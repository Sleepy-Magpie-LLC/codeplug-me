# QYT KT-5000 protocol and memory map

Reference for the second radio this app programs. The AnyTone equivalent is
`codeplug-memory-map.md`; this document mirrors its structure.

Everything here is derived from two sources:

1. **USB captures of the vendor Windows CPS** talking to a real KT-5000, kept in
   `docs/reference/QYT/` (gitignored). Eight raw byte streams covering a full read,
   a full write, a config-only read, and a config-only write, each split into host
   and radio directions.
2. **CHIRP's `chirp/drivers/btech.py`**, read as a hardware-facts reference for the
   field layouts our captures could not resolve, then reimplemented independently
   in Rust. CHIRP is GPLv3 and this project is MIT, so no code was copied; this is
   the same treatment `PLAN.md` already documents for qdmr. Working notes with the
   verbatim struct quotes are in `docs/reference/qyt-kt5000-chirp-notes.md`.
   Sources: <https://raw.githubusercontent.com/kk7ds/chirp/master/chirp/drivers/btech.py>
   and <https://github.com/kk7ds/chirp/pull/1302>.

Claims below are marked **confirmed** when they hold in our own capture bytes, and
**CHIRP only** when they come from the driver without a capture example.

---

## Wire protocol

A different protocol family from the AnyTone: 16-bit addresses, no checksums,
different command bytes, and asymmetric read/write block sizes.

| Step | Bytes |
|---|---|
| Host handshake | `55 20 21 01 23 FF DC 02` |
| Radio identity reply | 61 bytes: `A1 20 00 00 00 00 02 00 02 00` `06` + 10-byte header + `"KT-500"` + same header + `"WP9114"` + `04 00 00 04 08 00 00 01` twice + `55 05` |
| Host session prologue | `06 53 3D F0 10` |
| Prologue reply | `58 3D F0 10` + 16 bytes, **no leading `06`** |
| Host read | `06 53 AH AL LL` where `LL` = `0x40` |
| Read reply | `06 58 AH AL LL` + `LL` bytes, **with leading `06`** |
| Host write | `06 58 AH AL LL` + `LL` bytes, `LL` = `0x10` |
| Write reply | bare `06` |
| End of a read session | host sends one bare `06` |
| On port close | radio emits `A1 20 00 00 00 00 02 00 00 00` `06` + a 106-byte NXP UART demo banner, then reboots |

`0x53` is ASCII `'S'` (read), `0x58` is `'X'` (write). Addresses are **16-bit
big-endian**. There is **no checksum anywhere** in either direction. All confirmed.

### The asymmetric leading `0x06`

The prologue reply omits the leading `0x06`; every data read reply carries it. Our
`read_block` therefore tolerates an optional leading `0x06` before the `0x58`. That
single tolerance also absorbs stray chatter, which is why no resync machinery is
needed. Confirmed against all four capture pairs.

### The `0x3DF0` prologue

Every operation, read or write, begins by reading 16 bytes at `0x3DF0`. The radio
returns 12 ASCII spaces followed by four zero bytes. Its purpose is unknown; a
firmware/region probe and a link-alive check are both plausible. We reproduce it
because it costs one 16-byte read and omitting an observed handshake step is the
riskier choice.

### Stream arithmetic

These reconcile exactly, which is what makes byte-identical golden tests possible
without hardware (`core/tests/qyt_capture.rs`):

| Capture | Bytes | Breakdown |
|---|---|---|
| `read-radio_write` | 1019 | 8 + 5 + 201x5 + 1 |
| `read-radio_read` | 14067 | 61 + 20 + 201x69 + 117 |
| `write-radio_write` | 16477 | 8 + 5 + 784x21 |
| `write-radio_read` | 982 | 61 + 20 + 784x1 + 117 |

Note **201** reads, not 200: the vendor CPS reads address `0x0000` **twice** before
walking `0x0000` to `0x31C0` in `0x40` steps. We reproduce the duplicate read so our
host stream matches the vendor's byte for byte. Writes cover `0x0000` to `0x30F0` in
`0x10` steps, 784 blocks.

### Reboot behaviour

**The radio reboots after every read or write, and there is no exit command.** So
there is no in-session read-back verification of a write; the vendor CPS does not
attempt one either. Verification is always a separate post-reboot session: close the
port, wait, reconnect, read. This is the main operational difference from the
AnyTone path, whose `write_region` verifies each block inline.

---

## Memory map

Image is `0x0000`-`0x31FF`, 12800 bytes. An unprogrammed channel slot is all `0xFF`.

| Range | Contents | Status |
|---|---|---|
| `0x0000`-`0x0C7F` | Channel records, 200 x 16 B | confirmed |
| `0x0C80`-`0x0DFF` | `0xFF` | confirmed |
| `0x0E00`-`0x0E35` | Settings header (modelled) | confirmed |
| `0x0E36`-`0x0EFF` | Further settings, **not modelled**, byte-preserved | see below |
| `0x0F00`-`0x0F7F` | Four 32-byte VFO-shaped slots; only A and B are real VFOs | confirmed |
| `0x0F80`-`0x0FDF` | CHIRP's `poweron_msg` (8 lines x 8 chars); all `0xFF` on our radio | CHIRP only |
| `0x0FE0`-`0x0FEF` | LCD static message, ships as ASCII `Model:KT-5000   ` | confirmed |
| `0x1000`-`0x1C7F` | Channel names, 200 x 16 B, index-parallel | confirmed |
| `0x2000`-`0x23EF` | Real writable memory; holds whatever was last written. Preserve it. | confirmed |
| `0x2AF0`-`0x2AFF` | Factory blob | confirmed |
| `0x3000`-`0x30FF` | DTMF / autodial memories, 16 records | hypothesis |
| `0x3100`-`0x31FF` | `0xFF`; read but **never written** | confirmed |
| `0x3DF0`-`0x3DFF` | Prologue probe target | confirmed |

### The static message at `0x0FE0`

This is the line the radio draws on its own screen, CHIRP's `static_msg`, and it
is user text rather than an identity field. It only reads like one because the
factory ships it as `Model:KT-5000` followed by three spaces, which is what a
stock radio shows and what put "Model:KT-5000" on the display.

The app writes it space-padded to all 16 bytes, exactly the shape the factory
uses, so no `0xFF` ever lands in a field the firmware draws. Values are limited
to printable ASCII for the same reason.

The field is 16 bytes wide but the LCD only draws the first 13, which is why
the factory value is exactly 13 characters. The app caps user text at 13 so a
message cannot silently lose its tail on the radio's screen; bytes 14-16 stay
spaces.

CHIRP also declares a `poweron_msg` block of eight 8-character lines just below
it at `0x0F80`. Every byte of it is `0xFF` in both of our captures, so it is
unwritten on this radio and the app leaves it alone.

One consequence worth recording: the file-format detector used to identify a
KT-5000 image by the `Model:` prefix at `0x0FE0`. It no longer can, since a user
who renames the message would have made their own saved images unopenable. QYT
images are now recognized by their 12800-byte length plus a rejection of an
image that is entirely `0xFF` or `0x00`, which is a failed read rather than a
codeplug.

Channel capacity of 200 is confirmed two ways: CHIRP declares `_upper = 199`, and our
capture holds 159 programmed plus 41 empty slots, summing to exactly 200. The map is
also self-consistent: 200 x 16 = `0x0C80` fits under the settings block at `0x0E00`,
and `0x1000` + `0x0C80` = `0x1C80` fits under `0x2000`.

---

## Channel record

16 bytes at `0x0000 + index * 16`, with the name at `0x1000 + index * 16`.

| Off | Field | Encoding |
|---|---|---|
| 0-3 | RX frequency | packed BCD **little-endian**, 10 Hz units |
| 4-7 | TX frequency | same; `FF FF FF FF` means TX not set |
| 8-9 | RX tone | `u16` little-endian, see below |
| 10-11 | TX tone | `u16` little-endian |
| 12 | `unknown0:4, scode:4` | low nibble is the PTT-ID signal code index |
| 13 | `unknown1:2, spmute:2, unknown2:2, optsig:2` | |
| 14 | `unknown3:3, scramble:1, unknown4:2, power:2` | |
| 15 | `unknown5:1, wide:1, unknown6:2, bcl:1, add:1, pttid:2` | |

**Bit sub-fields are listed MSB first**, so the first-named sub-field occupies the
high bits. In byte 15 that puts `wide` at bit 6, `bcl` at bit 3, `add` at bit 2, and
`pttid` in bits 1-0.

The frequency encoding differs from the AnyTone, which uses BCD **big-endian**. Worked
example, channel 0: `50 62 25 46` reversed is `46 25 62 50`, read as decimal digits
46256250, times 10 Hz = **462.56250 MHz**, which is FRS/GMRS channel 1. Confirmed.

### Flag bits confirmed on hardware, 2026-08-06

The original captures could not resolve these: every programmed channel carried
identical flag bytes, so there was nothing to diff. The owner then changed one field at
a time in the vendor software, and a fresh read was diffed against the image we had just
written. Thirteen bytes changed, all exactly where CHIRP said they would be.

| Field changed | Slots | Byte | Transition | Conclusion |
|---|---|---|---|---|
| Power to Low | 0-2, 163-165 | 14 | `0x00` -> `0x01` | **power = byte 14 bits 1-0**, `0` = High, `1` = Low |
| Scan to skip | 3-5 | 15 | `0x04` -> `0x00` | **add = byte 15 bit 2**, set means in scan, clear means skipped |
| BCL on | 6-8 | 15 | `0x04` -> `0x0C` | **bcl = byte 15 bit 3** |

Pinned by `flag_bits_match_the_hardware_diff` in `core/src/qyt/codeplug/channel.rs`.

**`add` is the one flag bit with a setter.** `QytChannel::set_scan_add` rewrites bit 2 of
byte 15 and nothing else, and the edit spec exposes it as `scan_add` on a channel update
or add. Power, BCL and the rest stay read-only: confirming that a bit *decodes* correctly
is not the same as confirming the radio accepts a write there, and scan-skip is the only
one anyone has asked to change.

**`unknown6` (byte 15 bits 5-4) looks vestigial.** Slot 0 was the only channel that ever
carried `0x24` rather than `0x04`. Editing its power in the vendor software cleared those
two bits to `0x04`, so the vendor tool does not preserve them and the radio did not mind
losing them. We still round-trip them untouched: nothing is gained by clearing bits we do
not understand.

**Bandwidth is still unconfirmed.** No channel had its bandwidth changed in this pass, so
`wide` (byte 15 bit 6) remains CHIRP-derived. Every channel reads `0`, which
`MODES = ["FM", "NFM"]` makes wide, and that matches a GMRS plus 2 m / 70 cm mix.

Value tables (**CHIRP only** except the three confirmed above, and consistent with every
captured value):

| Field | Values |
|---|---|
| `power` | 2 bits, only 2 used: `0` = High (25 W), `1` = Low (5 W) |
| `wide` | `MODES = ["FM", "NFM"]`, so **`0` = wide, `1` = narrow** |
| `add` | `0` = skipped, `1` = in scan |
| `bcl`, `scramble` | booleans |
| `pttid` | `OFF`, `BOT`, `EOT`, `BOTH` |
| `optsig` | `OFF`, `DTMF`, `2TONE`, `5TONE` |
| `spmute` | `Tone/DTCS`, `Tone/DTCS and Optsig`, `Tone/DTCS or Optsig` |
| `scode` | index into codes 1-15; raw `15` is treated as `0` |

Note `wide` = 0 meaning wide is easy to misread as "everything is narrow". All 159
programmed channels in the capture read 0, which is correct for a mix of GMRS and 2 m
/ 70 cm repeaters.

### Byte 15 bits 5-4 are unnamed

CHIRP calls these `unknown6` and does not name them in **any** BTECH format in the
file. They are the entire difference between channel 0 (`0x24`) and every other
programmed channel (`0x04`). We round-trip them opaquely and never synthesize a
value. `unknown6_bits_survive_a_roundtrip` asserts this.

### Names

16 bytes, maximum 7 ASCII characters, `0xFF`-padded. An empty slot's name record is
all `0xFF`. Confirmed: 157 of 159 records have `0xFF` from offset 7 onward; two carry
a stray `0x05` at offset 7, which is leftover garbage rather than a field.

---

## Tone encoding

One scheme covers CTCSS and DCS in the same `u16`:

| Raw value | Meaning |
|---|---|
| `0x0000` | no tone |
| > 600 | CTCSS, frequency in 0.1 Hz units |
| 1-105 | DCS normal |
| 106-600 | DCS inverted (**CHIRP only**, no capture example) |

DCS values index `sorted(DTCS_CODES + [645])`.

Worked examples, all confirmed: `0x0375` = 885 = **88.5 Hz** CTCSS; `0x03E8` = 1000 =
**100.0 Hz**. The six distinct small values in the capture (1, 3, 15, 16, 41, 88)
decode to **D023N, D026N, D073N, D074N, D245N, D606N**, all real DCS codes, which is
strong confirmation of the index scheme. Split tone is supported and used: channel 64
(`N7FYZ`) has no RX tone and a 100.0 Hz TX tone, the classic repeater configuration.

The inverted branch is the same arithmetic on the other side of the 105 boundary, but
nothing in the capture exercises it, so treat it as unverified until hardware says
otherwise.

---

## VFO channels

The image holds four 32-byte slots at `0x0F00` (A–D), inherited from the WP-9900
layout. **The KT-5000 only has two VFOs (A and B).** Slots C and D are parsed and
written back verbatim so they survive a round trip, but they are not exposed in
the dump, the UI, or the edit path.

| Off | Field |
|---|---|
| 0-7 | frequency digit string |
| 8-13 | offset |
| 16 | RX tone |
| 18 | TX tone |
| 20 | scode |
| 21 | spmute |
| 22 | optsig |
| 23 | scramble |
| 24 | wide |
| 25 | power |
| 26 | shift direction |
| 27 | step |

### The encoding divergence, read this before touching VFO frequencies

**`freq[8]` does NOT use the packed BCD of the channel table. It stores one decimal
digit per raw byte.** Confirmed by decoding record 0 to 462.5625 MHz, which matches
channel 0 exactly.

This is the sharpest trap in the format. Reading these bytes as packed BCD does not
fail loudly; it produces a plausible-looking wrong frequency. The two decoders are
kept deliberately separate in `core/src/qyt/codeplug/bands.rs` for that reason, and
`band_frequency_uses_one_digit_per_byte` pins it.

---

## Write policy

**`0x3100`-`0x31FF` is read but never written.** It reads back as `0xFF` and the
vendor CPS never writes it. Corroborated independently: CHIRP's `UPLOAD_MEM_SIZE` is
`0x3100`, and 784 writes x `0x10` lands exactly there. We read it into the image so
backups stay faithful and diffable, and exclude it from the write map.
`qyt_write_never_touches_0x3100_or_above` asserts this.

**`0x2000`-`0x23EF` is never written at all.** Since 2026-09-09 the write path skips
those 63 blocks, so they keep whatever the radio already holds. Together with the
duplicated first block below, `WRITE_BLOCKS` is 722, not the vendor's 784.

**`0x0000` is sent twice.** Since 2026-10-16 a full write opens with a duplicate of the
first block. The radio acks the second write of a session but does not keep it (see
"The second write of a session is dropped"). Without the duplicate that was always
`0x0010`, channel 1's record, so every write left channel 1 with its old Rx/Tx
frequencies and every post-write verify reported 8 bytes at `0x0010`. Re-sending
identical bytes is idempotent, so the duplicate absorbs the drop. Pinned by
`a_dropped_second_write_lands_on_the_duplicate_not_on_real_data` and the capture test.
**Not yet confirmed on hardware.**

An earlier version of this document claimed the radio read that region back as `0xFF`
regardless of what was written, and concluded the vendor's bytes were a harmless
uninitialized buffer. **That was wrong**, and the mistake is worth recording because it
is easy to repeat: the `read-radio` capture was taken *before* the `write-radio`
capture. The read showed `0xFF` because the write had not happened yet, not because the
region rejects writes.

Confirmed on hardware 2026-08-06: a fresh read of the same radio returns those 950
bytes **byte-for-byte as the CPS wrote them**. Some of it looks structured rather than
random (for example an ascending `07 04 04 04 05 05 06 06` run at `0x2100`), so do not
assume it is junk.

**Why skipping beats preserving by round trip.** Until 2026-09-09 the rule was that the
write sends exactly what the image holds, so a read-modify-write round trip preserves
the region automatically and only an image synthesized from scratch could blank it. That
held right up until an image that had never been near this radio was written from the
Android app and erased all 950 bytes. Nothing models those bytes, so no edit can ever
produce a meaningful value there: the only two outcomes a write can have are preserve or
destroy, and skipping the range removes the second one. `qyt_write_leaves_the_calibration_window_exactly_as_the_radio_had_it`
pins it, and the golden capture test pins that the skip is the *only* divergence from
the vendor stream.

Restoring the region from a backup is still possible, deliberately through the
explicit-address path instead: `codeplugme-cli rescue write <backup.bin> --address 0x2000
--length 0x3F0 --force`.

**General lesson for this codeplug.** Two captures taken at different times are not two
views of one state. Diffing them shows change over time, not a property of the radio.
Any conclusion of the form "the radio ignores writes to X" needs a write followed by a
read in that order, on the same radio, with a reboot in between.

---

## Settings

**`0x0E00`-`0x0E35` is modelled**: `tmr`, `squelch`, `tot`, `volume`,
`backlight_timer`, `beep`, `scan_rev`, and `remember_power`. Unnamed bytes inside
that range live in `raw` and are never rewritten.

`tmr` is Menu 0 TMR (dual-band waiting): **Off / On only**, matching the KT-5000
manual. CHIRP's WP-9900 `LIST_TMR7` (`M+A`…`M+ABC`) does not apply; this radio
has no A/B/C watch areas and no scan lists.

### Settings confirmed on hardware, 2026-08-06 / 2026-08-14

One-field-at-a-time diffs. Each variant differs from its baseline by exactly one
byte:

| Field changed | Address | Transition | Conclusion |
|---|---|---|---|
| Squelch 6 → 3 | `0x0E02` | `0x06` → `0x03` | **sql already modelled**; confirms the offset |
| Beep off → on | `0x0E09` | `0x00` → `0x01` | **beep = bit 0 of `0x0E09`** (matches CHIRP) |
| Scan rev CO → SE | `0x0E11` | `0x01` → `0x02` | **scan_rev at `0x0E11`**, not CHIRP's `0x0E12` |
| Scan rev CO → TO | `0x0E11` | `0x01` → `0x00` | values `0`=TO, `1`=CO, `2`=SE |
| Remember power (menu) | `0x0E35` | `0x00` ↔ `0x01` | **remember_power at `0x0E35`**, not CHIRP's `reps`. API/UI is inverted vs the stored byte: on ↔ `0x00`, off ↔ `0x01`. |

Pinned by `hardware_cps_diffs_match_the_documented_offsets` in
`core/src/qyt/codeplug/settings.rs`. Captures for the last row live in
`docs/reference/QYT/Configs/power-remember/`.

**`scan_rev` and `remember_power` are the instructive disagreements with CHIRP.**
The WP-9900 struct puts `screv` at `0x0E12` and `reps` (tone-burst frequency) at
`0x0E35`. On the KT-5000 those offsets hold scan resume and remember-power
instead. Shipping CHIRP's labels would write the wrong meaning while the UI
claimed success. This is why settings stay confirmed-only.

`remember_power`: when on, applying supply power restores the radio's last
on/off state; when off, applying power always leaves the radio off. The byte at
`0x0E35` is stored inverted relative to that API value (`true` ↔ `0x00`,
`false` ↔ `0x01`).

Everything from `0x0E36` to `0x0FFF` is **byte-preserved and not modelled**.

This is deliberate. CHIRP sets `has_settings = False` for the KT5000, and its own class
docstring states that the memory map inherited from the WP-9900 is wrong for this
model, giving the example that setting "C display mode" overwrites VOX. Three fields
(`pttid`, `anil`, `tmrtx`) hold values in our capture that are out of range for the
lists CHIRP assigns them, which CHIRP silently clamps before display. Shipping those
labels would mean writing a field the user did not intend to change.

Further names still come from the same hardware one-field-diff pass: change one setting
at a time, dump, and diff. `testUnmodelledSettingsBytesSurviveAnEdit` guarantees the
unmodelled bytes survive an edit untouched.

---

## Hardware verification, 2026-08-06

Run against a real KT-5000 over `/dev/cu.usbserial-10`.

| Check | Result |
|---|---|
| Identify | `KT-500 WP9114`, matching the captures |
| Full read | 12800 bytes, 201 blocks, parses to 159 channels (max index 165) |
| Read vs vendor capture | identical except 2 settings bytes the owner had changed, plus `0x2000`-`0x23EF` (see below) |
| Full write | 784 blocks accepted |
| Read-back vs pre-write backup | **exactly 6 bytes differ**, the intended channel rename at `0x1A50` |
| Read-back vs transmitted image | **0 bytes differ** below the write limit |
| `0x3100`-`0x31FF` | untouched, as intended |
| `0x2000`-`0x23EF` | preserved exactly |

Three bugs only hardware could find, all fixed:

1. **The handshake was over-strict.** The `A1 20 …` prefix is a port-state frame, not
   part of the identity reply, and this radio did not send one. Now optional. See the
   note on [`STATUS_LEN`].
2. **The write path was broken.** `write_verified_image` called `identify()` and then
   `write_image()`, which is two sessions and therefore two handshakes. The KT-5000
   accepts exactly one per open link, so every write would have failed. The identity
   check now happens inside the write session, and the mock enforces one handshake per
   link so this cannot regress silently.
3. **The `0x2000`-`0x23EF` assumption was wrong**, as described in the write policy
   section above.

A second pass the same day confirmed the channel flag bits by changing power, scan-skip,
and BCL one field at a time and diffing. See "Flag bits confirmed on hardware" above.
Bandwidth remains the one unconfirmed flag.

A third pass confirmed three settings bytes via config-only writes (beep, scan_rev,
squelch). See "Settings confirmed on hardware" above. `scan_rev` sits at `0x0E11`, one
byte earlier than CHIRP's WP-9900 map.

A fourth pass on 2026-08-14 confirmed `remember_power` at `0x0E35` (CHIRP's `reps`
slot) by flipping the menu setting on the radio and dumping before/after.

## Recovering a radio that will not stay up

Written 2026-09-09, after an Android write left a KT-5000 rebooting a few
seconds after every boot. Holding MENU during power-on stops the reboot but
lands in a password prompt with no programming interface.

**The arithmetic is the whole problem.** At 9600 baud a full write is 784 blocks
of 21 host bytes plus an ack, about 18 seconds on the wire; a full read is about
15. A radio that resets a few seconds after power-on can never be recovered by
`restore`, no matter how well timed.

So `core/src/qyt/rescue.rs` does the same protocol over **one address range**.
`0x0E00`-`0x0FFF`, the settings header plus the four VFO records, is 32 write
blocks: under a second, which fits in almost any window. The CLI wraps it as
`codeplugme-cli rescue {watch,knock,read,write}`.

The order that gets the most information for the least risk:

1. **`rescue watch`** sends nothing and timestamps everything the radio says.
   The radio prints an NXP SDK UART banner on boot (see the port-close row in
   the wire protocol table above), so a boot loop is directly visible as that
   banner repeating, and the gap between repeats is the loop period.
2. **`rescue knock`** retries the handshake until the radio answers, then keeps
   probing `0x3DF0` and reports how long the link stayed alive. This is the
   number that decides everything: if the link survives a minute, an ordinary
   `restore` will fit and nothing here is needed. Each attempt is its own open
   and close, because the radio accepts exactly one handshake per open link.
3. **`rescue read --address 0x0E00 --length 0x200`** pulls the settings and VFO
   block out of a caught window. Diff it against the pre-write backup; the
   difference is the damage.
4. **`rescue write <backup.bin> --address ... --force`** puts the backup's bytes
   back over just that range.

**Where to look first, and why.** The channel table is bounded data the firmware
only reads when you tune to a channel; the settings header and the VFO records
are read at boot. The VFO records are also the one place in this format where a
plausible-looking wrong value is easy to produce, because `freq[8]` stores one
decimal digit per byte while the channel table uses packed BCD (see "The
encoding divergence" above), and `Band::set_frequency` range-checks only that the
value fits in eight digits, not that the radio can tune it. An out-of-band VFO
frequency is a good candidate for a radio that boots, tries to tune, and resets.

### What the window actually turned out to be, 2026-09-09

Measured, not guessed. `rescue watch` listens without sending anything, and the
radio prints its 107-byte NXP SDK boot banner on every reset:

    banner bursts at: 5.17s, 16.91s, 28.85s
    periods:          11.74s, 11.94s

**A metronome-regular reset every ~11.9 seconds.** Regular is the important
word: a supply sagging under load would be irregular and would move with what
the radio is doing, and this does not move at all. It fires while the radio sits
idle and it fires while the radio is busy answering reads, which is the shape of
a watchdog that nothing is kicking, with the serial side still alive because it
is interrupt-driven (the banner names the SDK's interrupt UART example).

So the usable window is `11.9s` minus however late in the cycle the handshake
landed, which is why measurements ranged from 6 to 11 seconds. A full read needs
about 15 seconds and a full write about 18. Neither fits, ever.

### Transfers span boot cycles

The answer is not to time the knock better, it is to stop needing one window.
Each block carries its own address, memory survives the reset, and re-sending
identical bytes is idempotent, so [`Chunked`] runs until the link dies, reports
how far it got, and the caller knocks again and resumes. A full write took 26
windows and landed all 784 blocks.

### The second write of a session is dropped

Found while restoring `0x2000`-`0x23FF` across eight windows: **every block that
failed to stick was the last one of its window**, and the failures fell in a
perfect alternating pattern because each window happened to carry two blocks.
Repeating the write with the range starting one block later landed the missing
block every time.

The radio acknowledges a write on receipt and commits it afterwards, so a reset
arriving in between loses a block the host was told it had. `Chunked::resume`
therefore gives back the last acknowledged write block whenever a window ends
badly, which costs one re-sent block and nothing else. Pinned by
`a_broken_write_window_gives_back_its_last_block`.

**A write is not confirmed until it is read back.** On a radio that is resetting
under the transfer, the acknowledgement is not proof. `rescue verify` is the
proof.

### A byte the radio writes itself

After a byte-exact restore, two addresses still read back different, repeatedly,
no matter how many times they are written. Only `0x0E51` turned out to be the
firmware. `0x001E` is in block `0x0010`, the dropped second write of every session, so
it was a lost write and is no longer ignored by verify (2026-10-16):

| Address | Backup | Radio | Note |
|---|---|---|---|
| `0x001E` | `0x00` | `0x01` | channel 1, byte 14 |
| `0x0E51` | `0x3A` | `0x04`, then `0x05` | **increments between reads: a counter** |

`0x0E51` incrementing on its own is the useful one. It says the firmware writes
to its own configuration memory while it is running, so a diff against a backup
will never be empty, and it is not evidence of a failed write.

### `0x2000`-`0x23EF` was erased, and restoring it changed nothing

On the boot-looping radio this region read back **entirely `0xFF`** while the
August backup held 950 bytes of structured data: repeating 32-byte high-entropy
blocks at `0x2000`-`0x20FF`, then small ascending values (`07 04 04 04 05 05 06
06`) that look like per-band tables. Calibration is the obvious guess.

That is a real defect, caused by writing an image whose bytes there were `0xFF`
(the write path sent the image's bytes, so a stale image blanked the region).
It was restored byte for byte. **The boot loop did not change, not even in
period.** Worth knowing both ways: the region is recoverable, and it is not what
this fault is about.

**Fixed at the source on 2026-09-09**: the write path no longer sends that range at
all, on any host. See the write policy section above. Two things were also added on the
Android side, because the erasure went unnoticed for days: a write is followed by a
read-back that diffs what the radio kept against what was sent (acks are not proof, as
the section on dropped blocks below explains), and the write confirmation now says
whether the codeplug came off a radio or out of a file.

### Memory above `0x3200`

Reads are harmless and the prologue already proved the radio answers up here, so
`rescue read` may go anywhere in the 16-bit space. `0x3200`-`0x3FFF` on a real
radio, everything not listed being `0xFF`:

| Range | Contents |
|---|---|
| `0x3200`-`0x3B7F` | `0xFF` |
| `0x3B90` | `20 12 23 34 45 56 67 78 20 20 20 20 00 00 00`, an ascending BCD-looking pattern that reads as a factory default rather than a set value |
| `0x3C00`-`0x3D0F` | tables of small values, several rows of the shape `26 14 15 16 17 18 1B 1E 21 23 25 …`; more calibration |
| `0x3D80`-`0x3D9F` | the identity block the handshake returns, `KT-500` and `WP9114`, stored verbatim |
| `0x3DE0`-`0x3DFF` | the prologue window: 12 spaces then zeros |
| `0x3F60`-`0x3FFF` | ASCII `0` fill with `WP9114` embedded at `0x3F70` |

None of it is written by anything in this codebase.

### Resolved: a faulty faceplate extension cable

**The reset loop was the Cat-5 cable extending the detachable head to the front
of the vehicle.** Swap in another cable and the radio boots and stays up; plug
that one back in and the loop returns. The owner found it after everything below
had ruled out the codeplug.

Every measurement fits once you know it. The head talks to the body over that
cable; a main loop stuck waiting on the head never kicks the watchdog, and a
watchdog fires on a fixed interval no matter what else is happening, which is the
11.9-second metronome. The display still drew channels because enough of the link
worked to push data to the head. The programming interface kept answering
throughout because it is a separate, interrupt-driven UART that does not care
whether the main loop is alive.

**The lesson worth keeping: a steady reset period plus a programming interface
that still answers means the main loop is blocked on a peripheral, not that the
data is bad.** On this radio the head link is the peripheral to suspect first,
and it is the one that runs through a user-replaceable cable.

The write did do real damage, and that part was not a coincidence: it erased
`0x2000`-`0x23EF`, described below. That erasure and the boot loop were two
unrelated faults arriving at the same time, which is what made this hard.

### The codeplug is not the fault

The decisive test, run 2026-09-09: restore the whole August image, verify it
byte for byte, and watch. The radio ended up holding exactly the 12,800 bytes it
booted fine on in August, apart from the two bytes it maintains itself, and it
kept resetting on the same ~11.9-second period.

So nothing in the codeplug explains this, and no further writing will help. What
that leaves is the firmware in the MCU's own flash, or the hardware: a main loop
stuck waiting on something that never answers would hang exactly like this while
leaving the interrupt-driven serial side able to answer every command we send.
That is what it turned out to be; see the resolution above.

### One wrong turn, kept because the mistake is repeatable

Between the two measurements above sat a false conclusion worth recording: a
small `rescue write` completed without the radio rebooting, and that was read as
"the radio does not reset while it holds a programming session, so the window is
not the constraint". It was really just a 32-block write that happened to fit in
a window. One transfer finishing proves nothing about how long the link lives;
only timing the link does, which is what `rescue knock` and `rescue watch` are
for. **Measure the thing itself, do not infer it from something that succeeded.**

**If the serial path never answers**, the remaining options are outside this
codebase: the vendor firmware updater, which runs from a bootloader that does not
read the codeplug at all, or reprogramming the memory directly on the board. The
AnyTone equivalent of this incident is in `diagnosing-cps-breakage.md`, and both
of those recoveries needed a firmware flash.

## Link settings

**9600 8-N-1.** Not 115200, which is what this repo asked for until 2026-09-04 and
which worked anyway on macOS: the OS PL2303 driver was evidently not applying the
rate it was given, and the chip stayed at its 9600 default. Android drives the same
chip itself through `usb-serial-for-android`, applied 115200 for real, and the radio
went silent with the handshake going out and nothing coming back.

Found by the link probe in `android/.../RadioSession.kt`, which handshakes across
both rates with the control lines each way and reports what answered. Only
`9600, DTR/RTS on` replied, with `KT-500 WP9114`. The rate now lives in
`Model::baud` and every host surface reads it from there, so the macOS app, the
CLI, and the Android app cannot drift apart again.

9600 also matches the CHIRP drivers for this radio family, which is the
corroboration that was available all along and went unnoticed because the desktop
worked.

## Known unknowns

| Item | Status |
|---|---|
| USB VID/PID | **There is none to find.** Confirmed 2026-08-06: the cable enumerates as `067b:2303`, a generic Prolific PL2303 shared with thousands of unrelated devices. Attributing that to the KT-5000 would claim every PL2303 adapter is this radio, so the explicit model picker is the correct permanent design, not a stopgap. `KNOWN_USB_IDS` deliberately has no QYT entry. |
| Byte 15 bits 5-4 (`unknown6`) | Unnamed in CHIRP too, and cleared by the vendor software when a channel is edited, so probably vestigial. Round-tripped opaquely regardless. |
| Settings `0x0E36`-`0x0FFF` | Partially modelled through `0x0E35`; the rest awaits further one-field diffs. |
| DCS inverted branch | CHIRP only, no capture example. |
| `0x3000`-`0x30FF` | Believed DTMF/autodial memories. Not modelled. |
| `0x3DF0` prologue purpose | Unknown. Reproduced because it is observed. |
| Channel flag bits: power, scan-add, BCL | **Confirmed on hardware 2026-08-06** by a one-field-at-a-time CPS diff. `scan-add` is writable; the other two are read-only. |
| Channel flag bit: bandwidth | Still CHIRP-derived. No channel had its bandwidth changed during the hardware pass. |
| Settings: beep, scan_rev, squelch, remember_power | **Confirmed on hardware** by one-field diffs. `scan_rev` and `remember_power` offsets disagree with CHIRP. |
