//! The AnyTone CTCSS tone table.
//!
//! A channel stores its CTCSS tone as a one-byte index into this table, not as
//! a frequency. The order is the radio's (qdmr `AnytoneCodeplug` CTCSS table):
//! the 50 standard EIA tones sorted ascending, with 62.5 Hz first. DCS has no
//! table: it is stored as the code's octal digits read as a binary number, plus
//! [`DCS_INVERTED_FLAG`] for inverted polarity (qdmr `setTXDCS`).

/// CTCSS tones in tenths of a hertz, in the radio's index order.
pub const CTCSS_TENTHS: [u16; 51] = [
    625, 670, 693, 719, 744, 770, 797, 825, 854, 885, 915, 948, 974, 1000, 1035, 1072, 1109, 1148,
    1188, 1230, 1273, 1318, 1365, 1413, 1462, 1514, 1567, 1598, 1622, 1655, 1679, 1713, 1738,
    1773, 1799, 1835, 1862, 1899, 1928, 1966, 1995, 2035, 2065, 2107, 2181, 2257, 2291, 2336,
    2418, 2503, 2541,
];

/// Bit set in the stored DCS word for inverted polarity.
pub const DCS_INVERTED_FLAG: u16 = 0x200;

/// The table index for a CTCSS tone, or `None` if the radio has no such tone.
pub fn ctcss_index(hz_tenths: u16) -> Option<u8> {
    CTCSS_TENTHS
        .iter()
        .position(|&t| t == hz_tenths)
        .map(|i| i as u8)
}

/// The stored DCS word for a code written as on the radio (23 for D023).
/// Returns `None` when a digit is not octal or the code is out of range.
pub fn dcs_word(code: u16, inverted: bool) -> Option<u16> {
    let digits = format!("{code:03}");
    let value = u16::from_str_radix(&digits, 8).ok()?;
    if value >= DCS_INVERTED_FLAG {
        return None;
    }
    Some(if inverted { value | DCS_INVERTED_FLAG } else { value })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_tones_map_to_their_index() {
        assert_eq!(ctcss_index(625), Some(0));
        assert_eq!(ctcss_index(670), Some(1));
        assert_eq!(ctcss_index(1000), Some(13));
        assert_eq!(ctcss_index(2541), Some(50));
        assert_eq!(ctcss_index(1001), None);
    }

    #[test]
    fn table_is_strictly_ascending() {
        assert!(CTCSS_TENTHS.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn dcs_words_are_octal() {
        assert_eq!(dcs_word(23, false), Some(0o23));
        assert_eq!(dcs_word(754, false), Some(0o754));
        assert_eq!(dcs_word(23, true), Some(0o23 | 0x200));
        assert_eq!(dcs_word(28, false), None);
    }
}
