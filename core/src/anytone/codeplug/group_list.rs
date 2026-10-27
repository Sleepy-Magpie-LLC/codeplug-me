//! RX group list records: a member list of contact indices plus a name.
//!
//! Layout from qdmr `lib/anytone_codeplug.hh`
//! `AnytoneCodeplug::GroupListElement` (`size()` = 0x120, stored in a 0x200
//! slot spaced by `Offset::betweenGroupLists()`):
//! - members 0x00: 64 entries of u32 little-endian contact indices, spaced 4
//!   bytes (`Offset::members/betweenMembers`, `Limit::members()` = 64);
//!   `0xffffffff` marks an unused entry.
//! - name 0x100, 16 bytes, Latin1, `0x00`-padded (`Limit::nameLength()` = 16).
//!
//! The full 0x200 slot is preserved as `raw` so serialization is byte-lossless
//! (the 0xe0 bytes past the element are carried through untouched).

use serde::Serialize;

use crate::bytes::{read_ascii, write_ascii};
use crate::error::{Error, Result};

/// Byte stride of a group-list slot (qdmr `Offset::betweenGroupLists()` = 0x200).
pub const GROUP_LIST_SLOT: usize = 0x200;
/// Byte size of the group-list element actually transferred to the radio (qdmr
/// `GroupListElement::size()` = 0x120); the rest of the 0x200 slot is padding.
pub const GROUP_LIST_ELEMENT: usize = 0x120;
/// Offset of the member array within the slot (qdmr `Offset::members()`).
const MEMBERS_OFFSET: usize = 0x00;
/// Maximum members per group list (qdmr `Limit::members()`).
const MAX_MEMBERS: usize = 64;
/// Offset of the 16-byte name within the slot (qdmr `Offset::name()`).
const NAME_OFFSET: usize = 0x100;
/// Maximum group-list name length (qdmr `Limit::nameLength()`).
const NAME_LEN: usize = 16;
/// Sentinel marking an unused member entry.
const EMPTY: u32 = 0xffff_ffff;

/// An RX group list: a name plus the contact indices it receives. Decoded
/// `name`/`members` are for inspection/JSON; `raw` is authoritative.
#[derive(Debug, Clone, Serialize)]
pub struct GroupList {
    /// Group-list index (position in the group-list bitmap).
    pub index: usize,
    /// Group-list name.
    pub name: String,
    /// Member contact indices, in order (unused `0xffffffff` slots dropped).
    pub members: Vec<u32>,
    /// Authoritative raw 0x200 slot; skipped in JSON output.
    #[serde(skip)]
    raw: [u8; GROUP_LIST_SLOT],
}

impl GroupList {
    /// Parse a group list from its 0x200 slot. Returns [`Error::Parse`] if the
    /// slice is the wrong length.
    pub fn parse(index: usize, rec: &[u8]) -> Result<GroupList> {
        if rec.len() != GROUP_LIST_SLOT {
            return Err(Error::Parse(format!(
                "group-list slot is {} bytes, expected {GROUP_LIST_SLOT}",
                rec.len()
            )));
        }
        let mut raw = [0u8; GROUP_LIST_SLOT];
        raw.copy_from_slice(rec);
        Ok(Self::from_raw(index, raw))
    }

    /// The raw 0x200 slot for serialization (lossless).
    pub fn encode(&self) -> &[u8; GROUP_LIST_SLOT] {
        &self.raw
    }

    /// Set the group-list name, rewriting only the 16-byte name field.
    pub fn set_name(&mut self, name: &str) {
        write_ascii(
            &mut self.raw[NAME_OFFSET..NAME_OFFSET + NAME_LEN],
            name,
            0x00,
        );
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Replace the member contact-index list. Up to [`MAX_MEMBERS`] are written
    /// (extras are dropped); the remaining slots are set to `0xffffffff`.
    pub fn set_members(&mut self, members: &[u32]) {
        for i in 0..MAX_MEMBERS {
            let value = members.get(i).copied().unwrap_or(EMPTY);
            let off = MEMBERS_OFFSET + i * 4;
            self.raw[off..off + 4].copy_from_slice(&value.to_le_bytes());
        }
        *self = Self::from_raw(self.index, self.raw);
    }

    /// A blank group list for `index`: no members, named "NEW".
    pub fn default_record(index: usize) -> GroupList {
        let mut raw = [0u8; GROUP_LIST_SLOT];
        for i in 0..MAX_MEMBERS {
            let off = MEMBERS_OFFSET + i * 4;
            raw[off..off + 4].copy_from_slice(&EMPTY.to_le_bytes());
        }
        write_ascii(&mut raw[NAME_OFFSET..NAME_OFFSET + NAME_LEN], "NEW", 0x00);
        Self::from_raw(index, raw)
    }

    /// Rebuild the decoded fields from `raw`.
    fn from_raw(index: usize, raw: [u8; GROUP_LIST_SLOT]) -> GroupList {
        let mut members = Vec::new();
        for i in 0..MAX_MEMBERS {
            let off = MEMBERS_OFFSET + i * 4;
            let v = u32::from_le_bytes([raw[off], raw[off + 1], raw[off + 2], raw[off + 3]]);
            if v != EMPTY {
                members.push(v);
            }
        }
        GroupList {
            index,
            name: read_ascii(&raw[NAME_OFFSET..NAME_OFFSET + NAME_LEN], 0x00),
            members,
            raw,
        }
    }
}
