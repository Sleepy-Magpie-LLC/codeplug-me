//! Read-only APRS configuration view.
//!
//! **Read-only.** The APRS settings live at `0x02501000`, inside the
//! radio-settings block (`0x02500000 – 0x025014FF`) that this tool must never
//! write (see `docs/codeplug-memory-map.md`: writing that block once locked a
//! radio behind an unknown power-on password with Chinese menus). So this module
//! only *parses* the block for display; there is no setter and nothing here is
//! ever serialized back.
//!
//! Offsets are from qdmr `D878UVCodeplug::APRSSettingsElement::Offset`
//! (`lib/d878uv_codeplug.hh`, `size()` = 0x100). Only a focused, high-confidence
//! subset of fields is decoded — the identity and analog-TX basics the operator
//! usually cares about. Anything not confidently sourced is left out rather than
//! shown as a guess. Values should still be verified against the vendor CPS.

use serde::Serialize;

use crate::bytes::{get_bcd8_be, read_ascii};

/// Byte size of the APRS settings element (qdmr `APRSSettingsElement::size()`).
pub const APRS_SETTINGS_SIZE: usize = 0x100;

/// Offset of the manual TX interval, u8 (qdmr `Offset::manualTXInterval`).
const MANUAL_TX_INTERVAL_OFFSET: usize = 0x0a;
/// Offset of the automatic TX interval, u8 (qdmr `Offset::autoTXInterval`).
const AUTO_TX_INTERVAL_OFFSET: usize = 0x0b;
/// Offset of the destination (TOCALL) call sign, 6 bytes ASCII
/// (qdmr `Offset::destination`).
const DEST_CALL_OFFSET: usize = 0x16;
/// Offset of the destination SSID, u8 (qdmr `Offset::destinationSSID`).
const DEST_SSID_OFFSET: usize = 0x1c;
/// Offset of the source (own) call sign, 6 bytes ASCII (qdmr `Offset::source`).
const SOURCE_CALL_OFFSET: usize = 0x1d;
/// Offset of the source SSID, u8 (qdmr `Offset::sourceSSID`).
const SOURCE_SSID_OFFSET: usize = 0x23;
/// Length of an APRS call-sign field in bytes.
const CALL_LEN: usize = 6;
/// Offset of the APRS symbol table selector, u8 (qdmr `Offset::symbolTable`).
const SYMBOL_TABLE_OFFSET: usize = 0x39;
/// Offset of the APRS map icon / symbol, u8 (qdmr `Offset::symbol`).
const SYMBOL_OFFSET: usize = 0x3a;
/// Offset of the analog (FM) TX power selector, u8 (qdmr `Offset::fmPower`).
const FM_POWER_OFFSET: usize = 0x3b;
/// Offset of the first analog (FM) TX frequency, BCD8 big-endian, ×10 Hz
/// (qdmr `Offset::fmFrequencies`).
const FM_FREQ_OFFSET: usize = 0xac;

/// A read-only projection of the radio's APRS settings for display. Every field
/// is decoded from the settings block and never written back.
#[derive(Debug, Clone, Serialize)]
pub struct AprsConfig {
    /// Own (source) APRS call sign.
    pub source_call: String,
    /// Own APRS SSID.
    pub source_ssid: u8,
    /// Destination (TOCALL) call sign.
    pub destination_call: String,
    /// Destination SSID.
    pub destination_ssid: u8,
    /// APRS symbol table selector byte (e.g. b'/' or b'\\').
    pub symbol_table: u8,
    /// APRS map icon / symbol byte.
    pub symbol: u8,
    /// Manual TX interval (raw stored value).
    pub manual_tx_interval: u8,
    /// Automatic TX interval (raw stored value).
    pub auto_tx_interval: u8,
    /// Analog (FM) TX power selector (raw stored value).
    pub fm_power: u8,
    /// First analog (FM) TX frequency in Hz (0 when unset).
    pub fm_tx_frequency_hz: u32,
}

impl AprsConfig {
    /// Parse the APRS settings element from `rec` (at least
    /// [`APRS_SETTINGS_SIZE`] bytes). Returns `None` if the slice is too short.
    pub fn parse(rec: &[u8]) -> Option<AprsConfig> {
        if rec.len() < APRS_SETTINGS_SIZE {
            return None;
        }
        Some(AprsConfig {
            source_call: read_ascii(
                &rec[SOURCE_CALL_OFFSET..SOURCE_CALL_OFFSET + CALL_LEN],
                0x00,
            ),
            source_ssid: rec[SOURCE_SSID_OFFSET],
            destination_call: read_ascii(&rec[DEST_CALL_OFFSET..DEST_CALL_OFFSET + CALL_LEN], 0x00),
            destination_ssid: rec[DEST_SSID_OFFSET],
            symbol_table: rec[SYMBOL_TABLE_OFFSET],
            symbol: rec[SYMBOL_OFFSET],
            manual_tx_interval: rec[MANUAL_TX_INTERVAL_OFFSET],
            auto_tx_interval: rec[AUTO_TX_INTERVAL_OFFSET],
            fm_power: rec[FM_POWER_OFFSET],
            fm_tx_frequency_hz: get_bcd8_be(&rec[FM_FREQ_OFFSET..FM_FREQ_OFFSET + 4]) * 10,
        })
    }
}
