//! The QYT KT-5000 channel: a 16-byte record at `0x0000 + index*16` plus a
//! separate 16-byte name record at `0x1000 + index*16`.
//!
//! Field layout comes from CHIRP's `COLOR9900_MEM_FORMAT` (`btech.py`), read as
//! a hardware reference and reimplemented here; see
//! `docs/reference/qyt-kt5000-chirp-notes.md`. Every offset and every value
//! table below was cross-checked against the vendor CPS captures in
//! `docs/reference/QYT/`, and anything the captures could not confirm is called
//! out inline.
//!
//! Like the AnyTone records, the struct keeps its raw bytes as the source of
//! truth so [`encode`](QytChannel::encode) is lossless. The decoded fields are a
//! cache rebuilt from `raw` after every setter, which is what makes unmodelled
//! bits (byte 15 bits 5-4, and byte 7 onward of the name record) survive an
//! edit untouched.

use serde::{Deserialize, Serialize};

use crate::bytes::{get_bcd_le, read_ascii, set_bcd_le, write_ascii};
use crate::error::{Error, Result};

/// Size of a channel record in bytes.
pub const CHANNEL_SIZE: usize = 16;

/// Size of a channel *name* record in bytes. The name itself is shorter; the
/// remainder is unmodelled and preserved verbatim.
pub const NAME_RECORD_SIZE: usize = 16;

/// Maximum channel name length in characters (CHIRP `char name[7]`).
pub const NAME_LEN: usize = 7;

/// Fill byte for unused name characters and for an empty record.
pub const FILL: u8 = 0xFF;

/// Offset of the RX frequency, 4 bytes of little-endian packed BCD, 10 Hz units.
const RX_FREQ_OFFSET: usize = 0;
/// Offset of the TX frequency, same encoding. All-`0xFF` means "no TX".
const TX_FREQ_OFFSET: usize = 4;
/// Offset of the RX tone word, `u16` little-endian.
const RX_TONE_OFFSET: usize = 8;
/// Offset of the TX tone word, `u16` little-endian.
const TX_TONE_OFFSET: usize = 10;

/// First of the four packed flag bytes (12-15), exposed whole as `flags_raw`.
const FLAGS_OFFSET: usize = 12;
/// Byte 12: `unknown0:4, scode:4`. The PTT-ID signal code lives in the low nibble.
const SCODE_BYTE: usize = 12;
/// Byte 13: `unknown1:2, spmute:2, unknown2:2, optsig:2`.
const SIGNALING_BYTE: usize = 13;
/// Byte 14: `unknown3:3, scramble:1, unknown4:2, power:2`.
const POWER_BYTE: usize = 14;
/// Byte 15: `unknown5:1, wide:1, unknown6:2, bcl:1, add:1, pttid:2`.
const TAIL_BYTE: usize = 15;

/// Bit shift of `spmute` within [`SIGNALING_BYTE`] (2 bits).
const SPMUTE_SHIFT: u8 = 4;
/// Bit shift of `optsig` within [`SIGNALING_BYTE`] (2 bits).
const OPTSIG_SHIFT: u8 = 0;
/// Bit index of `scramble` within [`POWER_BYTE`].
const SCRAMBLE_BIT: u8 = 4;
/// Bit shift of `power` within [`POWER_BYTE`] (2 bits).
const POWER_SHIFT: u8 = 0;
/// Bit index of `wide` within [`TAIL_BYTE`].
const WIDE_BIT: u8 = 6;
/// Bit index of `bcl` (busy channel lockout) within [`TAIL_BYTE`].
const BCL_BIT: u8 = 3;
/// Bit index of `add` (include in scan) within [`TAIL_BYTE`].
const ADD_BIT: u8 = 2;
/// Bit shift of `pttid` within [`TAIL_BYTE`] (2 bits).
const PTTID_SHIFT: u8 = 0;

/// Highest tone word that is still a DCS code; anything larger is CTCSS.
const DCS_MAX: u16 = 600;
/// Highest tone word that is a *normal*-polarity DCS code.
const DCS_NORMAL_MAX: u16 = 105;
/// Tone word offset at which inverted-polarity DCS codes start.
const DCS_INVERTED_BASE: u16 = 106;

