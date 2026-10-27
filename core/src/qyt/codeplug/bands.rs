//! The VFO records at `0x0F00`.
//!
//! The image layout holds four 32-byte slots (A–D), inherited from the WP-9900
//! map. The KT-5000 only has two VFOs (A and B); slots C and D are parsed and
//! written back verbatim so they survive a round trip, but they are not exposed
//! in the dump or accepted by edits. See [`NUM_VFOS`].
//!
//! # The encoding trap
//!
//! `freq[8]` here does **not** use the packed BCD of the channel table. It
//! stores **one decimal digit per raw byte**, eight bytes for eight digits, in
//! units of 10 Hz. Confirmed by decoding record 0 of the vendor capture,
//! `04 06 02 05 06 02 05 00`, to 46256250 and therefore 462.5625 MHz, which is
//! exactly channel 0's frequency. Reading these bytes as packed BCD instead
//! silently produces a plausible-looking wrong frequency, so the two decoders
//! are deliberately kept apart: [`crate::bytes::get_bcd_le`] is for channels,
//! [`digits_to_value`] is for bands.
//!
//! Field layout is CHIRP's `settings_vfo`. `offset[6]` is assumed to use the
//! same one-digit-per-byte encoding, but no capture value confirms it (the four
//! image slots decode to 10 kHz, 21 kHz, 106.5 kHz and 16.25 kHz, none of them a
//! usual repeater offset), so the offset and the shift direction that goes with
//! it are read-only here.

use serde::Serialize;

use crate::error::{Error, Result};
use crate::qyt::codeplug::channel::{Bandwidth, OptSignaling, Power, SpeakerMute, Tone};

/// Number of VFO slots the KT-5000 actually has (A and B).
pub const NUM_VFOS: usize = 2;

/// Number of 32-byte VFO-shaped records in the image at `0x0F00`.
///
/// Larger than [`NUM_VFOS`] because the WP-9900 layout this image inherits
/// reserved four slots. Slots beyond [`NUM_VFOS`] are byte-preserved only.
pub const NUM_BANDS: usize = 4;

/// Size of one band/VFO record in bytes.
pub const BAND_SIZE: usize = 32;

/// Offset and length of the frequency digit string.
const FREQ: std::ops::Range<usize> = 0..8;
/// Offset and length of the offset digit string.
const OFFSET: std::ops::Range<usize> = 8..14;
/// Offset of the RX tone word, `u16` little-endian.
const RX_TONE: usize = 16;
/// Offset of the TX tone word, `u16` little-endian.
const TX_TONE: usize = 18;
/// Offset of the PTT-ID signal code (a whole byte here, not a nibble).
const SCODE: usize = 20;
/// Offset of the speaker-mute mode.
const SPMUTE: usize = 21;
/// Offset of the optional-signaling mode.
const OPTSIG: usize = 22;
/// Offset of the byte whose bit 0 is the scrambler flag.
const SCRAMBLE: usize = 23;
/// Offset of the bandwidth byte (`0` = wide, as in the channel table).
const WIDE: usize = 24;
/// Offset of the power byte (`0` = High, `1` = Low).
const POWER: usize = 25;
/// Offset of the repeater shift direction.
const SHIFTD: usize = 26;
/// Offset of the tuning step index.
const STEP: usize = 27;

/// Tuning steps in Hz, indexed by the `step` byte (CHIRP's `STEPS`, kHz).
const STEPS_HZ: [u32; 6] = [2_500, 5_000, 6_250, 10_000, 12_500, 25_000];

/// Repeater shift direction (CHIRP `LIST_SHIFT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Shift {
    /// Simplex.
    Off,
    /// Transmit above the receive frequency.
    Plus,
    /// Transmit below the receive frequency.
    Minus,
}

impl Shift {
    /// Decode the shift byte. Anything outside 0-2 reads as simplex.
    fn from_byte(v: u8) -> Shift {
        match v {
            1 => Shift::Plus,
            2 => Shift::Minus,
            _ => Shift::Off,
        }
    }
}

