//! HTTP for the repeater sources. Behind the `net` feature so the parsers and
//! builder build without a TLS stack.
//!
//! Hearham and RadioID publish one whole-world file each, so those are cached
//! in the host's cache directory for a day. RepeaterBook is always fetched
//! live: its terms rule out keeping offline copies.

use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::geo::rate_limit;
use crate::geo::sources::repeaterbook;
use crate::geo::RepeaterBookAuth;

/// Hearham's full repeater list.
const HEARHAM_URL: &str = "https://hearham.com/api/repeaters/v1";
/// RadioID's full DMR repeater list.
const RADIOID_URL: &str = "https://www.radioid.net/static/rptrs.json";
/// RepeaterBook's North America export endpoint.
const REPEATERBOOK_URL: &str = "https://www.repeaterbook.com/api/export.php";
/// RepeaterBook's rest-of-world export endpoint.
const REPEATERBOOK_ROW_URL: &str = "https://www.repeaterbook.com/api/exportROW.php";
/// How long a cached download stays fresh.
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
/// Largest body accepted. Hearham is about 9 MB today.
const MAX_BODY: u64 = 64 * 1024 * 1024;
/// Overall per-request timeout.
const TIMEOUT: Duration = Duration::from_secs(90);

/// The User-Agent for the free sources.
fn user_agent() -> String {
    format!("codeplug-me/{}", env!("CARGO_PKG_VERSION"))
}

/// The User-Agent registered with RepeaterBook, which denies API access to
/// any request whose User-Agent does not match it.
pub const REPEATERBOOK_USER_AGENT: &str = "CodeplugMe/1.0 (+https://sleepymagpie.com/tools/codeplug-me; winter@faulk.me)";

/// An agent that hands back non-2xx responses instead of erroring, so the
/// error body can be shown.
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into()
}

