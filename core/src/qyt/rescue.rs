//! Partial reads and writes for a KT-5000 that will not stay up long enough
//! for a whole-image transfer.
//!
//! # Why this exists
//!
//! [`crate::qyt::device`] transfers the entire image: 201 read blocks or 784
//! write blocks. At this radio's 9600 baud that is roughly 15 seconds to read
//! and 18 to write. A radio stuck in a boot loop only offers the window between
//! power-on and the reset, which is far shorter, so neither fits.
//!
//! Everything here speaks exactly the same protocol as `device.rs`, but over a
//! caller-chosen address range, and through [`Chunked`], which can pick a
//! transfer back up in the next boot window when the radio resets mid-way.
//! Repairing just the settings header and the VFO records (`0x0E00`-`0x0FFF`,
//! 32 write blocks, well under a second on the wire) fits in one window;
//! a whole image takes two or three, and gets them.
//!
//! # Constraints inherited from the protocol
//!
//! Reads move in `READ_LEN` (`0x40`) blocks and writes in `WRITE_LEN` (`0x10`)
//! blocks, so a range must be aligned to the block size of the direction it is
//! used in. The write range is additionally capped at [`WRITE_LIMIT`]: the
//! vendor CPS never writes above it and neither do we.
//!
//! Every window opens with the `0x3DF0` prologue, because every observed session
//! does, and the caller is expected to have handshaked already on that link. The
//! radio accepts exactly one handshake per link, so each window is its own open,
//! handshake, transfer, close.

use crate::error::{Error, Result};
use crate::qyt::device::{is_preserved, IMAGE_SIZE, WRITE_LIMIT};
use crate::qyt::protocol::{
    end_read_session, read_block, session_prologue, write_block, READ_LEN, WRITE_LEN,
};
use crate::transport::Transport;

/// The ceiling on a rescue read: the whole 16-bit address space.
///
/// Deliberately not [`IMAGE_SIZE`]. The image is what the vendor CPS transfers,
/// not what the radio answers for: the session prologue reads `0x3DF0`, well
/// past `0x3200`, and gets a reply. Since a read cannot damage anything, the
/// diagnostic is allowed to look anywhere, which is the only way to find out
/// what else is up there.
pub const READ_CEILING: usize = 0x1_0000;

/// A transfer that survives the radio resetting under it.
///
/// # The problem this solves
///
/// Measured on the boot-looping radio on 2026-09-09: the link stays up for
/// something between 6 and 11 seconds, depending on how late in the boot cycle
/// the handshake lands. A full read needs about 15 seconds of wire time and a
/// full write about 18, so neither fits in one window, and no amount of timing
/// the knock changes that.
///
/// But the transfer does not have to be one session. Each block carries its own
/// address, the radio holds its memory across a reset, and a rewrite of the same
/// bytes is idempotent. So a transfer can be picked up where the last window
/// dropped it: [`resume`](Chunked::resume) runs until the link fails, reports how
/// far it got, and the caller knocks again and calls it once more.
///
/// The cursor counts *blocks*, not bytes, because a block is the unit the radio
/// acknowledges. A block that timed out may or may not have landed; it is simply
/// re-sent next window, which for a write of identical bytes costs nothing.
pub struct Chunked {
    /// Address the transfer starts at.
    base: u16,
    /// Size of one block: [`READ_LEN`] or [`WRITE_LEN`].
    block: usize,
    /// Total blocks in the transfer.
    total: usize,
    /// Blocks confirmed done.
    done: usize,
    /// For a read, the bytes collected so far. For a write, the source image.
    buf: Vec<u8>,
}

impl Chunked {
    /// Start a resumable read of `len` bytes at `addr`.
    pub fn read(addr: u16, len: u32) -> Result<Self> {
        check_range("read", addr, len, READ_LEN, READ_CEILING)?;
        Ok(Self {
            base: addr,
            block: READ_LEN as usize,
            total: len as usize / READ_LEN as usize,
            done: 0,
            buf: Vec::with_capacity(len as usize),
        })
    }

