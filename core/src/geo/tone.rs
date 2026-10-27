//! Parsing the free-form tone strings repeater databases publish.
//!
//! Hearham alone has "100.0", "100.", "123", "DCS023", "DCS 411", "D244",
//! "DCS411N", "CC4", and slash-joined mixes like "100.0/CC7/NAC 293". Each
//! slash-separated token is read on its own; the first CTCSS or DCS token wins
//! and the first `CC` token gives the color code.

use crate::qyt::codeplug::Tone;

/// Lowest and highest CTCSS tone anyone uses, in tenths of a hertz. Values
/// outside this range ("0.00", "000.0") mean no tone.
const CTCSS_RANGE: std::ops::RangeInclusive<u16> = 600..=2600;

/// What a tone field held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Parsed {
    /// The sub-audible tone, if any.
    pub tone: Option<Tone>,
    /// A DMR color code, if any.
    pub color_code: Option<u8>,
}

/// Parse a tone field. Unrecognized tokens are ignored, not errors: a missed
/// tone leaves a channel on carrier squelch, which the user can fix, while
/// rejecting the record would hide the repeater entirely.
pub fn parse(field: &str) -> Parsed {
    let mut out = Parsed::default();
    for token in field.split('/') {
        let t = token.trim().to_ascii_uppercase().replace(' ', "");
        if t.is_empty() {
            continue;
        }
        if let Some(cc) = t.strip_prefix("CC") {
            if out.color_code.is_none() {
                out.color_code = cc.parse::<u8>().ok().filter(|&c| c <= 15);
            }
            continue;
        }
        if out.tone.is_none() {
            out.tone = parse_dcs(&t).or_else(|| parse_ctcss(&t));
        }
    }
    out
}

/// "DCS023", "D244", "DCS411N" (normal) or "DCS411I" (inverted).
fn parse_dcs(t: &str) -> Option<Tone> {
    let rest = t.strip_prefix("DCS").or_else(|| t.strip_prefix('D'))?;
    let (digits, inverted) = match rest.strip_suffix('I') {
        Some(d) => (d, true),
        None => (rest.strip_suffix('N').unwrap_or(rest), false),
    };
    if digits.is_empty() || !digits.chars().all(|c| ('0'..='7').contains(&c)) {
        return None;
    }
    Some(Tone::Dcs {
        code: digits.parse().ok()?,
        inverted,
    })
}

/// "100.0", "100.", "123", "71.9".
fn parse_ctcss(t: &str) -> Option<Tone> {
    let hz: f64 = t.trim_end_matches('.').parse().ok()?;
    let tenths = (hz * 10.0).round() as u16;
    CTCSS_RANGE
        .contains(&tenths)
        .then_some(Tone::Ctcss { hz_tenths: tenths })
}

/// A CTCSS frequency in Hz from a number field (RepeaterBook's "PL").
pub fn ctcss_from_hz(field: &str) -> Option<Tone> {
    parse_ctcss(field.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctcss(hz_tenths: u16) -> Option<Tone> {
        Some(Tone::Ctcss { hz_tenths })
    }

    #[test]
    fn plain_ctcss_forms() {
        assert_eq!(parse("100.0").tone, ctcss(1000));
        assert_eq!(parse("100.").tone, ctcss(1000));
        assert_eq!(parse("123").tone, ctcss(1230));
        assert_eq!(parse(" 71.9").tone, ctcss(719));
    }

    #[test]
    fn zero_and_junk_mean_no_tone() {
        for s in ["0.00", "000.0", "0", "", " ", "COS", "C", "A", "DMR", "RAN1"] {
            assert_eq!(parse(s).tone, None, "{s:?}");
        }
    }

    #[test]
    fn dcs_forms() {
        let d = |code, inverted| Some(Tone::Dcs { code, inverted });
        assert_eq!(parse("DCS023").tone, d(23, false));
        assert_eq!(parse("DCS 411").tone, d(411, false));
        assert_eq!(parse("DCS411N").tone, d(411, false));
        assert_eq!(parse("D244").tone, d(244, false));
        assert_eq!(parse("DCS100I").tone, d(100, true));
        // 8 is not an octal digit, so this is not a DCS code.
        assert_eq!(parse("DCS028").tone, None);
    }

    #[test]
    fn mixed_fields() {
        let p = parse("100.0/CC7/NAC 293");
        assert_eq!(p.tone, ctcss(1000));
        assert_eq!(p.color_code, Some(7));
        assert_eq!(parse("CC11").color_code, Some(11));
        assert_eq!(parse("NAC293/136.5").tone, ctcss(1365));
        assert_eq!(parse("71.9/127.3").tone, ctcss(719));
    }
}
