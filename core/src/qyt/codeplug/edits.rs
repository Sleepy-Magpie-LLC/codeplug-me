//! JSON-driven batch editing of a [`QytCodeplug`], the `qyt` half of the edit
//! envelope handled by `codeplugme_apply_edits` and the CLI.
//!
//! The shape mirrors the AnyTone spec: an update array per family (index plus
//! optional fields), `add_channels` (fields with no index), `remove_channels`,
//! and `move_channels`. Within any field object, an absent field means "leave
//! it alone".
//!
//! Settings and VFOs need no add/remove/move machinery. `settings` is a single
//! object of optional fields, and `bands` is an update-by-index array over the
//! radio's two VFOs (A and B). Image slots C and D are not editable.
//!
//! Operations run update → moves → removals → adds, and the result is verified
//! by re-parsing before it is returned.

use serde::{Deserialize, Deserializer};

use super::channel::{Bandwidth, Power, Tone};
use super::{QytChannel, QytCodeplug, NUM_VFOS};

/// Deserialize a field that distinguishes "absent" from "explicitly null".
///
/// Plain `Option<Option<T>>` collapses both to `None`. Wrapping the inner value
/// in `Some` here, together with `#[serde(default)]`, makes absent read as
/// `None` and an explicit `null` read as `Some(None)`. That is what lets an
/// edit clear a channel's TX frequency (making it receive-only) rather than
/// merely decline to change it.
fn present_even_if_null<'de, D, T>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(de).map(Some)
}

/// A batch of QYT edits. Every array defaults to empty.
#[derive(Deserialize, Default)]
pub struct QytEditSpec {
    #[serde(default)]
    channels: Vec<ChannelEdit>,
    #[serde(default)]
    add_channels: Vec<ChannelFields>,
    #[serde(default)]
    remove_channels: Vec<usize>,
    #[serde(default)]
    move_channels: Vec<MoveOp>,
    #[serde(default)]
    settings: Option<SettingsFields>,
    #[serde(default)]
    bands: Vec<BandEdit>,
    /// The ASCII line the radio shows on its LCD. Absent leaves it alone.
    #[serde(default)]
    static_message: Option<String>,
}

/// Relocate the channel at `from` to the free slot `to`.
#[derive(Deserialize)]
struct MoveOp {
    from: usize,
    to: usize,
}

/// Editable channel fields, shared by add and update.
#[derive(Deserialize, Default)]
struct ChannelFields {
    name: Option<String>,
    rx_frequency_hz: Option<u32>,
    /// Explicit `null` clears the TX frequency, making the channel
    /// receive-only; absent leaves it as it was.
    #[serde(default, deserialize_with = "present_even_if_null")]
    tx_frequency_hz: Option<Option<u32>>,
    rx_tone: Option<Tone>,
    tx_tone: Option<Tone>,
    /// Include the channel in scanning; false skips it. The only flag bit that
    /// is writable, because it is the only one confirmed against the radio.
    scan_add: Option<bool>,
}

/// One channel update: which slot, and what to change.
#[derive(Deserialize)]
struct ChannelEdit {
    index: usize,
    #[serde(flatten)]
    fields: ChannelFields,
}

/// Editable settings-header fields. Only the fifty-four bytes at `0x0E00` are
/// modelled; see [`super::settings`] for why.
#[derive(Deserialize, Default)]
struct SettingsFields {
    tmr: Option<u8>,
    squelch: Option<u8>,
    tot: Option<u8>,
    volume: Option<u8>,
    backlight_timer: Option<u8>,
    beep: Option<bool>,
    /// `"TO"` | `"CO"` | `"SE"`.
    scan_rev: Option<String>,
    remember_power: Option<bool>,
}

/// Editable VFO fields. The offset and shift direction are deliberately
/// absent: their encoding is assumed rather than confirmed.
#[derive(Deserialize, Default)]
struct BandFields {
    frequency_hz: Option<u32>,
    rx_tone: Option<Tone>,
    tx_tone: Option<Tone>,
    /// "wide" | "narrow".
    bandwidth: Option<String>,
    /// "high" | "low".
    power: Option<String>,
    step_hz: Option<u32>,
}