/// The DCS code table the tone word indexes: the standard 104 DCS codes plus
/// 645, sorted ascending. Confirmed by decoding the six distinct code words
/// present in the vendor capture (1, 3, 15, 16, 41, 88), which land on D023,
/// D026, D073, D074, D245 and D606.
const DCS_CODES: [u16; 105] = [
    23, 25, 26, 31, 32, 36, 43, 47, 51, 53, 54, 65, 71, 72, 73, 74, 114, 115, 116, 122, 125, 131,
    132, 134, 143, 145, 152, 155, 156, 162, 165, 172, 174, 205, 212, 223, 225, 226, 243, 244, 245,
    246, 251, 252, 255, 261, 263, 265, 266, 271, 274, 306, 311, 315, 325, 331, 332, 343, 346, 351,
    356, 364, 365, 371, 411, 412, 413, 423, 431, 432, 445, 446, 452, 454, 455, 462, 464, 465, 466,
    503, 506, 516, 523, 526, 532, 546, 565, 606, 612, 624, 627, 631, 632, 645, 654, 662, 664, 703,
    712, 723, 731, 732, 734, 743, 754,
];

/// A sub-audible squelch tone.
///
/// The radio stores one `u16` per direction covering all three cases, so this
/// is modelled as a tagged enum rather than the AnyTone's parallel
/// mode/index/code fields. Serialized as `{"kind":"none"}`,
/// `{"kind":"ctcss","hz_tenths":885}`, or
/// `{"kind":"dcs","code":23,"inverted":false}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Tone {
    /// No tone; the squelch is carrier-operated.
    None,
    /// CTCSS, in tenths of a hertz (885 = 88.5 Hz).
    Ctcss {
        /// Tone frequency in units of 0.1 Hz.
        hz_tenths: u16,
    },
    /// DCS / DTCS, by its three-digit octal code.
    Dcs {
        /// The DCS code as it is written on the radio, e.g. 23 for D023.
        code: u16,
        /// Inverted (reversed) polarity.
        inverted: bool,
    },
}

impl Tone {
    /// Decode the stored tone word.
    ///
    /// `0x0000` and `0xFFFF` both mean "off". Words above 600 are CTCSS in
    /// 0.1 Hz units; 1-105 are normal-polarity DCS; 106-600 are
    /// inverted-polarity DCS. The inverted branch has no example in the vendor
    /// captures and is CHIRP-derived; its arithmetic can address past the end
    /// of the 105-entry table, and such a word decodes to [`Tone::None`]
    /// rather than guessing. The raw word is preserved either way.
    pub fn decode(word: u16) -> Tone {
        match word {
            0 | 0xFFFF => Tone::None,
            w if w > DCS_MAX => Tone::Ctcss { hz_tenths: w },
            w if w <= DCS_NORMAL_MAX => match DCS_CODES.get((w - 1) as usize) {
                Some(&code) => Tone::Dcs {
                    code,
                    inverted: false,
                },
                None => Tone::None,
            },
            w => match DCS_CODES.get((w - DCS_INVERTED_BASE) as usize) {
                Some(&code) => Tone::Dcs {
                    code,
                    inverted: true,
                },
                None => Tone::None,
            },
        }
    }

    /// Encode back to the stored tone word, the exact inverse of
    /// [`decode`](Tone::decode). Errors rather than silently writing a wrong
    /// tone when the value cannot be represented.
    pub fn encode(self) -> std::result::Result<u16, String> {
        match self {
            Tone::None => Ok(0),
            Tone::Ctcss { hz_tenths } => {
                if hz_tenths <= DCS_MAX {
                    return Err(format!(
                        "CTCSS {}.{} Hz is below the {} Hz the radio can store",
                        hz_tenths / 10,
                        hz_tenths % 10,
                        DCS_MAX / 10
                    ));
                }
                Ok(hz_tenths)
            }
            Tone::Dcs { code, inverted } => {
                let i = DCS_CODES
                    .iter()
                    .position(|&c| c == code)
                    .ok_or_else(|| format!("D{code:03} is not a DCS code this radio knows"))?
                    as u16;
                Ok(if inverted {
                    i + DCS_INVERTED_BASE
                } else {
                    i + 1
                })
            }
        }
    }
}

/// Transmit power. The field is 2 bits wide but the KT-5000 has only two
/// levels: High (25 W) and Low (5 W).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Power {
    /// 25 W.
    High,
    /// 5 W.
    Low,
}

impl Power {
    /// Decode the 2-bit power field. Values 2 and 3 are unused by this radio
    /// and read back as High, the level index 0 means.
    pub(crate) fn from_bits(v: u8) -> Power {
        if v & 0b11 == 1 {
            Power::Low
        } else {
            Power::High
        }
    }
}

/// Channel bandwidth. Note the stored bit is *inverted* relative to the
/// AnyTone's: CHIRP's `MODES = ["FM", "NFM"]` makes `0` mean wide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Bandwidth {
    /// 25 kHz wide (stored as 0).
    Wide,
    /// 12.5 kHz narrow (stored as 1).
    Narrow,
}

