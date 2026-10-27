//! High-level radio operations built on the [`protocol`] primitives: program
//! mode entry/exit, identify, region reads/writes, and whole-codeplug backup
//! and restore. Transfers are sparse and bitmap-driven, matching the vendor CPS:
//! only active records move, written in address order. The radio commits the
//! codeplug on program-mode exit, so `write_codeplug` relies on each block's
//! write ACK rather than an immediate read-back (which would return pre-commit
//! data); [`Radio::write_region`] still offers verified writes for callers that
//! want them.

use std::collections::BTreeSet;

use crate::anytone::codeplug::{
    channel_addr, contact_block_addr, global_offset, group_list_addr, radio_id_addr,
    scan_list_addr, zone_channels_addr, zone_name_addr, Codeplug, BETWEEN_CONTACT_BANKS,
    CHANNEL_BITMAP, CHANNEL_SIZE, CONTACT_BANKS, CONTACT_BITMAP, CONTACT_BLOCK, CONTACT_ID_TABLE,
    CONTACT_INDEX, GROUP_LISTS, GROUP_LIST_BITMAP, GROUP_LIST_ELEMENT, NUM_CHANNELS, NUM_CONTACTS,
    NUM_GROUP_LISTS, NUM_RADIO_IDS, NUM_SCAN_LISTS, NUM_ZONES, RADIO_IDS, RADIO_ID_BITMAP,
    RADIO_ID_SIZE, SCAN_LIST_BITMAP, SCAN_LIST_SIZE, ZONE_BITMAP, ZONE_CHANNELS_SLOT,
    ZONE_CHANNEL_LIST, ZONE_CHANNEL_LIST_SIZE, ZONE_NAME_SLOT,
};
use crate::anytone::protocol::{
    enter_program_mode, exit_program_mode, identify, read_block, write_block, BLOCK_SIZE,
};
use crate::error::{Error, Result};
use crate::transport::Transport;

/// Flash erase granularity. The radio erases in `0x8000`-byte sectors: the first
/// write anywhere in a sector blanks the whole sector to `0xff`, and only the
/// blocks sent afterwards are reprogrammed. [`Radio::write_codeplug`] must
/// therefore reproduce every byte of every sector it touches.
///
/// Derived from vendor CPS USB captures on firmware `D878UV2V101`: comparing what
/// the CPS wrote against what the radio returned on a later read, `0x8000` is the
/// smallest sector size under which every erased-but-not-rewritten block falls in
/// a sector the write touched (2723/2723; `0x4000` explains only 1699). See
/// `docs/diagnosing-cps-breakage.md`. A future firmware could differ.
pub const SECTOR_SIZE: u32 = 0x8000;

/// A contiguous span of radio memory. The codeplug is the ordered concatenation
/// of all regions' contents.
#[derive(Debug, Clone, Copy)]
pub struct Region {
    /// Big-endian start address in radio memory.
    pub address: u32,
    /// Byte length; must be a multiple of [`BLOCK_SIZE`].
    pub length: usize,
}

/// Real D878UVII codeplug region map.
///
/// Addresses/sizes are ported from qdmr's `D868UVCodeplug::Offset` /
/// `::Limit` and the `AnytoneCodeplug::*Element::size()` constants (the D878UV
/// channel element extends the D868UV one and keeps this layout). Cross-checked
/// against `docs/codeplug-memory-map.md`.
///
/// Covered regions (radio address · length · qdmr source):
/// - Channel banks: 0x00800000, 32 banks spaced 0x00040000, 0x2000 (128 ch ×
///   64 B) each — `Offset::channelBanks/betweenChannelBanks`,
///   `Limit::channelsPerBank/numChannels`, `ChannelElement::size` = 0x40.
/// - Zone channel lists: 0x01000000, 250 × 0x200 — `Offset::zoneChannels`,
///   `Size::zoneChannels`, `Limit::numZones`.
/// - Zone valid-bitmap: 0x024c1300, 0x20 — `Offset::zoneBitmap`,
///   `ZoneBitmapElement::size`.
/// - Channel valid-bitmap: 0x024c1500, 0x200 — `Offset::channelBitmap`,
///   `ChannelBitmapElement::size`.
/// - Zone names: 0x02540000, 250 × 0x20 — `Offset::zoneNames`, `Size::zoneName`.
/// - Zone channel list: 0x02500100, 0x400 — `Offset::zoneChannelList`,
///   `ZoneChannelListElement::size`. **Read-only**, see [`ZONE_CHANNEL_LIST`]:
///   it lives in the radio-settings block and writing it locked a radio.
/// - Contact banks: 0x02680000, 10 banks spaced 0x00040000, 0x186a0 (1000
///   contacts × 100 B) each — `Offset::contactBanks/betweenContactBanks`,
///   `Limit::contactsPerBank/numContacts`, `ContactElement::size` = 0x64.
/// - Contact valid-bitmap (inverted): 0x02640000, 0x500 — `Offset::contactBitmap`,
///   `ContactBitmapElement::size`.
/// - Contact index (u32-le list): 0x02600000, 10000 × 4 — `Offset::contactIndex`.
/// - Contact ID table (id→index map): 0x04800000, 10000 × 8 —
///   `ContactMapElement::size` = 0x08; address from the vendor CPS write trace
///   (qdmr's D868UV 0x04340000 is wrong for the II Plus).
/// - RX group lists: 0x02980000, 250 × 0x200 (0x120 element in a 0x200 slot) —
///   `Offset::groupLists/betweenGroupLists`, `GroupListElement::size`.
/// - Group-list valid-bitmap: 0x025c0b10, 0x20 — `Offset::groupListBitmap`,
///   `GroupListBitmapElement::size`.
/// - Radio IDs: 0x02580000, 250 × 0x20 — `Offset::radioIDs`,
///   `RadioIDElement::size`.
/// - Radio-ID valid-bitmap: 0x024c1320, 0x20 — `Offset::radioIDBitmap`,
///   `RadioIDBitmapElement::size`.
///
/// Every length is a multiple of the 16-byte block size (asserted in a test).
///
/// Note: growing this map grows [`codeplug_size`], so codeplug `.bin` files
/// captured before a region was added will no longer parse.
pub const REGIONS: &[Region] = &REGION_TABLE;

