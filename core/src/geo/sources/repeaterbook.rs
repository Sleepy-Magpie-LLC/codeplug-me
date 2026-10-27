//! RepeaterBook's export API (`https://www.repeaterbook.com/api/export.php`).
//!
//! Needs a per-user `rbuapp_` token and a User-Agent naming a contact. The API
//! has no radius search, so it is queried by state (US, by FIPS `state_id`) or
//! by country, and [`crate::geo::select`] filters by distance.
//!
//! The response is `{"count": n, "results": [...]}`. The field names below are
//! the ones existing RepeaterBook clients read; RepeaterBook's own wiki lists
//! only labels, so they are confirmed against a live response once a token is
//! in hand. Numbers arrive as strings.

use serde::Deserialize;
use serde_json::Value;

use crate::geo::sources::{as_f64, mhz_to_hz};
use crate::geo::{tone, Mode, Repeater, Source};
use crate::qyt::codeplug::Tone;

/// Top-level wrapper of a response.
#[derive(Deserialize)]
struct Response {
    #[serde(default)]
    results: Vec<Record>,
}

/// The subset of a RepeaterBook record the importer reads.
#[derive(Deserialize)]
struct Record {
    #[serde(rename = "Callsign", default)]
    callsign: String,
    #[serde(rename = "Frequency", default)]
    frequency: String,
    #[serde(rename = "Input Freq", default)]
    input_freq: String,
    #[serde(rename = "PL", default)]
    pl: Value,
    #[serde(rename = "TSQ", default)]
    tsq: Value,
    #[serde(rename = "Nearest City", default)]
    nearest_city: String,
    #[serde(rename = "State", default)]
    state: String,
    #[serde(rename = "Lat", default)]
    lat: Value,
    #[serde(rename = "Long", default)]
    long: Value,
    #[serde(rename = "Operational Status", default)]
    status: String,
    #[serde(rename = "FM Analog", default)]
    fm_analog: String,
    #[serde(rename = "DMR", default)]
    dmr: String,
    #[serde(rename = "DMR Color Code", default)]
    dmr_color_code: Value,
}

/// A RepeaterBook yes/no field.
fn yes(field: &str) -> bool {
    field.trim().eq_ignore_ascii_case("yes")
}

/// A tone that arrives as either a number or a string.
fn tone_of(v: &Value) -> Tone {
    match v {
        Value::String(s) => tone::parse(s).tone.unwrap_or(Tone::None),
        Value::Number(n) => tone::ctcss_from_hz(&n.to_string()).unwrap_or(Tone::None),
        _ => Tone::None,
    }
}

/// Parse a response. A repeater that is both FM and DMR becomes two records,
/// one per mode, since a channel is one or the other.
pub fn parse(body: &[u8]) -> Result<Vec<Repeater>, String> {
    let response: Response =
        serde_json::from_slice(body).map_err(|e| format!("RepeaterBook response: {e}"))?;
    let mut out = Vec::new();
    for r in response.results {
        if r.status.trim().to_ascii_lowercase().starts_with("off") {
            continue;
        }
        let (Some(rx), Some(tx)) = (mhz_to_hz(&r.frequency), mhz_to_hz(&r.input_freq)) else {
            continue;
        };
        let location = match (r.nearest_city.trim(), r.state.trim()) {
            (c, "") => c.to_string(),
            (c, s) => format!("{c}, {s}"),
        };
        let base = Repeater {
            source: Source::Repeaterbook,
            callsign: r.callsign.trim().to_string(),
            location,
            rx_hz: rx,
            tx_hz: tx,
            mode: Mode::Analog,
            tx_tone: Tone::None,
            rx_tone: Tone::None,
            color_code: None,
            lat: as_f64(&r.lat),
            lon: as_f64(&r.long),
            distance_km: None,
            duplicate: false,
        };
        if yes(&r.fm_analog) {
            out.push(Repeater {
                tx_tone: tone_of(&r.pl),
                rx_tone: tone_of(&r.tsq),
                ..base.clone()
            });
        }
        if yes(&r.dmr) {
            out.push(Repeater {
                mode: Mode::Dmr,
                color_code: as_f64(&r.dmr_color_code)
                    .map(|c| c as u8)
                    .filter(|&c| c <= 15),
                ..base
            });
        }
    }
    Ok(out)
}

