//! The 64-byte D878UVII channel record.
//!
//! Offsets are from qdmr `lib/anytone_codeplug.hh`
//! `AnytoneCodeplug::ChannelElement::Offset` (the D878UV element extends this
//! base and keeps the layout; `size()` = 0x40). See `mod.rs` for the full
//! offset/source table. The struct keeps the raw 64 bytes as the source of
//! truth so [`encode`](Channel::encode) is lossless; the decoded fields are a
//! cache refreshed on every edit.

use serde::Serialize;

use crate::bytes::{get_bcd8_be, read_ascii, set_bcd8_be, write_ascii};
use crate::error::{Error, Result};

/// Size of a channel record in bytes (qdmr `ChannelElement::size()` = 0x40).
pub const CHANNEL_SIZE: usize = 64;

/// Offset of the 16-byte channel name (qdmr `Offset::name()`).
const NAME_OFFSET: usize = 0x23;
/// Maximum channel name length (qdmr `Limit::nameLength()`).
const NAME_LEN: usize = 16;
/// Offset of the packed mode/power/bandwidth/repeater flags byte
/// (qdmr `Offset::channelMode()` etc., all within byte 0x08).
const FLAGS_OFFSET: usize = 0x08;
/// Offset of the digital-contact index, u32 little-endian
/// (qdmr `Offset::contactIndex()`).
const CONTACT_INDEX_OFFSET: usize = 0x14;
/// Offset of the radio-ID index, u8 (qdmr `Offset::radioIdIndex()`).
const RADIO_ID_INDEX_OFFSET: usize = 0x18;
/// Offset of the RX group-list index, u8 (qdmr `Offset::groupListIndex()`).
const GROUP_LIST_INDEX_OFFSET: usize = 0x1c;
/// Offset of the DMR color code, u8 (qdmr `Offset::colorCode()`).
const COLOR_CODE_OFFSET: usize = 0x20;
/// Offset of the byte holding the time-slot bit (qdmr `Offset::timeSlot()`).
const TIME_SLOT_OFFSET: usize = 0x21;

/// Offset of the second flags byte: RX/TX signaling mode and per-channel option
/// bits (qdmr `ChannelElement` offsets `{0x09, ...}`).
const FLAGS2_OFFSET: usize = 0x09;
/// Bit within [`FLAGS2_OFFSET`] shifting the 2-bit RX signaling mode
/// (qdmr `rxSignalingMode` `{0x09,0}`).
const RX_SIGNALING_SHIFT: u8 = 0;
/// Bit within [`FLAGS2_OFFSET`] shifting the 2-bit TX signaling mode
/// (qdmr `txSignalingMode` `{0x09,2}`).
const TX_SIGNALING_SHIFT: u8 = 2;
/// Bit within [`FLAGS2_OFFSET`] for RX-only / "PTT prohibit"
/// (qdmr `rxOnly` `{0x09,5}`).
const RX_ONLY_BIT: u8 = 5;
/// Bit within [`FLAGS2_OFFSET`] for call confirmation (qdmr `callConfirm` `{0x09,6}`).
const CALL_CONFIRM_BIT: u8 = 6;
/// Bit within [`FLAGS2_OFFSET`] for talk-around/simplex (qdmr `talkaround` `{0x09,7}`).
const TALK_AROUND_BIT: u8 = 7;

/// Offset of the TX CTCSS tone index, u8 (qdmr `txCTCSS` at 0x0a).
const TX_CTCSS_OFFSET: usize = 0x0a;
/// Offset of the RX CTCSS tone index, u8 (qdmr `rxCTCSS` at 0x0b).
const RX_CTCSS_OFFSET: usize = 0x0b;
/// Offset of the TX DCS code word, u16 little-endian (qdmr `txDCS` at 0x0c).
const TX_DCS_OFFSET: usize = 0x0c;
/// Offset of the RX DCS code word, u16 little-endian (qdmr `rxDCS` at 0x0e).
const RX_DCS_OFFSET: usize = 0x0e;
/// Offset of the 2-tone decode index, u16 little-endian
/// (qdmr `twoToneDecodeIndex` at 0x12).
const TWO_TONE_DECODE_OFFSET: usize = 0x12;

/// Offset of the byte holding the squelch-mode bit (qdmr `squelchMode` `{0x19,4}`).
const SQUELCH_OFFSET: usize = 0x19;
/// Bit within [`SQUELCH_OFFSET`] selecting silent (tone) squelch.
const SQUELCH_BIT: u8 = 4;

/// Offset of the byte holding admit criterion and optional-signaling fields
/// (qdmr `admit` `{0x1a,0}`, `optionalSignaling` `{0x1a,4}`).
const ADMIT_OFFSET: usize = 0x1a;
/// Bit within [`ADMIT_OFFSET`] shifting the 2-bit admit criterion / "TX permit".
const ADMIT_SHIFT: u8 = 0;
/// Bit within [`ADMIT_OFFSET`] shifting the 2-bit optional-signaling selection.
const OPT_SIGNALING_SHIFT: u8 = 4;