/// Number of channel banks (32 banks × 128 channels ≥ 4000 channels).
const CHANNEL_BANKS: usize = 32;
/// Number of contact banks (10 banks × 1000 contacts = 10000 contacts).
const CONTACT_BANK_COUNT: usize = 10;
/// Number of scan-list banks (16 banks × 16 scan lists ≥ 250 scan lists).
const SCAN_LIST_BANK_COUNT: usize = 16;
/// Fixed regions after the channel banks (zones + bitmaps + DMR entities +
/// the scan-list bitmap + the read-only APRS settings block).
const FIXED_REGIONS: usize = 14;

/// Total number of entries in [`REGION_TABLE`].
const NUM_REGIONS: usize =
    CHANNEL_BANKS + CONTACT_BANK_COUNT + SCAN_LIST_BANK_COUNT + FIXED_REGIONS;

/// Backing storage for [`REGIONS`], built at compile time so the channel,
/// contact, and scan-list banks don't have to be written out by hand.
const REGION_TABLE: [Region; NUM_REGIONS] = build_regions();

/// Assemble the region table: the channel banks, then zones and their bitmaps,
/// then the DMR contact banks + reverse-lookup tables, group lists, and radio
/// IDs with their bitmaps.
const fn build_regions() -> [Region; NUM_REGIONS] {
    let mut arr = [Region {
        address: 0,
        length: 0,
    }; NUM_REGIONS];
    let mut i = 0;
    while i < CHANNEL_BANKS {
        arr[i] = Region {
            address: 0x0080_0000 + (i as u32) * 0x0004_0000,
            length: 0x2000,
        };
        i += 1;
    }
    // Zone channel lists: 250 zones × 0x200 bytes.
    arr[CHANNEL_BANKS] = Region {
        address: 0x0100_0000,
        length: 250 * 0x200,
    };
    // Zone valid-bitmap.
    arr[CHANNEL_BANKS + 1] = Region {
        address: 0x024c_1300,
        length: 0x20,
    };
    // Channel valid-bitmap.
    arr[CHANNEL_BANKS + 2] = Region {
        address: 0x024c_1500,
        length: 0x200,
    };
    // Zone names: 250 zones × 0x20 bytes.
    arr[CHANNEL_BANKS + 3] = Region {
        address: 0x0254_0000,
        length: 250 * 0x20,
    };
    // Contact valid-bitmap (inverted: active = bit clear).
    arr[CHANNEL_BANKS + 4] = Region {
        address: 0x0264_0000,
        length: 0x500,
    };
    // Contact index: 10000 × u32-le.
    arr[CHANNEL_BANKS + 5] = Region {
        address: 0x0260_0000,
        length: 10000 * 4,
    };
    // Contact ID table: 10000 × 8-byte id→index map entries. Address per the
    // vendor CPS write trace (0x04800000), not qdmr's D868UV 0x04340000.
    arr[CHANNEL_BANKS + 6] = Region {
        address: 0x0480_0000,
        length: 10000 * 8,
    };
    // RX group lists: 250 × 0x200 slots (0x120 element each).
    arr[CHANNEL_BANKS + 7] = Region {
        address: 0x0298_0000,
        length: 250 * 0x200,
    };
    // Group-list valid-bitmap.
    arr[CHANNEL_BANKS + 8] = Region {
        address: 0x025c_0b10,
        length: 0x20,
    };
    // Radio IDs: 250 × 0x20 bytes.
    arr[CHANNEL_BANKS + 9] = Region {
        address: 0x0258_0000,
        length: 250 * 0x20,
    };
    // Radio-ID valid-bitmap.
    arr[CHANNEL_BANKS + 10] = Region {
        address: 0x024c_1320,
        length: 0x20,
    };
    // Zone channel list: the per-zone selected channel for VFO A and VFO B.
    // Read-only — backups capture it, but write_codeplug must never send it.
    arr[CHANNEL_BANKS + 11] = Region {
        address: ZONE_CHANNEL_LIST,
        length: ZONE_CHANNEL_LIST_SIZE,
    };
    // Scan-list valid-bitmap.
    arr[CHANNEL_BANKS + 12] = Region {
        address: 0x024c_1340,
        length: 0x20,
    };
    // APRS settings block (0x02501000 – 0x02501490). READ-ONLY: captured for
    // display / backup, never written (it lives in the radio-settings block).
    arr[CHANNEL_BANKS + 13] = Region {
        address: 0x0250_1000,
        length: 0x490,
    };
    // Contact banks: 10 banks × 1000 contacts × 0x64.
    let mut b = 0;
    while b < CONTACT_BANK_COUNT {
        arr[CHANNEL_BANKS + FIXED_REGIONS + b] = Region {
            address: 0x0268_0000 + (b as u32) * 0x0004_0000,
            length: 1000 * 0x64,
        };
        b += 1;
    }
    // Scan-list banks: 16 banks × 16 scan lists × 0x200, at 0x40000 stride.
    // Only the first 0x2000 (16 × 0x200) of each 0x40000 bank is mapped.
    let mut s = 0;
    while s < SCAN_LIST_BANK_COUNT {
        arr[CHANNEL_BANKS + FIXED_REGIONS + CONTACT_BANK_COUNT + s] = Region {
            address: 0x0108_0000 + (s as u32) * 0x0004_0000,
            length: 16 * 0x200,
        };
        s += 1;
    }
    arr
}

