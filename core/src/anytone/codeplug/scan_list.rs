//! The 144-byte D878UVII scan-list record.
//!
//! Layout from qdmr `lib/anytone_codeplug.hh` `AnytoneCodeplug::ScanListElement`
//! (`size()` = 0x90), cross-checked against `lib/d868uv_codeplug.hh` for the
//! bank/bitmap addresses. Fields:
//! - priorityChannelSelect 0x01 (u8) — which priority channels are active
//!   (qdmr `PriChannel`: 0 = off, 1 = primary, 2 = secondary, 3 = both).
//! - priorityChannel1 (primary) 0x02, priorityChannel2 (secondary) 0x04 —
//!   u16-le channel indices, `0xffff` = none.
//! - lookBackTimeA 0x06, lookBackTimeB 0x08, dropOutDelay 0x0a, dwellTime 0x0c —
//!   u16-le. qdmr reads these as `value * 10` deci-seconds; we keep the raw
//!   stored value and leave the scaling to the UI.
//! - revertChannel 0x0e (u8) — enum (qdmr `RevertChannel`).
//! - name 0x0f, 16 bytes, Latin1, `0x00`-padded/terminated.
//! - members 0x20, up to [`MAX_MEMBERS`] × u16-le channel indices,
//!   `0xffff` = unused (qdmr `hasMemberIndex` tests against `0xffff`).
//!
//! The raw 144 bytes are kept as the source of truth so [`ScanList::encode`] is
//! byte-lossless; the decoded fields are a cache refreshed on every edit.

use serde::Serialize;

use crate::bytes::{read_ascii, write_ascii};
use crate::error::{Error, Result};

/// Size of a scan-list record in bytes (qdmr `ScanListElement::size()` = 0x90).
pub const SCAN_LIST_SIZE: usize = 0x90;

/// Offset of the priority-channel-select byte (qdmr `Offset::priorityChannel`).
const PRIORITY_SELECT_OFFSET: usize = 0x01;
/// Offset of the primary priority channel index, u16-le
/// (qdmr `Offset::primaryChannel`).
const PRIORITY1_OFFSET: usize = 0x02;
/// Offset of the secondary priority channel index, u16-le
/// (qdmr `Offset::secondaryChannel`).
const PRIORITY2_OFFSET: usize = 0x04;
/// Offset of look-back time A, u16-le (qdmr `Offset::lookBackTimeA`).
const LOOKBACK_A_OFFSET: usize = 0x06;
/// Offset of look-back time B, u16-le (qdmr `Offset::lookBackTimeB`).
const LOOKBACK_B_OFFSET: usize = 0x08;
/// Offset of dropout delay, u16-le (qdmr `Offset::dropOutDelay`).
const DROPOUT_OFFSET: usize = 0x0a;
/// Offset of dwell time, u16-le (qdmr `Offset::dwellTime`).
const DWELL_OFFSET: usize = 0x0c;
/// Offset of the revert-channel byte (qdmr `Offset::revertChannel`).
const REVERT_OFFSET: usize = 0x0e;
/// Offset of the 16-byte scan-list name (qdmr `Offset::name`).
const NAME_OFFSET: usize = 0x0f;
/// Maximum scan-list name length (qdmr `Limit::nameLength` = 16).
const NAME_LEN: usize = 16;
/// Offset of the first member channel index (qdmr `Offset::members`).
const MEMBERS_OFFSET: usize = 0x20;
/// Maximum member channel entries in a scan list (AnyTone limit).
pub const MAX_MEMBERS: usize = 50;
/// Sentinel marking a priority-channel or member entry as unused.
const EMPTY: u16 = 0xffff;

/// A scan list: a name, an ordered list of member channels, and the scan
/// timing / priority parameters. Decoded fields are for inspection/JSON; `raw`
/// is authoritative and [`encode`](ScanList::encode) returns it unchanged unless
/// a setter has edited it.
#[derive(Debug, Clone, Serialize)]
pub struct ScanList {
    /// Scan-list index (position in the scan-list bitmap / bank layout).
    pub index: usize,
    /// Scan-list name.
    pub name: String,
    /// Member channel indices, in order (unused `0xffff` slots dropped).
    pub members: Vec<u16>,
    /// Priority-channel select (0 = off, 1 = primary, 2 = secondary, 3 = both).
    pub priority_channel_select: u8,
    /// Primary priority channel index (`0xffff` = none).
    pub priority_channel_1: u16,
    /// Secondary priority channel index (`0xffff` = none).
    pub priority_channel_2: u16,
    /// Look-back time A (raw stored value; qdmr interprets as ×10 deci-seconds).
    pub look_back_a: u16,
    /// Look-back time B (raw stored value).
    pub look_back_b: u16,
    /// Dropout delay (raw stored value).
    pub dropout_delay: u16,
    /// Dwell time (raw stored value).
    pub dwell_time: u16,
    /// Revert-channel selection (raw qdmr `RevertChannel` enum value).
    pub revert_channel: u8,
    /// Authoritative raw record; skipped in JSON output.
    #[serde(skip)]
    raw: [u8; SCAN_LIST_SIZE],
}

impl ScanList {
    /// Parse a 144-byte record at scan-list `index`. Returns [`Error::Parse`] if
    /// the slice is not exactly [`SCAN_LIST_SIZE`] bytes.
    pub fn parse(index: usize, rec: &[u8]) -> Result<ScanList> {
        if rec.len() != SCAN_LIST_SIZE {
            return Err(Error::Parse(format!(
                "scan-list record is {} bytes, expected {SCAN_LIST_SIZE}",
                rec.len()
            )));
        }
        let mut raw = [0u8; SCAN_LIST_SIZE];
        raw.copy_from_slice(rec);
        Ok(Self::from_raw(index, raw))
    }