/// Offset of the scan-list index, u8 (qdmr `scanListIndex` at 0x1b).
const SCAN_LIST_INDEX_OFFSET: usize = 0x1b;
/// Offset of the 2-tone ID index, u8 (qdmr `twoToneIDIndex` at 0x1d).
const TWO_TONE_ID_OFFSET: usize = 0x1d;
/// Offset of the 5-tone ID index, u8 (qdmr `fiveToneIDIndex` at 0x1e).
const FIVE_TONE_ID_OFFSET: usize = 0x1e;
/// Offset of the DTMF ID index, u8 (qdmr `dtmfIDIndex` at 0x1f).
const DTMF_ID_OFFSET: usize = 0x1f;

/// Bit within [`TIME_SLOT_OFFSET`] for simplex TDMA (qdmr `simplexTDMA` `{0x21,2}`).
const SIMPLEX_TDMA_BIT: u8 = 2;
/// Bit within [`TIME_SLOT_OFFSET`] for RX APRS (qdmr `rxAPRS` `{0x21,5}`).
const RX_APRS_BIT: u8 = 5;
/// Bit within [`TIME_SLOT_OFFSET`] for lone-worker / "work alone"
/// (qdmr `loneWorker` `{0x21,7}`).
const LONE_WORKER_BIT: u8 = 7;

/// Channel operating mode (qdmr `ChannelElement::Mode`, 2-bit field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ChannelMode {
    /// Analog FM.
    Analog,
    /// Digital DMR.
    Digital,
    /// Mixed: analog TX, digital-capable RX.
    MixedAnalog,
    /// Mixed: digital TX, analog-capable RX.
    MixedDigital,
}

impl ChannelMode {
    /// Decode the 2-bit mode field.
    fn from_bits(v: u8) -> ChannelMode {
        match v & 0b11 {
            0 => ChannelMode::Analog,
            1 => ChannelMode::Digital,
            2 => ChannelMode::MixedAnalog,
            _ => ChannelMode::MixedDigital,
        }
    }

    /// Encode as the 2-bit mode field.
    fn to_bits(self) -> u8 {
        match self {
            ChannelMode::Analog => 0,
            ChannelMode::Digital => 1,
            ChannelMode::MixedAnalog => 2,
            ChannelMode::MixedDigital => 3,
        }
    }
}

/// Transmit power level (qdmr `ChannelElement::Power`, 2-bit field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Power {
    /// Low power.
    Low,
    /// Medium power.
    Mid,
    /// High power.
    High,
    /// Turbo/maximum power.
    Turbo,
}

impl Power {
    /// Decode the 2-bit power field.
    fn from_bits(v: u8) -> Power {
        match v & 0b11 {
            0 => Power::Low,
            1 => Power::Mid,
            2 => Power::High,
            _ => Power::Turbo,
        }
    }

    /// Encode as the 2-bit power field.
    fn to_bits(self) -> u8 {
        match self {
            Power::Low => 0,
            Power::Mid => 1,
            Power::High => 2,
            Power::Turbo => 3,
        }
    }
}

/// Channel bandwidth (qdmr `ChannelElement`, bit 4 of byte 0x08).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Bandwidth {
    /// 12.5 kHz narrow.
    Narrow,
    /// 25 kHz wide.
    Wide,
}

impl Bandwidth {
    /// Decode the 2-bit bandwidth field (0 = narrow, otherwise wide).
    fn from_bits(v: u8) -> Bandwidth {
        if v & 0b11 == 0 {
            Bandwidth::Narrow
        } else {
            Bandwidth::Wide
        }
    }

    /// Encode as the 2-bit bandwidth field (narrow = 0, wide = 1).
    fn to_bits(self) -> u8 {
        match self {
            Bandwidth::Narrow => 0,
            Bandwidth::Wide => 1,
        }
    }
}

/// Repeater/offset mode (qdmr `ChannelElement::RepeaterMode`, bits 6-7 of
/// byte 0x08).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RepeaterMode {
    /// Simplex: TX = RX, the offset is ignored.
    Simplex,
    /// Repeater with a positive TX offset.
    Positive,
    /// Repeater with a negative TX offset.
    Negative,
}

impl RepeaterMode {
    /// Decode the 2-bit repeater-mode field.
    fn from_bits(v: u8) -> RepeaterMode {
        match v & 0b11 {
            1 => RepeaterMode::Positive,
            2 => RepeaterMode::Negative,
            _ => RepeaterMode::Simplex,
        }
    }

    /// Encode as the 2-bit repeater-mode field.
    fn to_bits(self) -> u8 {
        match self {
            RepeaterMode::Simplex => 0,
            RepeaterMode::Positive => 1,
            RepeaterMode::Negative => 2,
        }
    }
}

/// Sub-audible signaling type for a channel's RX or TX tone
/// (qdmr `ChannelElement::SignalingMode`, a 2-bit field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SignalingMode {
    /// No sub-audible signaling (carrier squelch).
    None,
    /// CTCSS continuous tone; the tone index is stored separately.
    Ctcss,
    /// DCS digital code; the code word is stored separately.
    Dcs,
}

