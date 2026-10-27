//! Radio-agnostic byte primitives shared by every codeplug model: bitmap tests,
//! big-endian BCD, fixed-length Latin1 strings, and size alignment.
//!
//! Encodings follow qdmr's `lib/codeplug.cc` (GPLv3; referenced, not copied),
//! which every vendor in this space happens to share. Anything that depends on
//! a particular radio's memory map stays with that radio's module.

/// Test whether bit `i` is set in a little-endian bit-per-item bitmap
/// (`data[i/8] & (1 << (i%8))`).
pub(crate) fn bit_set(bitmap: &[u8], i: usize) -> bool {
    let byte = i / 8;
    let bit = i % 8;
    bitmap
        .get(byte)
        .map(|b| (b >> bit) & 1 == 1)
        .unwrap_or(false)
}

/// Test whether bit `i` is *clear* — the "active" test for an inverted bitmap
/// (qdmr `InvertedBitmapElement`, used by the contact bitmap).
pub(crate) fn bit_clear(bitmap: &[u8], i: usize) -> bool {
    !bit_set(bitmap, i)
}

/// Decode 8 big-endian BCD digits (4 bytes, most-significant nibble first).
pub(crate) fn get_bcd8_be(bytes: &[u8]) -> u32 {
    let mut v = 0u32;
    for &b in &bytes[..4] {
        v = v * 100 + (b >> 4) as u32 * 10 + (b & 0x0f) as u32;
    }
    v
}

/// Encode `val` as 8 big-endian BCD digits into `out[..4]` (most-significant
/// nibble first). Digits above 8 are dropped (`val` is taken mod 10^8).
pub(crate) fn set_bcd8_be(out: &mut [u8], mut val: u32) {
    for i in (0..4).rev() {
        let pair = (val % 100) as u8;
        out[i] = ((pair / 10) << 4) | (pair % 10);
        val /= 100;
    }
}

/// Decode packed BCD in little-endian byte order: the last byte holds the most
/// significant digit pair, and within each byte the high nibble is the more
/// significant digit. This is CHIRP's `lbcd`, used by the QYT channel record's
/// frequency fields.
pub(crate) fn get_bcd_le(bytes: &[u8]) -> u32 {
    let mut v = 0u32;
    for &b in bytes.iter().rev() {
        v = v * 100 + (b >> 4) as u32 * 10 + (b & 0x0f) as u32;
    }
    v
}

/// Encode `val` as packed BCD in little-endian byte order into `out`, the
/// inverse of [`get_bcd_le`]. Digits beyond `out.len() * 2` are dropped.
pub(crate) fn set_bcd_le(out: &mut [u8], mut val: u32) {
    for slot in out.iter_mut() {
        let pair = (val % 100) as u8;
        *slot = ((pair / 10) << 4) | (pair % 10);
        val /= 100;
    }
}

/// Read a fixed-length Latin1 string, stopping at the first `fill` byte, and
/// trim trailing spaces for display.
pub(crate) fn read_ascii(bytes: &[u8], fill: u8) -> String {
    let end = bytes.iter().position(|&b| b == fill).unwrap_or(bytes.len());
    let s: String = bytes[..end].iter().map(|&b| b as char).collect();
    s.trim_end_matches(' ').to_string()
}

/// Write a fixed-length Latin1 string into `dst`, padding/terminating the
/// remainder with `fill`. `s` is truncated to `dst.len()` bytes.
pub(crate) fn write_ascii(dst: &mut [u8], s: &str, fill: u8) {
    for b in dst.iter_mut() {
        *b = fill;
    }
    for (slot, byte) in dst.iter_mut().zip(s.bytes()) {
        *slot = byte;
    }
}

/// Round `n` up to the next multiple of `a` (qdmr `align_size`).
pub(crate) fn align_up(n: usize, a: usize) -> usize {
    n.div_ceil(a) * a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bcd_roundtrip() {
        let mut buf = [0u8; 4];
        set_bcd8_be(&mut buf, 43_900_000);
        assert_eq!(buf, [0x43, 0x90, 0x00, 0x00]);
        assert_eq!(get_bcd8_be(&buf), 43_900_000);
    }

    #[test]
    fn little_endian_bcd_decodes_a_real_qyt_channel_frequency() {
        // Channel 0 of the vendor capture: 462.56250 MHz in units of 10 Hz.
        assert_eq!(get_bcd_le(&[0x50, 0x62, 0x25, 0x46]), 46_256_250);
    }

    #[test]
    fn little_endian_bcd_round_trips() {
        let mut buf = [0u8; 4];
        set_bcd_le(&mut buf, 46_256_250);
        assert_eq!(buf, [0x50, 0x62, 0x25, 0x46]);
        assert_eq!(get_bcd_le(&buf), 46_256_250);

        set_bcd_le(&mut buf, 14_732_000);
        assert_eq!(get_bcd_le(&buf), 14_732_000);

        set_bcd_le(&mut buf, 0);
        assert_eq!(buf, [0, 0, 0, 0]);
        assert_eq!(get_bcd_le(&buf), 0);
    }

    #[test]
    fn little_endian_bcd_is_not_the_big_endian_one() {
        // A guard against wiring the wrong helper into the QYT channel record:
        // the two disagree on every value that is not a palindrome.
        let buf = [0x50, 0x62, 0x25, 0x46];
        assert_ne!(get_bcd_le(&buf), get_bcd8_be(&buf));
    }
}
