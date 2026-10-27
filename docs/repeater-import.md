# Repeater import

Both apps can fill a codeplug with repeaters near a location, pulled from open
repeater databases. You pick which ones go in.

There are two ways to use the picks:

- **Add to Codeplug (the default):** they're staged into the open
  codeplug as one unsaved change. Undo removes the whole import, and nothing
  reaches the file until you Save.
- **Save as New File… (macOS) / Save as new (Android):** the open
  codeplug plus the picks are written to a new file, which then opens. The
  open codeplug is left alone.

Both apps keep the import window's location, sources, results and options
until the app quits. Rows you have added show as "already in codeplug". If
you open a different codeplug, the results are cleared but the search form
is kept.

The logic lives in `core/src/geo/`, so macOS, Android and the CLI share one
implementation. The apps only geocode the location and draw the picker.

## Sources

| Source | Key needed | Coverage | Coordinates | Modes |
| --- | --- | --- | --- | --- |
| [Hearham](https://hearham.com/repeaters) | none | worldwide, about 23k repeaters | yes | FM, DMR (color code in the tone field) |
| [RadioID](https://radioid.net) | none | about 11k registered DMR repeaters | no, city and state only | DMR |
| [RepeaterBook](https://www.repeaterbook.com/wiki/doku.php?id=api) | your own `rbuapp_` token | best North American coverage | yes | FM, DMR |

How each source is used:

- **Hearham** and **RadioID** each publish one file covering the whole world.
  It is downloaded once and kept in the app's cache directory for 24 hours.
- **RadioID records have no coordinates.** They are filtered to the query's
  state, then given coordinates from a Hearham or RepeaterBook record with the
  same callsign and frequency. Ones with no match are still listed, at the
  bottom, with no distance.
- **RepeaterBook** needs approved access. Each user makes a personal token
  under *API Apps* on repeaterbook.com and enters it, plus a contact email, in
  the app's settings.
  - The email goes in the User-Agent, which RepeaterBook requires.
  - Results are fetched live and never cached, because RepeaterBook's terms
    rule out offline copies.
  - The app shows "Data courtesy of RepeaterBook.com" whenever RepeaterBook
    results are listed.
  - The API has no radius search. US queries go by state (FIPS `state_id`);
    other countries go by country, and the core filters by distance.

RadioReference is not supported. Its API needs a developer key plus each user's
Premium login.

## The KT-5000 and DMR

The KT-5000 is analog only. For a KT-5000 codeplug:

- RadioID is never queried.
- DMR records from the other sources are dropped.
- A build that somehow includes one is rejected.

## Clean vs Merge

**Merge** keeps everything in the base codeplug and adds the picked repeaters
into free channel slots. Any repeater whose frequencies (and DMR color code)
are already in the base is marked "already in codeplug" and can't be picked.

**Clean** removes the base's channels, zones and scan lists, then adds the
picked repeaters. It does not start from an empty file. Settings, contacts,
radio IDs and every region the editor does not model are kept, and that matters:

- Both radios carry state that is written back byte for byte.
- On the KT-5000 that includes calibration. An image with blank calibration
  can leave the radio unable to boot.

That is why both modes need a real codeplug to start from: one opened from a
file or read from the radio.

## What gets written

Each picked repeater becomes one channel, nearest first.

| | AT-D878UVII Plus | KT-5000 |
| --- | --- | --- |
| Name | `W5LEX 444.850`, or `K1DQ 145.110D` for DMR (16 characters) | the callsign (7 characters) |
| Duplicate names | a number replaces the last characters | same |
| Analog | wide, high power, TX tone only (RX stays on carrier squelch) | TX tone only, in scan |
| DMR | TS1, the listed color code, radio ID 0, a "Local TG9" group contact (created if missing) | not allowed |
| Grouping | one zone named after the import, plus a scan list of the nearest 50 | none (no zones) |

Review DMR channels before transmitting. The databases don't say which talk
groups or time slots a repeater carries.

## Capacity

Capacity is a hard limit, and nothing is silently dropped.

- **The apps keep you inside it.** They show "N selected / M available" while
  you pick. If you go over, they warn you and disable Build until the selection
  fits.
- **The core enforces it too.** It rejects an oversize selection with a message
  saying how many to deselect.

| Radio | Mode | Available |
| --- | --- | --- |
| AT-D878UVII Plus | Clean | 4000 channel slots, but a zone holds 250, so at most 250 per import |
| AT-D878UVII Plus | Merge | the free slots, still capped at 250 per zone. A codeplug with all 250 zone slots used can't take an import until a zone is removed |
| KT-5000 | Clean | 200 |
| KT-5000 | Merge | the free slots |

## Tests

```sh
cargo test -p codeplugme-core geo
# Live check against Hearham and RadioID (downloads about 15 MB):
cargo test -p codeplugme-core live_ -- --ignored --nocapture
```

The parsers are tested against trimmed fixtures in `core/tests/fixtures/geo/`.
The RepeaterBook fixture uses the field names other RepeaterBook clients read,
because RepeaterBook's wiki only lists labels. Check them against a live
response once you have a token.

## Open checks

- **The 878's CTCSS tone index table** (`core/src/anytone/codeplug/ctcss.rs`)
  is ported from qdmr. Compare one imported channel against a CPS-written image
  before trusting it on air.
- **The 878's DCS encoding** follows qdmr too: the octal value, plus `0x200`
  for inverted. Verify it the same way.