    /// The raw 144-byte record for serialization (lossless).
    pub fn encode(&self) -> &[u8; SCAN_LIST_SIZE] {
        &self.raw
    }

    /// Set the scan-list name, rewriting only the 16-byte name field.
    pub fn set_name(&mut self, name: &str) {
        write_ascii(
            &mut self.raw[NAME_OFFSET..NAME_OFFSET + NAME_LEN],
            name,
            0x00,
        );
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Replace the member channel-index list. Up to [`MAX_MEMBERS`] are written
    /// (extras dropped); the remaining member slots are set to `0xffff`. Only the
    /// member area (0x20 .. 0x20 + MAX_MEMBERS×2) is touched.
    pub fn set_members(&mut self, members: &[u16]) {
        for j in 0..MAX_MEMBERS {
            let value = members.get(j).copied().unwrap_or(EMPTY);
            let off = MEMBERS_OFFSET + j * 2;
            self.raw[off..off + 2].copy_from_slice(&value.to_le_bytes());
        }
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the priority-channel-select byte (0 = off, 1 = primary, 2 = secondary,
    /// 3 = both).
    pub fn set_priority_channel_select(&mut self, v: u8) {
        self.raw[PRIORITY_SELECT_OFFSET] = v;
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the primary priority channel index (`0xffff` = none).
    pub fn set_priority_channel_1(&mut self, v: u16) {
        self.raw[PRIORITY1_OFFSET..PRIORITY1_OFFSET + 2].copy_from_slice(&v.to_le_bytes());
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the secondary priority channel index (`0xffff` = none).
    pub fn set_priority_channel_2(&mut self, v: u16) {
        self.raw[PRIORITY2_OFFSET..PRIORITY2_OFFSET + 2].copy_from_slice(&v.to_le_bytes());
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the raw look-back time A value (u16-le at 0x06).
    pub fn set_look_back_a(&mut self, v: u16) {
        self.raw[LOOKBACK_A_OFFSET..LOOKBACK_A_OFFSET + 2].copy_from_slice(&v.to_le_bytes());
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the raw look-back time B value (u16-le at 0x08).
    pub fn set_look_back_b(&mut self, v: u16) {
        self.raw[LOOKBACK_B_OFFSET..LOOKBACK_B_OFFSET + 2].copy_from_slice(&v.to_le_bytes());
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the raw dropout-delay value (u16-le at 0x0a).
    pub fn set_dropout_delay(&mut self, v: u16) {
        self.raw[DROPOUT_OFFSET..DROPOUT_OFFSET + 2].copy_from_slice(&v.to_le_bytes());
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the raw dwell-time value (u16-le at 0x0c).
    pub fn set_dwell_time(&mut self, v: u16) {
        self.raw[DWELL_OFFSET..DWELL_OFFSET + 2].copy_from_slice(&v.to_le_bytes());
        *self = Self::from_raw(self.index, self.raw);
    }

    /// Set the revert-channel enum value (u8 at 0x0e).
    pub fn set_revert_channel(&mut self, v: u8) {
        self.raw[REVERT_OFFSET] = v;
        *self = Self::from_raw(self.index, self.raw);
    }

    /// A blank scan list for `index`: no members, no priority channels, named
    /// "NEW". Callers typically edit it immediately after adding.
    pub fn default_record(index: usize) -> ScanList {
        let mut raw = [0u8; SCAN_LIST_SIZE];
        // Priority channels and every member slot start as `0xffff` = none.
        raw[PRIORITY1_OFFSET..PRIORITY1_OFFSET + 2].copy_from_slice(&EMPTY.to_le_bytes());
        raw[PRIORITY2_OFFSET..PRIORITY2_OFFSET + 2].copy_from_slice(&EMPTY.to_le_bytes());
        for j in 0..MAX_MEMBERS {
            let off = MEMBERS_OFFSET + j * 2;
            raw[off..off + 2].copy_from_slice(&EMPTY.to_le_bytes());
        }
        write_ascii(&mut raw[NAME_OFFSET..NAME_OFFSET + NAME_LEN], "NEW", 0x00);
        Self::from_raw(index, raw)
    }

    /// Rebuild the decoded fields from `raw`.
    fn from_raw(index: usize, raw: [u8; SCAN_LIST_SIZE]) -> ScanList {
        let read_u16 = |off: usize| u16::from_le_bytes([raw[off], raw[off + 1]]);

        let mut members = Vec::new();
        for j in 0..MAX_MEMBERS {
            let v = read_u16(MEMBERS_OFFSET + j * 2);
            if v != EMPTY {
                members.push(v);
            }
        }

        ScanList {
            index,
            name: read_ascii(&raw[NAME_OFFSET..NAME_OFFSET + NAME_LEN], 0x00),
            members,
            priority_channel_select: raw[PRIORITY_SELECT_OFFSET],
            priority_channel_1: read_u16(PRIORITY1_OFFSET),
            priority_channel_2: read_u16(PRIORITY2_OFFSET),
            look_back_a: read_u16(LOOKBACK_A_OFFSET),
            look_back_b: read_u16(LOOKBACK_B_OFFSET),
            dropout_delay: read_u16(DROPOUT_OFFSET),
            dwell_time: read_u16(DWELL_OFFSET),
            revert_channel: raw[REVERT_OFFSET],
            raw,
        }
    }
}