impl Bandwidth {
    /// Decode the 1-bit `wide` field.
    pub(crate) fn from_bit(set: bool) -> Bandwidth {
        if set {
            Bandwidth::Narrow
        } else {
            Bandwidth::Wide
        }
    }
}

/// PTT-ID transmission mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PttId {
    /// Never send the PTT ID.
    #[serde(rename = "OFF")]
    Off,
    /// Send at the beginning of transmission.
    #[serde(rename = "BOT")]
    Bot,
    /// Send at the end of transmission.
    #[serde(rename = "EOT")]
    Eot,
    /// Send at both ends.
    #[serde(rename = "BOTH")]
    Both,
}

impl PttId {
    /// Decode the 2-bit `pttid` field.
    fn from_bits(v: u8) -> PttId {
        match v & 0b11 {
            1 => PttId::Bot,
            2 => PttId::Eot,
            3 => PttId::Both,
            _ => PttId::Off,
        }
    }
}

/// Optional signaling selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptSignaling {
    /// No optional signaling.
    #[serde(rename = "OFF")]
    Off,
    /// DTMF.
    #[serde(rename = "DTMF")]
    Dtmf,
    /// 2-tone.
    #[serde(rename = "2TONE")]
    TwoTone,
    /// 5-tone.
    #[serde(rename = "5TONE")]
    FiveTone,
}

impl OptSignaling {
    /// Decode the 2-bit `optsig` field.
    pub(crate) fn from_bits(v: u8) -> OptSignaling {
        match v & 0b11 {
            1 => OptSignaling::Dtmf,
            2 => OptSignaling::TwoTone,
            3 => OptSignaling::FiveTone,
            _ => OptSignaling::Off,
        }
    }
}

/// What has to match before the speaker unmutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpeakerMute {
    /// Sub-audible tone or code only.
    #[serde(rename = "Tone/DTCS")]
    ToneDcs,
    /// Both the sub-audible tone/code and the optional signaling.
    #[serde(rename = "Tone/DTCS and Optsig")]
    ToneDcsAndOptSig,
    /// Either the sub-audible tone/code or the optional signaling.
    #[serde(rename = "Tone/DTCS or Optsig")]
    ToneDcsOrOptSig,
}

impl SpeakerMute {
    /// Decode the 2-bit `spmute` field. Only three of the four values are
    /// defined; the fourth reads back as the default.
    pub(crate) fn from_bits(v: u8) -> SpeakerMute {
        match v & 0b11 {
            1 => SpeakerMute::ToneDcsAndOptSig,
            2 => SpeakerMute::ToneDcsOrOptSig,
            _ => SpeakerMute::ToneDcs,
        }
    }
}

/// One channel: the 16-byte record and its name record, decoded.
///
/// The flag fields (`scode` through `ptt_id`) are decoded for display but have
/// no setters yet. Their bit assignments come from CHIRP alone and have not
/// been confirmed against this radio one field at a time, so they round-trip
/// through `raw` untouched and `flags_raw` exposes bytes 12-15 verbatim.
#[derive(Debug, Clone, Serialize)]
pub struct QytChannel {
    /// Slot number, `0..200`.
    pub index: usize,
    /// Channel name, up to [`NAME_LEN`] characters.
    pub name: String,
    /// Receive frequency in Hz.
    pub rx_frequency_hz: u32,
    /// Transmit frequency in Hz, or `None` when the record stores no TX
    /// frequency (all-`0xFF`), which makes the channel receive-only.
    pub tx_frequency_hz: Option<u32>,
    /// Sub-audible tone the radio listens for.
    pub rx_tone: Tone,
    /// Sub-audible tone the radio transmits.
    pub tx_tone: Tone,
    /// PTT-ID signal code index (byte 12 low nibble). CHIRP treats a stored 15
    /// as 0; the raw value is reported here without that adjustment.
    pub scode: u8,
    /// Speaker mute condition.
    pub speaker_mute: SpeakerMute,
    /// Optional signaling selection.
    pub optional_signaling: OptSignaling,
    /// Voice inversion scrambler.
    pub scramble: bool,
    /// Transmit power level.
    pub power: Power,
    /// Channel bandwidth.
    pub bandwidth: Bandwidth,
    /// Busy channel lockout.
    pub busy_lockout: bool,
    /// Included in scan (CHIRP's `add`; 0 means skipped).
    pub scan_add: bool,
    /// PTT-ID transmission mode.
    pub ptt_id: PttId,
    /// Bytes 12-15 verbatim, so the app can show and diff bits this model does
    /// not name yet.
    pub flags_raw: [u8; 4],
    /// Authoritative channel record; skipped in JSON output.
    #[serde(skip)]
    raw: [u8; CHANNEL_SIZE],
    /// Authoritative name record. Bytes past [`NAME_LEN`] are unmodelled and
    /// never rewritten: two channels in the vendor capture carry `0x05` at byte
    /// 7 rather than the `0xFF` fill.
    #[serde(skip)]
    name_raw: [u8; NAME_RECORD_SIZE],
}