/// Read a response, turning a non-2xx status into an error that carries
/// the start of the body.
fn read(name: &str, resp: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<Vec<u8>, String> {
    let (status, body) = read_any(name, resp)?;
    if !(200..300).contains(&status) {
        let text = String::from_utf8_lossy(&body);
        let snippet: String = text.chars().take(200).collect();
        return Err(format!("{name} returned HTTP {status}: {snippet}"));
    }
    Ok(body)
}

/// Read a response whatever its status, for callers that explain their own
/// error bodies.
fn read_any(
    name: &str,
    resp: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<(u16, Vec<u8>), String> {
    let mut resp = resp.map_err(|e| format!("{name}: {e}"))?;
    let status = resp.status().as_u16();
    let body = resp
        .body_mut()
        .with_config()
        .limit(MAX_BODY)
        .read_to_vec()
        .map_err(|e| format!("{name}: {e}"))?;
    Ok((status, body))
}

/// A cached copy of `file` in `dir` if it is younger than [`CACHE_TTL`].
fn fresh_cache(dir: &Path, file: &str) -> Option<Vec<u8>> {
    let path = dir.join(file);
    let age = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())?;
    if age > CACHE_TTL {
        return None;
    }
    std::fs::read(path).ok()
}

/// Download `url` unless `dir/file` is fresh, then cache the result.
/// A cache write failure only costs a re-download next time, so it is
/// ignored rather than failing a fetch that worked.
fn cached_get(name: &str, url: &str, dir: &Path, file: &str) -> Result<Vec<u8>, String> {
    if let Some(body) = fresh_cache(dir, file) {
        return Ok(body);
    }
    let body = read(name, agent().get(url).header("User-Agent", user_agent()).call())?;
    if std::fs::create_dir_all(dir).is_ok() {
        let _ = std::fs::write(dir.join(file), &body);
    }
    Ok(body)
}

/// Hearham's full list, from cache when fresh.
pub fn fetch_hearham(cache_dir: &Path) -> Result<Vec<u8>, String> {
    cached_get("Hearham", HEARHAM_URL, cache_dir, "hearham.json")
}

/// RadioID's full DMR list, from cache when fresh.
pub fn fetch_radioid(cache_dir: &Path) -> Result<Vec<u8>, String> {
    cached_get("RadioID", RADIOID_URL, cache_dir, "radioid.json")
}

/// One RepeaterBook export query, never cached. Refused before sending when
/// it would break the local limits in [`rate_limit`], whose history lives in
/// `cache_dir`.
pub fn fetch_repeaterbook(
    auth: &RepeaterBookAuth,
    country: Option<&str>,
    state: Option<&str>,
    analog_only: bool,
    cache_dir: &Path,
) -> Result<Vec<u8>, String> {
    let token = auth.token.trim();
    let contact = auth.contact.trim();
    if token.is_empty() || contact.is_empty() {
        return Err("RepeaterBook needs a token and contact email in settings".into());
    }
    let params = repeaterbook::query_params(country, state, analog_only)?;
    rate_limit::acquire(cache_dir)?;
    let north_america = country.is_some_and(|c| {
        ["United States", "Canada", "Mexico"]
            .iter()
            .any(|na| na.eq_ignore_ascii_case(c.trim()))
    });
    let url = if north_america {
        REPEATERBOOK_URL
    } else {
        REPEATERBOOK_ROW_URL
    };
    let mut req = agent()
        .get(url)
        .header("User-Agent", REPEATERBOOK_USER_AGENT)
        .header("X-RB-App-Token", token);
    for (k, v) in &params {
        req = req.query(*k, v);
    }
    let (status, body) = read_any("RepeaterBook", req.call())?;
    if !(200..300).contains(&status) {
        return Err(repeaterbook::explain_error(status, &body));
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_or_missing_cache_is_ignored() {
        let dir = std::env::temp_dir().join(format!("geo-cache-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(fresh_cache(&dir, "nope.json").is_none());
        std::fs::write(dir.join("fresh.json"), b"[]").unwrap();
        assert_eq!(fresh_cache(&dir, "fresh.json").as_deref(), Some(&b"[]"[..]));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn repeaterbook_without_credentials_fails_before_any_request() {
        let auth = RepeaterBookAuth {
            token: " ".into(),
            contact: "a@b.c".into(),
        };
        let err = fetch_repeaterbook(&auth, Some("United States"), Some("Colorado"), false, &std::env::temp_dir())
            .unwrap_err();
        assert!(err.contains("token"), "{err}");
    }

    #[test]
    fn repeaterbook_user_agent_matches_the_registered_value() {
        assert_eq!(REPEATERBOOK_USER_AGENT, "CodeplugMe/1.0 (+https://sleepymagpie.com/tools/codeplug-me; winter@faulk.me)");
    }
}

/// Live check against the real free sources. Ignored by default because it
/// downloads about 15 MB; run with `cargo test -p codeplugme-core live_ -- --ignored`.
#[cfg(test)]
mod live {
    use crate::document::Document;
    use crate::geo::{fetch, GeoQuery, Mode, Source};

    #[test]
    #[ignore]
    fn live_fetch_around_denver() {
        use crate::anytone::codeplug::{global_offset, CONTACT_BITMAP};
        let mut raw = vec![0u8; crate::model::Model::AnyToneD878UV2.image_size()];
        let off = global_offset(CONTACT_BITMAP).unwrap();
        raw[off..off + 0x500].fill(0xff);
        let doc = Document::parse(&raw).unwrap();
        let query = GeoQuery {
            lat: 39.7392,
            lon: -104.9903,
            radius_km: 40.0,
            country: Some("United States".into()),
            state: Some("Colorado".into()),
            sources: vec![Source::Hearham, Source::Radioid],
            repeaterbook: None,
        };
        let cache = std::env::temp_dir().join("anytone-geo-live");
        let found = fetch(&query, &doc, &cache).unwrap();
        let dmr = found.iter().filter(|r| r.mode == Mode::Dmr).count();
        eprintln!("{} repeaters, {dmr} DMR; nearest: {:?}", found.len(), found.first());
        assert!(found.len() > 10);
    }
}