/// Decode a run of one-decimal-digit-per-byte values, most significant first.
/// Returns `None` if any byte is not a digit, which is how an unprogrammed
/// (`0xFF`-filled) record is recognised.
pub fn digits_to_value(bytes: &[u8]) -> Option<u32> {
    let mut v = 0u32;
    for &b in bytes {
        if b > 9 {
            return None;
        }
        v = v * 10 + b as u32;
    }
    Some(v)
}

/// Encode `value` as one decimal digit per byte, most significant first,
/// filling `out` exactly. Digits beyond `out.len()` are dropped.
fn value_to_digits(out: &mut [u8], mut value: u32) {
    for slot in out.iter_mut().rev() {
        *slot = (value % 10) as u8;
        value /= 10;
    }
}

/// One band/VFO record.
#[derive(Debug, Clone, Serialize)]
pub struct Band {
    /// Record number: 0 = VFO A, 1 = VFO B. Image also has unused slots 2–3.
    pub index: usize,
    /// Whether the record holds a decodable frequency at all.
    pub programmed: bool,
    /// Frequency in Hz, or `None` for an unprogrammed record.
    pub frequency_hz: Option<u32>,
    /// Repeater offset in Hz. Read-only: the encoding is assumed, not
    /// confirmed. See the module docs.
    pub offset_hz: Option<u32>,
    /// Repeater shift direction. Read-only for the same reason as the offset.
    pub shift: Shift,
    /// Sub-audible tone the radio listens for.
    pub rx_tone: Tone,
    /// Sub-audible tone the radio transmits.
    pub tx_tone: Tone,
    /// PTT-ID signal code index.
    pub scode: u8,
    /// Speaker mute condition.
    pub speaker_mute: SpeakerMute,
    /// Optional signaling selection.
    pub optional_signaling: OptSignaling,
    /// Voice inversion scrambler.
    pub scramble: bool,
    /// Channel bandwidth.
    pub bandwidth: Bandwidth,
    /// Transmit power level.
    pub power: Power,
    /// Tuning step in Hz, or `None` if the stored index is out of range.
    pub step_hz: Option<u32>,
    /// Authoritative record; skipped in JSON output.
    #[serde(skip)]
    raw: [u8; BAND_SIZE],
}

impl Band {
    /// Parse band record `index`. Returns [`Error::Parse`] on a wrong length.
    pub fn parse(index: usize, rec: &[u8]) -> Result<Band> {
        if rec.len() != BAND_SIZE {
            return Err(Error::Parse(format!(
                "QYT band record is {} bytes, expected {BAND_SIZE}",
                rec.len()
            )));
        }
        let mut raw = [0u8; BAND_SIZE];
        raw.copy_from_slice(rec);
        Ok(Self::from_raw(index, raw))
    }

    /// The raw record for serialization (lossless).
    pub fn encode(&self) -> &[u8; BAND_SIZE] {
        &self.raw
    }

    /// Set the frequency in Hz, rewriting only the eight digit bytes. Stored in
    /// units of 10 Hz, so the eight digits top out just under 1 GHz.
    pub fn set_frequency(&mut self, hz: u32) -> std::result::Result<(), String> {
        let value = hz / 10;
        if value > 99_999_999 {
            return Err(format!("{hz} Hz does not fit in eight digits"));
        }
        value_to_digits(&mut self.raw[FREQ], value);
        *self = Self::from_raw(self.index, self.raw);
        Ok(())
    }

    /// Set the receive tone, rewriting only bytes 16-17.
    pub fn set_rx_tone(&mut self, tone: Tone) -> std::result::Result<(), String> {
        let word = tone.encode()?;
        self.raw[RX_TONE..RX_TONE + 2].copy_from_slice(&word.to_le_bytes());
        *self = Self::from_raw(self.index, self.raw);
        Ok(())
    }