    /// Start a resumable write of `image`'s `addr..addr + len` slice. `image` is
    /// a whole [`IMAGE_SIZE`] image, so file and radio addresses are the same
    /// number.
    pub fn write(image: &[u8], addr: u16, len: u32) -> Result<Self> {
        if image.len() != IMAGE_SIZE {
            return Err(Error::InvalidArgument(format!(
                "QYT image is {} bytes, expected {IMAGE_SIZE}",
                image.len()
            )));
        }
        check_range("write", addr, len, WRITE_LEN, WRITE_LIMIT as usize)?;
        Ok(Self {
            base: addr,
            block: WRITE_LEN as usize,
            total: len as usize / WRITE_LEN as usize,
            done: 0,
            buf: image.to_vec(),
        })
    }

    /// Blocks done and blocks in total.
    pub fn progress(&self) -> (usize, usize) {
        (self.done, self.total)
    }

    /// Whether every block has been transferred.
    pub fn is_complete(&self) -> bool {
        self.done == self.total
    }

    /// The bytes read so far. Meaningful for a read; for a write this is the
    /// source image.
    pub fn bytes(&self) -> &[u8] {
        &self.buf
    }

    /// Whether this cursor is a read. Writes carry the whole source image, so
    /// the two are told apart by block size rather than by buffer length.
    fn is_read(&self) -> bool {
        self.block == READ_LEN as usize
    }

    /// Give back the last acknowledged write block when a window ends badly.
    ///
    /// Observed on hardware 2026-09-09, restoring `0x2000`-`0x23FF` across eight
    /// windows: every block that failed to stick was the last one of a window.
    /// The radio acknowledges a write on receipt and commits it afterwards, so a
    /// reset arriving in between loses a block the host was told it had. Since
    /// re-sending identical bytes costs one block and changes nothing, the cheap
    /// fix is to distrust the last ack of a broken window.
    ///
    /// Reads need none of this: a read that did not arrive was never counted.
    fn rewind_uncommitted(&mut self, window_start: usize) {
        if !self.is_read() && self.done > window_start {
            self.done -= 1;
        }
    }

    /// Transfer as many blocks as this window allows, over an already
    /// handshaked link.
    ///
    /// Returns the blocks added and the error that ended the window, which is
    /// `None` only when the transfer completed. **A failure is not returned as
    /// `Err`**, because in this situation the link dying is the expected case,
    /// not the exceptional one: the caller knocks again and calls this again.
    /// The error is handed back so it can still be shown.
    pub fn resume(
        &mut self,
        t: &mut dyn Transport,
        progress: &mut dyn FnMut(usize, usize),
    ) -> (usize, Option<Error>) {
        let before = self.done;
        if let Err(e) = session_prologue(t) {
            return (0, Some(e));
        }

        let mut scratch = [0u8; READ_LEN as usize];
        while self.done < self.total {
            let offset = self.done * self.block;
            let addr = self.base + offset as u16;
            let step = self.block;

            let outcome = if self.is_read() {
                read_block(t, addr, READ_LEN, &mut scratch[..step]).map(|()| {
                    self.buf.extend_from_slice(&scratch[..step]);
                })
            } else {
                let from = addr as usize;
                write_block(t, addr, &self.buf[from..from + step])
            };

            if let Err(e) = outcome {
                self.rewind_uncommitted(before);
                return (self.done.saturating_sub(before), Some(e));
            }
            self.done += 1;
            progress(self.done, self.total);
        }

        // A read session is closed with one bare STX, as the vendor does.
        if self.is_read() {
            if let Err(e) = end_read_session(t) {
                return (self.done - before, Some(e));
            }
        }
        (self.done - before, None)
    }
}

/// One named span of the KT-5000 image, for reporting a diff in terms a person
/// can act on rather than as a wall of byte offsets.
///
/// Straight out of the map in `docs/qyt-kt5000-memory-map.md`. Spans are
/// half-open and in ascending order, and they tile `0x0000`-`0x3200` with no
/// gaps, so every differing byte lands in exactly one of them.
pub const REGIONS: &[(u16, u16, &str)] = &[
    (0x0000, 0x0C80, "channel records"),
    (0x0C80, 0x0E00, "unused (0xFF)"),
    (0x0E00, 0x0E36, "settings header"),
    (0x0E36, 0x0F00, "settings, unmodelled"),
    (0x0F00, 0x0F80, "VFO records"),
    (0x0F80, 0x0FE0, "unused (0xFF)"),
    (0x0FE0, 0x0FF0, "model string"),
    (0x0FF0, 0x1000, "unused (0xFF)"),
    (0x1000, 0x1C80, "channel names"),
    (0x1C80, 0x2000, "unused (0xFF)"),
    (0x2000, 0x23F0, "persistent scratch"),
    (0x23F0, 0x2AF0, "unused (0xFF)"),
    (0x2AF0, 0x2B00, "factory blob"),
    (0x2B00, 0x3000, "unused (0xFF)"),
    (0x3000, 0x3100, "DTMF / autodial"),
    (0x3100, 0x3200, "never written"),
];

