//! Repeater import: pull repeaters near a location from open databases and
//! turn the ones the user picks into codeplug channels.
//!
//! The flow is three steps, each its own host call:
//!
//! 1. [`fetch`] downloads from the chosen [`Source`]s and hands the records to
//!    [`select`], which normalizes, filters to the radius and the radio's
//!    bands, and flags channels the base codeplug already has.
//! 2. [`capacity`] says how many channels the user may pick, so the UI can
//!    stop them before they pick too many.
//! 3. [`build`] writes exactly the picked records into a new image.
//!
//! Everything except the HTTP in [`net`] is pure, so the parsers and the
//! builder are tested without a network.

pub mod build;
pub mod distance;
#[cfg(feature = "net")]
pub mod net;
pub mod sources;
pub mod tone;

use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::model::Model;
use crate::qyt::codeplug::Tone;

pub use build::{build, capacity, BuildMode, BuildOptions, Capacity};

/// Where a record came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// hearham.com: free, worldwide, has coordinates.
    Hearham,
    /// radioid.net's DMR repeater list: free, city and state only.
    Radioid,
    /// RepeaterBook: needs the user's own app token.
    Repeaterbook,
}

impl Source {
    /// Whether this source only ever lists DMR repeaters.
    pub fn is_dmr_only(self) -> bool {
        matches!(self, Source::Radioid)
    }

    /// Preference when two sources list the same repeater; lower wins.
    fn rank(self) -> u8 {
        match self {
            Source::Repeaterbook => 0,
            Source::Hearham => 1,
            Source::Radioid => 2,
        }
    }
}

/// How a repeater is worked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// FM voice.
    Analog,
    /// DMR.
    Dmr,
}

/// One repeater, normalized across sources. Frequencies are from the radio's
/// side: `rx_hz` is the repeater's output, `tx_hz` its input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Repeater {
    /// Database the record came from.
    pub source: Source,
    /// Station callsign.
    pub callsign: String,
    /// Nearest city or landmark, for display.
    pub location: String,
    /// Frequency the radio receives on.
    pub rx_hz: u32,
    /// Frequency the radio transmits on.
    pub tx_hz: u32,
    /// Analog or DMR.
    pub mode: Mode,
    /// Tone the radio sends to open the repeater.
    pub tx_tone: Tone,
    /// Tone the repeater sends back, when listed.
    pub rx_tone: Tone,
    /// DMR color code.
    pub color_code: Option<u8>,
    /// Latitude in degrees, when known.
    pub lat: Option<f64>,
    /// Longitude in degrees, when known.
    pub lon: Option<f64>,
    /// Distance from the query point, filled in by [`select`].
    #[serde(default)]
    pub distance_km: Option<f64>,
    /// The base codeplug already has a channel on these frequencies.
    #[serde(default)]
    pub duplicate: bool,
}

/// RepeaterBook credentials. RepeaterBook binds tokens to a User-Agent that
/// names a contact address, so both come from the user's settings.
#[derive(Debug, Clone, Deserialize)]
pub struct RepeaterBookAuth {
    /// The user's `rbuapp_` token.
    pub token: String,
    /// Contact email for the User-Agent.
    pub contact: String,
}

/// What to look for and where.
#[derive(Debug, Clone, Deserialize)]
pub struct GeoQuery {
    /// Center latitude in degrees.
    pub lat: f64,
    /// Center longitude in degrees.
    pub lon: f64,
    /// Search radius in kilometres.
    pub radius_km: f64,
    /// Country name as the host geocoder reports it ("United States").
    #[serde(default)]
    pub country: Option<String>,
    /// State or province name ("Colorado"). RadioID and RepeaterBook are
    /// queried by it, since neither can search by coordinates.
    #[serde(default)]
    pub state: Option<String>,
    /// Databases to query.
    pub sources: Vec<Source>,
    /// Needed only when `sources` includes RepeaterBook.
    #[serde(default)]
    pub repeaterbook: Option<RepeaterBookAuth>,
}

