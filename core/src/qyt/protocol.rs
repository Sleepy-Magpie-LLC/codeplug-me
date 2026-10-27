//! Low-level QYT KT-5000 programming protocol: the handshake, the per-session
//! prologue probe, and the block read/write framing.
//!
//! The wire format is much simpler than the AnyTone's. Addresses are 16-bit
//! big-endian, there is **no checksum anywhere**, and `0x06` doubles as both a
//! command prefix and the write acknowledgement. Every frame is:
//!
//! | Direction | Bytes |
//! |---|---|
//! | Host handshake | `55 20 21 01 23 FF DC 02` |
//! | Radio identity | 61 bytes, carrying `"KT-500"` and `"WP9114"` |
//! | Host read | `06 53 AH AL LL` |
//! | Read reply | `[06] 58 AH AL LL` + `LL` data bytes |
//! | Host write | `06 58 AH AL LL` + `LL` data bytes |
//! | Write reply | bare `06` |
//!
//! The bracketed `06` on the read reply is genuinely optional on the wire: the
//! `0x3DF0` prologue reply omits it, every data reply carries it. See
//! [`read_block`].
//!
//! Every function here operates over a [`Transport`], so the whole protocol is
//! testable against [`crate::qyt::mock::QytMock`] with no hardware attached.

use crate::error::{Error, Result};
use crate::transport::Transport;

/// Opening bytes the host sends to wake the radio into programming mode.
pub const HANDSHAKE: [u8; 8] = [0x55, 0x20, 0x21, 0x01, 0x23, 0xFF, 0xDC, 0x02];

/// Length of the identity block as it appears in the vendor CPS captures: a
/// 10-byte status frame followed by the 51-byte body.
pub const IDENTITY_LEN: usize = 61;

/// Length of the status frame that may precede the identity body.
///
/// The captures begin `A1 20 00 00 00 00 02 00 02 00`, and the radio emits the
/// same shape with byte 8 cleared when the port closes. That makes this a
/// **port-state frame, not part of the handshake reply**: the captures record
/// the whole stream from port-open to port-close, so it merely sat at the front.
/// Confirmed on hardware 2026-08-06, where a real KT-5000 answered the handshake
/// with the body alone and no status frame at all. Treat it as optional.
pub const STATUS_LEN: usize = 10;

/// Length of the identity body proper, beginning with [`STX`].
pub const IDENTITY_BODY_LEN: usize = IDENTITY_LEN - STATUS_LEN;

/// First byte of the optional status frame.
const STATUS_LEAD: u8 = 0xA1;

/// Model tag embedded in the identity block. Note the radio reports `KT-500`,
/// six characters, not `KT-5000`; the trailing digit is only in the `Model:`
/// string at image offset `0x0FE0`.
pub const MODEL_TAG: &[u8; 6] = b"KT-500";

/// Firmware/variant fingerprint embedded in the identity block.
pub const FINGERPRINT: &[u8; 6] = b"WP9114";

/// Bytes per block on the read path.
pub const READ_LEN: u8 = 0x40;

/// Bytes per block on the write path.
pub const WRITE_LEN: u8 = 0x10;

/// Address the vendor CPS probes once at the start of every session, read or
/// write. Its purpose is unknown: the 16 bytes it returns are 12 spaces then
/// four zeros, which reads either as a firmware/region string the CPS ignores
/// or as a plain link-alive check. Reproduced because it is unambiguously safer
/// to keep than to drop.
pub const PROLOGUE_ADDR: u16 = 0x3DF0;

/// Bytes returned by the prologue probe.
pub const PROLOGUE_LEN: u8 = 0x10;

/// Command prefix on host frames, and the radio's write acknowledgement.
pub const STX: u8 = 0x06;

/// Second byte of a host read command.
const CMD_READ: u8 = 0x53;

/// Second byte of a host write command, and of every read reply.
const CMD_WRITE: u8 = 0x58;

/// Offset of the ASCII model tag within the identity **body**. (Add
/// [`STATUS_LEN`] for the offset within a capture that carries a status frame.)
const MODEL_TAG_OFFSET: usize = 11;

/// Offset of the ASCII fingerprint within the identity body.
const FINGERPRINT_OFFSET: usize = 27;

/// Length of both ASCII fields in the identity block.
const TAG_LEN: usize = 6;

