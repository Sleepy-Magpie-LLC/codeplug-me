//! Golden-capture tests for the QYT KT-5000 wire protocol.
//!
//! These replay the raw vendor CPS captures in `docs/reference/QYT/` through
//! the real protocol code and assert that the byte stream we put on the wire is
//! identical to the one the Windows CPS puts on the wire. That is the strongest
//! correctness argument available for a radio we cannot attach to CI, and it is
//! stronger than any mock: the radio's half of the conversation is the real
//! recorded bytes, not something we invented.
//!
//! The capture directory is gitignored (the dumps carry personal data), so
//! every test here skips gracefully with a printed note when the files are
//! absent rather than failing.

use codeplugme_core::error::Result;
use codeplugme_core::qyt::codeplug::{QytCodeplug, Tone};
use codeplugme_core::qyt::device::{
    is_preserved, read_image, write_image, IMAGE_SIZE, PRESERVED_END, PRESERVED_START,
    WRITE_BLOCKS,
};
use codeplugme_core::qyt::protocol::handshake;
use codeplugme_core::transport::Transport;

/// Directory holding the eight raw capture files.
fn capture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/reference/QYT")
}

/// Load one capture, or `None` when the gitignored directory is not present.
fn capture(name: &str) -> Option<Vec<u8>> {
    let path = capture_dir().join(name);
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(_) => {
            println!(
                "skipping: capture {} is not present (gitignored)",
                path.display()
            );
            None
        }
    }
}

/// A transport that replays a recorded radio-to-host stream and records the
/// host-to-radio stream we generate, so the two can be diffed against the
/// vendor's own capture pair.
struct CaptureTransport {
    /// The recorded `*_read` stream, i.e. everything the radio said.
    to_host: Vec<u8>,
    /// How much of `to_host` has been consumed.
    pos: usize,
    /// Everything the host said, for byte-exact comparison with `*_write`.
    written: Vec<u8>,
}

impl CaptureTransport {
    /// Wrap a recorded radio-to-host byte stream.
    fn new(to_host: Vec<u8>) -> Self {
        Self {
            to_host,
            pos: 0,
            written: Vec::new(),
        }
    }
}

impl Transport for CaptureTransport {
    /// Record what the host sends; the replay never reacts to it.
    fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        self.written.extend_from_slice(buf);
        Ok(())
    }

    /// Hand back the next slice of the recorded stream, or time out at its end.
    fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        let end = self.pos + buf.len();
        if end > self.to_host.len() {
            return Err(codeplugme_core::error::Error::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "capture exhausted: wanted {} bytes at offset {}, only {} remain",
                    buf.len(),
                    self.pos,
                    self.to_host.len() - self.pos
                ),
            )));
        }
        buf.copy_from_slice(&self.to_host[self.pos..end]);
        self.pos = end;
        Ok(())
    }
}

/// Reconstruct the radio image from the recorded read session, the same way
/// `read_image` does but driven straight off the capture.
fn image_from_read_capture() -> Option<Vec<u8>> {
    let replies = capture("read-radio_read")?;
    let mut t = CaptureTransport::new(replies);
    handshake(&mut t).expect("handshake against the capture");
    Some(read_image(&mut t, &mut |_, _| {}).expect("read_image against the capture"))
}

#[test]
fn read_session_host_stream_is_byte_identical_to_the_vendor_capture() {
    let (Some(replies), Some(expected)) = (capture("read-radio_read"), capture("read-radio_write"))
    else {
        return;
    };

    let mut t = CaptureTransport::new(replies);
    handshake(&mut t).unwrap();
    let image = read_image(&mut t, &mut |_, _| {}).unwrap();

    assert_eq!(image.len(), IMAGE_SIZE);
    assert_eq!(
        expected.len(),
        1019,
        "capture read-radio_write changed size unexpectedly"
    );
    assert_eq!(
        t.written.len(),
        expected.len(),
        "host stream is {} bytes, vendor sent {}",
        t.written.len(),
        expected.len()
    );
    assert_eq!(
        t.written, expected,
        "host read stream diverges from the vendor CPS"
    );
}

