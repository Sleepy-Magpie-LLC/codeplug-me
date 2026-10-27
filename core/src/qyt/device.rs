//! High-level QYT KT-5000 image transfer: read the whole image, write the
//! writable part of it back.
//!
//! The KT-5000 memory map is flat, so unlike the AnyTone there is no region
//! table, no bitmap, and no sector read-modify-write. A read walks `0x0000` to
//! `0x31FF` in `0x40` steps; a write walks `0x0000` to `0x30FF` in `0x10`
//! steps. Both are preceded by the `0x3DF0` prologue probe.
//!
//! # The radio reboots after every transfer
//!
//! There is no exit command, and the radio resets itself once the port closes.
//! That means **no in-session read-back verification of a write is possible**,
//! and none is attempted below. The vendor CPS does not attempt one either.
//! Verifying a write means opening a fresh session after the radio comes back
//! and diffing the image. Do not add a read-back loop here; it will time out.

use crate::error::{Error, Result};
use crate::qyt::protocol::{
    end_read_session, read_block, session_prologue, write_block, READ_LEN, WRITE_LEN,
};
use crate::transport::Transport;

/// Size of the full image the radio exposes: `0x0000` through `0x31FF`.
pub const IMAGE_SIZE: usize = 0x3200;

/// First address never written. `0x3100`-`0x31FF` is read into the image so
/// backups stay faithful and diffable, but the vendor CPS never writes it and
/// neither do we. Corroborated twice over: CHIRP's `UPLOAD_MEM_SIZE` is
/// `0x3100`, and the capture's 784 writes at `0x10` each land exactly here.
pub const WRITE_LIMIT: u16 = 0x3100;

/// Number of read commands a full image read issues. One more than
/// `IMAGE_SIZE / READ_LEN` because the vendor CPS reads `0x0000` twice; see
/// [`read_image`].
pub const READ_BLOCKS: usize = IMAGE_SIZE / READ_LEN as usize + 1;

/// First address of the region the write path preserves, on top of everything
/// at or above [`WRITE_LIMIT`].
///
/// `0x2000`-`0x23EF` is real, persistent memory that nothing in this codebase
/// models: high-entropy 32-byte blocks followed by small per-band tables, most
/// likely calibration. Because no edit can ever produce a meaningful value
/// there, the only two things a write can do to it are preserve it (when the
/// image came from a read of this same radio) or destroy it (when it came from
/// anywhere else). On 2026-09-09 an image whose bytes there were `0xFF` blanked
/// all of it on a real radio. Skipping the range makes that outcome impossible.
///
/// Restoring the region from a backup is still possible, deliberately through
/// the explicit-address path instead: `codeplugme-cli rescue write --address 0x2000`.
pub const PRESERVED_START: usize = 0x2000;

/// First address past the preserved region. Block-aligned, so the write loop
/// skips whole blocks; `preserved_range_is_block_aligned` pins that.
pub const PRESERVED_END: usize = 0x23F0;

/// Number of write commands a full image write issues.
///
/// 784 minus the 63 blocks inside [`PRESERVED_START`]..[`PRESERVED_END`], plus
/// the one sacrificial block described at [`write_image`]. The vendor CPS sends
/// exactly 784; `core/tests/qyt_capture.rs` pins that these are the only two
/// differences from its stream.
pub const WRITE_BLOCKS: usize =
    (WRITE_LIMIT as usize - (PRESERVED_END - PRESERVED_START)) / WRITE_LEN as usize + 1;

/// Whether `addr` falls in the region a full-image write leaves alone.
pub const fn is_preserved(addr: usize) -> bool {
    addr >= PRESERVED_START && addr < PRESERVED_END
}