/// What the radio reports about itself after the handshake. The body is
/// retained whole because most of its bytes are undecoded and a future hardware
/// pass may want them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// ASCII model tag, expected to be `KT-500`.
    pub model_tag: String,
    /// ASCII firmware/variant fingerprint, expected to be `WP9114`.
    pub fingerprint: String,
    /// The identity body exactly as received, starting at its [`STX`].
    pub raw: [u8; IDENTITY_BODY_LEN],
    /// The port-state frame, when the radio sent one ahead of the body. Absent
    /// on a link that is already open, which is the common case in practice.
    pub status: Option<[u8; STATUS_LEN]>,
}

impl Identity {
    /// Whether this identity is the KT-5000 we know how to program. Callers
    /// decide what to do about a mismatch; the protocol layer does not refuse
    /// to talk to an unknown radio.
    pub fn is_kt5000(&self) -> bool {
        self.model_tag.as_bytes() == MODEL_TAG && self.fingerprint.as_bytes() == FINGERPRINT
    }
}

/// Render a fixed-width ASCII field from the identity block, dropping padding.
fn ascii_field(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_matches(|c: char| c == '\0' || c.is_whitespace())
        .to_string()
}

/// Send the handshake and parse the 61-byte identity block the radio replies
/// with. Validates only the `A1 20` framing prefix; the model tag and
/// fingerprint are returned for the caller to judge.
pub fn handshake(t: &mut dyn Transport) -> Result<Identity> {
    t.write_all(&HANDSHAKE)?;

    // The radio may lead with a port-state frame or go straight to the body.
    // Peek one byte to tell the two apart rather than assuming either shape.
    let mut lead = [0u8; 1];
    t.read_exact(&mut lead)?;

    let status = if lead[0] == STATUS_LEAD {
        let mut rest = [0u8; STATUS_LEN - 1];
        t.read_exact(&mut rest)?;
        let mut frame = [0u8; STATUS_LEN];
        frame[0] = lead[0];
        frame[1..].copy_from_slice(&rest);
        // Body follows the status frame; consume its first byte.
        t.read_exact(&mut lead)?;
        Some(frame)
    } else {
        None
    };

    if lead[0] != STX {
        return Err(Error::Protocol(format!(
            "expected 0x{STX:02X} to open the QYT identity body, got 0x{:02X}",
            lead[0]
        )));
    }

    let mut raw = [0u8; IDENTITY_BODY_LEN];
    raw[0] = lead[0];
    t.read_exact(&mut raw[1..])?;

    Ok(Identity {
        model_tag: ascii_field(&raw[MODEL_TAG_OFFSET..MODEL_TAG_OFFSET + TAG_LEN]),
        fingerprint: ascii_field(&raw[FINGERPRINT_OFFSET..FINGERPRINT_OFFSET + TAG_LEN]),
        raw,
        status,
    })
}

/// Read `len` bytes at `addr` into `out`. Sends `06 53 AH AL LL` and validates
/// the echoed address and length in the reply before filling `out`.
///
/// The reply's leading `0x06` is treated as **optional**. That is not
/// defensive slop, it is the observed wire behaviour: the `0x3DF0` prologue
/// reply arrives as `58 3D F0 10 ...` while every data reply arrives as
/// `06 58 AH AL LL ...`. Tolerating both here is what lets one function serve
/// both cases, and it incidentally absorbs stray link chatter, so the driver
/// needs no resync machinery.
pub fn read_block(t: &mut dyn Transport, addr: u16, len: u8, out: &mut [u8]) -> Result<()> {
    if out.len() != len as usize {
        return Err(Error::InvalidArgument(format!(
            "read buffer is {} bytes but {len} were requested at 0x{addr:04X}",
            out.len()
        )));
    }

    t.write_all(&[STX, CMD_READ, (addr >> 8) as u8, addr as u8, len])?;

    let mut lead = [0u8; 1];
    t.read_exact(&mut lead)?;
    if lead[0] == STX {
        t.read_exact(&mut lead)?;
    }
    if lead[0] != CMD_WRITE {
        return Err(Error::Protocol(format!(
            "expected 0x58 opening the read reply at 0x{addr:04X}, got 0x{:02X}",
            lead[0]
        )));
    }

    let mut hdr = [0u8; 3];
    t.read_exact(&mut hdr)?;

    let echoed = u16::from_be_bytes([hdr[0], hdr[1]]);
    if echoed != addr {
        return Err(Error::Protocol(format!(
            "read reply address 0x{echoed:04X} != requested 0x{addr:04X}"
        )));
    }
    if hdr[2] != len {
        return Err(Error::Protocol(format!(
            "read reply length {} != requested {len} at 0x{addr:04X}",
            hdr[2]
        )));
    }

    t.read_exact(out)?;
    Ok(())
}