/// US state names to the FIPS codes RepeaterBook's `state_id` takes.
const US_FIPS: [(&str, &str); 52] = [
    ("Alabama", "01"),
    ("Alaska", "02"),
    ("Arizona", "04"),
    ("Arkansas", "05"),
    ("California", "06"),
    ("Colorado", "08"),
    ("Connecticut", "09"),
    ("Delaware", "10"),
    ("District of Columbia", "11"),
    ("Florida", "12"),
    ("Georgia", "13"),
    ("Hawaii", "15"),
    ("Idaho", "16"),
    ("Illinois", "17"),
    ("Indiana", "18"),
    ("Iowa", "19"),
    ("Kansas", "20"),
    ("Kentucky", "21"),
    ("Louisiana", "22"),
    ("Maine", "23"),
    ("Maryland", "24"),
    ("Massachusetts", "25"),
    ("Michigan", "26"),
    ("Minnesota", "27"),
    ("Mississippi", "28"),
    ("Missouri", "29"),
    ("Montana", "30"),
    ("Nebraska", "31"),
    ("Nevada", "32"),
    ("New Hampshire", "33"),
    ("New Jersey", "34"),
    ("New Mexico", "35"),
    ("New York", "36"),
    ("North Carolina", "37"),
    ("North Dakota", "38"),
    ("Ohio", "39"),
    ("Oklahoma", "40"),
    ("Oregon", "41"),
    ("Pennsylvania", "42"),
    ("Rhode Island", "44"),
    ("South Carolina", "45"),
    ("South Dakota", "46"),
    ("Tennessee", "47"),
    ("Texas", "48"),
    ("Utah", "49"),
    ("Vermont", "50"),
    ("Virginia", "51"),
    ("Washington", "53"),
    ("West Virginia", "54"),
    ("Wisconsin", "55"),
    ("Wyoming", "56"),
    ("Puerto Rico", "72"),
];

/// The FIPS code for a US state name, case-insensitively.
pub fn us_state_fips(state: &str) -> Option<&'static str> {
    US_FIPS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(state.trim()))
        .map(|&(_, code)| code)
}

/// Query parameters for a lookup. US queries narrow by state; anywhere else
/// RepeaterBook only takes the country, and distance filtering does the rest.
pub fn query_params(
    country: Option<&str>,
    state: Option<&str>,
    analog_only: bool,
) -> Result<Vec<(&'static str, String)>, String> {
    let country = country.ok_or("RepeaterBook needs a country for the location")?;
    let mut params = vec![("country", country.to_string())];
    let is_us = country.eq_ignore_ascii_case("United States");
    if is_us {
        let state = state.ok_or("RepeaterBook needs a US state for the location")?;
        let fips = us_state_fips(state).ok_or_else(|| format!("{state} is not a US state"))?;
        params.push(("state_id", fips.to_string()));
    }
    if analog_only {
        params.push(("mode", "analog".to_string()));
    }
    Ok(params)
}

/// The JSON RepeaterBook sends with a refusal.
#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default)]
    error_code: String,
    #[serde(default)]
    message: String,
}