impl QytChannel {
    /// Parse the channel record and name record for slot `index`. Returns
    /// [`Error::Parse`] if either slice is the wrong length.
    pub fn parse(index: usize, rec: &[u8], name_rec: &[u8]) -> Result<QytChannel> {
        if rec.len() != CHANNEL_SIZE {
            return Err(Error::Parse(format!(
                "QYT channel record is {} bytes, expected {CHANNEL_SIZE}",
                rec.len()
            )));
        }
        if name_rec.len() != NAME_RECORD_SIZE {
            return Err(Error::Parse(format!(
                "QYT channel name record is {} bytes, expected {NAME_RECORD_SIZE}",
                name_rec.len()
            )));
        }
        let mut raw = [0u8; CHANNEL_SIZE];
        raw.copy_from_slice(rec);
        let mut name_raw = [0u8; NAME_RECORD_SIZE];
        name_raw.copy_from_slice(name_rec);
        Ok(Self::from_raw(index, raw, name_raw))
    }

    /// The raw 16-byte channel record for serialization (lossless).
    pub fn encode(&self) -> &[u8; CHANNEL_SIZE] {
        &self.raw
    }

    /// The raw 16-byte name record for serialization (lossless).
    pub fn encode_name(&self) -> &[u8; NAME_RECORD_SIZE] {
        &self.name_raw
    }

    /// Set the channel name, rewriting only the first [`NAME_LEN`] bytes of the
    /// name record and leaving the unmodelled tail alone.
    pub fn set_name(&mut self, name: &str) {
        write_ascii(&mut self.name_raw[..NAME_LEN], name, FILL);
        *self = Self::from_raw(self.index, self.raw, self.name_raw);
    }

    /// Set the receive frequency in Hz, rewriting only bytes 0-3. Stored in
    /// units of 10 Hz as little-endian packed BCD.
    pub fn set_rx_frequency(&mut self, hz: u32) {
        set_bcd_le(&mut self.raw[RX_FREQ_OFFSET..RX_FREQ_OFFSET + 4], hz / 10);
        *self = Self::from_raw(self.index, self.raw, self.name_raw);
    }

    /// Set the transmit frequency in Hz, rewriting only bytes 4-7. `None`
    /// writes the all-`0xFF` "no TX" marker, making the channel receive-only.
    pub fn set_tx_frequency(&mut self, hz: Option<u32>) {
        let field = &mut self.raw[TX_FREQ_OFFSET..TX_FREQ_OFFSET + 4];
        match hz {
            Some(hz) => set_bcd_le(field, hz / 10),
            None => field.fill(FILL),
        }
        *self = Self::from_raw(self.index, self.raw, self.name_raw);
    }

    /// Set the receive tone, rewriting only bytes 8-9. Errors if the tone
    /// cannot be represented, leaving the record untouched.
    pub fn set_rx_tone(&mut self, tone: Tone) -> std::result::Result<(), String> {
        let word = tone.encode()?;
        self.raw[RX_TONE_OFFSET..RX_TONE_OFFSET + 2].copy_from_slice(&word.to_le_bytes());
        *self = Self::from_raw(self.index, self.raw, self.name_raw);
        Ok(())
    }

    /// Set the transmit tone, rewriting only bytes 10-11. Errors if the tone
    /// cannot be represented, leaving the record untouched.
    pub fn set_tx_tone(&mut self, tone: Tone) -> std::result::Result<(), String> {
        let word = tone.encode()?;
        self.raw[TX_TONE_OFFSET..TX_TONE_OFFSET + 2].copy_from_slice(&word.to_le_bytes());
        *self = Self::from_raw(self.index, self.raw, self.name_raw);
        Ok(())
    }

    /// Include this channel in scanning, or skip it, rewriting only bit 2 of
    /// byte 15.
    ///
    /// This is the one flag bit with a setter. Its assignment was confirmed
    /// against a real KT-5000 (see `flag_bits_match_the_hardware_diff`), so a
    /// write here lands where the radio reads it; the rest of the flag bits are
    /// still CHIRP-derived and stay read-only.
    pub fn set_scan_add(&mut self, on: bool) {
        if on {
            self.raw[TAIL_BYTE] |= 1 << ADD_BIT;
        } else {
            self.raw[TAIL_BYTE] &= !(1 << ADD_BIT);
        }
        *self = Self::from_raw(self.index, self.raw, self.name_raw);
    }