/// Write `data` at `addr`. Sends `06 58 AH AL LL` + data and requires a bare
/// `0x06` acknowledgement. There is no checksum to compute and none to check.
pub fn write_block(t: &mut dyn Transport, addr: u16, data: &[u8]) -> Result<()> {
    let len = u8::try_from(data.len()).map_err(|_| {
        Error::InvalidArgument(format!(
            "write block at 0x{addr:04X} is {} bytes, which does not fit the length byte",
            data.len()
        ))
    })?;
    if len == 0 {
        return Err(Error::InvalidArgument(format!(
            "refusing to write an empty block at 0x{addr:04X}"
        )));
    }

    let mut cmd = Vec::with_capacity(5 + data.len());
    cmd.extend_from_slice(&[STX, CMD_WRITE, (addr >> 8) as u8, addr as u8, len]);
    cmd.extend_from_slice(data);
    t.write_all(&cmd)?;

    let mut ack = [0u8; 1];
    t.read_exact(&mut ack)?;
    if ack[0] != STX {
        return Err(Error::Protocol(format!(
            "write at 0x{addr:04X} not acknowledged, got 0x{:02X}",
            ack[0]
        )));
    }
    Ok(())
}

/// Run the per-session probe at [`PROLOGUE_ADDR`] and return its 16 bytes. The
/// vendor CPS does this before every read and every write.
pub fn session_prologue(t: &mut dyn Transport) -> Result<[u8; PROLOGUE_LEN as usize]> {
    let mut buf = [0u8; PROLOGUE_LEN as usize];
    read_block(t, PROLOGUE_ADDR, PROLOGUE_LEN, &mut buf)?;
    Ok(buf)
}

