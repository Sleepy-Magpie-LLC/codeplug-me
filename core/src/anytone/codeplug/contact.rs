//! The 100-byte (0x64) DMR digital-contact record.
//!
//! Offsets are from qdmr `lib/anytone_codeplug.hh`
//! `AnytoneCodeplug::ContactElement::Offset` (`size()` = 0x64):
//! - callType 0x00 (u8): 0 = Private, 1 = Group, 2 = All (qdmr `type()`).
//! - name 0x01, 16 bytes, Latin1, `0x00`-padded (`Limit::nameLength()` = 16).
//! - number 0x23, BCD8 big-endian (`getBCD8_be`) — the DMR ID.
//! - alertType 0x27 (u8): ring/online alert, preserved but not modeled.
//!
//! A "talk group" is just a contact with call type Group, so this one record
//! type covers both. The struct keeps the raw 100 bytes authoritative so
//! [`encode`](Contact::encode) is lossless; decoded fields are a cache.

use serde::Serialize;

use crate::bytes::{get_bcd8_be, read_ascii, set_bcd8_be, write_ascii};
use crate::error::{Error, Result};

/// Size of a contact record in bytes (qdmr `ContactElement::size()` = 0x64).
pub const CONTACT_SIZE: usize = 0x64;

/// Offset of the call-type byte (qdmr `Offset::type()`).
const TYPE_OFFSET: usize = 0x00;
/// Offset of the 16-byte contact name (qdmr `Offset::name()`).
const NAME_OFFSET: usize = 0x01;
/// Maximum contact name length (qdmr `Limit::nameLength()`).
const NAME_LEN: usize = 16;
/// Offset of the DMR number, BCD8 big-endian (qdmr `Offset::number()`).
const NUMBER_OFFSET: usize = 0x23;

/// DMR call type (qdmr `DMRContact::Type` as stored by `ContactElement`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CallType {
    /// Private (individual) call.
    Private,
    /// Group call — i.e. a talk group.
    Group,
    /// All call (broadcast).
    All,
}

impl CallType {
    /// Decode the call-type byte (0 = Private, 1 = Group, 2 = All).
    fn from_u8(v: u8) -> CallType {
        match v {
            1 => CallType::Group,
            2 => CallType::All,
            _ => CallType::Private,
        }
    }

    /// Encode as the call-type byte.
    fn to_u8(self) -> u8 {
        match self {
            CallType::Private => 0,
            CallType::Group => 1,
            CallType::All => 2,
        }
    }
}

/// A single DMR digital contact (or talk group when `call_type` is `Group`).
/// Decoded fields are for inspection/JSON; `raw` is authoritative for lossless
/// serialization.
#[derive(Debug, Clone, Serialize)]
pub struct Contact {
    /// Contact index (position in the contact bitmap / bank layout).
    pub index: usize,
    /// Contact name.
    pub name: String,
    /// DMR ID / talk-group number.
    pub number: u32,
    /// Call type (private / group / all).
    pub call_type: CallType,
    /// Authoritative raw record; skipped in JSON output.
    #[serde(skip)]
    raw: [u8; CONTACT_SIZE],
}

impl Contact {
    /// Parse a 100-byte record at contact `index`. Returns [`Error::Parse`] if
    /// the slice is not exactly [`CONTACT_SIZE`] bytes.
    pub fn parse(index: usize, rec: &[u8]) -> Result<Contact> {
        if rec.len() != CONTACT_SIZE {
            return Err(Error::Parse(format!(
                "contact record is {} bytes, expected {CONTACT_SIZE}",
                rec.len()
            )));
        }
        let mut raw = [0u8; CONTACT_SIZE];
        raw.copy_from_slice(rec);
        Ok(Self::from_raw(index, raw))
    }

    /// The raw 100-byte record for serialization (lossless).
    pub fn encode(&self) -> &[u8; CONTACT_SIZE] {
        &self.raw
    }

    /// Set the contact name, rewriting only the 16-byte name field.
    pub fn set_name(&mut self, name: &str) {
        write_ascii(
            &mut self.raw[NAME_OFFSET..NAME_OFFSET + NAME_LEN],
            name,
            0x00,
        );
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the DMR number, rewriting only the 4 BCD bytes at 0x23.
    pub fn set_number(&mut self, number: u32) {
        set_bcd8_be(&mut self.raw[NUMBER_OFFSET..NUMBER_OFFSET + 4], number);
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the call type, rewriting only the type byte at 0x00.
    pub fn set_call_type(&mut self, call_type: CallType) {
        self.raw[TYPE_OFFSET] = call_type.to_u8();
        *self = Self::from_raw(self.index, self.raw);
    }

    /// A blank contact for `index`: a Group call (talk group), number 0, named
    /// "NEW". Callers typically edit it immediately after adding.
    pub fn default_record(index: usize) -> Contact {
        let mut raw = [0u8; CONTACT_SIZE];
        raw[TYPE_OFFSET] = CallType::Group.to_u8();
        write_ascii(&mut raw[NAME_OFFSET..NAME_OFFSET + NAME_LEN], "NEW", 0x00);
        Self::from_raw(index, raw)
    }

    /// Rebuild the decoded fields from `raw`.
    fn from_raw(index: usize, raw: [u8; CONTACT_SIZE]) -> Contact {
        Contact {
            index,
            name: read_ascii(&raw[NAME_OFFSET..NAME_OFFSET + NAME_LEN], 0x00),
            number: get_bcd8_be(&raw[NUMBER_OFFSET..NUMBER_OFFSET + 4]),
            call_type: CallType::from_u8(raw[TYPE_OFFSET]),
            raw,
        }
    }
}