impl SignalingMode {
    /// Decode the 2-bit signaling-mode field.
    fn from_bits(v: u8) -> SignalingMode {
        match v & 0b11 {
            1 => SignalingMode::Ctcss,
            2 => SignalingMode::Dcs,
            _ => SignalingMode::None,
        }
    }

    /// Encode as the 2-bit signaling-mode field.
    fn to_bits(self) -> u8 {
        match self {
            SignalingMode::None => 0,
            SignalingMode::Ctcss => 1,
            SignalingMode::Dcs => 2,
        }
    }
}

/// Transmit admit criterion, shown as "TX Permit" in the vendor CPS
/// (qdmr `ChannelElement::Admit`, a 2-bit field). The names follow qdmr; the
/// radio interprets the busy/color-code variants per the channel mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Admit {
    /// Always permit transmit.
    Always,
    /// Permit only when the channel is free.
    ChannelFree,
    /// Permit only on a different color code (digital).
    DifferentColorCode,
    /// Permit only on the same color code (digital).
    SameColorCode,
}

impl Admit {
    /// Decode the 2-bit admit field.
    fn from_bits(v: u8) -> Admit {
        match v & 0b11 {
            1 => Admit::ChannelFree,
            2 => Admit::DifferentColorCode,
            3 => Admit::SameColorCode,
            _ => Admit::Always,
        }
    }

    /// Encode as the 2-bit admit field.
    fn to_bits(self) -> u8 {
        match self {
            Admit::Always => 0,
            Admit::ChannelFree => 1,
            Admit::DifferentColorCode => 2,
            Admit::SameColorCode => 3,
        }
    }
}

/// FM squelch mode (qdmr `AnytoneFMChannelExtension::SquelchMode`, a 1-bit
/// field at `{0x19,4}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SquelchMode {
    /// Carrier squelch (open on any carrier).
    Carrier,
    /// Silent squelch, opened by the received sub-audible tone/code.
    Tone,
}

impl SquelchMode {
    /// Decode the 1-bit squelch-mode field.
    fn from_bits(v: u8) -> SquelchMode {
        if v & 0b1 == 0 {
            SquelchMode::Carrier
        } else {
            SquelchMode::Tone
        }
    }

    /// Encode as the 1-bit squelch-mode field.
    fn to_bits(self) -> u8 {
        match self {
            SquelchMode::Carrier => 0,
            SquelchMode::Tone => 1,
        }
    }
}

/// Optional-signaling selection (qdmr `ChannelElement::OptSignaling`, a 2-bit
/// field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum OptSignaling {
    /// No optional signaling.
    Off,
    /// DTMF signaling (uses the DTMF ID index).
    Dtmf,
    /// 2-tone signaling (uses the 2-tone ID index).
    TwoTone,
    /// 5-tone signaling (uses the 5-tone ID index).
    FiveTone,
}

impl OptSignaling {
    /// Decode the 2-bit optional-signaling field.
    fn from_bits(v: u8) -> OptSignaling {
        match v & 0b11 {
            1 => OptSignaling::Dtmf,
            2 => OptSignaling::TwoTone,
            3 => OptSignaling::FiveTone,
            _ => OptSignaling::Off,
        }
    }

    /// Encode as the 2-bit optional-signaling field.
    fn to_bits(self) -> u8 {
        match self {
            OptSignaling::Off => 0,
            OptSignaling::Dtmf => 1,
            OptSignaling::TwoTone => 2,
            OptSignaling::FiveTone => 3,
        }
    }
}