/// Split a captured host stream into its write frames, keyed by the image
/// address each frame's header carries.
///
/// Layout after the 8-byte handshake and the 5-byte prologue is a run of
/// 21-byte frames: `STX`, `CMD_WRITE`, address high, address low, length, then
/// 16 data bytes.
fn write_frames(stream: &[u8]) -> Vec<(usize, Vec<u8>)> {
    const PREFIX: usize = 8 + 5;
    const FRAME: usize = 5 + 16;
    assert!(stream.len() >= PREFIX);
    assert_eq!(
        (stream.len() - PREFIX) % FRAME,
        0,
        "stream tail is not a whole number of write frames"
    );
    stream[PREFIX..]
        .chunks(FRAME)
        .map(|f| {
            assert_eq!(f[4] as usize, 16, "unexpected write block length");
            (((f[2] as usize) << 8) | f[3] as usize, f[5..].to_vec())
        })
        .collect()
}

#[test]
fn write_session_host_stream_matches_the_vendor_capture_outside_the_preserved_window() {
    let (Some(image), Some(replies), Some(expected)) = (
        image_from_read_capture(),
        capture("write-radio_read"),
        capture("write-radio_write"),
    ) else {
        return;
    };

    let mut t = CaptureTransport::new(replies);
    handshake(&mut t).unwrap();
    let mut acks = 0usize;
    write_image(&mut t, &image, &mut |_, _| acks += 1).unwrap();

    assert_eq!(acks, WRITE_BLOCKS);
    assert_eq!(
        expected.len(),
        16477,
        "capture write-radio_write changed size unexpectedly"
    );
    // The handshake and the prologue are still byte-identical; only the frame
    // run below is allowed to differ.
    assert_eq!(t.written[..13], expected[..13]);

    let mut ours = write_frames(&t.written);
    let theirs = write_frames(&expected);
    assert_eq!(theirs.len(), 784, "the vendor sends every block");

    // First divergence: 0x0000 is sent twice, so the radio's dropped second
    // write lands on a duplicate. See write_image in core/src/qyt/device.rs.
    assert_eq!(ours[0], ours[1], "the first block is sent twice");
    assert_eq!(ours[0].0, 0x0000);
    ours.remove(0);

    // Every frame we do send is the vendor's frame, byte for byte, at the same
    // address and in the same order.
    let kept: Vec<&(usize, Vec<u8>)> = theirs
        .iter()
        .filter(|(addr, _)| !is_preserved(*addr))
        .collect();
    assert_eq!(ours.len(), kept.len());
    for (ours, theirs) in ours.iter().zip(kept.iter()) {
        assert_eq!(ours.0, theirs.0, "frame address diverges from the vendor");
        assert_eq!(
            ours.1, theirs.1,
            "frame data at 0x{:04X} diverges from the vendor",
            ours.0
        );
    }

    // The other divergence: the 63 blocks of 0x2000-0x23EF we never send. The
    // vendor uploads an uninitialised buffer there; we leave the radio's own
    // bytes alone. See PRESERVED_START in core/src/qyt/device.rs.
    let skipped: Vec<usize> = theirs
        .iter()
        .map(|(addr, _)| *addr)
        .filter(|addr| !ours.iter().any(|(a, _)| a == addr))
        .collect();
    assert_eq!(skipped.len(), 63);
    assert_eq!(*skipped.first().unwrap(), PRESERVED_START);
    assert_eq!(*skipped.last().unwrap(), PRESERVED_END - 16);
    assert!(skipped.iter().all(|&addr| is_preserved(addr)));
    // And the image we handed over really is the erased one that caused this
    // rule to exist, so the test would fail if the skip were removed.
    assert!(image[PRESERVED_START..PRESERVED_END].iter().all(|&b| b == 0xFF));
}