/// One VFO update, addressed by index 0 (A) or 1 (B).
#[derive(Deserialize)]
struct BandEdit {
    index: usize,
    #[serde(flatten)]
    fields: BandFields,
}

/// Parse `edits_json`, apply it to a fresh parse of `data`, verify by
/// re-parsing, and return the serialized image. Any failure comes back as a
/// human-readable string.
pub fn apply_edits(data: &[u8], edits_json: &str) -> Result<Vec<u8>, String> {
    let spec: QytEditSpec =
        serde_json::from_str(edits_json).map_err(|e| format!("invalid QYT edits JSON: {e}"))?;
    let mut cp = QytCodeplug::parse(data).map_err(|e| e.to_string())?;

    // --- 1. Per-index channel updates, against the pre-move slot numbers. ---
    for e in &spec.channels {
        let ch = cp
            .channel_mut(e.index)
            .ok_or_else(|| format!("channel {} is not programmed in this codeplug", e.index))?;
        apply_channel_fields(ch, e.index, &e.fields)?;
    }

    // --- 2. Settings and bands: no slots, so order relative to the rest of
    // the batch does not matter. ---
    if let Some(f) = &spec.settings {
        apply_settings_fields(&mut cp, f)?;
    }
    for e in &spec.bands {
        apply_band_fields(&mut cp, e.index, &e.fields)?;
    }
    if let Some(msg) = &spec.static_message {
        cp.set_static_message(msg).map_err(|e| e.to_string())?;
    }

    // --- 3. Moves, then removals, then adds. ---
    for m in &spec.move_channels {
        cp.move_channel(m.from, m.to)?;
    }
    for &i in &spec.remove_channels {
        if !cp.remove_channel(i) {
            return Err(format!("cannot remove channel {i}: not programmed"));
        }
    }
    for f in &spec.add_channels {
        let i = cp.add_channel().ok_or("no free channel slots")?;
        apply_channel_fields(cp.channel_mut(i).unwrap(), i, f)?;
    }

    let out = cp.serialize();

    // --- 4. Round-trip verification: nothing leaves here that does not read
    // back as what was asked for. ---
    let check = QytCodeplug::parse(&out).map_err(|e| e.to_string())?;
    for e in &spec.channels {
        let ch = check
            .channels()
            .find(|c| c.index == e.index)
            .ok_or_else(|| format!("channel {} missing after edit", e.index))?;
        verify_channel(ch, &e.fields)?;
    }
    for &i in &spec.remove_channels {
        if check.channels().any(|c| c.index == i) {
            return Err(format!("channel {i} still programmed after removal"));
        }
    }
    for m in &spec.move_channels {
        if check.channels().any(|c| c.index == m.from) {
            return Err(format!(
                "channel {} still occupies its old slot after the move",
                m.from
            ));
        }
        if !check.channels().any(|c| c.index == m.to) {
            return Err(format!("channel slot {} is empty after the move", m.to));
        }
    }
    if let Some(f) = &spec.settings {
        verify_settings(&check, f)?;
    }
    if let Some(msg) = &spec.static_message {
        // The field is space-padded, and reading it back trims that padding, so
        // compare against the trimmed form the radio will actually show.
        let want = msg.trim_end_matches(' ');
        if check.static_message() != want {
            return Err(format!(
                "static message verification failed: wrote {want:?}, read back {:?}",
                check.static_message()
            ));
        }
    }
    for e in &spec.bands {
        if let Some(hz) = e.fields.frequency_hz {
            let got = check.bands()[e.index].frequency_hz;
            if got != Some(hz) {
                return Err(format!(
                    "band {} frequency verification failed: wrote {hz}, read back {got:?}",
                    e.index
                ));
            }
        }
    }

    Ok(out)
}