/// Bands a model can transmit on, in Hz. Both radios are 2 m / 70 cm.
pub fn bands(model: Model) -> &'static [(u32, u32)] {
    match model {
        Model::AnyToneD878UV2 | Model::QytKt5000 => {
            &[(136_000_000, 174_000_000), (400_000_000, 480_000_000)]
        }
    }
}

/// Whether both of a repeater's frequencies fall inside one of `model`'s bands.
fn in_band(model: Model, r: &Repeater) -> bool {
    let ok = |hz: u32| bands(model).iter().any(|&(lo, hi)| hz >= lo && hz <= hi);
    ok(r.rx_hz) && ok(r.tx_hz)
}

/// Tolerance when matching the same repeater across sources, which round
/// frequencies differently.
const SAME_FREQ_HZ: u32 = 1_000;

/// Whether two records describe the same repeater.
fn same_repeater(a: &Repeater, b: &Repeater) -> bool {
    a.mode == b.mode
        && a.rx_hz.abs_diff(b.rx_hz) <= SAME_FREQ_HZ
        && a.callsign.eq_ignore_ascii_case(&b.callsign)
}

/// Turn raw source records into the list the user picks from.
///
/// Records with no coordinates (RadioID) borrow them from a matching record in
/// another source. Ones still without coordinates stay in the list, sorted
/// after everything with a distance, because their source already filtered
/// them to the query's state. Anything out of radius, out of band, or DMR on a
/// radio without DMR is dropped.
pub fn select(query: &GeoQuery, doc: &Document, records: Vec<Repeater>) -> Vec<Repeater> {
    let model = doc.model();
    let supports_dmr = model.capabilities().supports_dmr;

    let mut records = records;
    records.sort_by_key(|r| r.source.rank());

    let mut out: Vec<Repeater> = Vec::new();
    for mut r in records {
        if let Some(existing) = out.iter_mut().find(|e| same_repeater(e, &r)) {
            if existing.color_code.is_none() {
                existing.color_code = r.color_code;
            }
            continue;
        }
        if r.lat.is_none() {
            if let Some(found) = out.iter().find(|e| same_repeater(e, &r) && e.lat.is_some()) {
                r.lat = found.lat;
                r.lon = found.lon;
            }
        }
        out.push(r);
    }

    out.retain(|r| (supports_dmr || r.mode == Mode::Analog) && in_band(model, r));
    for r in &mut out {
        r.distance_km = match (r.lat, r.lon) {
            (Some(lat), Some(lon)) => Some(distance::haversine_km(query.lat, query.lon, lat, lon)),
            _ => None,
        };
        r.duplicate = build::already_in(doc, r);
    }
    out.retain(|r| r.distance_km.is_none_or(|d| d <= query.radius_km));
    out.sort_by(|a, b| match (a.distance_km, b.distance_km) {
        (Some(x), Some(y)) => x.total_cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.callsign.cmp(&b.callsign),
    });
    out
}

/// Download from every source in `query`, then [`select`] against `doc`.
///
/// `cache_dir` holds the free sources' full downloads for a day.
/// RepeaterBook is never cached; its terms rule out offline copies.
#[cfg(feature = "net")]
pub fn fetch(
    query: &GeoQuery,
    doc: &Document,
    cache_dir: &std::path::Path,
) -> Result<Vec<Repeater>, String> {
    let supports_dmr = doc.model().capabilities().supports_dmr;
    let mut records = Vec::new();
    for &source in &query.sources {
        if source.is_dmr_only() && !supports_dmr {
            continue;
        }
        let parsed = match source {
            Source::Hearham => sources::hearham::parse(&net::fetch_hearham(cache_dir)?)?,
            Source::Radioid => sources::radioid::parse(
                &net::fetch_radioid(cache_dir)?,
                query.country.as_deref(),
                query.state.as_deref(),
            )?,
            Source::Repeaterbook => {
                let auth = query
                    .repeaterbook
                    .as_ref()
                    .ok_or("RepeaterBook needs a token and contact email in settings")?;
                let analog_only = !supports_dmr;
                sources::repeaterbook::parse(&net::fetch_repeaterbook(
                    auth,
                    query.country.as_deref(),
                    query.state.as_deref(),
                    analog_only,
                )?)?
            }
        };
        records.extend(parsed);
    }
    Ok(select(query, doc, records))
}