/// A single channel. Decoded fields are for inspection/JSON; `raw` is the
/// authoritative image that [`encode`](Channel::encode) returns unchanged
/// unless a setter has edited it.
#[derive(Debug, Clone, Serialize)]
pub struct Channel {
    /// Channel index (position in the bitmap / bank layout).
    pub index: usize,
    /// Channel name.
    pub name: String,
    /// Receive frequency in Hz.
    pub rx_frequency_hz: u32,
    /// Transmit-offset magnitude in Hz (as stored).
    pub tx_offset_hz: u32,
    /// Effective transmit frequency in Hz, derived from RX + repeater mode.
    pub tx_frequency_hz: u32,
    /// Repeater/offset mode.
    pub repeater_mode: RepeaterMode,
    /// Operating mode (analog/digital/mixed).
    pub mode: ChannelMode,
    /// Transmit power level.
    pub power: Power,
    /// Channel bandwidth.
    pub bandwidth: Bandwidth,
    /// DMR color code (0-15).
    pub color_code: u8,
    /// DMR time slot (1 or 2).
    pub time_slot: u8,
    /// Index into the digital contact table.
    pub contact_index: u32,
    /// Index into the radio-ID table.
    pub radio_id_index: u8,
    /// Index into the RX group-list table (`0xff` = none).
    pub group_list_index: u8,
    /// RX-only / "PTT prohibit": the channel cannot transmit.
    pub rx_only: bool,
    /// Talk-around / simplex: TX on the RX frequency, ignoring the offset.
    pub talk_around: bool,
    /// Call-confirmation before transmit (digital).
    pub call_confirm: bool,
    /// Lone-worker / "work alone" reminder.
    pub work_alone: bool,
    /// Simplex TDMA (pseudo-trunk on a single frequency).
    pub simplex_tdma: bool,
    /// Receive APRS frames on this channel.
    pub rx_aprs: bool,
    /// Transmit admit criterion ("TX Permit").
    pub admit: Admit,
    /// FM squelch mode (carrier vs. tone).
    pub squelch_mode: SquelchMode,
    /// Optional-signaling selection.
    pub optional_signaling: OptSignaling,
    /// RX sub-audible signaling type.
    pub rx_signaling_mode: SignalingMode,
    /// TX sub-audible signaling type.
    pub tx_signaling_mode: SignalingMode,
    /// RX CTCSS tone index (qdmr tone table; meaningful when RX mode is CTCSS).
    pub rx_ctcss: u8,
    /// TX CTCSS tone index (qdmr tone table; meaningful when TX mode is CTCSS).
    pub tx_ctcss: u8,
    /// RX DCS code word as stored (qdmr octal-code word; meaningful when RX mode
    /// is DCS).
    pub rx_dcs: u16,
    /// TX DCS code word as stored (qdmr octal-code word; meaningful when TX mode
    /// is DCS).
    pub tx_dcs: u16,
    /// Index into the scan-list table (`0xff` = none).
    pub scan_list_index: u8,
    /// Index into the DTMF ID table.
    pub dtmf_id_index: u8,
    /// Index into the 2-tone ID table.
    pub two_tone_id_index: u8,
    /// Index into the 5-tone ID table.
    pub five_tone_id_index: u8,
    /// Index into the 2-tone decode table.
    pub two_tone_decode_index: u16,
    /// Authoritative raw record; skipped in JSON output.
    #[serde(skip)]
    raw: [u8; CHANNEL_SIZE],
}

impl Channel {
    /// Parse a 64-byte record at channel `index`. Returns [`Error::Parse`] if
    /// the slice is not exactly [`CHANNEL_SIZE`] bytes.
    pub fn parse(index: usize, rec: &[u8]) -> Result<Channel> {
        if rec.len() != CHANNEL_SIZE {
            return Err(Error::Parse(format!(
                "channel record is {} bytes, expected {CHANNEL_SIZE}",
                rec.len()
            )));
        }
        let mut raw = [0u8; CHANNEL_SIZE];
        raw.copy_from_slice(rec);
        Ok(Self::from_raw(index, raw))
    }

    /// The raw 64-byte record for serialization (lossless).
    pub fn encode(&self) -> &[u8; CHANNEL_SIZE] {
        &self.raw
    }