/// A run of differing bytes, with the region it fell in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffRun {
    /// First differing address in the run.
    pub addr: u16,
    /// How many consecutive bytes differ.
    pub len: usize,
    /// The [`REGIONS`] label covering `addr`.
    pub region: &'static str,
}

/// Diff two full images and return the differing runs, each labelled with its
/// region.
///
/// This is what turns "the restore did not help" into a fact. Read the radio
/// back after a write and diff it against what you sent: if the answer is empty,
/// the radio holds exactly the bytes you meant it to and the codeplug is no
/// longer a suspect, which is a conclusion no amount of staring at the write
/// path can reach.
///
/// Runs are split at region boundaries so a long difference is reported as the
/// parts of the map it damaged rather than as one anonymous span.
pub fn diff_regions(a: &[u8], b: &[u8]) -> Result<Vec<DiffRun>> {
    diff_filtered(a, b, |_| true)
}

/// Shared body of [`diff_regions`] and [`verify_diff`]: build labelled runs from
/// every differing address `considered` accepts.
fn diff_filtered(
    a: &[u8],
    b: &[u8],
    considered: impl Fn(u16) -> bool,
) -> Result<Vec<DiffRun>> {
    if a.len() != IMAGE_SIZE || b.len() != IMAGE_SIZE {
        return Err(Error::InvalidArgument(format!(
            "both images must be {IMAGE_SIZE} bytes, got {} and {}",
            a.len(),
            b.len()
        )));
    }

    let mut runs: Vec<DiffRun> = Vec::new();
    for i in 0..IMAGE_SIZE {
        if a[i] == b[i] || !considered(i as u16) {
            continue;
        }
        let addr = i as u16;
        let region = region_of(addr);
        // Extend the previous run only if this byte continues it inside the
        // same region.
        match runs.last_mut() {
            Some(run) if run.addr as usize + run.len == i && run.region == region => {
                run.len += 1;
            }
            _ => runs.push(DiffRun {
                addr,
                len: 1,
                region,
            }),
        }
    }
    Ok(runs)
}

/// Regions a working KT-5000 always holds data in, and which a full-image write
/// still sends verbatim, so writing an image that is erased there blanks them.
///
/// Measured across both full backups in `docs/reference/QYT`: the factory blob
/// and the DTMF/autodial memories are populated in each. The channel table and
/// the name table are left out deliberately, because a codeplug with no
/// channels is unusual but legitimate. The model string is left out because an
/// image erased there never reaches the library: [`crate::model::Model::detect`]
/// keys off it.
const ALWAYS_POPULATED: &[u16] = &[0x2AF0, 0x3000];

/// Region labels this image would blank if it were written to a radio.
///
/// The write path sends the image's own bytes for everything below
/// [`WRITE_LIMIT`] outside the preserved window, so a stale or synthesised image
/// erases whatever it does not carry. This is what turns that into a sentence
/// before the write rather than a diff afterwards. An image read from the radio
/// being written to always returns empty.
pub fn erased_regions(image: &[u8]) -> Vec<&'static str> {
    if image.len() != IMAGE_SIZE {
        return Vec::new();
    }
    ALWAYS_POPULATED
        .iter()
        .filter(|&&start| !is_preserved(start as usize))
        .filter_map(|&start| {
            let (s, e, label) = *REGIONS.iter().find(|&&(s, _, _)| s == start)?;
            image[s as usize..e as usize]
                .iter()
                .all(|&b| b == 0xFF)
                .then_some(label)
        })
        .collect()
}