// --- Host entry points: JSON in, JSON or bytes out, shared by both FFIs. ---

/// [`fetch`] for a host: `query_json` is a [`GeoQuery`], `data` the base
/// image. Returns the picked-from list as a JSON array of [`Repeater`]s.
#[cfg(feature = "net")]
pub fn fetch_json(query_json: &str, data: &[u8], cache_dir: &str) -> Result<String, String> {
    let query: GeoQuery =
        serde_json::from_str(query_json).map_err(|e| format!("invalid query JSON: {e}"))?;
    let doc = Document::parse(data).map_err(|e| e.to_string())?;
    let found = fetch(&query, &doc, std::path::Path::new(cache_dir))?;
    serde_json::to_string(&found).map_err(|e| format!("failed to encode repeaters: {e}"))
}

/// [`capacity`] for a host: `mode` is `"clean"` or `"merge"`. Returns a
/// [`Capacity`] as JSON.
pub fn capacity_json(data: &[u8], mode: &str) -> Result<String, String> {
    let mode: BuildMode = serde_json::from_value(serde_json::Value::String(mode.into()))
        .map_err(|_| format!("unknown build mode {mode:?}; expected clean or merge"))?;
    let doc = Document::parse(data).map_err(|e| e.to_string())?;
    serde_json::to_string(&capacity(&doc, mode)).map_err(|e| format!("failed to encode capacity: {e}"))
}