/// Apply one channel's optional fields, naming the slot in any error.
fn apply_channel_fields(
    ch: &mut QytChannel,
    index: usize,
    f: &ChannelFields,
) -> Result<(), String> {
    if let Some(name) = &f.name {
        ch.set_name(name);
    }
    if let Some(hz) = f.rx_frequency_hz {
        ch.set_rx_frequency(hz);
    }
    if let Some(hz) = f.tx_frequency_hz {
        ch.set_tx_frequency(hz);
    }
    if let Some(t) = f.rx_tone {
        ch.set_rx_tone(t)
            .map_err(|e| format!("channel {index} RX tone: {e}"))?;
    }
    if let Some(t) = f.tx_tone {
        ch.set_tx_tone(t)
            .map_err(|e| format!("channel {index} TX tone: {e}"))?;
    }
    if let Some(on) = f.scan_add {
        ch.set_scan_add(on);
    }
    Ok(())
}

/// Apply the settings-header fields.
fn apply_settings_fields(cp: &mut QytCodeplug, f: &SettingsFields) -> Result<(), String> {
    let s = cp.settings_mut();
    if let Some(v) = f.tmr {
        s.set_tmr(v)?;
    }
    if let Some(v) = f.squelch {
        s.set_squelch(v)?;
    }
    if let Some(v) = f.tot {
        s.set_tot(v)?;
    }
    if let Some(v) = f.volume {
        s.set_volume(v);
    }
    if let Some(v) = f.backlight_timer {
        s.set_backlight_timer(v)?;
    }
    if let Some(on) = f.beep {
        s.set_beep(on);
    }
    if let Some(label) = &f.scan_rev {
        s.set_scan_rev(super::ScanRev::parse_label(label)?);
    }
    if let Some(on) = f.remember_power {
        s.set_remember_power(on);
    }
    Ok(())
}

/// Apply one VFO record's fields. Only slots A and B ([`NUM_VFOS`]) are
/// editable; C and D exist in the image only.
fn apply_band_fields(cp: &mut QytCodeplug, index: usize, f: &BandFields) -> Result<(), String> {
    if index >= NUM_VFOS {
        return Err(format!(
            "VFO {index} does not exist (this radio has {NUM_VFOS}: A and B)"
        ));
    }
    let band = cp
        .band_mut(index)
        .ok_or_else(|| format!("VFO {index} does not exist (this radio has {NUM_VFOS})"))?;
    if let Some(hz) = f.frequency_hz {
        band.set_frequency(hz)
            .map_err(|e| format!("VFO {index} frequency: {e}"))?;
    }
    if let Some(t) = f.rx_tone {
        band.set_rx_tone(t)
            .map_err(|e| format!("VFO {index} RX tone: {e}"))?;
    }
    if let Some(t) = f.tx_tone {
        band.set_tx_tone(t)
            .map_err(|e| format!("VFO {index} TX tone: {e}"))?;
    }
    if let Some(s) = &f.bandwidth {
        band.set_bandwidth(match s.to_ascii_lowercase().as_str() {
            "wide" => Bandwidth::Wide,
            "narrow" => Bandwidth::Narrow,
            _ => return Err(format!("VFO {index} bandwidth: unknown value {s:?}")),
        });
    }
    if let Some(s) = &f.power {
        band.set_power(match s.to_ascii_lowercase().as_str() {
            "high" => Power::High,
            "low" => Power::Low,
            _ => return Err(format!("VFO {index} power: unknown value {s:?}")),
        });
    }
    if let Some(hz) = f.step_hz {
        band.set_step(hz)
            .map_err(|e| format!("VFO {index} step: {e}"))?;
    }
    Ok(())
}