/// Addresses the radio's own firmware rewrites while it runs, which therefore
/// differ on any read-back no matter what was sent.
///
/// `0x0E51` increments between consecutive reads (a counter of some kind),
/// measured on 2026-09-09 during the restore. Reporting it as a verification
/// failure would mean every honest write looked broken.
///
/// `0x001E` used to be listed too. It sits in block `0x0010`, the block the
/// radio dropped as the second write of every session, so it was a lost write
/// rather than the firmware, and hiding it would hide that bug (2026-10-16).
pub const SELF_WRITTEN: &[u16] = &[0x0E51];

/// Diff an image that was sent to the radio against one read back afterwards,
/// reporting only differences that mean the write did not land.
///
/// Ignores three kinds of byte, because a difference there says nothing about
/// the transfer: anything at or above [`WRITE_LIMIT`], the preserved window the
/// write path deliberately skips (see `PRESERVED_START`), and [`SELF_WRITTEN`].
///
/// An empty result is the only proof a write landed. The radio acknowledges a
/// block on receipt and commits it afterwards, so an ack is not proof: a reset
/// arriving in between loses a block the host was told it had. Measured on
/// 2026-09-09, where every block that failed to stick was the last one of its
/// boot window.
pub fn verify_diff(sent: &[u8], read_back: &[u8]) -> Result<Vec<DiffRun>> {
    diff_filtered(sent, read_back, |addr| {
        addr < WRITE_LIMIT && !is_preserved(addr as usize) && !SELF_WRITTEN.contains(&addr)
    })
}

/// The [`REGIONS`] label covering `addr`, or a catch-all for anything past the
/// mapped image.
pub fn region_of(addr: u16) -> &'static str {
    REGIONS
        .iter()
        .find(|&&(start, end, _)| addr >= start && addr < end)
        .map(|&(_, _, label)| label)
        .unwrap_or("outside the mapped image")
}

/// Check a read range without touching the radio.
///
/// [`read_range`] validates the same thing, but only once it holds the link.
/// A rescue caller races a boot window, so it wants to be told about a bad
/// address before it spends a minute knocking, not after.
pub fn check_read_range(addr: u16, len: u32) -> Result<()> {
    check_range("read", addr, len, READ_LEN, READ_CEILING)
}

/// Check a write range without touching the radio. See [`check_read_range`].
pub fn check_write_range(addr: u16, len: u32) -> Result<()> {
    check_range("write", addr, len, WRITE_LEN, WRITE_LIMIT as usize)
}