#[test]
fn read_capture_parses_to_an_image_with_the_expected_landmarks() {
    let Some(image) = image_from_read_capture() else {
        return;
    };

    assert_eq!(image.len(), 12_800);

    // Channel 0: 462.5625 MHz simplex, no tones, flag byte 0x24.
    assert_eq!(
        &image[0x0000..0x0010],
        &[
            0x50, 0x62, 0x25, 0x46, 0x50, 0x62, 0x25, 0x46, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x24
        ]
    );

    // The LCD static message, as it ships from the factory.
    assert_eq!(&image[0x0FE0..0x0FF0], b"Model:KT-5000   ");

    // Unused channel-name space and the read-only tail are erased.
    assert!(image[0x0A60..0x0E00].iter().all(|&b| b == 0xFF));
    assert!(image[0x3100..0x3200].iter().all(|&b| b == 0xFF));
}

#[test]
fn write_capture_replies_are_784_acks() {
    let Some(replies) = capture("write-radio_read") else {
        return;
    };

    // Identity (61) + prologue reply (20), then one bare 0x06 per write block,
    // then the 117-byte port-close banner the radio emits as it reboots.
    const HEADER: usize = 61 + 20;
    const TRAILER: usize = 117;
    assert_eq!(replies.len(), 982);
    let acks = &replies[HEADER..replies.len() - TRAILER];
    assert_eq!(acks.len(), 784);
    assert!(acks.iter().all(|&b| b == 0x06));
}

#[test]
fn the_real_capture_parses_into_the_expected_codeplug() {
    let Some(image) = image_from_read_capture() else {
        return;
    };
    let cp = QytCodeplug::parse(&image).expect("the captured image parses");

    // 159 programmed slots out of 200, the highest at index 165.
    assert_eq!(cp.channels().count(), 159);
    assert_eq!(cp.channels().map(|c| c.index).max(), Some(165));
    assert_eq!(cp.static_message(), "Model:KT-5000");

    let ch0 = cp.channels().find(|c| c.index == 0).unwrap();
    assert_eq!(ch0.name, "FRS-01");
    assert_eq!(ch0.rx_frequency_hz, 462_562_500);
    assert_eq!(ch0.tx_frequency_hz, Some(462_562_500));
    assert_eq!(ch0.rx_tone, Tone::None);
    // The two bits CHIRP leaves unnamed, set on this channel alone.
    assert_eq!(ch0.flags_raw, [0, 0, 0, 0x24]);

    let ch64 = cp.channels().find(|c| c.index == 64).unwrap();
    assert_eq!(ch64.name, "N7FYZ");
    assert_eq!(ch64.rx_frequency_hz, 147_320_000);
    assert_eq!(ch64.tx_frequency_hz, Some(147_920_000));
    assert_eq!(ch64.rx_tone, Tone::None);
    assert_eq!(ch64.tx_tone, Tone::Ctcss { hz_tenths: 1000 });

    // Every tone word in the capture decodes to something real, never to the
    // "unmappable" fallback on a non-zero word.
    for ch in cp.channels() {
        for tone in [ch.rx_tone, ch.tx_tone] {
            if let Tone::Ctcss { hz_tenths } = tone {
                assert!((670..=2503).contains(&hz_tenths), "channel {}", ch.index);
            }
        }
    }
}

#[test]
fn the_real_capture_round_trips_byte_for_byte() {
    let Some(image) = image_from_read_capture() else {
        return;
    };
    let cp = QytCodeplug::parse(&image).expect("the captured image parses");
    assert_eq!(cp.serialize(), image);
}

#[test]
fn an_edit_to_the_real_capture_disturbs_nothing_else() {
    let Some(image) = image_from_read_capture() else {
        return;
    };
    let mut cp = QytCodeplug::parse(&image).expect("the captured image parses");
    cp.channel_mut(64).unwrap().set_name("EDITED");
    let out = cp.serialize();

    let touched: Vec<usize> = (0..image.len()).filter(|&i| image[i] != out[i]).collect();
    assert!(touched.iter().all(|&i| (0x1000 + 64 * 16..0x1000 + 64 * 16 + 7).contains(&i)));
    // Channel 1's unmodelled 0x05 at name byte 7 is still there.
    assert_eq!(out[0x1000 + 16 + 7], 0x05);
    // So is channel 0's unnamed tail.
    assert_eq!(out[15], 0x24);
}