    /// A blank but valid channel for `index`: 146.520 MHz simplex, no tones,
    /// wide, high power, in the scan list, named "NEW".
    ///
    /// The tail byte is `0x04` rather than `0x00` because bit 2 (`add`) means
    /// "include in scan", which is what every programmed channel in the vendor
    /// capture has set.
    pub fn default_record(index: usize) -> QytChannel {
        let mut raw = [0u8; CHANNEL_SIZE];
        set_bcd_le(&mut raw[RX_FREQ_OFFSET..RX_FREQ_OFFSET + 4], 146_520_000 / 10);
        set_bcd_le(&mut raw[TX_FREQ_OFFSET..TX_FREQ_OFFSET + 4], 146_520_000 / 10);
        raw[TAIL_BYTE] = 1 << ADD_BIT;
        let mut name_raw = [FILL; NAME_RECORD_SIZE];
        write_ascii(&mut name_raw[..NAME_LEN], "NEW", FILL);
        Self::from_raw(index, raw, name_raw)
    }

    /// Rebuild the decoded fields from the two raw records.
    fn from_raw(
        index: usize,
        raw: [u8; CHANNEL_SIZE],
        name_raw: [u8; NAME_RECORD_SIZE],
    ) -> QytChannel {
        let tx_field = &raw[TX_FREQ_OFFSET..TX_FREQ_OFFSET + 4];
        let tx_frequency_hz = if tx_field.iter().all(|&b| b == FILL) {
            None
        } else {
            Some(get_bcd_le(tx_field) * 10)
        };
        let tail = raw[TAIL_BYTE];
        QytChannel {
            index,
            name: read_ascii(&name_raw[..NAME_LEN], FILL),
            rx_frequency_hz: get_bcd_le(&raw[RX_FREQ_OFFSET..RX_FREQ_OFFSET + 4]) * 10,
            tx_frequency_hz,
            rx_tone: Tone::decode(u16::from_le_bytes([
                raw[RX_TONE_OFFSET],
                raw[RX_TONE_OFFSET + 1],
            ])),
            tx_tone: Tone::decode(u16::from_le_bytes([
                raw[TX_TONE_OFFSET],
                raw[TX_TONE_OFFSET + 1],
            ])),
            scode: raw[SCODE_BYTE] & 0x0F,
            speaker_mute: SpeakerMute::from_bits(raw[SIGNALING_BYTE] >> SPMUTE_SHIFT),
            optional_signaling: OptSignaling::from_bits(raw[SIGNALING_BYTE] >> OPTSIG_SHIFT),
            scramble: raw[POWER_BYTE] & (1 << SCRAMBLE_BIT) != 0,
            power: Power::from_bits(raw[POWER_BYTE] >> POWER_SHIFT),
            bandwidth: Bandwidth::from_bit(tail & (1 << WIDE_BIT) != 0),
            busy_lockout: tail & (1 << BCL_BIT) != 0,
            scan_add: tail & (1 << ADD_BIT) != 0,
            ptt_id: PttId::from_bits(tail >> PTTID_SHIFT),
            flags_raw: [
                raw[FLAGS_OFFSET],
                raw[FLAGS_OFFSET + 1],
                raw[FLAGS_OFFSET + 2],
                raw[FLAGS_OFFSET + 3],
            ],
            raw,
            name_raw,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Channel 64 of the vendor capture (`N7FYZ`): 147.32000 RX, 147.92000 TX,
    /// 100.0 Hz TX tone, tail `0x04`.
    const CH64: [u8; CHANNEL_SIZE] = [
        0x00, 0x20, 0x73, 0x14, 0x00, 0x20, 0x79, 0x14, 0x00, 0x00, 0xE8, 0x03, 0x00, 0x00, 0x00,
        0x04,
    ];
    /// Channel 64's name record.
    const CH64_NAME: [u8; NAME_RECORD_SIZE] = [
        b'N', b'7', b'F', b'Y', b'Z', 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF,
    ];

    /// Channel 1 of the vendor capture, whose name record carries an
    /// unmodelled `0x05` at byte 7.
    const CH1_NAME: [u8; NAME_RECORD_SIZE] = [
        b'F', b'R', b'S', b'-', b'0', b'2', 0xFF, 0x05, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        0xFF,
    ];

    /// Parse a record with an all-`0xFF` name record.
    fn parse(rec: [u8; CHANNEL_SIZE]) -> QytChannel {
        QytChannel::parse(0, &rec, &[FILL; NAME_RECORD_SIZE]).unwrap()
    }

    /// The byte offsets whose value differs between two channel records.
    fn changed(a: &QytChannel, b: &QytChannel) -> Vec<usize> {
        (0..CHANNEL_SIZE)
            .filter(|&i| a.raw[i] != b.raw[i])
            .collect()
    }

    /// Assert an edit changed at least one byte and none outside `field`.
    /// Subset rather than equality because two encodings of nearby values share
    /// most of their bytes: 147.32 and 146.52 MHz differ in exactly one.
    fn assert_confined(a: &QytChannel, b: &QytChannel, field: std::ops::Range<usize>) {
        let touched = changed(a, b);
        assert!(!touched.is_empty(), "the edit changed nothing");
        assert!(
            touched.iter().all(|i| field.contains(i)),
            "changed {touched:?}, expected only {field:?}"
        );
    }

    #[test]
    fn frequencies_decode_from_real_capture_bytes() {
        // Channel 0: 50 62 25 46 is 462.56250 MHz in little-endian packed BCD.
        let ch = parse([
            0x50, 0x62, 0x25, 0x46, 0x50, 0x62, 0x25, 0x46, 0, 0, 0, 0, 0, 0, 0, 0x24,
        ]);
        assert_eq!(ch.rx_frequency_hz, 462_562_500);
        assert_eq!(ch.tx_frequency_hz, Some(462_562_500));

        let ch = QytChannel::parse(64, &CH64, &CH64_NAME).unwrap();
        assert_eq!(ch.rx_frequency_hz, 147_320_000);
        assert_eq!(ch.tx_frequency_hz, Some(147_920_000));
        assert_eq!(ch.name, "N7FYZ");
    }

    #[test]
    fn an_all_ff_tx_field_means_receive_only() {
        let mut rec = CH64;
        rec[TX_FREQ_OFFSET..TX_FREQ_OFFSET + 4].fill(0xFF);
        assert_eq!(parse(rec).tx_frequency_hz, None);
    }

    #[test]
    fn tone_words_decode_to_ctcss_dcs_and_none() {
        // 0x0375 = 885 = 88.5 Hz, straight out of the capture.
        assert_eq!(Tone::decode(0x0375), Tone::Ctcss { hz_tenths: 885 });
        // Channel 64's TX tone.
        assert_eq!(Tone::decode(0x03E8), Tone::Ctcss { hz_tenths: 1000 });
        assert_eq!(Tone::decode(0), Tone::None);
        assert_eq!(Tone::decode(0xFFFF), Tone::None);

        // Every distinct DCS word in the capture.
        for (word, code) in [(1u16, 23u16), (3, 26), (15, 73), (16, 74), (41, 245), (88, 606)] {
            assert_eq!(
                Tone::decode(word),
                Tone::Dcs {
                    code,
                    inverted: false
                },
                "word {word}"
            );
        }

        // Inverted polarity is CHIRP-derived; no capture exercises it.
        assert_eq!(
            Tone::decode(106),
            Tone::Dcs {
                code: 23,
                inverted: true
            }
        );
    }

    #[test]
    fn tone_encoding_is_the_inverse_of_decoding() {
        for word in [0u16, 1, 3, 15, 16, 41, 88, 105, 106, 210, 670, 885, 1000, 2503] {
            let tone = Tone::decode(word);
            if tone == Tone::None && word != 0 {
                continue; // unmappable inverted index; decode documents this
            }
            assert_eq!(tone.encode().unwrap(), word, "word {word}");
        }
    }

    #[test]
    fn unrepresentable_tones_are_rejected_rather_than_written_wrong() {
        assert!(Tone::Dcs {
            code: 999,
            inverted: false
        }
        .encode()
        .is_err());
        // A CTCSS value low enough to collide with the DCS range.
        assert!(Tone::Ctcss { hz_tenths: 500 }.encode().is_err());

        let mut ch = parse(CH64);
        let before = *ch.encode();
        assert!(ch
            .set_tx_tone(Tone::Dcs {
                code: 999,
                inverted: false
            })
            .is_err());
        assert_eq!(ch.encode(), &before);
    }

    #[test]
    fn flag_bits_decode_per_the_chirp_layout() {
        let ch = parse(CH64);
        assert_eq!(ch.scode, 0);
        assert_eq!(ch.speaker_mute, SpeakerMute::ToneDcs);
        assert_eq!(ch.optional_signaling, OptSignaling::Off);
        assert!(!ch.scramble);
        assert_eq!(ch.power, Power::High);
        // Tail 0x04: wide (bit 6 clear), no BCL, in scan, PTT-ID off.
        assert_eq!(ch.bandwidth, Bandwidth::Wide);
        assert!(!ch.busy_lockout);
        assert!(ch.scan_add);
        assert_eq!(ch.ptt_id, PttId::Off);
        assert_eq!(ch.flags_raw, [0, 0, 0, 0x04]);

        let mut rec = CH64;
        rec[POWER_BYTE] = 0b0001_0001; // scramble set, power = Low
        rec[SIGNALING_BYTE] = 0b0010_0011; // spmute = 2, optsig = 3
        rec[SCODE_BYTE] = 0x0B;
        rec[TAIL_BYTE] = 0b0100_1110; // narrow, BCL, in scan, pttid = 2
        let ch = parse(rec);
        assert_eq!(ch.scode, 0x0B);
        assert_eq!(ch.speaker_mute, SpeakerMute::ToneDcsOrOptSig);
        assert_eq!(ch.optional_signaling, OptSignaling::FiveTone);
        assert!(ch.scramble);
        assert_eq!(ch.power, Power::Low);
        assert_eq!(ch.bandwidth, Bandwidth::Narrow);
        assert!(ch.busy_lockout);
        assert!(ch.scan_add);
        assert_eq!(ch.ptt_id, PttId::Eot);
    }

    #[test]
    fn encode_returns_an_untouched_record_verbatim() {
        let ch = QytChannel::parse(64, &CH64, &CH64_NAME).unwrap();
        assert_eq!(ch.encode(), &CH64);
        assert_eq!(ch.encode_name(), &CH64_NAME);
    }

    #[test]
    fn each_setter_touches_only_its_own_bytes() {
        let base = parse(CH64);

        let mut ch = base.clone();
        ch.set_rx_frequency(446_000_000);
        assert_confined(&base, &ch, 0..4);

        let mut ch = base.clone();
        ch.set_tx_frequency(Some(446_000_000));
        assert_confined(&base, &ch, 4..8);

        let mut ch = base.clone();
        ch.set_tx_frequency(None);
        assert_confined(&base, &ch, 4..8);

        let mut ch = base.clone();
        ch.set_rx_tone(Tone::Ctcss { hz_tenths: 885 }).unwrap();
        assert_confined(&base, &ch, 8..10);

        let mut ch = base.clone();
        ch.set_tx_tone(Tone::None).unwrap();
        assert_confined(&base, &ch, 10..12);

        // A name edit changes nothing at all in the channel record.
        let mut ch = base.clone();
        ch.set_name("TEST");
        assert!(changed(&base, &ch).is_empty());
    }

    #[test]
    fn unknown6_bits_survive_a_roundtrip() {
        // Channel 0 is the only record in the capture with bits 5-4 of byte 15
        // set. Nothing we write may synthesize a value for them.
        let mut ch = parse([
            0x50, 0x62, 0x25, 0x46, 0x50, 0x62, 0x25, 0x46, 0, 0, 0, 0, 0, 0, 0, 0x24,
        ]);
        ch.set_name("RENAMED");
        ch.set_rx_frequency(462_575_000);
        ch.set_tx_tone(Tone::Ctcss { hz_tenths: 1000 }).unwrap();
        assert_eq!(ch.encode()[TAIL_BYTE], 0x24);
        assert_eq!(ch.flags_raw, [0, 0, 0, 0x24]);
    }

    #[test]
    fn renaming_preserves_the_unmodelled_name_record_tail() {
        // Channels 1 and 3 of the capture carry 0x05 at byte 7 of the name
        // record. A 16-byte name rewrite would erase it.
        let mut ch = QytChannel::parse(1, &CH64, &CH1_NAME).unwrap();
        assert_eq!(ch.name, "FRS-02");
        ch.set_name("AB");
        assert_eq!(ch.encode_name()[..7], [b'A', b'B', 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(ch.encode_name()[7], 0x05);
        assert_eq!(&ch.encode_name()[8..], &[0xFF; 8]);
    }

    #[test]
    fn a_name_is_truncated_to_seven_characters() {
        let mut ch = parse(CH64);
        ch.set_name("TOOLONGNAME");
        assert_eq!(ch.name, "TOOLONG");
        assert_eq!(ch.encode_name()[7], 0xFF);
    }

    #[test]
    fn default_record_is_a_usable_simplex_channel() {
        let ch = QytChannel::default_record(7);
        assert_eq!(ch.index, 7);
        assert_eq!(ch.name, "NEW");
        assert_eq!(ch.rx_frequency_hz, 146_520_000);
        assert_eq!(ch.tx_frequency_hz, Some(146_520_000));
        assert_eq!(ch.rx_tone, Tone::None);
        assert!(ch.scan_add);
        assert_eq!(ch.bandwidth, Bandwidth::Wide);
        assert_eq!(ch.power, Power::High);
        // Never looks like an empty slot, which is keyed off byte 0 being 0xFF.
        assert_ne!(ch.encode()[0], FILL);
    }

    #[test]
    fn parse_rejects_wrongly_sized_records() {
        assert!(QytChannel::parse(0, &[0u8; 15], &[FILL; NAME_RECORD_SIZE]).is_err());
        assert!(QytChannel::parse(0, &CH64, &[FILL; 8]).is_err());
    }

    #[test]
    fn tones_serialize_with_a_kind_tag() {
        let v = serde_json::to_value(Tone::Ctcss { hz_tenths: 885 }).unwrap();
        assert_eq!(v["kind"], "ctcss");
        assert_eq!(v["hz_tenths"], 885);

        let v = serde_json::to_value(Tone::Dcs {
            code: 23,
            inverted: false,
        })
        .unwrap();
        assert_eq!(v["kind"], "dcs");
        assert_eq!(v["code"], 23);
        assert_eq!(v["inverted"], false);

        let v = serde_json::to_value(Tone::None).unwrap();
        assert_eq!(v["kind"], "none");
        assert_eq!(v.as_object().unwrap().len(), 1);
    }

    /// Bit assignments confirmed against a real KT-5000 on 2026-08-06.
    ///
    /// The owner changed one field at a time in the vendor software and we
    /// diffed a fresh read against the image we had just written. Before this,
    /// every programmed channel on the radio carried identical flag bytes, so
    /// these three assignments came from CHIRP and could not be checked. The
    /// observed byte transitions are quoted verbatim below; if a refactor ever
    /// breaks one of these, it is decoding a real radio's memory wrongly.
    #[test]
    fn flag_bits_match_the_hardware_diff() {
        // Names are irrelevant here; the flags live in the channel record.
        fn blank_name() -> [u8; NAME_RECORD_SIZE] {
            [0xFF; NAME_RECORD_SIZE]
        }

        // Power: byte 14 0x00 -> 0x01 on the channels set to Low.
        let mut raw = [0u8; CHANNEL_SIZE];
        raw[POWER_BYTE] = 0x00;
        assert_eq!(QytChannel::from_raw(0, raw, blank_name()).power, Power::High);
        raw[POWER_BYTE] = 0x01;
        assert_eq!(QytChannel::from_raw(0, raw, blank_name()).power, Power::Low);

        // Scan: byte 15 0x04 -> 0x00 on the channels set to skip, so bit 2 set
        // means "in scan" and clear means "skipped".
        let mut raw = [0u8; CHANNEL_SIZE];
        raw[TAIL_BYTE] = 0x04;
        assert!(QytChannel::from_raw(0, raw, blank_name()).scan_add);
        raw[TAIL_BYTE] = 0x00;
        assert!(!QytChannel::from_raw(0, raw, blank_name()).scan_add);

        // BCL: byte 15 0x04 -> 0x0C on the channels given busy channel lockout.
        let mut raw = [0u8; CHANNEL_SIZE];
        raw[TAIL_BYTE] = 0x04;
        assert!(!QytChannel::from_raw(0, raw, blank_name()).busy_lockout);
        raw[TAIL_BYTE] = 0x0C;
        let ch = QytChannel::from_raw(0, raw, blank_name());
        assert!(ch.busy_lockout);
        assert!(ch.scan_add, "bit 2 is untouched by the BCL change");
    }

    #[test]
    fn setting_scan_add_touches_only_its_own_bit() {
        // Channel 0 of the capture carries 0x24 in the tail: wide (bit 6), in
        // scan (bit 2), and bit 5 unmodelled.
        let before = parse([
            0x50, 0x62, 0x25, 0x46, 0x50, 0x62, 0x25, 0x46, 0, 0, 0, 0, 0, 0, 0, 0x24,
        ]);
        assert!(before.scan_add);

        let mut ch = before.clone();
        ch.set_scan_add(false);
        assert!(!ch.scan_add);
        assert_eq!(ch.encode()[TAIL_BYTE], 0x20, "only bit 2 cleared");
        assert_confined(&before, &ch, TAIL_BYTE..TAIL_BYTE + 1);
        // The neighbouring flags decode unchanged.
        assert_eq!(ch.bandwidth, before.bandwidth);
        assert_eq!(ch.busy_lockout, before.busy_lockout);
        assert_eq!(ch.ptt_id, before.ptt_id);

        ch.set_scan_add(true);
        assert!(ch.scan_add);
        assert_eq!(ch.encode(), before.encode(), "the round trip is lossless");
    }

    #[test]
    fn setting_scan_add_to_what_it_already_is_changes_nothing() {
        let ch = parse(CH64);
        let mut same = ch.clone();
        same.set_scan_add(true);
        assert_eq!(same.encode(), ch.encode());
        assert_eq!(same.encode_name(), ch.encode_name());
    }
}