/// Read the radio's full `0x3200`-byte image. `progress` is called after each
/// block with `(blocks_done, total_blocks)`.
///
/// The duplicated `0x0000` read is deliberate. The vendor CPS issues it, and
/// reproducing its host stream byte for byte is the cheapest correctness
/// argument available for hardware we cannot unit-test against. A second
/// 64-byte read costs nothing.
pub fn read_image(
    t: &mut dyn Transport,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<Vec<u8>> {
    session_prologue(t)?;

    let step = READ_LEN as usize;
    let mut image = vec![0u8; IMAGE_SIZE];
    let mut buf = [0u8; READ_LEN as usize];
    let mut done = 0usize;

    read_block(t, 0, READ_LEN, &mut buf)?;
    image[..step].copy_from_slice(&buf);
    done += 1;
    progress(done, READ_BLOCKS);

    for addr in (0..IMAGE_SIZE).step_by(step) {
        read_block(t, addr as u16, READ_LEN, &mut buf)?;
        image[addr..addr + step].copy_from_slice(&buf);
        done += 1;
        progress(done, READ_BLOCKS);
    }

    end_read_session(t)?;
    Ok(image)
}

/// Write `data` back to the radio, covering `0x0000` through `0x30FF` only.
/// `progress` is called after each acknowledged block.
///
/// `data` must be a full [`IMAGE_SIZE`] image even though the tail is not sent,
/// so that callers always hand over the same shape they got from
/// [`read_image`].
///
/// Two deliberate divergences from the vendor CPS, both pinned by
/// `core/tests/qyt_capture.rs`:
///
/// - `0x2000`-`0x23EF` is skipped entirely and keeps whatever the radio already
///   holds. See [`PRESERVED_START`] for why.
/// - The first block, `0x0000`, is sent twice. The radio acks the second write
///   of a session but does not keep it. Before this, that was always `0x0010`
///   (channel 1's frequencies), and every verify after a write reported 8
///   bytes there (2026-10-16). Re-sending identical bytes is idempotent, so the
///   duplicate absorbs the drop and every real block lands.
///
/// Everything else in `data` is sent verbatim, so the caller owns every byte
/// below `0x2000` and from `0x23F0` to [`WRITE_LIMIT`]. Unlike the AnyTone
/// there is no read-modify-write here: the image must already be a faithful
/// starting point, which in practice means a read of the radio being written.
pub fn write_image(
    t: &mut dyn Transport,
    data: &[u8],
    progress: &mut dyn FnMut(usize, usize),
) -> Result<()> {
    if data.len() != IMAGE_SIZE {
        return Err(Error::InvalidArgument(format!(
            "QYT image is {} bytes, expected {IMAGE_SIZE}",
            data.len()
        )));
    }

    session_prologue(t)?;

    let step = WRITE_LEN as usize;
    write_block(t, 0, &data[..step])?;
    let mut done = 1usize;
    progress(done, WRITE_BLOCKS);
    for addr in (0..WRITE_LIMIT as usize).step_by(step) {
        if is_preserved(addr) {
            continue;
        }
        write_block(t, addr as u16, &data[addr..addr + step])?;
        done += 1;
        progress(done, WRITE_BLOCKS);
    }

    // No read-back verification here, by design. See the module docs.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qyt::mock::QytMock;
    use crate::qyt::protocol::{handshake, PROLOGUE_ADDR};

    /// Drive a full read against a mock seeded with a recognisable pattern.
    fn seeded_mock() -> QytMock {
        let mut m = QytMock::new();
        let pattern: Vec<u8> = (0..IMAGE_SIZE).map(|i| (i % 251) as u8).collect();
        m.seed(0, &pattern);
        m
    }

    #[test]
    fn block_counts_match_the_vendor_capture() {
        assert_eq!(READ_BLOCKS, 201);
        // The vendor sends 784; we skip the 63 blocks of the preserved window
        // and send 0x0000 one extra time to absorb the dropped second write.
        assert_eq!(WRITE_BLOCKS, 784 - 63 + 1);
    }

    #[test]
    fn preserved_range_is_block_aligned() {
        assert_eq!(PRESERVED_START % WRITE_LEN as usize, 0);
        assert_eq!(PRESERVED_END % WRITE_LEN as usize, 0);
        assert!(PRESERVED_END <= WRITE_LIMIT as usize);
        assert!(is_preserved(0x2000) && is_preserved(0x23E0));
        assert!(!is_preserved(0x1FF0) && !is_preserved(0x23F0));
    }

    #[test]
    fn read_image_issues_201_reads_plus_the_prologue_and_trailing_stx() {
        let mut m = seeded_mock();
        let image = read_image(&mut m, &mut |_, _| {}).unwrap();

        assert_eq!(image.len(), IMAGE_SIZE);
        assert_eq!(&image[..], &m.memory()[..IMAGE_SIZE]);

        // Prologue plus 201 data reads.
        assert_eq!(m.reads.len(), READ_BLOCKS + 1);
        assert_eq!(m.reads[0], (PROLOGUE_ADDR, 0x10));
        let data_reads: Vec<u16> = m.reads[1..].iter().map(|&(a, _)| a).collect();
        assert!(m.reads[1..].iter().all(|&(_, len)| len == READ_LEN));

        // The duplicated 0x0000, then a clean 0x40 stride to 0x31C0.
        assert_eq!(data_reads[0], 0x0000);
        assert_eq!(data_reads[1], 0x0000);
        let mut expected = vec![0x0000u16];
        expected.extend((0..IMAGE_SIZE).step_by(0x40).map(|a| a as u16));
        assert_eq!(data_reads, expected);
        assert_eq!(*data_reads.last().unwrap(), 0x31C0);

        // The session closes with one bare 0x06.
        assert_eq!(*m.written.last().unwrap(), 0x06);
    }

    #[test]
    fn read_image_reports_progress_for_every_block() {
        let mut m = seeded_mock();
        let mut seen = Vec::new();
        read_image(&mut m, &mut |done, total| seen.push((done, total))).unwrap();
        assert_eq!(seen.len(), READ_BLOCKS);
        assert_eq!(seen.first(), Some(&(1, READ_BLOCKS)));
        assert_eq!(seen.last(), Some(&(READ_BLOCKS, READ_BLOCKS)));
    }

    #[test]
    fn write_image_issues_a_0x10_stride_over_everything_but_the_preserved_window() {
        let mut m = QytMock::new();
        let data = vec![0x42u8; IMAGE_SIZE];
        write_image(&mut m, &data, &mut |_, _| {}).unwrap();

        assert_eq!(m.writes.len(), WRITE_BLOCKS);
        assert!(m.writes.iter().all(|&(_, len)| len == WRITE_LEN as usize));
        let addrs: Vec<u16> = m.writes.iter().map(|&(a, _)| a).collect();
        // 0x0000 twice, then the plain stride.
        let mut expected = vec![0x0000u16];
        expected.extend(
            (0..WRITE_LIMIT)
                .step_by(WRITE_LEN as usize)
                .filter(|&a| !is_preserved(a as usize)),
        );
        assert_eq!(addrs, expected);
        assert_eq!(*addrs.last().unwrap(), 0x30F0);

        // The prologue is the only read a write session performs.
        assert_eq!(m.reads, vec![(PROLOGUE_ADDR, 0x10)]);
    }

    #[test]
    fn qyt_write_never_touches_0x3100_or_above() {
        let mut m = QytMock::new();
        // A sentinel above the write limit that a stray write would clobber.
        m.seed(0x3100, &[0xAAu8; 0x100]);
        let data = vec![0x00u8; IMAGE_SIZE];
        write_image(&mut m, &data, &mut |_, _| {}).unwrap();

        assert!(m.writes.iter().all(|&(addr, _)| addr < WRITE_LIMIT));
        assert_eq!(&m.memory()[0x3100..0x3200], &[0xAAu8; 0x100]);
        assert!(m.memory()[..PRESERVED_START].iter().all(|&b| b == 0x00));
        assert!(m.memory()[PRESERVED_END..0x3100].iter().all(|&b| b == 0x00));
    }

    /// The regression this whole window exists for: an image carrying `0xFF`
    /// there must not be able to erase what the radio holds.
    #[test]
    fn qyt_write_leaves_the_calibration_window_exactly_as_the_radio_had_it() {
        let mut m = QytMock::new();
        let calibration: Vec<u8> = (PRESERVED_START..PRESERVED_END)
            .map(|i| (i % 251) as u8)
            .collect();
        m.seed(PRESERVED_START, &calibration);

        // A stale image: erased where the radio holds calibration, which is what
        // any backup taken before that region was populated looks like.
        let mut data = vec![0x00u8; IMAGE_SIZE];
        for b in &mut data[PRESERVED_START..PRESERVED_END] {
            *b = 0xFF;
        }
        write_image(&mut m, &data, &mut |_, _| {}).unwrap();

        assert!(m
            .writes
            .iter()
            .all(|&(addr, _)| !is_preserved(addr as usize)));
        assert_eq!(&m.memory()[PRESERVED_START..PRESERVED_END], &calibration[..]);
    }

    /// The 2026-10-16 regression: a radio that acks but loses the second write
    /// of a session must still end up holding every block, channel 1 included.
    #[test]
    fn a_dropped_second_write_lands_on_the_duplicate_not_on_real_data() {
        let mut m = QytMock::new();
        m.drop_second_write = true;
        let data: Vec<u8> = (0..IMAGE_SIZE).map(|i| (i % 241) as u8).collect();
        write_image(&mut m, &data, &mut |_, _| {}).unwrap();

        assert_eq!(m.writes[0], m.writes[1], "the dropped write is the duplicate");
        assert_eq!(&m.memory()[..PRESERVED_START], &data[..PRESERVED_START]);
        assert_eq!(
            &m.memory()[PRESERVED_END..WRITE_LIMIT as usize],
            &data[PRESERVED_END..WRITE_LIMIT as usize]
        );
    }

    #[test]
    fn write_image_reports_progress_for_every_block() {
        let mut m = QytMock::new();
        let mut seen = Vec::new();
        write_image(&mut m, &vec![0u8; IMAGE_SIZE], &mut |d, t| seen.push((d, t))).unwrap();
        assert_eq!(seen.len(), WRITE_BLOCKS);
        assert_eq!(seen.last(), Some(&(WRITE_BLOCKS, WRITE_BLOCKS)));
    }

    #[test]
    fn write_image_rejects_a_wrongly_sized_image() {
        let mut m = QytMock::new();
        let err = write_image(&mut m, &[0u8; 16], &mut |_, _| {}).unwrap_err();
        assert!(matches!(err, Error::InvalidArgument(_)), "got {err:?}");
        assert!(m.written.is_empty());
    }

    #[test]
    fn qyt_write_then_read_roundtrips_an_image() {
        let mut m = QytMock::new();
        let mut data: Vec<u8> = (0..IMAGE_SIZE).map(|i| (i % 241) as u8).collect();
        // The tail and the preserved window are never written, so an honest
        // round trip has to expect back whatever the radio already holds there,
        // which on a fresh mock is the erased 0xFF.
        for b in &mut data[WRITE_LIMIT as usize..] {
            *b = 0xFF;
        }
        for b in &mut data[PRESERVED_START..PRESERVED_END] {
            *b = 0xFF;
        }

        write_image(&mut m, &data, &mut |_, _| {}).unwrap();
        let read_back = read_image(&mut m, &mut |_, _| {}).unwrap();
        assert_eq!(read_back, data);
    }

    #[test]
    fn a_full_session_starts_with_the_handshake() {
        let mut m = seeded_mock();
        let id = handshake(&mut m).unwrap();
        assert!(id.is_kt5000());
        read_image(&mut m, &mut |_, _| {}).unwrap();
        assert_eq!(
            &m.written[..8],
            &[0x55, 0x20, 0x21, 0x01, 0x23, 0xFF, 0xDC, 0x02]
        );
        assert_eq!(&m.written[8..13], &[0x06, 0x53, 0x3D, 0xF0, 0x10]);
    }
}