/// Total number of bytes a full codeplug backup/restore moves: the sum of all
/// region lengths.
pub fn codeplug_size() -> usize {
    REGIONS.iter().map(|r| r.length).sum()
}

/// Owns a [`Transport`] and exposes the high-level programming operations.
pub struct Radio<T: Transport> {
    transport: T,
}

impl<T: Transport> Radio<T> {
    /// Wrap a transport. The caller is responsible for opening the serial port.
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    /// Consume the radio and return the underlying transport (useful in tests).
    pub fn into_transport(self) -> T {
        self.transport
    }

    /// Enter program mode (`"PROGRAM"` handshake). Must precede identify/reads.
    pub fn enter(&mut self) -> Result<()> {
        enter_program_mode(&mut self.transport)
    }

    /// Exit program mode (`"END"` handshake). Call once when finished.
    pub fn exit(&mut self) -> Result<()> {
        exit_program_mode(&mut self.transport)
    }

    /// Send the identify command and return the model/version string. Must be
    /// called while in program mode.
    pub fn identify(&mut self) -> Result<String> {
        identify(&mut self.transport)
    }

    /// Read `length` bytes starting at `addr` as a sequence of 16-byte blocks.
    /// `length` must be a multiple of [`BLOCK_SIZE`].
    pub fn read_region(&mut self, addr: u32, length: usize) -> Result<Vec<u8>> {
        if !length.is_multiple_of(BLOCK_SIZE) {
            return Err(Error::InvalidArgument(format!(
                "region length {length} is not a multiple of {BLOCK_SIZE}"
            )));
        }
        let mut out = Vec::with_capacity(length);
        let mut offset = 0usize;
        while offset < length {
            let block = read_block(&mut self.transport, addr + offset as u32)?;
            out.extend_from_slice(&block);
            offset += BLOCK_SIZE;
        }
        Ok(out)
    }

    /// Write `data` starting at `addr` as 16-byte blocks, reading each block
    /// back immediately and erroring on any mismatch. `data.len()` must be a
    /// multiple of [`BLOCK_SIZE`].
    pub fn write_region(&mut self, addr: u32, data: &[u8]) -> Result<()> {
        if !data.len().is_multiple_of(BLOCK_SIZE) {
            return Err(Error::InvalidArgument(format!(
                "region length {} is not a multiple of {BLOCK_SIZE}",
                data.len()
            )));
        }
        let mut offset = 0usize;
        while offset < data.len() {
            let block_addr = addr + offset as u32;
            let mut block = [0u8; BLOCK_SIZE];
            block.copy_from_slice(&data[offset..offset + BLOCK_SIZE]);
            write_block(&mut self.transport, block_addr, &block)?;
            // Mandatory read-back verification: never trust a write blindly.
            let readback = read_block(&mut self.transport, block_addr)?;
            if readback != block {
                return Err(Error::Verify { addr: block_addr });
            }
            offset += BLOCK_SIZE;
        }
        Ok(())
    }