/// Validate that `addr`/`len` are block-aligned, non-empty, and inside `limit`.
///
/// `what` names the direction so the message says which block size and which
/// ceiling were violated; the two differ (`0x40` to `0x3200` for reads, `0x10`
/// to `0x3100` for writes) and mixing them up is the easy mistake here.
fn check_range(what: &str, addr: u16, len: u32, block: u8, limit: usize) -> Result<()> {
    let block = block as u32;
    if len == 0 {
        return Err(Error::InvalidArgument(format!(
            "{what} range length must not be zero"
        )));
    }
    if !(addr as u32).is_multiple_of(block) || !len.is_multiple_of(block) {
        return Err(Error::InvalidArgument(format!(
            "{what} range 0x{addr:04X}+0x{len:04X} is not a multiple of the \
             0x{block:02X}-byte {what} block"
        )));
    }
    let end = addr as usize + len as usize;
    if end > limit {
        return Err(Error::InvalidArgument(format!(
            "{what} range ends at 0x{end:04X}, past this radio's 0x{limit:04X} \
             {what} limit"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qyt::mock::QytMock;
    use crate::qyt::protocol::{handshake, PROLOGUE_ADDR, PROLOGUE_LEN};

    /// A mock seeded with a recognisable pattern, handshaken and ready.
    fn ready_mock() -> QytMock {
        let mut m = QytMock::new();
        let pattern: Vec<u8> = (0..IMAGE_SIZE).map(|i| (i % 251) as u8).collect();
        m.seed(0, &pattern);
        handshake(&mut m).unwrap();
        m
    }

    /// A full image whose every byte differs from what the mock holds.
    fn replacement_image() -> Vec<u8> {
        (0..IMAGE_SIZE).map(|i| !((i % 251) as u8)).collect()
    }

    /// Reply sizes, so a window budget can be written in blocks rather than in
    /// magic numbers. A data reply is `06 58 AH AL LL` plus payload; the
    /// prologue reply drops the leading `06`, which is the asymmetry the
    /// protocol layer tolerates.
    const PROLOGUE_REPLY: usize = 4 + PROLOGUE_LEN as usize;
    const READ_REPLY: usize = 5 + READ_LEN as usize;

    /// A link that stops answering after `budget` reply bytes, standing in for
    /// the radio resetting mid-transfer. Rebooting it models the power cycle:
    /// memory survives, the session does not, and the budget starts over.
    struct DyingLink {
        radio: QytMock,
        budget: usize,
        spent: usize,
    }

    impl DyingLink {
        fn new(budget: usize) -> Self {
            Self { radio: ready_mock(), budget, spent: 0 }
        }

        /// Power-cycle: a fresh window with a fresh budget, and the handshake
        /// the real caller has to redo.
        fn power_cycle(&mut self) {
            self.radio.reboot();
            self.spent = 0;
            handshake(&mut self.radio).unwrap();
        }
    }

    impl Transport for DyingLink {
        fn write_all(&mut self, buf: &[u8]) -> Result<()> {
            if self.spent >= self.budget {
                return Err(Error::Protocol("link died".into()));
            }
            self.radio.write_all(buf)
        }

        fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
            if self.spent + buf.len() > self.budget {
                self.spent = self.budget;
                return Err(Error::Protocol("link died".into()));
            }
            self.spent += buf.len();
            self.radio.read_exact(buf)
        }
    }

    #[test]
    fn a_read_returns_only_the_requested_span() {
        let mut m = ready_mock();
        let mut c = Chunked::read(0x0E00, 0x0200).unwrap();
        let (added, err) = c.resume(&mut m, &mut |_, _| {});

        assert!(err.is_none(), "a healthy link should not fail: {err:?}");
        assert_eq!(added, 8);
        assert!(c.is_complete());
        assert_eq!(c.bytes(), &m.memory()[0x0E00..0x1000]);

        // Prologue, then exactly eight 0x40 reads walking the range once.
        assert_eq!(m.reads[0], (PROLOGUE_ADDR, 0x10));
        let expected: Vec<(u16, u8)> = (0x0E00..0x1000u16)
            .step_by(READ_LEN as usize)
            .map(|a| (a, READ_LEN))
            .collect();
        assert_eq!(&m.reads[1..], &expected[..]);

        // The session closes with one bare 0x06, as a real read does.
        assert_eq!(*m.written.last().unwrap(), 0x06);
    }

    #[test]
    fn a_read_reports_progress_per_block() {
        let mut m = ready_mock();
        let mut seen = Vec::new();
        let mut c = Chunked::read(0x0000, 0x0100).unwrap();
        c.resume(&mut m, &mut |done, total| seen.push((done, total)));
        assert_eq!(seen, vec![(1, 4), (2, 4), (3, 4), (4, 4)]);
    }

    #[test]
    fn a_write_touches_only_the_named_blocks() {
        let mut m = ready_mock();
        let before = m.memory().to_vec();
        let image = replacement_image();

        let mut c = Chunked::write(&image, 0x0F00, 0x0080).unwrap();
        let (added, err) = c.resume(&mut m, &mut |_, _| {});
        assert!(err.is_none(), "{err:?}");
        assert_eq!(added, 8);

        assert_eq!(m.writes.len(), 8);
        assert!(m
            .writes
            .iter()
            .all(|&(a, n)| (0x0F00..0x0F80).contains(&a) && n == WRITE_LEN as usize));

        assert_eq!(&m.memory()[0x0F00..0x0F80], &image[0x0F00..0x0F80]);
        assert_eq!(&m.memory()[..0x0F00], &before[..0x0F00]);
        assert_eq!(&m.memory()[0x0F80..IMAGE_SIZE], &before[0x0F80..IMAGE_SIZE]);
    }

    #[test]
    fn a_read_spanning_several_windows_reassembles_the_whole_range() {
        // Small enough that no window finishes the job on its own.
        let mut link = DyingLink::new(600);
        let mut c = Chunked::read(0x0000, 0x0400).unwrap();

        let mut windows = 0;
        while !c.is_complete() {
            let (_, err) = c.resume(&mut link, &mut |_, _| {});
            windows += 1;
            if c.is_complete() {
                assert!(err.is_none());
                break;
            }
            assert!(err.is_some(), "an unfinished window must say why it stopped");
            assert!(windows < 20, "made no progress across {windows} windows");
            link.power_cycle();
        }

        assert!(windows > 1, "the point of the test is more than one window");
        assert_eq!(c.bytes(), &link.radio.memory()[..0x0400]);
    }

    #[test]
    fn a_write_spanning_several_windows_lands_every_block() {
        let mut link = DyingLink::new(40);
        let image = replacement_image();
        let mut c = Chunked::write(&image, 0x0E00, 0x0200).unwrap();

        let mut windows = 0;
        while !c.is_complete() {
            c.resume(&mut link, &mut |_, _| {});
            windows += 1;
            assert!(windows < 100, "made no progress across {windows} windows");
            if !c.is_complete() {
                link.power_cycle();
            }
        }

        assert!(windows > 1);
        assert_eq!(
            &link.radio.memory()[0x0E00..0x1000],
            &image[0x0E00..0x1000],
            "every block must land despite the resets"
        );
    }

    #[test]
    fn a_broken_write_window_gives_back_its_last_block() {
        // Budget enough for the prologue plus a couple of write acks.
        let mut link = DyingLink::new(PROLOGUE_REPLY + 2);
        let image = replacement_image();
        let mut c = Chunked::write(&image, 0x0E00, 0x0100).unwrap();
        let (added, err) = c.resume(&mut link, &mut |_, _| {});

        assert!(err.is_some(), "the window must have been cut short");
        let (done, _) = c.progress();
        // Two acks arrived; one is given back as possibly uncommitted.
        assert_eq!(done, 1);
        assert_eq!(added, 1);
    }

    #[test]
    fn a_broken_read_window_keeps_every_block_it_got() {
        // A read block that did not arrive was never counted, so there is
        // nothing to give back and the cursor must not lose good data.
        let mut link = DyingLink::new(PROLOGUE_REPLY + 2 * READ_REPLY);
        let mut c = Chunked::read(0x0000, 0x0200).unwrap();
        let (added, err) = c.resume(&mut link, &mut |_, _| {});
        assert!(err.is_some());
        assert_eq!(added, 2);
        assert_eq!(c.bytes().len(), 2 * READ_LEN as usize);
    }

    #[test]
    fn a_window_that_dies_immediately_adds_nothing_and_says_so() {
        let mut link = DyingLink::new(0);
        let mut c = Chunked::read(0x0000, 0x0040).unwrap();
        let (added, err) = c.resume(&mut link, &mut |_, _| {});
        assert_eq!(added, 0);
        assert!(err.is_some());
        assert_eq!(c.progress(), (0, 1));
    }

    #[test]
    fn diff_regions_labels_and_splits_runs() {
        let a = vec![0u8; IMAGE_SIZE];
        let mut b = a.clone();
        // One byte in the settings header.
        b[0x0E02] = 3;
        // A run that straddles the settings/VFO boundary at 0x0F00.
        b[0x0EFE..0x0F02].fill(0xAA);
        let runs = diff_regions(&a, &b).unwrap();
        assert_eq!(
            runs,
            vec![
                DiffRun { addr: 0x0E02, len: 1, region: "settings header" },
                DiffRun { addr: 0x0EFE, len: 2, region: "settings, unmodelled" },
                DiffRun { addr: 0x0F00, len: 2, region: "VFO records" },
            ]
        );
    }

    #[test]
    fn identical_images_diff_to_nothing() {
        let a: Vec<u8> = (0..IMAGE_SIZE).map(|i| (i % 251) as u8).collect();
        assert!(diff_regions(&a, &a).unwrap().is_empty());
    }

    #[test]
    fn verify_ignores_the_bytes_a_write_never_claims_to_have_set() {
        let sent: Vec<u8> = (0..IMAGE_SIZE).map(|i| (i % 251) as u8).collect();
        let mut back = sent.clone();

        // The radio's own counter and the tail it keeps, plus the preserved
        // window the write path skips: none of these mean a write failed.
        back[0x0E51] ^= 0xFF;
        back[0x2100] ^= 0xFF;
        back[WRITE_LIMIT as usize + 4] ^= 0xFF;
        assert!(verify_diff(&sent, &back).unwrap().is_empty());

        // A byte inside the written range is a real failure, reported by region.
        back[0x1A50] ^= 0xFF;
        let runs = verify_diff(&sent, &back).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].addr, 0x1A50);
        assert_eq!(runs[0].region, "channel names");
    }

    #[test]
    fn verify_reports_channel_1_power_byte() {
        let sent = vec![0x00u8; IMAGE_SIZE];
        let mut back = sent.clone();
        back[0x001E] = 0x01;
        let runs = verify_diff(&sent, &back).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].addr, 0x001E);
    }

    #[test]
    fn erased_regions_names_what_a_write_would_blank() {
        let mut image = vec![0x11u8; IMAGE_SIZE];
        assert!(erased_regions(&image).is_empty());

        for b in &mut image[0x3000..0x3100] {
            *b = 0xFF;
        }
        assert_eq!(erased_regions(&image), vec!["DTMF / autodial"]);

        // The preserved window is never sent, so an image erased there blanks
        // nothing and must not be reported.
        for b in &mut image[0x2000..0x23F0] {
            *b = 0xFF;
        }
        assert_eq!(erased_regions(&image), vec!["DTMF / autodial"]);

        assert!(erased_regions(&[0xFFu8; 8]).is_empty());
    }

    #[test]
    fn the_region_table_tiles_the_image_with_no_gaps() {
        assert_eq!(REGIONS[0].0, 0x0000);
        assert_eq!(REGIONS.last().unwrap().1 as usize, IMAGE_SIZE);
        for pair in REGIONS.windows(2) {
            assert_eq!(pair[0].1, pair[1].0, "gap or overlap at 0x{:04X}", pair[0].1);
        }
        // The prologue window is real memory but outside the transferred image.
        assert_eq!(region_of(0x3DF0), "outside the mapped image");
    }

    #[test]
    fn reads_may_go_past_the_image_but_writes_may_not() {
        // The prologue proves the radio answers above 0x3200, and a read is
        // harmless, so the diagnostic is allowed up there.
        assert!(check_read_range(0x3DC0, 0x0040).is_ok());
        assert!(check_write_range(0x3DC0, 0x0010).is_err());
        // The whole 16-bit space is a legal read range.
        assert!(Chunked::read(0x0000, 0x1_0000).is_ok());
        assert!(Chunked::read(0x0040, 0x1_0000).is_err());
    }

    #[test]
    fn the_range_checks_are_callable_without_a_radio() {
        // Same verdicts as the transfer functions reach, minus the link.
        assert!(check_read_range(0x0E00, 0x0200).is_ok());
        assert!(check_read_range(0x0E20, 0x0040).is_err());
        assert!(check_write_range(0x0E00, 0x0010).is_ok());
        assert!(check_write_range(0x30F0, 0x0020).is_err());
    }

    #[test]
    fn a_write_past_the_write_limit_is_refused() {
        let image = replacement_image();
        // 0x3100 is the first address the vendor CPS never writes.
        assert!(Chunked::write(&image, 0x30F0, 0x0020).is_err());
    }

    #[test]
    fn misaligned_ranges_are_refused_before_anything_is_sent() {
        let image = replacement_image();
        // 0x0E08 is not on a 0x10 write boundary.
        assert!(Chunked::write(&image, 0x0E08, 0x0010).is_err());
        // 0x0E20 is, but 0x20 is not a whole 0x40 read block.
        assert!(Chunked::read(0x0E20, 0x0020).is_err());
        // Zero-length is a caller bug either way.
        assert!(Chunked::read(0x0E00, 0).is_err());
    }

    #[test]
    fn a_write_requires_a_full_size_image() {
        let short = vec![0u8; 16];
        match Chunked::write(&short, 0x0000, 0x0010) {
            Err(Error::InvalidArgument(_)) => {}
            Err(e) => panic!("wrong error: {e:?}"),
            Ok(_) => panic!("a 16-byte image must not be accepted"),
        }
    }

    #[test]
    fn a_repaired_range_reads_back_as_written() {
        let mut m = ready_mock();
        let image = replacement_image();
        Chunked::write(&image, 0x0E00, 0x0100)
            .unwrap()
            .resume(&mut m, &mut |_, _| {});

        // The radio reboots between sessions, exactly as on hardware.
        m.reboot();
        handshake(&mut m).unwrap();
        let mut back = Chunked::read(0x0E00, 0x0100).unwrap();
        back.resume(&mut m, &mut |_, _| {});
        assert_eq!(back.bytes(), &image[0x0E00..0x0F00]);
    }
}