/// [`build`] for a host: `repeaters_json` is the array of picked
/// [`Repeater`]s, `options_json` a [`BuildOptions`]. Returns the new image.
pub fn build_json(data: &[u8], repeaters_json: &str, options_json: &str) -> Result<Vec<u8>, String> {
    let picked: Vec<Repeater> = serde_json::from_str(repeaters_json)
        .map_err(|e| format!("invalid repeaters JSON: {e}"))?;
    let opts: BuildOptions =
        serde_json::from_str(options_json).map_err(|e| format!("invalid options JSON: {e}"))?;
    build(data, &picked, &opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rep(source: Source, call: &str, rx_hz: u32, mode: Mode, at: Option<(f64, f64)>) -> Repeater {
        Repeater {
            source,
            callsign: call.into(),
            location: String::new(),
            rx_hz,
            tx_hz: rx_hz + 5_000_000,
            mode,
            tx_tone: Tone::None,
            rx_tone: Tone::None,
            color_code: None,
            lat: at.map(|p| p.0),
            lon: at.map(|p| p.1),
            distance_km: None,
            duplicate: false,
        }
    }

    fn query() -> GeoQuery {
        GeoQuery {
            lat: 39.7392,
            lon: -104.9903,
            radius_km: 50.0,
            country: Some("United States".into()),
            state: Some("Colorado".into()),
            sources: vec![Source::Hearham, Source::Radioid],
            repeaterbook: None,
        }
    }

    fn anytone_doc() -> Document {
        use crate::anytone::codeplug::{global_offset, CONTACT_BITMAP};
        let mut raw = vec![0u8; Model::AnyToneD878UV2.image_size()];
        let off = global_offset(CONTACT_BITMAP).unwrap();
        raw[off..off + 0x500].fill(0xff);
        Document::parse(&raw).unwrap()
    }

    fn qyt_doc() -> Document {
        Document::parse(&crate::qyt::codeplug::test_support::synthetic_qyt_image()).unwrap()
    }

    const DENVER: Option<(f64, f64)> = Some((39.74, -104.99));
    const BOULDER: Option<(f64, f64)> = Some((40.015, -105.27));
    const ASPEN: Option<(f64, f64)> = Some((39.19, -106.82));

    #[test]
    fn filters_by_radius_and_sorts_nearest_first() {
        let records = vec![
            rep(Source::Hearham, "BOULDER", 447_000_000, Mode::Analog, BOULDER),
            rep(Source::Hearham, "ASPEN", 447_100_000, Mode::Analog, ASPEN),
            rep(Source::Hearham, "DENVER", 447_200_000, Mode::Analog, DENVER),
        ];
        let out = select(&query(), &anytone_doc(), records);
        let calls: Vec<_> = out.iter().map(|r| r.callsign.as_str()).collect();
        assert_eq!(calls, vec!["DENVER", "BOULDER"], "Aspen is ~170 km away");
        assert!(out[0].distance_km.unwrap() < 1.0);
    }

    #[test]
    fn radioid_borrows_coordinates_and_merges_with_its_match() {
        let mut hearham = rep(Source::Hearham, "W0DMR", 446_975_000, Mode::Dmr, BOULDER);
        hearham.color_code = None;
        let mut radioid = rep(Source::Radioid, "w0dmr", 446_975_000, Mode::Dmr, None);
        radioid.color_code = Some(7);
        let lonely = rep(Source::Radioid, "W0FAR", 445_000_000, Mode::Dmr, None);

        let out = select(&query(), &anytone_doc(), vec![radioid, lonely, hearham]);
        assert_eq!(out.len(), 2, "{out:#?}");
        assert_eq!(out[0].source, Source::Hearham);
        assert_eq!(out[0].color_code, Some(7), "color code filled from RadioID");
        assert_eq!(out[1].callsign, "W0FAR");
        assert_eq!(out[1].distance_km, None, "no coordinates sorts last");
    }

    #[test]
    fn qyt_never_sees_dmr_or_out_of_band() {
        let records = vec![
            rep(Source::Hearham, "FM", 447_000_000, Mode::Analog, DENVER),
            rep(Source::Hearham, "DMR", 446_000_000, Mode::Dmr, DENVER),
            rep(Source::Hearham, "SIXM", 53_000_000, Mode::Analog, DENVER),
        ];
        let out = select(&query(), &qyt_doc(), records);
        let calls: Vec<_> = out.iter().map(|r| r.callsign.as_str()).collect();
        assert_eq!(calls, vec!["FM"]);
    }

    #[test]
    fn flags_channels_the_base_already_has() {
        let mut doc_bytes = anytone_doc().serialize();
        doc_bytes = crate::document::apply_edits(
            &doc_bytes,
            r#"{"anytone":{"add_channels":[{"rx_frequency_hz":447000000,"tx_frequency_hz":452000000}]}}"#,
        )
        .unwrap();
        let doc = Document::parse(&doc_bytes).unwrap();
        let records = vec![
            rep(Source::Hearham, "HAVE", 447_000_000, Mode::Analog, DENVER),
            rep(Source::Hearham, "NEW", 447_100_000, Mode::Analog, DENVER),
        ];
        let out = select(&query(), &doc, records);
        assert!(out[0].duplicate || out[1].duplicate);
        assert_eq!(out.iter().filter(|r| r.duplicate).count(), 1);
        assert!(out.iter().find(|r| r.callsign == "HAVE").unwrap().duplicate);
    }

    #[test]
    fn host_entry_points_round_trip_json() {
        let data = anytone_doc().serialize();
        let cap: serde_json::Value =
            serde_json::from_str(&capacity_json(&data, "merge").unwrap()).unwrap();
        assert_eq!(cap["max_selectable"], 250);
        assert!(capacity_json(&data, "sideways").is_err());

        let picked = serde_json::to_string(&vec![rep(
            Source::Hearham,
            "W0A",
            447_000_000,
            Mode::Analog,
            DENVER,
        )])
        .unwrap();
        let out = build_json(&data, &picked, r#"{"mode":"clean","zone_name":"Home"}"#).unwrap();
        let Document::AnyTone(cp) = Document::parse(&out).unwrap() else {
            panic!();
        };
        assert_eq!(cp.channels().count(), 1);
        assert!(build_json(&data, "[", "{}").is_err());
    }
}
