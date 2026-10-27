//! Writing picked repeaters into a codeplug image.
//!
//! The base is always a real image, never a zero-filled one: both radios keep
//! state the codeplug layers do not model (the KT-5000's calibration among
//! it), and that is written back verbatim. "Clean" therefore means the base's
//! settings, contacts and radio IDs with its channels, zones and scan lists
//! removed, not an empty file.
//!
//! Capacity is a hard limit. A selection that does not fit is an error that
//! names how many to drop; nothing is ever silently truncated.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::anytone::codeplug::channel::SignalingMode;
use crate::anytone::codeplug::ctcss::{ctcss_index, dcs_word};
use crate::anytone::codeplug::{
    Bandwidth, CallType, Codeplug, ChannelMode, Power, CHANNELS_PER_ZONE,
};
use crate::document::Document;
use crate::geo::{Mode, Repeater};
use crate::qyt::codeplug::{QytCodeplug, Tone};

/// Scan-list member limit on the AnyTone.
const SCAN_LIST_MEMBERS: usize = 50;
/// Zone and scan-list name length on the AnyTone.
const LIST_NAME_LEN: usize = 16;
/// The talk group DMR channels are pointed at: TG 9, the local/repeater group
/// on Brandmeister and DMR-MARC alike.
const LOCAL_TALK_GROUP: u32 = 9;
/// Name of the contact created for [`LOCAL_TALK_GROUP`] when the base lacks one.
const LOCAL_TALK_GROUP_NAME: &str = "Local TG9";
/// Frequencies the radio stores in 10 Hz units compare equal within this.
const SAME_FREQ_HZ: u32 = 10;

/// Whether to keep the base's channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildMode {
    /// Remove the base's channels, zones and scan lists first.
    Clean,
    /// Keep everything and add into free slots.
    Merge,
}

/// How to build.
#[derive(Debug, Clone, Deserialize)]
pub struct BuildOptions {
    /// Clean or merge.
    pub mode: BuildMode,
    /// Name for the new zone and scan list (AnyTone only).
    #[serde(default)]
    pub zone_name: String,
}

/// How many channels the user may pick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Capacity {
    /// Free channel slots once the build mode is applied.
    pub available: usize,
    /// The most the user can select: `available`, capped by the zone size.
    pub max_selectable: usize,
    /// Members one zone holds, when the radio has zones.
    pub zone_limit: Option<usize>,
    /// Members the scan list holds; the nearest picks go in it.
    pub scan_list_limit: Option<usize>,
    /// Why nothing can be added, when `max_selectable` is zero for a reason
    /// other than full channel slots.
    pub blocked_reason: Option<String>,
}

/// How many channels a build of `doc` in `mode` can take.
pub fn capacity(doc: &Document, mode: BuildMode) -> Capacity {
    match doc {
        Document::AnyTone(cp) => {
            let total = crate::anytone::codeplug::NUM_CHANNELS;
            let used = if mode == BuildMode::Clean {
                0
            } else {
                cp.channels().count()
            };
            let available = total - used;
            let blocked_reason = (mode == BuildMode::Merge
                && cp.zones().count() >= crate::anytone::codeplug::NUM_ZONES)
                .then(|| "every zone slot is in use; remove a zone first".to_string());
            let max_selectable = if blocked_reason.is_some() {
                0
            } else {
                available.min(CHANNELS_PER_ZONE)
            };
            Capacity {
                available,
                max_selectable,
                zone_limit: Some(CHANNELS_PER_ZONE),
                scan_list_limit: Some(SCAN_LIST_MEMBERS),
                blocked_reason,
            }
        }
        Document::Qyt(cp) => {
            let total = crate::qyt::codeplug::NUM_CHANNELS;
            let used = if mode == BuildMode::Clean {
                0
            } else {
                cp.channels().count()
            };
            Capacity {
                available: total - used,
                max_selectable: total - used,
                zone_limit: None,
                scan_list_limit: None,
                blocked_reason: None,
            }
        }
    }
}

