//! The 32-byte (0x20) radio-ID record: one of the operator's own DMR IDs.
//!
//! Layout from qdmr `lib/anytone_codeplug.hh`
//! `AnytoneCodeplug::RadioIDElement` (`size()` = 0x20):
//! - number 0x00, BCD8 big-endian (`getBCD8_be`) — the DMR ID.
//! - name 0x05, 16 bytes, Latin1, `0x00`-padded (`Limit::nameLength()` = 16).
//!
//! The struct keeps the raw 32 bytes authoritative so [`encode`](RadioId::encode)
//! is lossless; decoded fields are a cache.

use serde::Serialize;

use crate::bytes::{get_bcd8_be, read_ascii, set_bcd8_be, write_ascii};
use crate::error::{Error, Result};

/// Size of a radio-ID record in bytes (qdmr `RadioIDElement::size()` = 0x20).
pub const RADIO_ID_SIZE: usize = 0x20;

/// Offset of the DMR number, BCD8 big-endian (qdmr `Offset::number()`).
const NUMBER_OFFSET: usize = 0x00;
/// Offset of the 16-byte name (qdmr `Offset::name()`).
const NAME_OFFSET: usize = 0x05;
/// Maximum radio-ID name length (qdmr `Limit::nameLength()`).
const NAME_LEN: usize = 16;

/// One radio ID: the operator's DMR ID plus a label. Decoded fields are for
/// inspection/JSON; `raw` is authoritative for lossless serialization.
#[derive(Debug, Clone, Serialize)]
pub struct RadioId {
    /// Radio-ID index (position in the radio-ID bitmap).
    pub index: usize,
    /// Radio-ID name/label.
    pub name: String,
    /// The DMR ID number.
    pub number: u32,
    /// Authoritative raw record; skipped in JSON output.
    #[serde(skip)]
    raw: [u8; RADIO_ID_SIZE],
}

impl RadioId {
    /// Parse a 32-byte record at radio-ID `index`. Returns [`Error::Parse`] if
    /// the slice is not exactly [`RADIO_ID_SIZE`] bytes.
    pub fn parse(index: usize, rec: &[u8]) -> Result<RadioId> {
        if rec.len() != RADIO_ID_SIZE {
            return Err(Error::Parse(format!(
                "radio-ID record is {} bytes, expected {RADIO_ID_SIZE}",
                rec.len()
            )));
        }
        let mut raw = [0u8; RADIO_ID_SIZE];
        raw.copy_from_slice(rec);
        Ok(Self::from_raw(index, raw))
    }

    /// The raw 32-byte record for serialization (lossless).
    pub fn encode(&self) -> &[u8; RADIO_ID_SIZE] {
        &self.raw
    }

    /// Set the radio-ID name, rewriting only the 16-byte name field.
    pub fn set_name(&mut self, name: &str) {
        write_ascii(
            &mut self.raw[NAME_OFFSET..NAME_OFFSET + NAME_LEN],
            name,
            0x00,
        );
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the DMR number, rewriting only the 4 BCD bytes at 0x00.
    pub fn set_number(&mut self, number: u32) {
        set_bcd8_be(&mut self.raw[NUMBER_OFFSET..NUMBER_OFFSET + 4], number);
        *self = Self::from_raw(self.index, self.raw);
    }

    /// A blank radio ID for `index`: number 0, named "NEW".
    pub fn default_record(index: usize) -> RadioId {
        let mut raw = [0u8; RADIO_ID_SIZE];
        write_ascii(&mut raw[NAME_OFFSET..NAME_OFFSET + NAME_LEN], "NEW", 0x00);
        Self::from_raw(index, raw)
    }

    /// Rebuild the decoded fields from `raw`.
    fn from_raw(index: usize, raw: [u8; RADIO_ID_SIZE]) -> RadioId {
        RadioId {
            index,
            name: read_ascii(&raw[NAME_OFFSET..NAME_OFFSET + NAME_LEN], 0x00),
            number: get_bcd8_be(&raw[NUMBER_OFFSET..NUMBER_OFFSET + 4]),
            raw,
        }
    }
}
