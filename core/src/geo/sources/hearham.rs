//! hearham.com's repeater list (`https://hearham.com/api/repeaters/v1`).
//!
//! One JSON array of every repeater worldwide, about 9 MB. Frequencies and
//! offsets are integer Hz, coordinates are always present, and the tone field
//! is free text (see [`crate::geo::tone`]). DMR records carry their color code
//! in that same field as "CC4".

use serde::Deserialize;

use crate::geo::{tone, Mode, Repeater, Source};
use crate::qyt::codeplug::Tone;

/// The subset of a Hearham record the importer reads.
#[derive(Deserialize)]
struct Record {
    callsign: String,
    latitude: f64,
    longitude: f64,
    #[serde(default)]
    city: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    encode: String,
    #[serde(default)]
    decode: String,
    frequency: i64,
    #[serde(default)]
    offset: i64,
    #[serde(default)]
    operational: i64,
}

/// Map Hearham's mode text to a mode, or `None` for modes neither radio
/// speaks (D-STAR, YSF-only, P25-only). "YSF/FM" and "P25/FM" repeaters also
/// pass FM, so they count as analog.
fn mode_of(text: &str) -> Option<Mode> {
    let t = text.trim().to_ascii_uppercase();
    if t == "DMR" {
        Some(Mode::Dmr)
    } else if t.split('/').any(|part| part == "FM") {
        Some(Mode::Analog)
    } else {
        None
    }
}

/// Parse the full Hearham download. Off-air records and modes neither radio
/// can use are dropped here.
pub fn parse(body: &[u8]) -> Result<Vec<Repeater>, String> {
    let records: Vec<Record> =
        serde_json::from_slice(body).map_err(|e| format!("Hearham response: {e}"))?;
    Ok(records
        .into_iter()
        .filter(|r| r.operational != 0)
        .filter_map(|r| {
            let mode = mode_of(&r.mode)?;
            let rx = u32::try_from(r.frequency).ok().filter(|&f| f > 0)?;
            let tx = u32::try_from(r.frequency + r.offset).ok()?;
            let enc = tone::parse(&r.encode);
            let dec = tone::parse(&r.decode);
            Some(Repeater {
                source: Source::Hearham,
                callsign: r.callsign.trim().to_string(),
                location: r.city.trim().to_string(),
                rx_hz: rx,
                tx_hz: tx,
                mode,
                tx_tone: enc.tone.unwrap_or(Tone::None),
                rx_tone: dec.tone.unwrap_or(Tone::None),
                color_code: enc.color_code.or(dec.color_code),
                lat: Some(r.latitude),
                lon: Some(r.longitude),
                distance_km: None,
                duplicate: false,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/geo/hearham.json");

    #[test]
    fn parses_fm_and_dmr_and_drops_the_rest() {
        let rs = parse(FIXTURE).unwrap();
        // The fixture has 6 records: FM, DMR, D-STAR, off-air FM, YSF/FM, DCS FM.
        assert_eq!(rs.len(), 4, "{rs:#?}");

        let fm = &rs[0];
        assert_eq!(fm.callsign, "W5LEX");
        assert_eq!(fm.mode, Mode::Analog);
        assert_eq!(fm.rx_hz, 444_850_000);
        assert_eq!(fm.tx_hz, 449_850_000);
        assert_eq!(fm.tx_tone, Tone::Ctcss { hz_tenths: 1035 });

        let dmr = &rs[1];
        assert_eq!(dmr.mode, Mode::Dmr);
        assert_eq!(dmr.color_code, Some(4));
        assert_eq!(dmr.tx_hz, 144_510_000);

        assert_eq!(rs[2].mode, Mode::Analog, "YSF/FM is worked as FM");
        assert_eq!(
            rs[3].tx_tone,
            Tone::Dcs {
                code: 23,
                inverted: false
            }
        );
    }

    #[test]
    fn rejects_non_json() {
        assert!(parse(b"<html>").is_err());
    }
}