/// Confirm a channel re-read carries the values the edit asked for.
fn verify_channel(ch: &QytChannel, f: &ChannelFields) -> Result<(), String> {
    if let Some(name) = &f.name {
        let expected: String = name.chars().take(super::NAME_LEN).collect();
        if ch.name != expected.trim_end_matches(' ') {
            return Err(format!(
                "channel {} name verification failed: wrote {expected:?}, read back {:?}",
                ch.index, ch.name
            ));
        }
    }
    if let Some(hz) = f.rx_frequency_hz {
        if ch.rx_frequency_hz != hz {
            return Err(format!(
                "channel {} RX verification failed: wrote {hz}, read back {}",
                ch.index, ch.rx_frequency_hz
            ));
        }
    }
    if let Some(hz) = f.tx_frequency_hz {
        if ch.tx_frequency_hz != hz {
            return Err(format!(
                "channel {} TX verification failed: wrote {hz:?}, read back {:?}",
                ch.index, ch.tx_frequency_hz
            ));
        }
    }
    for (wrote, read, dir) in [
        (f.rx_tone, ch.rx_tone, "RX"),
        (f.tx_tone, ch.tx_tone, "TX"),
    ] {
        if let Some(t) = wrote {
            if t != read {
                return Err(format!(
                    "channel {} {dir} tone verification failed: wrote {t:?}, read back {read:?}",
                    ch.index
                ));
            }
        }
    }
    if let Some(on) = f.scan_add {
        if ch.scan_add != on {
            return Err(format!(
                "channel {} scan_add verification failed: wrote {on}, read back {}",
                ch.index, ch.scan_add
            ));
        }
    }
    Ok(())
}