/// Close out a read session with the single bare `0x06` the vendor CPS sends.
/// There is no reply and there is no exit command; the radio reboots when the
/// port closes.
pub fn end_read_session(t: &mut dyn Transport) -> Result<()> {
    t.write_all(&[STX])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qyt::mock::QytMock;

    #[test]
    fn handshake_reports_the_model_tag_and_fingerprint() {
        let mut m = QytMock::new();
        let id = handshake(&mut m).unwrap();
        assert_eq!(id.model_tag, "KT-500");
        assert_eq!(id.fingerprint, "WP9114");
        assert!(id.is_kt5000());
        assert_eq!(m.written, HANDSHAKE.to_vec());
    }

    #[test]
    fn handshake_rejects_a_reply_without_the_a1_20_framing() {
        let mut m = QytMock::new();
        m.identity[0] = 0x00;
        let err = handshake(&mut m).unwrap_err();
        assert!(matches!(err, Error::Protocol(_)), "got {err:?}");
    }

    #[test]
    fn read_block_command_framing_is_exact() {
        let mut m = QytMock::new();
        m.seed(0x0040, &[0xAA; 0x40]);
        let mut out = [0u8; 0x40];
        read_block(&mut m, 0x0040, READ_LEN, &mut out).unwrap();
        assert_eq!(m.written, vec![0x06, 0x53, 0x00, 0x40, 0x40]);
        assert_eq!(out, [0xAA; 0x40]);
    }

    #[test]
    fn read_block_tolerates_a_leading_stx_and_its_absence() {
        // Data replies carry the leading 0x06.
        let mut with = QytMock::new();
        with.seed(0x0100, &[0x11; 0x40]);
        let mut a = [0u8; 0x40];
        read_block(&mut with, 0x0100, READ_LEN, &mut a).unwrap();
        assert_eq!(a, [0x11; 0x40]);

        // The prologue reply does not, and neither does a mock told to drop it.
        let mut without = QytMock::new();
        without.omit_stx_on_data_replies = true;
        without.seed(0x0100, &[0x22; 0x40]);
        let mut b = [0u8; 0x40];
        read_block(&mut without, 0x0100, READ_LEN, &mut b).unwrap();
        assert_eq!(b, [0x22; 0x40]);
    }

    #[test]
    fn read_block_rejects_a_mismatched_echoed_address() {
        let mut m = QytMock::new();
        m.corrupt_read_addr = true;
        let mut out = [0u8; 0x40];
        let err = read_block(&mut m, 0x0080, READ_LEN, &mut out).unwrap_err();
        match err {
            Error::Protocol(msg) => assert!(msg.contains("!= requested 0x0080"), "{msg}"),
            other => panic!("expected a protocol error, got {other:?}"),
        }
    }

    #[test]
    fn read_block_rejects_a_mismatched_echoed_length() {
        let mut m = QytMock::new();
        m.corrupt_read_len = true;
        let mut out = [0u8; 0x40];
        let err = read_block(&mut m, 0x0000, READ_LEN, &mut out).unwrap_err();
        match err {
            Error::Protocol(msg) => assert!(msg.contains("length"), "{msg}"),
            other => panic!("expected a protocol error, got {other:?}"),
        }
    }

    #[test]
    fn read_block_rejects_a_buffer_that_does_not_match_the_length() {
        let mut m = QytMock::new();
        let mut out = [0u8; 0x20];
        let err = read_block(&mut m, 0x0000, READ_LEN, &mut out).unwrap_err();
        assert!(matches!(err, Error::InvalidArgument(_)), "got {err:?}");
        // Nothing should have gone out on the wire.
        assert!(m.written.is_empty());
    }

    #[test]
    fn a_short_reply_surfaces_as_a_timeout_rather_than_a_hang() {
        let mut m = QytMock::new();
        m.truncate_read_reply = true;
        let mut out = [0u8; 0x40];
        let err = read_block(&mut m, 0x0000, READ_LEN, &mut out).unwrap_err();
        match err {
            Error::Io(e) => assert_eq!(e.kind(), std::io::ErrorKind::TimedOut),
            other => panic!("expected a timeout, got {other:?}"),
        }
    }

    #[test]
    fn write_block_command_framing_is_exact() {
        let mut m = QytMock::new();
        let data = [0x5Au8; 0x10];
        write_block(&mut m, 0x0030, &data).unwrap();
        let mut expected = vec![0x06, 0x58, 0x00, 0x30, 0x10];
        expected.extend_from_slice(&data);
        assert_eq!(m.written, expected);
        assert_eq!(&m.memory()[0x0030..0x0040], &data);
    }

    #[test]
    fn write_block_rejects_an_empty_block() {
        let mut m = QytMock::new();
        let err = write_block(&mut m, 0x0000, &[]).unwrap_err();
        assert!(matches!(err, Error::InvalidArgument(_)), "got {err:?}");
    }

    #[test]
    fn write_block_surfaces_a_non_ack_reply() {
        let mut m = QytMock::new();
        m.nak_writes = true;
        let err = write_block(&mut m, 0x0000, &[0x00; 0x10]).unwrap_err();
        match err {
            Error::Protocol(msg) => assert!(msg.contains("not acknowledged"), "{msg}"),
            other => panic!("expected a protocol error, got {other:?}"),
        }
    }

    #[test]
    fn session_prologue_reads_sixteen_bytes_at_0x3df0() {
        let mut m = QytMock::new();
        let got = session_prologue(&mut m).unwrap();
        assert_eq!(m.written, vec![0x06, 0x53, 0x3D, 0xF0, 0x10]);
        // What the real radio returns: twelve spaces then four zeros.
        assert_eq!(&got, b"            \x00\x00\x00\x00");
    }

    #[test]
    fn end_read_session_sends_one_bare_stx() {
        let mut m = QytMock::new();
        end_read_session(&mut m).unwrap();
        assert_eq!(m.written, vec![0x06]);
    }

    #[test]
    fn write_then_read_roundtrips_a_block() {
        let mut m = QytMock::new();
        let data = [0x3Cu8; 0x10];
        write_block(&mut m, 0x0200, &data).unwrap();
        let mut out = [0u8; 0x10];
        read_block(&mut m, 0x0200, 0x10, &mut out).unwrap();
        assert_eq!(out, data);
    }
}