    /// Set the transmit tone, rewriting only bytes 18-19.
    pub fn set_tx_tone(&mut self, tone: Tone) -> std::result::Result<(), String> {
        let word = tone.encode()?;
        self.raw[TX_TONE..TX_TONE + 2].copy_from_slice(&word.to_le_bytes());
        *self = Self::from_raw(self.index, self.raw);
        Ok(())
    }

    /// Set the bandwidth, rewriting only byte 24. Unlike the channel record's
    /// packed bit, this is a whole byte.
    pub fn set_bandwidth(&mut self, bandwidth: Bandwidth) {
        self.raw[WIDE] = match bandwidth {
            Bandwidth::Wide => 0,
            Bandwidth::Narrow => 1,
        };
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the transmit power, rewriting only byte 25.
    pub fn set_power(&mut self, power: Power) {
        self.raw[POWER] = match power {
            Power::High => 0,
            Power::Low => 1,
        };
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the tuning step to the nearest listed step, rewriting only byte 27.
    /// Errors if `hz` is not one of the six steps the radio offers.
    pub fn set_step(&mut self, hz: u32) -> std::result::Result<(), String> {
        let i = STEPS_HZ
            .iter()
            .position(|&s| s == hz)
            .ok_or_else(|| format!("{hz} Hz is not one of this radio's tuning steps"))?;
        self.raw[STEP] = i as u8;
        *self = Self::from_raw(self.index, self.raw);
        Ok(())
    }

    /// Rebuild the decoded fields from `raw`.
    fn from_raw(index: usize, raw: [u8; BAND_SIZE]) -> Band {
        let frequency = digits_to_value(&raw[FREQ]).map(|v| v * 10);
        Band {
            index,
            programmed: frequency.is_some(),
            frequency_hz: frequency,
            offset_hz: digits_to_value(&raw[OFFSET]).map(|v| v * 10),
            shift: Shift::from_byte(raw[SHIFTD]),
            rx_tone: Tone::decode(u16::from_le_bytes([raw[RX_TONE], raw[RX_TONE + 1]])),
            tx_tone: Tone::decode(u16::from_le_bytes([raw[TX_TONE], raw[TX_TONE + 1]])),
            scode: raw[SCODE],
            speaker_mute: SpeakerMute::from_bits(raw[SPMUTE]),
            optional_signaling: OptSignaling::from_bits(raw[OPTSIG]),
            scramble: raw[SCRAMBLE] & 1 != 0,
            bandwidth: Bandwidth::from_bit(raw[WIDE] != 0),
            power: Power::from_bits(raw[POWER]),
            step_hz: STEPS_HZ.get(raw[STEP] as usize).copied(),
            raw,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Band record 0 exactly as the vendor capture holds it.
    const CAPTURE: [u8; BAND_SIZE] = [
        0x04, 0x06, 0x02, 0x05, 0x06, 0x02, 0x05, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x24,
    ];

    #[test]
    fn band_frequency_uses_one_digit_per_byte() {
        let b = Band::parse(0, &CAPTURE).unwrap();
        // 04 06 02 05 06 02 05 00 reads as 46256250, times 10 Hz.
        assert_eq!(b.frequency_hz, Some(462_562_500));
        assert!(b.programmed);

        // The same bytes read as packed BCD would give something else entirely,
        // which is the mistake this test exists to catch.
        assert_ne!(
            crate::bytes::get_bcd_le(&CAPTURE[0..4]) * 10,
            462_562_500 / 10_000
        );
    }

    #[test]
    fn the_other_three_capture_records_decode_too() {
        let cases = [
            ([0x04, 0x06, 0x02, 0x06, 0x01, 0x05, 0x00, 0x00], 462_615_000),
            ([0x04, 0x05, 0x00, 0x00, 0x02, 0x05, 0x00, 0x00], 450_025_000),
            ([0x04, 0x06, 0x00, 0x06, 0x02, 0x05, 0x00, 0x00], 460_625_000),
        ];
        for (digits, hz) in cases {
            let mut rec = CAPTURE;
            rec[FREQ].copy_from_slice(&digits);
            assert_eq!(Band::parse(1, &rec).unwrap().frequency_hz, Some(hz));
        }
    }

    #[test]
    fn an_unprogrammed_record_has_no_frequency() {
        let b = Band::parse(3, &[0xFF; BAND_SIZE]).unwrap();
        assert_eq!(b.frequency_hz, None);
        assert_eq!(b.offset_hz, None);
        assert!(!b.programmed);
        // An out-of-range step index reads as unknown rather than as 2.5 kHz.
        assert_eq!(b.step_hz, None);
    }

    #[test]
    fn the_capture_record_decodes_its_remaining_fields() {
        let b = Band::parse(0, &CAPTURE).unwrap();
        assert_eq!(b.rx_tone, Tone::None);
        assert_eq!(b.tx_tone, Tone::None);
        assert_eq!(b.scode, 0);
        assert_eq!(b.speaker_mute, SpeakerMute::ToneDcs);
        assert_eq!(b.optional_signaling, OptSignaling::Off);
        assert!(!b.scramble);
        assert_eq!(b.bandwidth, Bandwidth::Wide);
        assert_eq!(b.power, Power::High);
        assert_eq!(b.shift, Shift::Off);
        assert_eq!(b.step_hz, Some(2_500));
    }

    #[test]
    fn encode_returns_an_untouched_record_verbatim() {
        assert_eq!(Band::parse(0, &CAPTURE).unwrap().encode(), &CAPTURE);
    }

    #[test]
    fn frequency_round_trips_through_the_digit_encoding() {
        let mut b = Band::parse(0, &CAPTURE).unwrap();
        b.set_frequency(147_320_000).unwrap();
        assert_eq!(b.frequency_hz, Some(147_320_000));
        assert_eq!(&b.encode()[FREQ], &[0x01, 0x04, 0x07, 0x03, 0x02, 0x00, 0x00, 0x00]);
        assert!(b.set_frequency(1_000_000_000).is_err());
    }

    #[test]
    fn each_setter_touches_only_its_own_bytes() {
        let base = Band::parse(0, &CAPTURE).unwrap();
        let changed = |b: &Band| -> Vec<usize> {
            (0..BAND_SIZE).filter(|&i| b.encode()[i] != CAPTURE[i]).collect()
        };

        let mut b = base.clone();
        b.set_frequency(147_320_000).unwrap();
        assert!(changed(&b).iter().all(|i| FREQ.contains(i)));

        let mut b = base.clone();
        b.set_rx_tone(Tone::Ctcss { hz_tenths: 885 }).unwrap();
        assert_eq!(changed(&b), vec![RX_TONE, RX_TONE + 1]);

        let mut b = base.clone();
        b.set_tx_tone(Tone::Dcs { code: 23, inverted: false }).unwrap();
        assert_eq!(changed(&b), vec![TX_TONE]);

        let mut b = base.clone();
        b.set_bandwidth(Bandwidth::Narrow);
        assert_eq!(changed(&b), vec![WIDE]);

        let mut b = base.clone();
        b.set_power(Power::Low);
        assert_eq!(changed(&b), vec![POWER]);

        let mut b = base.clone();
        b.set_step(25_000).unwrap();
        assert_eq!(changed(&b), vec![STEP]);
        assert!(b.set_step(3_000).is_err());
    }

    #[test]
    fn the_unmodelled_record_tail_survives_an_edit() {
        // Records 0 and 1 end in 0x24, records 2 and 3 in 0xFF; nothing here
        // may synthesize a value for those four unnamed bytes.
        let mut b = Band::parse(0, &CAPTURE).unwrap();
        b.set_frequency(147_320_000).unwrap();
        b.set_power(Power::Low);
        assert_eq!(&b.encode()[28..], &[0x00, 0x00, 0x00, 0x24]);
        assert_eq!(&b.encode()[14..16], &[0x00, 0x00]);
    }

    #[test]
    fn parse_rejects_a_wrongly_sized_record() {
        assert!(Band::parse(0, &[0u8; 16]).is_err());
    }
}