/// Confirm the settings header re-reads as the edit asked for.
fn verify_settings(cp: &QytCodeplug, f: &SettingsFields) -> Result<(), String> {
    let s = cp.settings();
    for (wrote, read, name) in [
        (f.tmr, s.tmr, "tmr"),
        (f.squelch, s.squelch, "squelch"),
        (f.tot, s.tot, "tot"),
        (f.volume, s.volume, "volume"),
        (f.backlight_timer, s.backlight_timer, "backlight_timer"),
    ] {
        if let Some(v) = wrote {
            if v != read {
                return Err(format!(
                    "settings {name} verification failed: wrote {v}, read back {read}"
                ));
            }
        }
    }
    if let Some(on) = f.beep {
        if on != s.beep {
            return Err(format!(
                "settings beep verification failed: wrote {on}, read back {}",
                s.beep
            ));
        }
    }
    if let Some(label) = &f.scan_rev {
        let expected = super::ScanRev::parse_label(label)?;
        if s.scan_rev != Some(expected) {
            return Err(format!(
                "settings scan_rev verification failed: wrote {label:?}, read back {:?}",
                s.scan_rev
            ));
        }
    }
    if let Some(on) = f.remember_power {
        if on != s.remember_power {
            return Err(format!(
                "settings remember_power verification failed: wrote {on}, read back {}",
                s.remember_power
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::test_support::synthetic_qyt_image;
    use super::*;

    /// Apply `json` to the synthetic image and re-parse the result.
    fn edit(json: &str) -> Result<QytCodeplug, String> {
        let out = apply_edits(&synthetic_qyt_image(), json)?;
        QytCodeplug::parse(&out).map_err(|e| e.to_string())
    }

    #[test]
    fn a_static_message_edit_changes_the_lcd_line() {
        let cp = edit(r#"{"static_message": "N7FYZ MOBILE"}"#).unwrap();
        assert_eq!(cp.static_message(), "N7FYZ MOBILE");
    }

    #[test]
    fn a_static_message_edit_is_rejected_when_it_does_not_fit() {
        let err = edit(r#"{"static_message": "this line is far too long"}"#).unwrap_err();
        assert!(err.contains("at most 13"), "{err}");
    }

    #[test]
    fn a_static_message_edit_is_rejected_past_the_lcd_width() {
        // 14 characters fit the 16-byte field but not the 13-column display.
        let err = edit(r#"{"static_message": "0123456789ABC!"}"#).unwrap_err();
        assert!(err.contains("at most 13"), "{err}");
        assert_eq!(
            edit(r#"{"static_message": "0123456789ABC"}"#)
                .unwrap()
                .static_message(),
            "0123456789ABC"
        );
    }

    #[test]
    fn an_empty_spec_leaves_the_image_byte_identical() {
        let img = synthetic_qyt_image();
        assert_eq!(apply_edits(&img, "{}").unwrap(), img);
    }

    #[test]
    fn a_channel_update_changes_only_what_it_names() {
        let cp = edit(r#"{"channels":[{"index":2,"name":"EDITED","rx_frequency_hz":146520000}]}"#)
            .unwrap();
        let ch = cp.channels().find(|c| c.index == 2).unwrap();
        assert_eq!(ch.name, "EDITED");
        assert_eq!(ch.rx_frequency_hz, 146_520_000);
        // Untouched fields keep their captured values.
        assert_eq!(ch.tx_frequency_hz, Some(147_920_000));
        assert_eq!(ch.tx_tone, Tone::Ctcss { hz_tenths: 1000 });
    }

    #[test]
    fn tones_can_be_set_from_the_tagged_json_form() {
        let cp = edit(
            r#"{"channels":[{"index":0,
                             "rx_tone":{"kind":"ctcss","hz_tenths":885},
                             "tx_tone":{"kind":"dcs","code":73,"inverted":false}}]}"#,
        )
        .unwrap();
        let ch = cp.channels().next().unwrap();
        assert_eq!(ch.rx_tone, Tone::Ctcss { hz_tenths: 885 });
        assert_eq!(
            ch.tx_tone,
            Tone::Dcs {
                code: 73,
                inverted: false
            }
        );
    }

    #[test]
    fn a_null_tx_frequency_clears_it_and_an_absent_one_does_not() {
        let cp = edit(r#"{"channels":[{"index":2,"tx_frequency_hz":null}]}"#).unwrap();
        assert_eq!(
            cp.channels().find(|c| c.index == 2).unwrap().tx_frequency_hz,
            None
        );

        let cp = edit(r#"{"channels":[{"index":2,"name":"KEEP"}]}"#).unwrap();
        assert_eq!(
            cp.channels().find(|c| c.index == 2).unwrap().tx_frequency_hz,
            Some(147_920_000)
        );
    }

    #[test]
    fn scan_add_can_be_cleared_and_set_without_disturbing_the_other_flags() {
        let img = synthetic_qyt_image();
        // Channel 0's tail is 0x24: in scan, wide, plus one unmodelled bit.
        let out = apply_edits(&img, r#"{"channels":[{"index":0,"scan_add":false}]}"#).unwrap();
        assert_eq!(out[15], 0x20);
        let cp = QytCodeplug::parse(&out).unwrap();
        let ch = cp.channels().next().unwrap();
        assert!(!ch.scan_add);
        assert_eq!(ch.bandwidth, Bandwidth::Wide);
        assert!(!ch.busy_lockout);

        // And back again, byte for byte.
        let back = apply_edits(&out, r#"{"channels":[{"index":0,"scan_add":true}]}"#).unwrap();
        assert_eq!(back, img);
    }

    #[test]
    fn an_absent_scan_add_leaves_the_flag_alone() {
        let cp = edit(r#"{"channels":[{"index":1,"name":"KEEP"}]}"#).unwrap();
        let ch = cp.channels().find(|c| c.index == 1).unwrap();
        assert!(ch.scan_add, "channel 1 was in scan before the name edit");
    }

    #[test]
    fn add_remove_and_move_channels() {
        let cp = edit(r#"{"add_channels":[{"name":"ADDED","rx_frequency_hz":446000000}]}"#).unwrap();
        let ch = cp.channels().find(|c| c.index == 3).unwrap();
        assert_eq!(ch.name, "ADDED");
        assert_eq!(ch.rx_frequency_hz, 446_000_000);

        let cp = edit(r#"{"remove_channels":[1]}"#).unwrap();
        assert!(cp.channels().all(|c| c.index != 1));
        assert_eq!(cp.channels().count(), 2);

        let cp = edit(r#"{"move_channels":[{"from":2,"to":100}]}"#).unwrap();
        let moved = cp.channels().find(|c| c.index == 100).unwrap();
        assert_eq!(moved.name, "N7FYZ");
        assert!(cp.channels().all(|c| c.index != 2));
    }

    #[test]
    fn a_settings_edit_touches_only_the_named_fields() {
        let cp = edit(
            r#"{"settings":{"squelch":9,"tot":0,"beep":true,"scan_rev":"SE","remember_power":true}}"#,
        )
        .unwrap();
        assert_eq!(cp.settings().squelch, 9);
        assert_eq!(cp.settings().tot, 0);
        assert_eq!(cp.settings().tot_seconds, 15);
        assert!(cp.settings().beep);
        assert_eq!(cp.settings().scan_rev, Some(crate::qyt::codeplug::ScanRev::SE));
        assert!(cp.settings().remember_power);
        // tmr was 1 in the capture and was not named, so it is still 1.
        assert_eq!(cp.settings().tmr, 1);
    }

    #[test]
    fn a_band_edit_updates_by_index() {
        let cp = edit(
            r#"{"bands":[{"index":0,"frequency_hz":147320000,"power":"low","step_hz":25000}]}"#,
        )
        .unwrap();
        let b = &cp.bands()[0];
        assert_eq!(b.frequency_hz, Some(147_320_000));
        assert_eq!(b.power, Power::Low);
        assert_eq!(b.step_hz, Some(25_000));
        // The other three records are untouched.
        assert!(!cp.bands()[1].programmed);
    }

    #[test]
    fn bad_edits_are_rejected_with_a_readable_message() {
        let cases = [
            (
                r#"{"channels":[{"index":9,"name":"X"}]}"#,
                "not programmed",
            ),
            (r#"{"remove_channels":[9]}"#, "cannot remove channel 9"),
            (r#"{"move_channels":[{"from":0,"to":1}]}"#, "occupied"),
            (r#"{"settings":{"squelch":40}}"#, "squelch must be 0-9"),
            (r#"{"settings":{"scan_rev":"XX"}}"#, "scan_rev must be TO"),
            // Index 2 is image slot C; the KT-5000 has only A and B.
            (r#"{"bands":[{"index":2,"frequency_hz":1}]}"#, "does not exist"),
            (r#"{"bands":[{"index":7,"frequency_hz":1}]}"#, "does not exist"),
            (r#"{"bands":[{"index":0,"power":"turbo"}]}"#, "unknown value"),
            (
                r#"{"channels":[{"index":0,"tx_tone":{"kind":"dcs","code":999,"inverted":false}}]}"#,
                "not a DCS code",
            ),
            (r#"{"channels":"nope"}"#, "invalid QYT edits JSON"),
        ];
        for (json, needle) in cases {
            let err = apply_edits(&synthetic_qyt_image(), json)
                .expect_err(&format!("{json} should have been rejected"));
            assert!(err.contains(needle), "{json} gave {err:?}");
        }
    }

    #[test]
    fn a_rejected_edit_produces_no_image_at_all() {
        // There is no partial application: the error path never returns bytes.
        assert!(apply_edits(&synthetic_qyt_image(), r#"{"remove_channels":[0,0]}"#).is_err());
    }

    #[test]
    fn edits_preserve_every_unmodelled_byte() {
        let img = synthetic_qyt_image();
        let out = apply_edits(
            &img,
            r#"{"channels":[{"index":1,"name":"Z"}],
                "settings":{"squelch":3},
                "bands":[{"index":0,"power":"low"}]}"#,
        )
        .unwrap();

        // Channel 0's unnamed byte-15 bits.
        assert_eq!(out[15], 0x24);
        // Channel 1's unmodelled name-record byte 7.
        assert_eq!(out[0x1000 + 16 + 7], 0x05);
        // Band 0's unnamed tail.
        assert_eq!(out[0x0F00 + 31], 0x24);
        // The whole unmodelled settings region past the fifty-four-byte header.
        assert_eq!(&out[0x0E36..0x0F00], &img[0x0E36..0x0F00]);
        // And everything above the tables this build understands.
        assert_eq!(&out[0x2000..], &img[0x2000..]);
    }
}