/// Whether `doc` already has a channel on `r`'s frequencies (and, for DMR,
/// its color code).
pub fn already_in(doc: &Document, r: &Repeater) -> bool {
    let near = |a: u32, b: u32| a.abs_diff(b) <= SAME_FREQ_HZ;
    match doc {
        Document::AnyTone(cp) => cp.channels().any(|c| {
            let digital = matches!(c.mode, ChannelMode::Digital | ChannelMode::MixedDigital);
            near(c.rx_frequency_hz, r.rx_hz)
                && near(c.tx_frequency_hz, r.tx_hz)
                && match r.mode {
                    Mode::Analog => !digital,
                    Mode::Dmr => digital && r.color_code.is_none_or(|cc| cc == c.color_code),
                }
        }),
        Document::Qyt(cp) => cp.channels().any(|c| {
            r.mode == Mode::Analog
                && near(c.rx_frequency_hz, r.rx_hz)
                && c.tx_frequency_hz.is_some_and(|tx| near(tx, r.tx_hz))
        }),
    }
}

/// Build a new image from `data` plus exactly the `selected` repeaters.
pub fn build(data: &[u8], selected: &[Repeater], opts: &BuildOptions) -> Result<Vec<u8>, String> {
    let mut doc = Document::parse(data).map_err(|e| e.to_string())?;
    let cap = capacity(&doc, opts.mode);
    if let Some(reason) = &cap.blocked_reason {
        return Err(format!("cannot add repeaters: {reason}"));
    }
    if selected.is_empty() {
        return Err("no repeaters selected".into());
    }
    if selected.len() > cap.max_selectable {
        return Err(format!(
            "{} channels selected but only {} fit; deselect {}",
            selected.len(),
            cap.max_selectable,
            selected.len() - cap.max_selectable
        ));
    }
    if opts.mode == BuildMode::Merge {
        if let Some(r) = selected.iter().find(|r| already_in(&doc, r)) {
            return Err(format!(
                "{} {} is already in this codeplug",
                r.callsign,
                mhz(r.rx_hz)
            ));
        }
    }

    let mut ordered = selected.to_vec();
    ordered.sort_by(|a, b| match (a.distance_km, b.distance_km) {
        (Some(x), Some(y)) => x.total_cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    let zone_name = match opts.zone_name.trim() {
        "" => "Nearby".to_string(),
        n => truncate(n, LIST_NAME_LEN),
    };
    let expected = match &mut doc {
        Document::AnyTone(cp) => build_anytone(cp, &ordered, opts.mode, &zone_name)?,
        Document::Qyt(cp) => build_qyt(cp, &ordered, opts.mode)?,
    };

    let out = doc.serialize();
    let check = Document::parse(&out).map_err(|e| format!("built image did not re-parse: {e}"))?;
    let count = match &check {
        Document::AnyTone(cp) => cp.channels().count(),
        Document::Qyt(cp) => cp.channels().count(),
    };
    if count != expected {
        return Err(format!(
            "built image has {count} channels, expected {expected}"
        ));
    }
    Ok(out)
}

/// Fill an AnyTone codeplug. Returns the channel count the result should have.
fn build_anytone(
    cp: &mut Codeplug,
    selected: &[Repeater],
    mode: BuildMode,
    zone_name: &str,
) -> Result<usize, String> {
    if mode == BuildMode::Clean {
        let channels: Vec<usize> = cp.channels().map(|c| c.index).collect();
        let zones: Vec<usize> = cp.zones().map(|z| z.index).collect();
        let lists: Vec<usize> = cp.scan_lists().map(|s| s.index).collect();
        for i in channels {
            cp.remove_channel(i);
        }
        for i in zones {
            cp.remove_zone(i);
        }
        for i in lists {
            cp.remove_scan_list(i);
        }
    }

    let talk_group = if selected.iter().any(|r| r.mode == Mode::Dmr) {
        Some(local_talk_group(cp)?)
    } else {
        None
    };
    // A full scan-list table is not fatal: the channels still go in the zone.
    let scan_list = cp.add_scan_list();

    let mut names: HashSet<String> = cp.channels().map(|c| c.name.clone()).collect();
    let mut members = Vec::with_capacity(selected.len());
    for (n, r) in selected.iter().enumerate() {
        let i = cp.add_channel().ok_or("no free channel slots")?;
        let ch = cp.channel_mut(i).expect("just added");
        ch.set_name(&unique_name(anytone_name(r), LIST_NAME_LEN, &mut names));
        ch.set_rx_frequency(r.rx_hz);
        ch.set_tx_frequency(r.tx_hz);
        ch.set_power(Power::High);
        ch.set_rx_signaling_mode(SignalingMode::None);
        ch.set_tx_signaling_mode(SignalingMode::None);
        match r.mode {
            Mode::Analog => {
                ch.set_mode(ChannelMode::Analog);
                ch.set_bandwidth(Bandwidth::Wide);
                match r.tx_tone {
                    Tone::Ctcss { hz_tenths } => {
                        if let Some(idx) = ctcss_index(hz_tenths) {
                            ch.set_tx_signaling_mode(SignalingMode::Ctcss);
                            ch.set_tx_ctcss(idx);
                        }
                    }
                    Tone::Dcs { code, inverted } => {
                        if let Some(word) = dcs_word(code, inverted) {
                            ch.set_tx_signaling_mode(SignalingMode::Dcs);
                            ch.set_tx_dcs(word);
                        }
                    }
                    Tone::None => {}
                }
            }
            Mode::Dmr => {
                ch.set_mode(ChannelMode::Digital);
                ch.set_bandwidth(Bandwidth::Narrow);
                ch.set_color_code(r.color_code.unwrap_or(1));
                ch.set_time_slot(1);
                ch.set_contact_index(talk_group.expect("set when any DMR is selected"));
                ch.set_radio_id_index(0);
            }
        }
        let in_scan = scan_list.is_some() && n < SCAN_LIST_MEMBERS;
        ch.set_scan_list_index(match (scan_list, in_scan) {
            (Some(s), true) => s as u8,
            _ => 0xff,
        });
        members.push(i as u16);
    }

    if let Some(s) = scan_list {
        let sl = cp.scan_list_mut(s).expect("just added");
        sl.set_name(zone_name);
        sl.set_members(&members[..members.len().min(SCAN_LIST_MEMBERS)]);
    }
    let z = cp.add_zone().ok_or("no free zone slots")?;
    let zone = cp.zone_mut(z).expect("just added");
    zone.set_name(zone_name);
    zone.set_members(&members);

    Ok(cp.channels().count())
}

/// The contact index of a TG 9 group call, adding one if the base has none.
fn local_talk_group(cp: &mut Codeplug) -> Result<u32, String> {
    if let Some(c) = cp
        .contacts()
        .find(|c| c.call_type == CallType::Group && c.number == LOCAL_TALK_GROUP)
    {
        return Ok(c.index as u32);
    }
    let i = cp.add_contact().ok_or("no free contact slots for the TG 9 contact")?;
    cp.set_contact_name(i, LOCAL_TALK_GROUP_NAME);
    cp.set_contact_number(i, LOCAL_TALK_GROUP);
    cp.set_contact_call_type(i, CallType::Group);
    Ok(i as u32)
}

/// Fill a KT-5000 codeplug. Returns the channel count the result should have.
fn build_qyt(cp: &mut QytCodeplug, selected: &[Repeater], mode: BuildMode) -> Result<usize, String> {
    if let Some(r) = selected.iter().find(|r| r.mode == Mode::Dmr) {
        return Err(format!(
            "{} {} is DMR, which the KT-5000 cannot use",
            r.callsign,
            mhz(r.rx_hz)
        ));
    }
    if mode == BuildMode::Clean {
        let channels: Vec<usize> = cp.channels().map(|c| c.index).collect();
        for i in channels {
            cp.remove_channel(i);
        }
    }

    let name_len = crate::qyt::codeplug::NAME_LEN;
    let mut names: HashSet<String> = cp.channels().map(|c| c.name.clone()).collect();
    for r in selected {
        let i = cp.add_channel().ok_or("no free channel slots")?;
        let ch = cp.channel_mut(i).expect("just added");
        ch.set_name(&unique_name(r.callsign.to_ascii_uppercase(), name_len, &mut names));
        ch.set_rx_frequency(r.rx_hz);
        ch.set_tx_frequency(Some(r.tx_hz));
        ch.set_rx_tone(Tone::None)?;
        // A tone the radio cannot store leaves the channel on carrier squelch
        // rather than failing the whole build.
        if ch.set_tx_tone(r.tx_tone).is_err() {
            ch.set_tx_tone(Tone::None)?;
        }
        ch.set_scan_add(true);
    }
    Ok(cp.channels().count())
}

/// "W5LEX 444.850" for analog, "K1DQ 145.110D" for DMR.
fn anytone_name(r: &Repeater) -> String {
    let suffix = if r.mode == Mode::Dmr { "D" } else { "" };
    format!("{} {}{suffix}", r.callsign.to_ascii_uppercase(), mhz(r.rx_hz))
}

/// A frequency as "444.850".
fn mhz(hz: u32) -> String {
    format!("{:.3}", f64::from(hz) / 1_000_000.0)
}

/// The first `max` characters of `s`.
fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// `base` cut to `max` characters, with a number swapped onto the end until it
/// is not in `used`. The result is recorded in `used`.
fn unique_name(base: String, max: usize, used: &mut HashSet<String>) -> String {
    let first = truncate(&base, max);
    let name = if used.contains(&first) {
        (2..)
            .map(|n| {
                let suffix = n.to_string();
                format!("{}{suffix}", truncate(&base, max.saturating_sub(suffix.len())))
            })
            .find(|candidate| !used.contains(candidate))
            .expect("an unbounded range always finds a free name")
    } else {
        first
    };
    used.insert(name.clone());
    name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::Source;

    /// An AnyTone image with no active records (the contact bitmap is
    /// inverted, so zero-fill alone would read as ten thousand contacts).
    fn empty_anytone() -> Vec<u8> {
        use crate::anytone::codeplug::{global_offset, CONTACT_BITMAP};
        let mut raw = vec![0u8; crate::model::Model::AnyToneD878UV2.image_size()];
        let off = global_offset(CONTACT_BITMAP).unwrap();
        raw[off..off + 0x500].fill(0xff);
        raw
    }

    fn rep(call: &str, rx_mhz: f64, offset_mhz: f64, mode: Mode, km: f64) -> Repeater {
        let rx = (rx_mhz * 1e6).round() as u32;
        Repeater {
            source: Source::Hearham,
            callsign: call.into(),
            location: "Town".into(),
            rx_hz: rx,
            tx_hz: (f64::from(rx) + offset_mhz * 1e6).round() as u32,
            mode,
            tx_tone: if mode == Mode::Analog {
                Tone::Ctcss { hz_tenths: 1000 }
            } else {
                Tone::None
            },
            rx_tone: Tone::None,
            color_code: (mode == Mode::Dmr).then_some(3),
            lat: Some(0.0),
            lon: Some(0.0),
            distance_km: Some(km),
            duplicate: false,
        }
    }

    fn opts(mode: BuildMode) -> BuildOptions {
        BuildOptions {
            mode,
            zone_name: "Denver".into(),
        }
    }

    /// An AnyTone image holding one analog channel on 146.520.
    fn anytone_with_one_channel() -> Vec<u8> {
        let data = empty_anytone();
        crate::document::apply_edits(
            &data,
            r#"{"anytone":{"add_channels":[{"name":"CALL","rx_frequency_hz":146520000,"tx_frequency_hz":146520000}]}}"#,
        )
        .unwrap()
    }

    #[test]
    fn anytone_merge_adds_channels_zone_and_scan_list() {
        let base = anytone_with_one_channel();
        let picks = vec![
            rep("W0FAR", 447.975, -5.0, Mode::Analog, 30.0),
            rep("W0NEAR", 146.94, -0.6, Mode::Analog, 2.0),
            rep("W0DMR", 446.975, -5.0, Mode::Dmr, 10.0),
        ];
        let out = build(&base, &picks, &opts(BuildMode::Merge)).unwrap();
        let Document::AnyTone(cp) = Document::parse(&out).unwrap() else {
            panic!("model changed");
        };
        let chans: Vec<_> = cp.channels().collect();
        assert_eq!(chans.len(), 4);
        assert_eq!(chans[0].name, "CALL", "base channel kept");
        // Sorted nearest first.
        assert_eq!(chans[1].name, "W0NEAR 146.940");
        assert_eq!(chans[1].tx_frequency_hz, 146_340_000);
        assert_eq!(chans[1].tx_ctcss, 13, "100.0 Hz is index 13");
        assert_eq!(chans[2].name, "W0DMR 446.975D");
        assert_eq!(chans[2].mode, ChannelMode::Digital);
        assert_eq!(chans[2].color_code, 3);

        let zone = cp.zones().next().expect("zone added");
        assert_eq!(zone.name, "Denver");
        assert_eq!(zone.channels, vec![1, 2, 3]);
        let sl = cp.scan_lists().next().expect("scan list added");
        assert_eq!(sl.name, "Denver");

        let tg = cp.contacts().find(|c| c.number == 9).expect("TG9 added");
        assert_eq!(tg.call_type, CallType::Group);
        assert_eq!(chans[2].contact_index, tg.index as u32);
    }

    #[test]
    fn anytone_clean_replaces_channels() {
        let base = anytone_with_one_channel();
        let picks = vec![rep("W0A", 146.94, -0.6, Mode::Analog, 1.0)];
        let out = build(&base, &picks, &opts(BuildMode::Clean)).unwrap();
        let Document::AnyTone(cp) = Document::parse(&out).unwrap() else {
            panic!();
        };
        let names: Vec<_> = cp.channels().map(|c| c.name.clone()).collect();
        assert_eq!(names, vec!["W0A 146.940"]);
        assert_eq!(cp.zones().count(), 1);
    }

    #[test]
    fn merge_rejects_a_channel_the_base_already_has() {
        let base = anytone_with_one_channel();
        let doc = Document::parse(&base).unwrap();
        let dup = rep("W0X", 146.52, 0.0, Mode::Analog, 1.0);
        assert!(already_in(&doc, &dup));
        let err = build(&base, &[dup], &opts(BuildMode::Merge)).unwrap_err();
        assert!(err.contains("already in"), "{err}");
    }

    #[test]
    fn anytone_selection_is_capped_by_the_zone() {
        let base = empty_anytone();
        let doc = Document::parse(&base).unwrap();
        let cap = capacity(&doc, BuildMode::Clean);
        assert_eq!(cap.available, 4000);
        assert_eq!(cap.max_selectable, 250);

        let picks: Vec<_> = (0..251)
            .map(|i| rep(&format!("W{i}"), 440.0 + i as f64 * 0.025, 5.0, Mode::Analog, 1.0))
            .collect();
        let err = build(&base, &picks, &opts(BuildMode::Clean)).unwrap_err();
        assert!(err.contains("251 channels selected but only 250 fit; deselect 1"), "{err}");
        assert!(build(&base, &picks[..250], &opts(BuildMode::Clean)).is_ok());
    }

    #[test]
    fn qyt_capacity_counts_only_free_slots_in_merge() {
        let base = crate::qyt::codeplug::test_support::synthetic_qyt_image();
        let doc = Document::parse(&base).unwrap();
        let used = match &doc {
            Document::Qyt(cp) => cp.channels().count(),
            _ => panic!(),
        };
        assert_eq!(capacity(&doc, BuildMode::Clean).max_selectable, 200);
        assert_eq!(capacity(&doc, BuildMode::Merge).max_selectable, 200 - used);
    }

    #[test]
    fn qyt_clean_fills_exactly_to_capacity_and_errors_one_over() {
        let base = crate::qyt::codeplug::test_support::synthetic_qyt_image();
        let picks: Vec<_> = (0..201)
            .map(|i| rep("K0ABC", 440.0 + i as f64 * 0.025, 5.0, Mode::Analog, 1.0))
            .collect();
        let err = build(&base, &picks, &opts(BuildMode::Clean)).unwrap_err();
        assert!(err.contains("deselect 1"), "{err}");

        let out = build(&base, &picks[..200], &opts(BuildMode::Clean)).unwrap();
        let Document::Qyt(cp) = Document::parse(&out).unwrap() else {
            panic!();
        };
        let chans: Vec<_> = cp.channels().collect();
        assert_eq!(chans.len(), 200);
        assert_eq!(chans[0].name, "K0ABC");
        assert_eq!(chans[1].name, "K0ABC2", "names stay unique in 7 characters");
        assert_eq!(chans[0].tx_tone, Tone::Ctcss { hz_tenths: 1000 });
        assert_eq!(chans[0].tx_frequency_hz, Some(445_000_000));
    }

    #[test]
    fn qyt_rejects_dmr() {
        let base = crate::qyt::codeplug::test_support::synthetic_qyt_image();
        let picks = vec![rep("W0DMR", 446.975, -5.0, Mode::Dmr, 1.0)];
        let err = build(&base, &picks, &opts(BuildMode::Clean)).unwrap_err();
        assert!(err.contains("DMR"), "{err}");
    }

    #[test]
    fn empty_selection_is_an_error() {
        let base = empty_anytone();
        assert!(build(&base, &[], &opts(BuildMode::Clean)).is_err());
    }

    #[test]
    fn unique_names_fit_the_limit() {
        let mut used = HashSet::new();
        assert_eq!(unique_name("ABCDEFGH".into(), 7, &mut used), "ABCDEFG");
        assert_eq!(unique_name("ABCDEFGH".into(), 7, &mut used), "ABCDEF2");
        assert_eq!(unique_name("ABCDEFGH".into(), 7, &mut used), "ABCDEF3");
    }
}