    /// Set the channel name, rewriting only the 16-byte name field in `raw`.
    pub fn set_name(&mut self, name: &str) {
        write_ascii(
            &mut self.raw[NAME_OFFSET..NAME_OFFSET + NAME_LEN],
            name,
            0x00,
        );
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the receive frequency in Hz, rewriting only the 4 BCD bytes at
    /// offset 0. The value is stored in units of 10 Hz.
    pub fn set_rx_frequency(&mut self, hz: u32) {
        set_bcd8_be(&mut self.raw[0..4], hz / 10);
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the effective transmit frequency in Hz. The radio stores TX as a
    /// repeater mode (bits 6-7 of the flags byte) plus a BCD offset magnitude
    /// at 0x04, both relative to RX; this rewrites exactly those bytes. Equal
    /// to RX means simplex; higher/lower selects the positive/negative offset.
    pub fn set_tx_frequency(&mut self, hz: u32) {
        let rx = get_bcd8_be(&self.raw[0..4]) * 10;
        let (mode, offset_hz) = match hz.cmp(&rx) {
            std::cmp::Ordering::Equal => (RepeaterMode::Simplex, 0),
            std::cmp::Ordering::Greater => (RepeaterMode::Positive, hz - rx),
            std::cmp::Ordering::Less => (RepeaterMode::Negative, rx - hz),
        };
        set_bcd8_be(&mut self.raw[4..8], offset_hz / 10);
        self.set_flag_field(6, mode.to_bits());
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the operating mode (analog/digital/mixed), bits 0-1 of the flags byte.
    pub fn set_mode(&mut self, mode: ChannelMode) {
        self.set_flag_field(0, mode.to_bits());
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the transmit power level, bits 2-3 of the flags byte.
    pub fn set_power(&mut self, power: Power) {
        self.set_flag_field(2, power.to_bits());
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the channel bandwidth, bits 4-5 of the flags byte.
    pub fn set_bandwidth(&mut self, bandwidth: Bandwidth) {
        self.set_flag_field(4, bandwidth.to_bits());
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the DMR color code (0-15), clamped, at 0x20.
    pub fn set_color_code(&mut self, cc: u8) {
        self.raw[COLOR_CODE_OFFSET] = cc.min(15);
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the DMR time slot (1 or 2); only bit 0 of byte 0x21 is touched.
    pub fn set_time_slot(&mut self, slot: u8) {
        let bit = if slot >= 2 { 1 } else { 0 };
        self.raw[TIME_SLOT_OFFSET] = (self.raw[TIME_SLOT_OFFSET] & !0b1) | bit;
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the digital-contact index (into the contact table), u32-le at 0x14.
    pub fn set_contact_index(&mut self, index: u32) {
        self.raw[CONTACT_INDEX_OFFSET..CONTACT_INDEX_OFFSET + 4]
            .copy_from_slice(&index.to_le_bytes());
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the radio-ID index (into the radio-ID table), u8 at 0x18.
    pub fn set_radio_id_index(&mut self, index: u8) {
        self.raw[RADIO_ID_INDEX_OFFSET] = index;
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the RX group-list index (into the group-list table), u8 at 0x1c.
    /// `0xff` means "none" in the vendor CPS.
    pub fn set_group_list_index(&mut self, index: u8) {
        self.raw[GROUP_LIST_INDEX_OFFSET] = index;
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set RX-only / "PTT prohibit"; touches only bit 5 of byte 0x09.
    pub fn set_rx_only(&mut self, on: bool) {
        self.set_bit(FLAGS2_OFFSET, RX_ONLY_BIT, on);
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set talk-around/simplex; touches only bit 7 of byte 0x09.
    pub fn set_talk_around(&mut self, on: bool) {
        self.set_bit(FLAGS2_OFFSET, TALK_AROUND_BIT, on);
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set call confirmation; touches only bit 6 of byte 0x09.
    pub fn set_call_confirm(&mut self, on: bool) {
        self.set_bit(FLAGS2_OFFSET, CALL_CONFIRM_BIT, on);
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set lone-worker / "work alone"; touches only bit 7 of byte 0x21.
    pub fn set_work_alone(&mut self, on: bool) {
        self.set_bit(TIME_SLOT_OFFSET, LONE_WORKER_BIT, on);
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set simplex TDMA; touches only bit 2 of byte 0x21.
    pub fn set_simplex_tdma(&mut self, on: bool) {
        self.set_bit(TIME_SLOT_OFFSET, SIMPLEX_TDMA_BIT, on);
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set RX APRS; touches only bit 5 of byte 0x21.
    pub fn set_rx_aprs(&mut self, on: bool) {
        self.set_bit(TIME_SLOT_OFFSET, RX_APRS_BIT, on);
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the admit criterion ("TX permit"); touches only bits 0-1 of byte 0x1a.
    pub fn set_admit(&mut self, admit: Admit) {
        self.set_bit_field(ADMIT_OFFSET, ADMIT_SHIFT, admit.to_bits());
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the squelch mode; touches only bit 4 of byte 0x19.
    pub fn set_squelch_mode(&mut self, mode: SquelchMode) {
        self.set_bit(SQUELCH_OFFSET, SQUELCH_BIT, mode.to_bits() != 0);
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the optional-signaling selection; touches only bits 4-5 of byte 0x1a.
    pub fn set_optional_signaling(&mut self, sig: OptSignaling) {
        self.set_bit_field(ADMIT_OFFSET, OPT_SIGNALING_SHIFT, sig.to_bits());
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the RX signaling mode; touches only bits 0-1 of byte 0x09.
    pub fn set_rx_signaling_mode(&mut self, mode: SignalingMode) {
        self.set_bit_field(FLAGS2_OFFSET, RX_SIGNALING_SHIFT, mode.to_bits());
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the TX signaling mode; touches only bits 2-3 of byte 0x09.
    pub fn set_tx_signaling_mode(&mut self, mode: SignalingMode) {
        self.set_bit_field(FLAGS2_OFFSET, TX_SIGNALING_SHIFT, mode.to_bits());
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the RX CTCSS tone index, u8 at 0x0b.
    pub fn set_rx_ctcss(&mut self, index: u8) {
        self.raw[RX_CTCSS_OFFSET] = index;
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the TX CTCSS tone index, u8 at 0x0a.
    pub fn set_tx_ctcss(&mut self, index: u8) {
        self.raw[TX_CTCSS_OFFSET] = index;
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the RX DCS code word, u16-le at 0x0e.
    pub fn set_rx_dcs(&mut self, code: u16) {
        self.raw[RX_DCS_OFFSET..RX_DCS_OFFSET + 2].copy_from_slice(&code.to_le_bytes());
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the TX DCS code word, u16-le at 0x0c.
    pub fn set_tx_dcs(&mut self, code: u16) {
        self.raw[TX_DCS_OFFSET..TX_DCS_OFFSET + 2].copy_from_slice(&code.to_le_bytes());
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the scan-list index, u8 at 0x1b. `0xff` means "none".
    pub fn set_scan_list_index(&mut self, index: u8) {
        self.raw[SCAN_LIST_INDEX_OFFSET] = index;
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the DTMF ID index, u8 at 0x1f.
    pub fn set_dtmf_id_index(&mut self, index: u8) {
        self.raw[DTMF_ID_OFFSET] = index;
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the 2-tone ID index, u8 at 0x1d.
    pub fn set_two_tone_id_index(&mut self, index: u8) {
        self.raw[TWO_TONE_ID_OFFSET] = index;
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the 5-tone ID index, u8 at 0x1e.
    pub fn set_five_tone_id_index(&mut self, index: u8) {
        self.raw[FIVE_TONE_ID_OFFSET] = index;
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the 2-tone decode index, u16-le at 0x12.
    pub fn set_two_tone_decode_index(&mut self, index: u16) {
        self.raw[TWO_TONE_DECODE_OFFSET..TWO_TONE_DECODE_OFFSET + 2]
            .copy_from_slice(&index.to_le_bytes());
        *self = Self::from_raw(self.index, self.raw);
    }

    /// A blank but valid channel record for `index`: analog, 146.520 MHz
    /// simplex, narrow, low power, TS1/CC0, named "NEW". Callers typically edit
    /// it immediately after adding.
    pub fn default_record(index: usize) -> Channel {
        let mut raw = [0u8; CHANNEL_SIZE];
        set_bcd8_be(&mut raw[0..4], 146_520_000 / 10);
        write_ascii(&mut raw[NAME_OFFSET..NAME_OFFSET + NAME_LEN], "NEW", 0x00);
        Self::from_raw(index, raw)
    }

    /// Overwrite a 2-bit field within the flags byte at `shift` (0/2/4/6),
    /// preserving the other three fields.
    fn set_flag_field(&mut self, shift: u8, value: u8) {
        let mask = 0b11u8 << shift;
        self.raw[FLAGS_OFFSET] = (self.raw[FLAGS_OFFSET] & !mask) | ((value & 0b11) << shift);
    }

    /// Set or clear a single `bit` within the byte at `offset`, leaving the
    /// other seven bits untouched.
    fn set_bit(&mut self, offset: usize, bit: u8, on: bool) {
        let mask = 1u8 << bit;
        if on {
            self.raw[offset] |= mask;
        } else {
            self.raw[offset] &= !mask;
        }
    }

    /// Overwrite a 2-bit field at `shift` within the byte at `offset`,
    /// preserving the surrounding bits.
    fn set_bit_field(&mut self, offset: usize, shift: u8, value: u8) {
        let mask = 0b11u8 << shift;
        self.raw[offset] = (self.raw[offset] & !mask) | ((value & 0b11) << shift);
    }

    /// Rebuild the decoded fields from `raw`.
    fn from_raw(index: usize, raw: [u8; CHANNEL_SIZE]) -> Channel {
        let rx_frequency_hz = get_bcd8_be(&raw[0..4]) * 10;
        let tx_offset_hz = get_bcd8_be(&raw[4..8]) * 10;
        let flags = raw[0x08];
        let repeater_mode = RepeaterMode::from_bits(flags >> 6);
        let tx_frequency_hz = match repeater_mode {
            RepeaterMode::Simplex => rx_frequency_hz,
            RepeaterMode::Positive => rx_frequency_hz.saturating_add(tx_offset_hz),
            RepeaterMode::Negative => rx_frequency_hz.saturating_sub(tx_offset_hz),
        };
        Channel {
            index,
            name: read_ascii(&raw[NAME_OFFSET..NAME_OFFSET + NAME_LEN], 0x00),
            rx_frequency_hz,
            tx_offset_hz,
            tx_frequency_hz,
            repeater_mode,
            mode: ChannelMode::from_bits(flags),
            power: Power::from_bits(flags >> 2),
            bandwidth: Bandwidth::from_bits(flags >> 4),
            color_code: raw[0x20],
            time_slot: 1 + (raw[0x21] & 0b1),
            contact_index: u32::from_le_bytes([raw[0x14], raw[0x15], raw[0x16], raw[0x17]]),
            radio_id_index: raw[RADIO_ID_INDEX_OFFSET],
            group_list_index: raw[GROUP_LIST_INDEX_OFFSET],
            rx_only: raw[FLAGS2_OFFSET] & (1 << RX_ONLY_BIT) != 0,
            talk_around: raw[FLAGS2_OFFSET] & (1 << TALK_AROUND_BIT) != 0,
            call_confirm: raw[FLAGS2_OFFSET] & (1 << CALL_CONFIRM_BIT) != 0,
            work_alone: raw[TIME_SLOT_OFFSET] & (1 << LONE_WORKER_BIT) != 0,
            simplex_tdma: raw[TIME_SLOT_OFFSET] & (1 << SIMPLEX_TDMA_BIT) != 0,
            rx_aprs: raw[TIME_SLOT_OFFSET] & (1 << RX_APRS_BIT) != 0,
            admit: Admit::from_bits(raw[ADMIT_OFFSET] >> ADMIT_SHIFT),
            squelch_mode: SquelchMode::from_bits(raw[SQUELCH_OFFSET] >> SQUELCH_BIT),
            optional_signaling: OptSignaling::from_bits(raw[ADMIT_OFFSET] >> OPT_SIGNALING_SHIFT),
            rx_signaling_mode: SignalingMode::from_bits(raw[FLAGS2_OFFSET] >> RX_SIGNALING_SHIFT),
            tx_signaling_mode: SignalingMode::from_bits(raw[FLAGS2_OFFSET] >> TX_SIGNALING_SHIFT),
            rx_ctcss: raw[RX_CTCSS_OFFSET],
            tx_ctcss: raw[TX_CTCSS_OFFSET],
            rx_dcs: u16::from_le_bytes([raw[RX_DCS_OFFSET], raw[RX_DCS_OFFSET + 1]]),
            tx_dcs: u16::from_le_bytes([raw[TX_DCS_OFFSET], raw[TX_DCS_OFFSET + 1]]),
            scan_list_index: raw[SCAN_LIST_INDEX_OFFSET],
            dtmf_id_index: raw[DTMF_ID_OFFSET],
            two_tone_id_index: raw[TWO_TONE_ID_OFFSET],
            five_tone_id_index: raw[FIVE_TONE_ID_OFFSET],
            two_tone_decode_index: u16::from_le_bytes([
                raw[TWO_TONE_DECODE_OFFSET],
                raw[TWO_TONE_DECODE_OFFSET + 1],
            ]),
            raw,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The set of byte offsets whose value differs between two records.
    fn changed_bytes(a: &Channel, b: &Channel) -> Vec<usize> {
        (0..CHANNEL_SIZE)
            .filter(|&i| a.encode()[i] != b.encode()[i])
            .collect()
    }

    /// A default record edited by `edit` must (a) change only `expected`
    /// offsets and (b) re-decode identically after a parse round-trip.
    fn assert_touches(expected: &[usize], edit: impl Fn(&mut Channel)) -> Channel {
        let base = Channel::default_record(0);
        let mut ch = base.clone();
        edit(&mut ch);
        assert_eq!(
            changed_bytes(&base, &ch),
            expected.to_vec(),
            "setter touched unexpected bytes"
        );
        // encode() is lossless: parsing the raw bytes back yields the same raw.
        let reparsed = Channel::parse(0, ch.encode()).unwrap();
        assert_eq!(reparsed.encode(), ch.encode(), "encode/parse not lossless");
        ch
    }

    #[test]
    fn rx_only_touches_only_byte_09_bit_5() {
        let ch = assert_touches(&[0x09], |c| c.set_rx_only(true));
        assert!(ch.rx_only);
        assert_eq!(ch.encode()[0x09], 1 << 5);
    }

    #[test]
    fn talk_around_touches_only_byte_09_bit_7() {
        let ch = assert_touches(&[0x09], |c| c.set_talk_around(true));
        assert!(ch.talk_around);
        assert_eq!(ch.encode()[0x09], 1 << 7);
    }

    #[test]
    fn call_confirm_touches_only_byte_09_bit_6() {
        let ch = assert_touches(&[0x09], |c| c.set_call_confirm(true));
        assert!(ch.call_confirm);
        assert_eq!(ch.encode()[0x09], 1 << 6);
    }

    #[test]
    fn work_alone_touches_only_byte_21_bit_7() {
        let ch = assert_touches(&[0x21], |c| c.set_work_alone(true));
        assert!(ch.work_alone);
        assert_eq!(ch.encode()[0x21], 1 << 7);
    }

    #[test]
    fn simplex_tdma_touches_only_byte_21_bit_2() {
        let ch = assert_touches(&[0x21], |c| c.set_simplex_tdma(true));
        assert!(ch.simplex_tdma);
        assert_eq!(ch.encode()[0x21], 1 << 2);
    }

    #[test]
    fn rx_aprs_touches_only_byte_21_bit_5() {
        let ch = assert_touches(&[0x21], |c| c.set_rx_aprs(true));
        assert!(ch.rx_aprs);
        assert_eq!(ch.encode()[0x21], 1 << 5);
    }

    #[test]
    fn time_slot_and_work_alone_are_independent_bits_of_byte_21() {
        // Setting the time slot must not disturb the lone-worker bit and vice
        // versa: both live in byte 0x21.
        let mut ch = Channel::default_record(0);
        ch.set_work_alone(true);
        ch.set_time_slot(2);
        assert!(ch.work_alone);
        assert_eq!(ch.time_slot, 2);
        assert_eq!(ch.encode()[0x21], (1 << 7) | 1);
    }

    #[test]
    fn admit_round_trips_and_touches_only_byte_1a() {
        for admit in [
            Admit::Always,
            Admit::ChannelFree,
            Admit::DifferentColorCode,
            Admit::SameColorCode,
        ] {
            let base = Channel::default_record(0);
            let mut ch = base.clone();
            ch.set_admit(admit);
            assert_eq!(ch.admit, admit);
            for i in 0..CHANNEL_SIZE {
                if i != 0x1a {
                    assert_eq!(base.encode()[i], ch.encode()[i], "byte {i:#x} changed");
                }
            }
        }
    }

    #[test]
    fn admit_and_optional_signaling_share_byte_1a_without_clobbering() {
        let mut ch = Channel::default_record(0);
        ch.set_admit(Admit::SameColorCode);
        ch.set_optional_signaling(OptSignaling::FiveTone);
        assert_eq!(ch.admit, Admit::SameColorCode);
        assert_eq!(ch.optional_signaling, OptSignaling::FiveTone);
        // bits 0-1 = 0b11, bits 4-5 = 0b11.
        assert_eq!(ch.encode()[0x1a], 0b0011_0011);
    }

    #[test]
    fn squelch_mode_touches_only_byte_19_bit_4() {
        let ch = assert_touches(&[0x19], |c| c.set_squelch_mode(SquelchMode::Tone));
        assert_eq!(ch.squelch_mode, SquelchMode::Tone);
        assert_eq!(ch.encode()[0x19], 1 << 4);
        // And back to carrier restores the byte.
        let base = Channel::default_record(0);
        let mut ch = base.clone();
        ch.set_squelch_mode(SquelchMode::Tone);
        ch.set_squelch_mode(SquelchMode::Carrier);
        assert_eq!(base.encode(), ch.encode());
    }

    #[test]
    fn optional_signaling_round_trips() {
        for sig in [
            OptSignaling::Off,
            OptSignaling::Dtmf,
            OptSignaling::TwoTone,
            OptSignaling::FiveTone,
        ] {
            let mut ch = Channel::default_record(0);
            ch.set_optional_signaling(sig);
            assert_eq!(ch.optional_signaling, sig);
        }
    }

    #[test]
    fn signaling_modes_share_byte_09_without_clobbering() {
        let mut ch = Channel::default_record(0);
        ch.set_rx_signaling_mode(SignalingMode::Dcs);
        ch.set_tx_signaling_mode(SignalingMode::Ctcss);
        assert_eq!(ch.rx_signaling_mode, SignalingMode::Dcs);
        assert_eq!(ch.tx_signaling_mode, SignalingMode::Ctcss);
        // rx = 0b10 at bits 0-1, tx = 0b01 at bits 2-3.
        assert_eq!(ch.encode()[0x09], 0b0000_0110);
    }

    #[test]
    fn rx_and_tx_ctcss_touch_only_their_bytes() {
        let ch = assert_touches(&[0x0b], |c| c.set_rx_ctcss(0x2a));
        assert_eq!(ch.rx_ctcss, 0x2a);
        let ch = assert_touches(&[0x0a], |c| c.set_tx_ctcss(0x2a));
        assert_eq!(ch.tx_ctcss, 0x2a);
    }

    #[test]
    fn rx_and_tx_dcs_round_trip_le() {
        let ch = assert_touches(&[0x0e, 0x0f], |c| c.set_rx_dcs(0x1234));
        assert_eq!(ch.rx_dcs, 0x1234);
        assert_eq!([ch.encode()[0x0e], ch.encode()[0x0f]], [0x34, 0x12]);
        let ch = assert_touches(&[0x0c, 0x0d], |c| c.set_tx_dcs(0xabcd));
        assert_eq!(ch.tx_dcs, 0xabcd);
        assert_eq!([ch.encode()[0x0c], ch.encode()[0x0d]], [0xcd, 0xab]);
    }

    #[test]
    fn scan_list_index_touches_only_byte_1b() {
        let ch = assert_touches(&[0x1b], |c| c.set_scan_list_index(7));
        assert_eq!(ch.scan_list_index, 7);
    }

    #[test]
    fn tone_id_indices_touch_only_their_bytes() {
        let ch = assert_touches(&[0x1d], |c| c.set_two_tone_id_index(3));
        assert_eq!(ch.two_tone_id_index, 3);
        let ch = assert_touches(&[0x1e], |c| c.set_five_tone_id_index(4));
        assert_eq!(ch.five_tone_id_index, 4);
        let ch = assert_touches(&[0x1f], |c| c.set_dtmf_id_index(5));
        assert_eq!(ch.dtmf_id_index, 5);
    }

    #[test]
    fn two_tone_decode_index_round_trips_le() {
        let ch = assert_touches(&[0x12, 0x13], |c| c.set_two_tone_decode_index(0x01fe));
        assert_eq!(ch.two_tone_decode_index, 0x01fe);
        assert_eq!([ch.encode()[0x12], ch.encode()[0x13]], [0xfe, 0x01]);
    }

    #[test]
    fn every_new_offset_is_inside_the_record() {
        // Guard against a future edit pointing a field outside the 64-byte
        // record — a write there would corrupt a neighbouring channel.
        for off in [
            FLAGS2_OFFSET,
            TX_CTCSS_OFFSET,
            RX_CTCSS_OFFSET,
            TX_DCS_OFFSET + 1,
            RX_DCS_OFFSET + 1,
            TWO_TONE_DECODE_OFFSET + 1,
            SQUELCH_OFFSET,
            ADMIT_OFFSET,
            SCAN_LIST_INDEX_OFFSET,
            TWO_TONE_ID_OFFSET,
            FIVE_TONE_ID_OFFSET,
            DTMF_ID_OFFSET,
        ] {
            assert!(off < CHANNEL_SIZE, "offset {off:#x} outside record");
        }
    }
}