/// A user-facing explanation of a refused request.
///
/// Every authorization problem comes back as HTTP 401, so the status alone
/// says nothing; the `error_code` in the body is what tells a wrong token from
/// an unapproved app or a User-Agent mismatch, and each gets the fix the user
/// can actually make.
pub fn explain_error(status: u16, body: &[u8]) -> String {
    let Ok(e) = serde_json::from_slice::<ErrorBody>(body) else {
        let text = String::from_utf8_lossy(body);
        let snippet: String = text.chars().take(200).collect();
        return format!("RepeaterBook returned HTTP {status}: {snippet}");
    };
    let hint = match e.error_code.as_str() {
        "auth_missing" => "Add your RepeaterBook token in settings.",
        "auth_invalid" => {
            "RepeaterBook does not accept the saved token for repeater export. The \
             export API takes only app tokens (rbuapp_ or app_), not the private \
             token on your account page; an rbuapp_ token comes from API Apps on \
             repeaterbook.com once RepeaterBook has approved the app."
        }
        "auth_inactive" => {
            "The token or its app is not active yet. RepeaterBook approves API \
             access by hand; check API Apps on repeaterbook.com."
        }
        "auth_revoked" => "The token was revoked. Generate a new one under API Apps on repeaterbook.com.",
        "auth_scope_denied" => {
            "The token is not approved for this region's endpoint (North America \
             and the rest of the world are approved separately)."
        }
        "ua_mismatch" => {
            "RepeaterBook ties the token to a User-Agent and this app's did not \
             match. Check the contact email in settings matches the one on file."
        }
        "rate_limited" => "Too many requests. Wait a few minutes before fetching again.",
        _ => "",
    };
    let detail = match (e.error_code.is_empty(), e.message.is_empty()) {
        (true, true) => format!("HTTP {status}"),
        (true, false) => e.message,
        (false, true) => e.error_code,
        (false, false) => format!("{}: {}", e.error_code, e.message),
    };
    if hint.is_empty() {
        format!("RepeaterBook refused the request ({detail}).")
    } else {
        format!("RepeaterBook refused the request ({detail}). {hint}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/geo/repeaterbook.json");

    #[test]
    fn splits_dual_mode_and_skips_off_air() {
        let rs = parse(FIXTURE).unwrap();
        assert_eq!(rs.len(), 3, "{rs:#?}");

        let fm = &rs[0];
        assert_eq!(fm.callsign, "W0CRA");
        assert_eq!(fm.mode, Mode::Analog);
        assert_eq!(fm.rx_hz, 146_940_000);
        assert_eq!(fm.tx_hz, 146_340_000);
        assert_eq!(fm.tx_tone, Tone::Ctcss { hz_tenths: 1000 });
        assert_eq!(fm.rx_tone, Tone::None);
        assert_eq!(fm.lat, Some(39.7392));
        assert_eq!(fm.location, "Denver, Colorado");

        assert_eq!(rs[1].mode, Mode::Analog);
        assert_eq!(rs[2].mode, Mode::Dmr);
        assert_eq!(rs[2].callsign, rs[1].callsign);
        assert_eq!(rs[2].color_code, Some(1));
        assert_eq!(rs[2].tx_tone, Tone::None);
    }

    #[test]
    fn us_queries_use_fips() {
        let p = query_params(Some("United States"), Some("colorado"), false).unwrap();
        assert_eq!(
            p,
            vec![
                ("country", "United States".to_string()),
                ("state_id", "08".to_string())
            ]
        );
        assert!(query_params(Some("United States"), None, false).is_err());
        assert!(query_params(Some("United States"), Some("Narnia"), false).is_err());
    }

    #[test]
    fn non_us_queries_use_country_and_analog_filter() {
        let p = query_params(Some("Canada"), Some("Alberta"), true).unwrap();
        assert_eq!(
            p,
            vec![
                ("country", "Canada".to_string()),
                ("mode", "analog".to_string())
            ]
        );
    }

    #[test]
    fn unknown_token_explains_where_tokens_come_from() {
        let msg = explain_error(
            401,
            br#"{"ok":false,"error_code":"auth_invalid","message":"Unknown app token."}"#,
        );
        assert!(msg.contains("auth_invalid: Unknown app token."), "{msg}");
        assert!(msg.contains("rbuapp_"), "{msg}");
    }

    #[test]
    fn non_json_errors_fall_back_to_the_status() {
        let msg = explain_error(503, b"<html>down</html>");
        assert!(msg.contains("HTTP 503"), "{msg}");
    }
}
