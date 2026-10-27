//! One parser per repeater database. Each takes the raw response body and
//! returns normalized [`Repeater`](super::Repeater)s; none of them touch the
//! network, so they are tested against fixtures in `core/tests/fixtures/geo`.

pub mod hearham;
pub mod radioid;
pub mod repeaterbook;

/// Parse a frequency given in MHz as text ("440.58750") into Hz.
pub(crate) fn mhz_to_hz(text: &str) -> Option<u32> {
    let mhz: f64 = text.trim().parse().ok()?;
    (mhz > 0.0).then(|| (mhz * 1_000_000.0).round() as u32)
}

/// Read a JSON value that some sources send as a number and others as a
/// string.
pub(crate) fn as_f64(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Case-insensitive match where an absent filter matches everything.
pub(crate) fn matches_filter(value: &str, filter: Option<&str>) -> bool {
    filter.is_none_or(|f| value.trim().eq_ignore_ascii_case(f.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mhz_text_to_hz() {
        assert_eq!(mhz_to_hz("440.58750"), Some(440_587_500));
        assert_eq!(mhz_to_hz(" 146.94 "), Some(146_940_000));
        assert_eq!(mhz_to_hz("0"), None);
        assert_eq!(mhz_to_hz("abc"), None);
    }
}
