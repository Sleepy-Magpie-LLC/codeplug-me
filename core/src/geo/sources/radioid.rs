//! radioid.net's DMR repeater list (`https://www.radioid.net/static/rptrs.json`).
//!
//! A single `{"rptrs": [...]}` file of every registered DMR repeater, about
//! 6 MB. Frequencies and offsets are MHz strings and there are no coordinates,
//! only city, state and country, so records are filtered by state here and
//! given coordinates later from a matching Hearham or RepeaterBook record
//! ([`crate::geo::select`]).

use serde::Deserialize;

use crate::geo::sources::{matches_filter, mhz_to_hz};
use crate::geo::{Mode, Repeater, Source};
use crate::qyt::codeplug::Tone;

/// Top-level wrapper of the download.
#[derive(Deserialize)]
struct File {
    rptrs: Vec<Record>,
}

/// The subset of a RadioID record the importer reads.
#[derive(Deserialize)]
struct Record {
    callsign: String,
    #[serde(default)]
    city: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    country: String,
    frequency: String,
    #[serde(default)]
    offset: String,
    /// Out-of-range values (some records carry 293) are dropped per record
    /// rather than failing the whole file.
    #[serde(default)]
    color_code: Option<i64>,
    #[serde(default)]
    status: String,
}

/// Parse the download, keeping active repeaters in `country` and `state`.
/// A `None` filter keeps every value.
pub fn parse(
    body: &[u8],
    country: Option<&str>,
    state: Option<&str>,
) -> Result<Vec<Repeater>, String> {
    let file: File = serde_json::from_slice(body).map_err(|e| format!("RadioID response: {e}"))?;
    Ok(file
        .rptrs
        .into_iter()
        .filter(|r| r.status.eq_ignore_ascii_case("active"))
        .filter(|r| matches_filter(&r.country, country) && matches_filter(&r.state, state))
        .filter_map(|r| {
            let rx = mhz_to_hz(&r.frequency)?;
            let offset_hz = r.offset.trim().parse::<f64>().ok().unwrap_or(0.0) * 1_000_000.0;
            let tx = u32::try_from((f64::from(rx) + offset_hz).round() as i64).ok()?;
            let location = match (r.city.trim(), r.state.trim()) {
                (c, "") => c.to_string(),
                ("", s) => s.to_string(),
                (c, s) => format!("{c}, {s}"),
            };
            Some(Repeater {
                source: Source::Radioid,
                callsign: r.callsign.trim().to_string(),
                location,
                rx_hz: rx,
                tx_hz: tx,
                mode: Mode::Dmr,
                tx_tone: Tone::None,
                rx_tone: Tone::None,
                color_code: r
                    .color_code
                    .filter(|c| (0..=15).contains(c))
                    .map(|c| c as u8),
                lat: None,
                lon: None,
                distance_km: None,
                duplicate: false,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/geo/radioid.json");

    #[test]
    fn filters_by_state_and_reads_offsets() {
        let rs = parse(FIXTURE, Some("United States"), Some("california")).unwrap();
        assert_eq!(rs.len(), 2, "{rs:#?}");
        assert_eq!(rs[0].callsign, "WB6ECE");
        assert_eq!(rs[0].rx_hz, 440_587_500);
        assert_eq!(rs[0].tx_hz, 445_587_500);
        assert_eq!(rs[0].color_code, Some(2));
        assert_eq!(rs[0].location, "Bonny Doon, California");
        assert_eq!(rs[1].tx_hz, 442_300_000, "negative offset");
        assert!(rs.iter().all(|r| r.mode == Mode::Dmr && r.lat.is_none()));
    }

    #[test]
    fn no_filter_keeps_every_active_record() {
        let rs = parse(FIXTURE, None, None).unwrap();
        assert_eq!(rs.len(), 3, "the inactive record is dropped");
        let denver = rs.iter().find(|r| r.callsign == "W0DMR").unwrap();
        assert_eq!(denver.color_code, None, "a bogus color code is dropped, not fatal");
    }
}
