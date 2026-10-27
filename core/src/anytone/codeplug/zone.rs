//! Zone records: a name slot plus a channel-index list.
//!
//! Layout from qdmr `lib/d868uv_codeplug.hh`: the zone name lives in a 32-byte
//! slot (`Size::zoneName` = 0x20, `Limit::zoneNameLength` = 16, Latin1,
//! `0x00`-padded) and the member list in a 512-byte slot (`Size::zoneChannels`
//! = 0x200) of little-endian `u16` channel indices, `0xffff` marking an unused
//! entry (qdmr `encodeZones`/`linkZones`). Both raw slots are preserved so
//! serialization is byte-lossless.

use serde::Serialize;

use super::CHANNELS_PER_ZONE;
use crate::bytes::{read_ascii, write_ascii};
use crate::error::{Error, Result};

/// Byte size of a zone name slot (qdmr `Size::zoneName()` = 0x20).
pub const ZONE_NAME_SLOT: usize = 0x20;
/// Byte size of a zone channel-list slot (qdmr `Size::zoneChannels()` = 0x200).
pub const ZONE_CHANNELS_SLOT: usize = 0x200;
/// Maximum zone name length (qdmr `Limit::zoneNameLength()`).
const ZONE_NAME_LEN: usize = 16;
/// Sentinel marking an unused channel-list entry.
const EMPTY: u16 = 0xffff;

/// A zone: a name plus the channel indices it contains. Decoded `name`/
/// `channels` are for inspection/JSON; the raw slots are authoritative for
/// lossless serialization.
#[derive(Debug, Clone, Serialize)]
pub struct Zone {
    /// Zone index (position in the zone bitmap).
    pub index: usize,
    /// Zone name.
    pub name: String,
    /// Member channel indices, in order (unused `0xffff` slots dropped).
    pub channels: Vec<u16>,
    /// The channel selected on VFO A for this zone (read-only display; sourced
    /// from the zone channel list in the radio-settings block, which this tool
    /// never writes). `None` when unset (`0xffff`).
    pub a_channel: Option<u16>,
    /// The channel selected on VFO B for this zone (read-only display). `None`
    /// when unset.
    pub b_channel: Option<u16>,
    /// Authoritative raw 32-byte name slot; skipped in JSON output.
    #[serde(skip)]
    name_raw: [u8; ZONE_NAME_SLOT],
    /// Authoritative raw 512-byte channel-list slot; skipped in JSON output.
    #[serde(skip)]
    channels_raw: [u8; ZONE_CHANNELS_SLOT],
}

impl Zone {
    /// Parse a zone from its name slot and channel-list slot. Returns
    /// [`Error::Parse`] if either slice is the wrong length.
    pub fn parse(index: usize, name: &[u8], channels: &[u8]) -> Result<Zone> {
        if name.len() != ZONE_NAME_SLOT {
            return Err(Error::Parse(format!(
                "zone name slot is {} bytes, expected {ZONE_NAME_SLOT}",
                name.len()
            )));
        }
        if channels.len() != ZONE_CHANNELS_SLOT {
            return Err(Error::Parse(format!(
                "zone channel slot is {} bytes, expected {ZONE_CHANNELS_SLOT}",
                channels.len()
            )));
        }
        let mut name_raw = [0u8; ZONE_NAME_SLOT];
        name_raw.copy_from_slice(name);
        let mut channels_raw = [0u8; ZONE_CHANNELS_SLOT];
        channels_raw.copy_from_slice(channels);

        let mut members = Vec::new();
        for j in 0..CHANNELS_PER_ZONE {
            let v = u16::from_le_bytes([channels_raw[j * 2], channels_raw[j * 2 + 1]]);
            if v != EMPTY {
                members.push(v);
            }
        }

        Ok(Zone {
            index,
            name: read_ascii(&name_raw[..ZONE_NAME_LEN], 0x00),
            channels: members,
            a_channel: None,
            b_channel: None,
            name_raw,
            channels_raw,
        })
    }

    /// Set the read-only VFO A/B channel selections for display. These come from
    /// the zone channel list (a read-only region) and are never serialized back.
    pub fn set_display_ab(&mut self, a: Option<u16>, b: Option<u16>) {
        self.a_channel = a;
        self.b_channel = b;
    }

    /// The raw 32-byte name slot for serialization.
    pub fn name_bytes(&self) -> &[u8; ZONE_NAME_SLOT] {
        &self.name_raw
    }

    /// The raw 512-byte channel-list slot for serialization.
    pub fn channel_bytes(&self) -> &[u8; ZONE_CHANNELS_SLOT] {
        &self.channels_raw
    }

    /// Set the zone name, rewriting only the 16-byte name field in the raw
    /// name slot.
    pub fn set_name(&mut self, name: &str) {
        write_ascii(&mut self.name_raw[..ZONE_NAME_LEN], name, 0x00);
        self.name = read_ascii(&self.name_raw[..ZONE_NAME_LEN], 0x00);
    }

    /// Replace the member channel-index list. Up to [`CHANNELS_PER_ZONE`] are
    /// written (extras dropped); the remaining entries are set to `0xffff`.
    pub fn set_members(&mut self, members: &[u16]) {
        for j in 0..CHANNELS_PER_ZONE {
            let value = members.get(j).copied().unwrap_or(EMPTY);
            self.channels_raw[j * 2..j * 2 + 2].copy_from_slice(&value.to_le_bytes());
        }
        self.channels = members.iter().copied().take(CHANNELS_PER_ZONE).collect();
    }

    /// A blank zone for `index`: no members, named "NEW".
    pub fn default_record(index: usize) -> Zone {
        let mut name_raw = [0u8; ZONE_NAME_SLOT];
        write_ascii(&mut name_raw[..ZONE_NAME_LEN], "NEW", 0x00);
        // All 0xff = every member entry empty.
        let channels_raw = [0xffu8; ZONE_CHANNELS_SLOT];
        Zone {
            index,
            name: read_ascii(&name_raw[..ZONE_NAME_LEN], 0x00),
            channels: Vec::new(),
            a_channel: None,
            b_channel: None,
            name_raw,
            channels_raw,
        }
    }
}