    /// Read the codeplug into a full-size buffer, but transfer it the way the
    /// radio actually maps its memory: the channels, zones, and every valid
    /// bitmap are read in full (contiguous config RAM), while the DMR data (the
    /// contact banks, RX group lists, and radio IDs) is read **sparsely** —
    /// only the records the bitmaps mark active. This avoids sending block
    /// commands for memory the radio does not map, which otherwise times out
    /// (see [`Error::Transfer`]). The derived contact index / ID table *are*
    /// read in full even though the write path rebuilds them: a region we write
    /// but never read is a region no backup can be diffed against, and no
    /// restore can be verified as a no-op. `progress` is called after each
    /// block. Assumes program mode has already been entered.
    pub fn read_codeplug(&mut self, progress: &mut dyn FnMut(usize, usize)) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; codeplug_size()];

        // Phase 1: read the DMR bitmaps first so the active records are known.
        // They are re-read in the full-region pass below; the few blocks cost
        // nothing and this keeps the reported progress total exact.
        let mut warmup = |_: usize, _: usize| {};
        for bm in [CONTACT_BITMAP, GROUP_LIST_BITMAP, RADIO_ID_BITMAP] {
            let len = region_length(bm).expect("bitmap region is in REGIONS");
            let mut dummy = 0usize;
            self.read_segment(&mut buf, bm, len, &mut dummy, 0, &mut warmup)?;
        }

        // Build the ordered transfer list: full regions, then active DMR records.
        let mut segments: Vec<(u32, usize)> = REGIONS
            .iter()
            .filter(|r| is_full_transfer_region(r.address))
            .map(|r| (r.address, r.length))
            .collect();
        segments.extend(active_transfer_segments(&buf));

        let total: usize = segments.iter().map(|(_, len)| len / BLOCK_SIZE).sum();
        let mut done = 0usize;
        for (addr, len) in segments {
            self.read_segment(&mut buf, addr, len, &mut done, total, progress)?;
        }
        Ok(buf)
    }

    /// Write a codeplug back to the radio, **sector by sector**, preserving every
    /// byte the model does not touch.
    ///
    /// The radio erases flash in [`SECTOR_SIZE`]-byte sectors: the first write
    /// anywhere in a sector blanks the whole sector to `0xff`, and only the blocks
    /// sent afterwards are reprogrammed. An earlier version wrote just the modelled
    /// blocks (bitmaps, active records, the contact index / ID table). That erased
    /// the sectors those blocks live in and destroyed everything else in them —
    /// scan lists, tones, roaming, and undocumented regions — ~43 KB per write,
    /// leaving the vendor CPS unable to parse its own read
    /// (`GetOptionFromCommData_Error`). See `docs/diagnosing-cps-breakage.md`.
    ///
    /// So this reads every sector it is about to touch, overlays the modelled
    /// blocks from `data` onto the sector's *current radio contents*, and writes
    /// the result back. Bytes the model does not know about come from the radio
    /// being written to and survive unchanged. Only blocks that are not entirely
    /// `0xff` are sent — the erase already provides the `0xff` fill — which keeps
    /// the write volume near what the vendor CPS emits rather than blasting every
    /// nominal block (which floods the radio with writes to unused slots and
    /// resets it mid-transfer).
    ///
    /// The radio-settings block (0x02500000 – 0x025014FF) is never modelled, so no
    /// segment lands in its sector and that sector is never read or written.
    ///
    /// Writes are **not** read-back verified: the radio commits on program-mode
    /// exit, so an immediate read returns the pre-commit value and verification
    /// would spuriously fail on every genuine edit. Each block's write ACK
    /// (checked in [`write_block`]) is the transfer check. `data.len()` must equal
    /// [`codeplug_size`]. Semantics: everything the model covers is written from
    /// `data`; everything else is left exactly as the target radio had it.
    pub fn write_codeplug(
        &mut self,
        data: &[u8],
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<()> {
        let expected = codeplug_size();
        if data.len() != expected {
            return Err(Error::InvalidArgument(format!(
                "codeplug is {} bytes, expected {expected}",
                data.len()
            )));
        }

        // Sized reverse-lookup tables, rebuilt from the active contacts in data.
        let cp = Codeplug::parse(data)?;
        let index_bytes = cp.contact_index_bytes();
        let idtable_bytes = cp.contact_id_table_bytes();

        // Assemble every modelled write segment as (address, bytes): the valid
        // bitmaps, the active records, and the sized contact index / ID table.
        let mut segments: Vec<(u32, Vec<u8>)> = Vec::new();
        for bm in [
            CHANNEL_BITMAP,
            ZONE_BITMAP,
            SCAN_LIST_BITMAP,
            CONTACT_BITMAP,
            GROUP_LIST_BITMAP,
            RADIO_ID_BITMAP,
        ] {
            let len = region_length(bm).expect("bitmap region is in REGIONS");
            let off = global_offset(bm).expect("bitmap address is mapped");
            segments.push((bm, data[off..off + len].to_vec()));
        }
        for (addr, len) in active_record_segments(data) {
            let off = global_offset(addr)
                .ok_or_else(|| Error::InvalidArgument(format!("address 0x{addr:08X} unmapped")))?;
            segments.push((addr, data[off..off + len].to_vec()));
        }
        if !index_bytes.is_empty() {
            segments.push((CONTACT_INDEX, index_bytes));
        }
        if !idtable_bytes.is_empty() {
            segments.push((CONTACT_ID_TABLE, idtable_bytes));
        }

        // Every 32 KB sector any segment falls in, ascending. A segment may span
        // a sector boundary, so a run is [first_sector ..= last_sector].
        let sector_blocks = SECTOR_SIZE as usize / BLOCK_SIZE;
        let mut sectors: BTreeSet<u32> = BTreeSet::new();
        for (addr, bytes) in &segments {
            let last = addr + (bytes.len() as u32 - 1);
            for s in (addr / SECTOR_SIZE)..=(last / SECTOR_SIZE) {
                sectors.insert(s);
            }
        }

        // Phase 1: read each touched sector off the radio, overlay the modelled
        // blocks, and collect the non-0xff blocks that need writing. Reading
        // before writing is essential — the read captures the pre-erase contents
        // this restore must preserve.
        let read_total = sectors.len() * sector_blocks;
        let mut done = 0usize;
        let mut pending: Vec<(u32, [u8; BLOCK_SIZE])> = Vec::new();
        for &sector in &sectors {
            let base = sector * SECTOR_SIZE;
            let mut buf = vec![0u8; SECTOR_SIZE as usize];
            let mut o = 0usize;
            while o < buf.len() {
                let a = base + o as u32;
                let block = read_block(&mut self.transport, a).map_err(|e| at(a, e))?;
                buf[o..o + BLOCK_SIZE].copy_from_slice(&block);
                o += BLOCK_SIZE;
                done += 1;
                progress(done, read_total);
            }
            for (addr, bytes) in &segments {
                let seg_end = addr + bytes.len() as u32;
                let lo = (*addr).max(base);
                let hi = seg_end.min(base + SECTOR_SIZE);
                if lo < hi {
                    let dst = (lo - base) as usize;
                    let src = (lo - addr) as usize;
                    let n = (hi - lo) as usize;
                    buf[dst..dst + n].copy_from_slice(&bytes[src..src + n]);
                }
            }
            for b in 0..sector_blocks {
                let blk = &buf[b * BLOCK_SIZE..(b + 1) * BLOCK_SIZE];
                if blk.iter().any(|&x| x != 0xff) {
                    let mut arr = [0u8; BLOCK_SIZE];
                    arr.copy_from_slice(blk);
                    pending.push((base + (b * BLOCK_SIZE) as u32, arr));
                }
            }
        }

        // Phase 2: write the collected blocks. The first write into each sector
        // triggers the radio's erase; the 0xff blocks we skipped are exactly the
        // fill that erase leaves behind.
        let total = read_total + pending.len();
        for (addr, block) in pending {
            write_block(&mut self.transport, addr, &block).map_err(|e| at(addr, e))?;
            done += 1;
            progress(done, total);
        }
        Ok(())
    }

    /// Read `len` bytes at `addr` block-by-block into `buf` at the address's
    /// global offset, wrapping any IO failure with the failing block address.
    fn read_segment(
        &mut self,
        buf: &mut [u8],
        addr: u32,
        len: usize,
        done: &mut usize,
        total: usize,
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<()> {
        let off = global_offset(addr)
            .ok_or_else(|| Error::InvalidArgument(format!("address 0x{addr:08X} unmapped")))?;
        let mut o = 0usize;
        while o < len {
            let a = addr + o as u32;
            let block = read_block(&mut self.transport, a).map_err(|e| at(a, e))?;
            buf[off + o..off + o + BLOCK_SIZE].copy_from_slice(&block);
            o += BLOCK_SIZE;
            *done += 1;
            progress(*done, total);
        }
        Ok(())
    }
}

/// True for regions transferred in full: channels, zones, and every valid
/// bitmap (contiguous config RAM the radio always services). The DMR *data*
/// regions are not here — they are transferred sparsely.
fn is_full_transfer_region(addr: u32) -> bool {
    !is_sparse_data_region(addr)
}

/// True for the DMR data regions that must not be sent as fixed full blocks: the
/// contact banks, RX group lists, and radio IDs. Their bitmaps are deliberately
/// excluded (bitmaps transfer in full).
///
/// The contact index and contact ID table are **not** listed here, so they read
/// in full. They are derived tables the write path rebuilds, and for a long time
/// they were written but never read — which meant every `.bin` this tool produced
/// held zeros there that came from [`read_codeplug`]'s own allocation rather than
/// from the radio. That silent gap turned a backup into a liar exactly where a
/// diff was needed, and cost an entire debugging session. Reading them costs
/// ~7500 blocks (~4s) and is what keeps
/// `every_written_range_is_also_read` honest.
fn is_sparse_data_region(addr: u32) -> bool {
    if addr == GROUP_LISTS || addr == RADIO_IDS {
        return true;
    }
    (0..10u32).any(|b| addr == CONTACT_BANKS + b * BETWEEN_CONTACT_BANKS)
}

/// The active *record* write segments for `data` (no bitmaps): only the
/// channels, zones, and DMR families the bitmaps mark active. Mirrors qdmr's
/// `allocateForEncoding` so the radio is never flooded with writes to unused
/// slots. Bitmaps and the sized contact index / ID table are added by the
/// caller, which then writes everything in address order.
fn active_record_segments(data: &[u8]) -> Vec<(u32, usize)> {
    let mut segments = Vec::new();

    // Active channels (one 0x40 element each).
    for i in 0..NUM_CHANNELS {
        if bitmap_active(data, CHANNEL_BITMAP, i, false) {
            segments.push((channel_addr(i), CHANNEL_SIZE));
        }
    }
    // Active zones (name slot + channel-list slot each).
    for i in 0..NUM_ZONES {
        if bitmap_active(data, ZONE_BITMAP, i, false) {
            segments.push((zone_name_addr(i), ZONE_NAME_SLOT));
            segments.push((zone_channels_addr(i), ZONE_CHANNELS_SLOT));
        }
    }

    // Active scan lists (one 0x90 element each).
    for i in 0..NUM_SCAN_LISTS {
        if bitmap_active(data, SCAN_LIST_BITMAP, i, false) {
            segments.push((scan_list_addr(i), SCAN_LIST_SIZE));
        }
    }

    // Active DMR records (contacts / group lists / radio IDs).
    segments.extend(active_transfer_segments(data));
    segments
}

/// The sparse DMR transfer segments implied by the bitmaps in `buf`: one
/// [`CONTACT_BLOCK`] per distinct block holding an active contact, one element
/// per active group list, and one record per active radio ID.
fn active_transfer_segments(buf: &[u8]) -> Vec<(u32, usize)> {
    let mut segments = Vec::new();

    // Contacts share 4-per-block storage; transfer each distinct block once.
    let mut blocks = BTreeSet::new();
    for i in 0..NUM_CONTACTS {
        if bitmap_active(buf, CONTACT_BITMAP, i, true) && blocks.insert(contact_block_addr(i)) {
            segments.push((contact_block_addr(i), CONTACT_BLOCK));
        }
    }
    for i in 0..NUM_GROUP_LISTS {
        if bitmap_active(buf, GROUP_LIST_BITMAP, i, false) {
            segments.push((group_list_addr(i), GROUP_LIST_ELEMENT));
        }
    }
    for i in 0..NUM_RADIO_IDS {
        if bitmap_active(buf, RADIO_ID_BITMAP, i, false) {
            segments.push((radio_id_addr(i), RADIO_ID_SIZE));
        }
    }
    segments
}

/// Test bit `i` of the bitmap at `bitmap_addr` within `buf`. For an `inverted`
/// bitmap (the contact bitmap) "active" means the bit is clear.
fn bitmap_active(buf: &[u8], bitmap_addr: u32, i: usize, inverted: bool) -> bool {
    let Some(base) = global_offset(bitmap_addr) else {
        return false;
    };
    let set = (buf[base + i / 8] >> (i % 8)) & 1 == 1;
    set != inverted
}

/// Length of the region at radio address `addr`, or `None` if not in [`REGIONS`].
fn region_length(addr: u32) -> Option<usize> {
    REGIONS.iter().find(|r| r.address == addr).map(|r| r.length)
}

/// Annotate a block-transfer error with the failing radio address. Errors that
/// already carry an address (verify/checksum/transfer) pass through unchanged;
/// anything else (notably an IO timeout when the radio does not map the address)
/// is wrapped in [`Error::Transfer`] so the failing block is visible.
fn at(addr: u32, e: Error) -> Error {
    match e {
        Error::Verify { .. } | Error::Checksum { .. } | Error::Transfer { .. } => e,
        other => Error::Transfer {
            addr,
            message: other.to_string(),
        },
    }
}

/// Best-effort check that an identify string looks like a D878UV-family radio.
/// Used as a guard before writing so a codeplug is not pushed to a mismatched
/// model.
pub fn is_supported_model(identify_string: &str) -> bool {
    identify_string.contains("878UV")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anytone::mock::MockTransport;

    /// Build a mock whose flat memory image is large enough to cover the
    /// highest-addressed region in [`REGIONS`].
    fn mock() -> MockTransport {
        let end = REGIONS
            .iter()
            .map(|r| r.address as usize + r.length)
            .max()
            .unwrap();
        MockTransport::new(end, "ID878UV V100")
    }

    /// Pin the AnyTone image size as a bare literal. Every later change that
    /// touches the region table has to justify moving this number, and the
    /// Swift side hardcodes the same value in its compatibility check.
    #[test]
    fn anytone_codeplug_size_is_pinned() {
        assert_eq!(codeplug_size(), 1_789_328);
    }

    #[test]
    fn every_region_length_is_block_aligned() {
        for r in REGIONS {
            assert_eq!(
                r.length % BLOCK_SIZE,
                0,
                "region at 0x{:08x} length {} is not a multiple of {BLOCK_SIZE}",
                r.address,
                r.length
            );
        }
    }

    #[test]
    fn regions_do_not_overlap() {
        // Sort a copy by address and check each region ends before the next starts.
        let mut spans: Vec<(u32, u32)> = REGIONS
            .iter()
            .map(|r| (r.address, r.address + r.length as u32))
            .collect();
        spans.sort_by_key(|s| s.0);
        for pair in spans.windows(2) {
            assert!(
                pair[0].1 <= pair[1].0,
                "region ending at 0x{:08x} overlaps region starting at 0x{:08x}",
                pair[0].1,
                pair[1].0
            );
        }
    }

    #[test]
    fn identify_guard_accepts_known_and_rejects_unknown() {
        assert!(is_supported_model("ID878UV V100"));
        assert!(!is_supported_model("ID578UV"));
    }

    #[test]
    fn read_region_rejects_unaligned_length() {
        let mut r = Radio::new(mock());
        let err = r.read_region(0, 17).unwrap_err();
        assert!(matches!(err, Error::InvalidArgument(_)));
    }

    /// An empty codeplug image: all zero, but with the inverted contact bitmap
    /// set so no contacts read as active.
    fn empty_image() -> Vec<u8> {
        let mut raw = vec![0u8; codeplug_size()];
        let off = global_offset(CONTACT_BITMAP).unwrap();
        for b in raw[off..off + 0x500].iter_mut() {
            *b = 0xff;
        }
        raw
    }

    /// Every byte range `write_codeplug` sends, as (start, end) radio addresses.
    /// Mirrors the segment assembly in `write_codeplug` — including the derived
    /// contact tables, which are computed rather than copied but still land on
    /// the radio. Leaving them out here once made
    /// `every_written_range_is_also_read` pass while the very ranges it exists to
    /// catch were being written blind.
    fn write_ranges(data: &[u8]) -> Vec<(u32, u32)> {
        let cp = Codeplug::parse(data).unwrap();
        let mut v: Vec<(u32, u32)> = Vec::new();
        for bm in [
            CHANNEL_BITMAP,
            ZONE_BITMAP,
            SCAN_LIST_BITMAP,
            CONTACT_BITMAP,
            GROUP_LIST_BITMAP,
            RADIO_ID_BITMAP,
        ] {
            let len = region_length(bm).unwrap();
            v.push((bm, bm + len as u32));
        }
        for (addr, len) in active_record_segments(data) {
            v.push((addr, addr + len as u32));
        }
        let index_bytes = cp.contact_index_bytes();
        if !index_bytes.is_empty() {
            v.push((CONTACT_INDEX, CONTACT_INDEX + index_bytes.len() as u32));
        }
        let idtable_bytes = cp.contact_id_table_bytes();
        if !idtable_bytes.is_empty() {
            v.push((
                CONTACT_ID_TABLE,
                CONTACT_ID_TABLE + idtable_bytes.len() as u32,
            ));
        }
        v
    }

    /// Every byte range `read_codeplug` fetches, as (start, end) radio
    /// addresses. Mirrors the segment assembly in `read_codeplug`.
    fn read_ranges(buf: &[u8]) -> Vec<(u32, u32)> {
        let mut v: Vec<(u32, u32)> = REGIONS
            .iter()
            .filter(|r| is_full_transfer_region(r.address))
            .map(|r| (r.address, r.address + r.length as u32))
            .collect();
        for (addr, len) in active_transfer_segments(buf) {
            v.push((addr, addr + len as u32));
        }
        v.sort();
        v
    }

    #[test]
    fn every_written_range_is_also_read() {
        // Necessary but no longer sufficient: `write_codeplug` now preserves
        // untouched bytes by sector read-modify-write, so a modelled range that
        // was not in the read path no longer corrupts the radio. It does still
        // corrupt the *backup* — a range we write but never read comes back as
        // our own zero-fill, so it cannot be diffed or verified. Keep every
        // modelled write range inside what `read_codeplug` fetches.
        let mut cp = Codeplug::parse(&empty_image()).unwrap();
        let ci = cp.add_channel().unwrap();
        cp.channel_mut(ci).unwrap().set_name("REPEATER");
        let zi = cp.add_zone().unwrap();
        cp.zone_mut(zi).unwrap().set_members(&[ci as u16]);
        let ct = cp.add_contact().unwrap();
        cp.set_contact_number(ct, 310);
        let gl = cp.add_group_list().unwrap();
        cp.group_list_mut(gl).unwrap().set_name("STATE");
        cp.add_radio_id().unwrap();
        let sl = cp.add_scan_list().unwrap();
        cp.scan_list_mut(sl).unwrap().set_members(&[ci as u16]);
        let data = cp.serialize();

        let reads = read_ranges(&data);
        let uncovered: Vec<(u32, u32)> = write_ranges(&data)
            .into_iter()
            .filter(|&(s, e)| !reads.iter().any(|&(rs, re)| rs <= s && e <= re))
            .collect();

        assert!(
            uncovered.is_empty(),
            "these ranges are written but never read, so a backup cannot verify them: {:x?}",
            uncovered
        );
    }

    #[test]
    fn mock_erases_a_whole_sector_on_first_write() {
        // Model fidelity. The radio erases flash in SECTOR_SIZE sectors: the
        // first write into a sector blanks the rest of it. Without the mock
        // reproducing this, no test could catch a write that destroys bytes it
        // never sent — which is exactly the bug that shipped.
        let mut t = MockTransport::new(SECTOR_SIZE as usize * 2, "ID878UV");
        // Pre-existing data: one byte-string in sector 0, one in sector 1.
        t.seed(0x0100, b"erase me on write");
        t.seed(SECTOR_SIZE as usize, b"other sector safe");
        let mut radio = Radio::new(t);
        radio.enter().unwrap();
        // Write a single block at the start of sector 0.
        radio.write_region(0x0000, &[0xAAu8; BLOCK_SIZE]).unwrap();
        radio.exit().unwrap();
        let mem = radio.into_transport();

        // The block we wrote is programmed.
        assert_eq!(&mem.memory()[0..BLOCK_SIZE], &[0xAAu8; BLOCK_SIZE]);
        // Everything else in sector 0 was erased to 0xff by that first write.
        assert_eq!(&mem.memory()[0x0100..0x0100 + 17], &[0xffu8; 17]);
        // A different sector is untouched.
        assert_eq!(
            &mem.memory()[SECTOR_SIZE as usize..SECTOR_SIZE as usize + 17],
            b"other sector safe"
        );
    }

    #[test]
    fn write_codeplug_preserves_bytes_outside_the_model() {
        // The regression test for the whole investigation. A write must leave
        // untouched every byte the model does not cover, including bytes that
        // merely share a 32 KB sector with something we do write. Pre-fix, the
        // sector erase destroyed ~43 KB of vendor data per write.
        let mut cp = Codeplug::parse(&empty_image()).unwrap();
        let ci = cp.add_channel().unwrap();
        cp.channel_mut(ci).unwrap().set_name("REPEATER");
        let zi = cp.add_zone().unwrap();
        cp.zone_mut(zi).unwrap().set_members(&[ci as u16]);
        let ct = cp.add_contact().unwrap();
        cp.set_contact_number(ct, 310);
        cp.add_group_list().unwrap();
        cp.add_radio_id().unwrap();
        let data = cp.serialize();

        // Addresses in sectors write_codeplug touches but the model never
        // covers: 0x024c4000 shares a sector with the channel/zone/radio-ID
        // bitmaps at 0x024c1300..0x024c1500; 0x025c0000 shares a sector with the
        // group-list bitmap at 0x025c0b10. Seed each sentinel's whole sector to
        // 0xff (erased flash) first, so surviving requires active preservation,
        // not coincidence with the mock's zero-fill.
        const SENTINEL: &[u8; 16] = b"DO NOT ERASE ME!";
        let sentinels = [0x024c_4000u32, 0x025c_0000u32];
        let mut transport = mock();
        for &s in &sentinels {
            let base = (s / SECTOR_SIZE * SECTOR_SIZE) as usize;
            transport.seed(base, &vec![0xffu8; SECTOR_SIZE as usize]);
            transport.seed(s as usize, SENTINEL);
        }

        let mut radio = Radio::new(transport);
        radio.enter().unwrap();
        let mut noop = |_: usize, _: usize| {};
        radio.write_codeplug(&data, &mut noop).unwrap();
        radio.exit().unwrap();
        let mem = radio.into_transport();

        for &s in &sentinels {
            assert_eq!(
                &mem.memory()[s as usize..s as usize + 16],
                SENTINEL,
                "byte at 0x{s:08x} is outside the model and must survive a write"
            );
        }
        // And the modelled data did land: the channel bitmap marks channel 0.
        let cbm = CHANNEL_BITMAP as usize;
        assert_eq!(
            mem.memory()[cbm] & 1,
            1,
            "channel 0 bitmap bit should be set"
        );
    }

    #[test]
    fn sparse_codeplug_roundtrips_all_entities() {
        // Build a realistic codeplug (a channel, a zone, a contact, a group
        // list, a radio ID) via the model, write it sparsely, read it back, and
        // confirm every entity survives the round trip.
        let mut cp = Codeplug::parse(&empty_image()).unwrap();
        let ci = cp.add_channel().unwrap();
        cp.channel_mut(ci).unwrap().set_name("REPEATER");
        cp.channel_mut(ci).unwrap().set_rx_frequency(146_940_000);
        let ct = cp.add_contact().unwrap();
        cp.set_contact_name(ct, "TAC 310");
        cp.set_contact_number(ct, 310);
        let gl = cp.add_group_list().unwrap();
        cp.group_list_mut(gl).unwrap().set_name("STATE");
        cp.group_list_mut(gl).unwrap().set_members(&[ct as u32]);
        let rid = cp.add_radio_id().unwrap();
        cp.radio_id_mut(rid).unwrap().set_number(3_141_592);
        let plug = cp.serialize();

        let mut radio = Radio::new(mock());
        radio.enter().unwrap();
        let mut noop = |_: usize, _: usize| {};
        radio.write_codeplug(&plug, &mut noop).unwrap();
        let back = radio.read_codeplug(&mut noop).unwrap();
        radio.exit().unwrap();

        let got = Codeplug::parse(&back).unwrap();
        assert_eq!(got.channels().next().unwrap().name, "REPEATER");
        assert_eq!(got.channels().next().unwrap().rx_frequency_hz, 146_940_000);
        let c = got.contacts().next().unwrap();
        assert_eq!((c.name.as_str(), c.number), ("TAC 310", 310));
        assert_eq!(got.group_lists().next().unwrap().members, vec![ct as u32]);
        assert_eq!(got.radio_ids().next().unwrap().number, 3_141_592);
    }

    #[test]
    fn write_codeplug_never_writes_the_radio_settings_block() {
        // 0x02500000 – 0x025014FF holds the power-on password flag and the menu
        // language, and its D878UVII layout is unverified. Writing one region
        // inside it (the zone channel list at 0x02500100) locked a radio behind
        // an unknown power-on password with its menus switched to Chinese. No
        // write segment may land in this range.
        const SETTINGS_START: u32 = 0x0250_0000;
        const SETTINGS_END: u32 = 0x0250_1500;

        let mut cp = Codeplug::parse(&empty_image()).unwrap();
        cp.add_channel().unwrap();
        let ci = cp.add_channel().unwrap();
        let zi = cp.add_zone().unwrap();
        cp.zone_mut(zi).unwrap().set_members(&[ci as u16]);
        cp.add_contact();
        cp.add_group_list().unwrap();
        cp.add_radio_id().unwrap();
        let plug = cp.serialize();

        // Seed the settings block with a sentinel standing in for the operator's
        // real settings; a write that reaches it will not leave 0xa5 behind.
        let sentinel = vec![0xa5u8; (SETTINGS_END - SETTINGS_START) as usize];
        let mut transport = mock();
        transport.seed(SETTINGS_START as usize, &sentinel);

        let mut radio = Radio::new(transport);
        radio.enter().unwrap();
        let mut noop = |_: usize, _: usize| {};
        radio.write_codeplug(&plug, &mut noop).unwrap();
        radio.exit().unwrap();

        let memory = radio.into_transport();
        let range = SETTINGS_START as usize..SETTINGS_END as usize;
        assert_eq!(&memory.memory()[range], &sentinel[..]);
    }

    #[test]
    fn write_codeplug_rejects_wrong_size() {
        let mut radio = Radio::new(mock());
        radio.enter().unwrap();
        let mut noop = |_: usize, _: usize| {};
        let err = radio.write_codeplug(&[0u8; 3], &mut noop).unwrap_err();
        assert!(matches!(err, Error::InvalidArgument(_)));
    }

    #[test]
    fn empty_codeplug_never_touches_the_contact_id_table() {
        // With no contacts, the far contact ID table (0x04340000) must not be
        // written at all — that region is the one the radio does not map, and
        // writing it is what caused the IO timeout.
        let mut radio = Radio::new(mock());
        radio.enter().unwrap();
        let mut noop = |_: usize, _: usize| {};
        radio.write_codeplug(&empty_image(), &mut noop).unwrap();
        // The mock starts zeroed; a bare read of the id table stays zero, i.e.
        // nothing was written there.
        let block = radio.read_region(CONTACT_ID_TABLE, BLOCK_SIZE).unwrap();
        assert_eq!(block, vec![0u8; BLOCK_SIZE]);
        radio.exit().unwrap();
    }

    #[test]
    fn progress_reports_every_transferred_block() {
        let mut radio = Radio::new(mock());
        radio.enter().unwrap();
        let mut count = 0usize;
        let mut last_total = 0usize;
        {
            let mut cb = |done: usize, total: usize| {
                count = done;
                last_total = total;
            };
            radio.read_codeplug(&mut cb).unwrap();
        }
        // The last callback reports the final block, matching the total.
        assert!(last_total > 0);
        assert_eq!(count, last_total);
    }
}
