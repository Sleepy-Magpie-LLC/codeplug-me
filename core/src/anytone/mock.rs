//! In-memory fake AnyTone radio used by the test suite.
//!
//! This lives under [`crate::anytone`] rather than next to the generic
//! [`crate::transport::Transport`] trait because it hardcodes the AnyTone
//! command grammar (PROGRAM / identify / READ / WRITE / END) and the device's
//! flash sector-erase behaviour. A second radio model needs its own mock.

use std::collections::VecDeque;

use crate::anytone::protocol::{checksum, BLOCK_SIZE};
use crate::error::{Error, Result};
use crate::transport::Transport;

/// In-memory fake radio used by the test suite. It holds a memory image and
/// speaks the exact PROGRAM / identify / READ / WRITE / END handshake the real
/// device does, including checksum computation and validation. Every byte the
/// host writes is recorded in `written` so tests can assert the exact request
/// stream.
pub struct MockTransport {
    /// The radio's memory image; reads and writes address into this vector.
    memory: Vec<u8>,
    /// ASCII model/version string returned by the identify command.
    model: String,
    /// Whether the host has entered program mode via `"PROGRAM"`.
    in_program_mode: bool,
    /// Bytes staged for the host to read back.
    out: VecDeque<u8>,
    /// Bytes written by the host that have not yet formed a complete command.
    pending_in: Vec<u8>,
    /// Every byte the host has written, in order, for exact-stream assertions.
    pub written: Vec<u8>,
    /// When true, read responses carry a deliberately wrong checksum so tests
    /// can exercise the protocol layer's checksum rejection.
    pub corrupt_read_checksum: bool,
    /// Sectors already erased in the current program-mode session. The radio
    /// erases flash in [`MOCK_SECTOR_SIZE`]-byte sectors: the first write into a
    /// sector blanks the whole sector to `0xff` before programming the block.
    /// Modelling this is the only way a test can catch a write that destroys
    /// neighbouring bytes it never sent, the bug that cost a debugging session.
    erased_sectors: std::collections::HashSet<u32>,
}

/// Flash erase granularity the mock models. Mirrors
/// `crate::anytone::device::SECTOR_SIZE`; a test asserts the mock actually
/// erases at this boundary.
const MOCK_SECTOR_SIZE: u32 = 0x8000;

impl MockTransport {
    /// Build a mock radio with a memory image of `size` zero bytes and the
    /// given identify string (e.g. `"ID878UV"`).
    pub fn new(size: usize, model: &str) -> Self {
        Self {
            memory: vec![0u8; size],
            model: model.to_string(),
            in_program_mode: false,
            out: VecDeque::new(),
            pending_in: Vec::new(),
            written: Vec::new(),
            corrupt_read_checksum: false,
            erased_sectors: std::collections::HashSet::new(),
        }
    }

    /// Read-only view of the current memory image (for test assertions).
    pub fn memory(&self) -> &[u8] {
        &self.memory
    }

    /// Overwrite a span of the memory image directly, bypassing the protocol.
    /// Handy for seeding fixtures a test then reads back over the wire.
    pub fn seed(&mut self, addr: usize, data: &[u8]) {
        self.memory[addr..addr + data.len()].copy_from_slice(data);
    }

    /// Consume as many complete commands as are buffered in `pending_in`,
    /// staging the matching responses into `out`. Called after every write.
    fn process(&mut self) -> Result<()> {
        loop {
            let Some(&first) = self.pending_in.first() else {
                return Ok(());
            };
            match first {
                // "PROGRAM" -> "QX" + ACK.
                b'P' => {
                    const CMD: &[u8] = b"PROGRAM";
                    if self.pending_in.len() < CMD.len() {
                        return Ok(());
                    }
                    if &self.pending_in[..CMD.len()] != CMD {
                        return Err(Error::Protocol("malformed PROGRAM command".into()));
                    }
                    self.pending_in.drain(..CMD.len());
                    self.in_program_mode = true;
                    // A fresh session: no sector has been erased yet.
                    self.erased_sectors.clear();
                    self.out.extend([b'Q', b'X', 0x06]);
                }
                // Identify (0x02) -> 'I' + model + ACK.
                0x02 => {
                    self.pending_in.drain(..1);
                    self.out.push_back(b'I');
                    self.out.extend(self.model.as_bytes().iter().copied());
                    self.out.push_back(0x06);
                }
                // Read block: 'R' + addr[4] + len.
                b'R' => {
                    if self.pending_in.len() < 6 {
                        return Ok(());
                    }
                    let addr = u32::from_be_bytes([
                        self.pending_in[1],
                        self.pending_in[2],
                        self.pending_in[3],
                        self.pending_in[4],
                    ]);
                    let len = self.pending_in[5] as usize;
                    self.pending_in.drain(..6);
                    self.stage_read_response(addr, len)?;
                }
                // Write block: 'W' + addr[4] + len + data[len] + checksum +
                // trailing 0x06 (the frame the real radio requires).
                b'W' => {
                    // Need at least the header to know the length.
                    if self.pending_in.len() < 6 {
                        return Ok(());
                    }
                    let len = self.pending_in[5] as usize;
                    let total = 6 + len + 1 + 1; // + checksum + trailing ACK
                    if self.pending_in.len() < total {
                        return Ok(());
                    }
                    let addr = u32::from_be_bytes([
                        self.pending_in[1],
                        self.pending_in[2],
                        self.pending_in[3],
                        self.pending_in[4],
                    ]);
                    let body: Vec<u8> = self.pending_in[1..6 + len].to_vec();
                    let claimed = self.pending_in[6 + len];
                    let trailer = self.pending_in[6 + len + 1];
                    self.pending_in.drain(..total);
                    let expected = checksum(&body);
                    if claimed != expected || trailer != 0x06 {
                        // Real device would reject; signal with a NAK byte.
                        self.out.push_back(0x15);
                    } else {
                        // First write into a sector erases the whole sector to
                        // 0xff before the block is programmed, exactly as the
                        // radio's flash does.
                        let sector = addr / MOCK_SECTOR_SIZE;
                        if self.erased_sectors.insert(sector) {
                            let s = (sector * MOCK_SECTOR_SIZE) as usize;
                            let e = (s + MOCK_SECTOR_SIZE as usize).min(self.memory.len());
                            for byte in &mut self.memory[s..e] {
                                *byte = 0xff;
                            }
                        }
                        let start = addr as usize;
                        self.memory[start..start + len].copy_from_slice(&body[5..5 + len]);
                        self.out.push_back(0x06);
                    }
                }
                // "END" -> ACK.
                b'E' => {
                    const CMD: &[u8] = b"END";
                    if self.pending_in.len() < CMD.len() {
                        return Ok(());
                    }
                    if &self.pending_in[..CMD.len()] != CMD {
                        return Err(Error::Protocol("malformed END command".into()));
                    }
                    self.pending_in.drain(..CMD.len());
                    self.in_program_mode = false;
                    self.out.push_back(0x06);
                }
                other => {
                    return Err(Error::Protocol(format!(
                        "unrecognized command byte 0x{other:02X}"
                    )));
                }
            }
        }
    }

    /// Stage a `'W' + addr + len + data + checksum + ACK` read response for the
    /// requested block, matching the real device's framing.
    fn stage_read_response(&mut self, addr: u32, len: usize) -> Result<()> {
        let start = addr as usize;
        if start + len > self.memory.len() {
            return Err(Error::Protocol(format!(
                "read out of range: 0x{addr:08X}+{len}"
            )));
        }
        let mut body = Vec::with_capacity(5 + len);
        body.extend_from_slice(&addr.to_be_bytes());
        body.push(len as u8);
        body.extend_from_slice(&self.memory[start..start + len]);
        let mut cs = checksum(&body);
        if self.corrupt_read_checksum {
            cs = cs.wrapping_add(1);
        }
        self.out.push_back(b'W');
        self.out.extend(body);
        self.out.push_back(cs);
        self.out.push_back(0x06);
        Ok(())
    }
}

impl Transport for MockTransport {
    /// Record the bytes, feed them to the command parser, and stage responses.
    fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        self.written.extend_from_slice(buf);
        self.pending_in.extend_from_slice(buf);
        self.process()
    }

    /// Drain exactly `buf.len()` staged response bytes, or report a timeout if
    /// the host tried to read more than the fake radio produced.
    fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        if self.out.len() < buf.len() {
            return Err(Error::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "mock radio has no more data to return",
            )));
        }
        for slot in buf.iter_mut() {
            *slot = self.out.pop_front().expect("length checked above");
        }
        Ok(())
    }
}

/// The fixed 16-byte block size the codeplug data path uses. Re-exported here
/// for transport-level callers; the canonical definition lives in `protocol`.
pub const MOCK_BLOCK_SIZE: usize = BLOCK_SIZE;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_program_handshake_replies_qx_ack() {
        let mut m = MockTransport::new(0x100, "ID878UV");
        m.write_all(b"PROGRAM").unwrap();
        let mut buf = [0u8; 3];
        m.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"QX\x06");
    }

    #[test]
    fn mock_identify_returns_model() {
        let mut m = MockTransport::new(0x100, "ID878UV");
        m.write_all(&[0x02]).unwrap();
        let mut head = [0u8; 1];
        m.read_exact(&mut head).unwrap();
        assert_eq!(head[0], b'I');
        let mut model = [0u8; 7];
        m.read_exact(&mut model).unwrap();
        assert_eq!(&model, b"ID878UV");
        let mut ack = [0u8; 1];
        m.read_exact(&mut ack).unwrap();
        assert_eq!(ack[0], 0x06);
    }

    #[test]
    fn mock_write_then_read_roundtrips() {
        let mut m = MockTransport::new(0x100, "ID878UV");
        let data = [0xAAu8; MOCK_BLOCK_SIZE];
        // Build a write command by hand to exercise the mock directly.
        let addr: u32 = 0x20;
        let mut body = Vec::new();
        body.extend_from_slice(&addr.to_be_bytes());
        body.push(MOCK_BLOCK_SIZE as u8);
        body.extend_from_slice(&data);
        let cs = checksum(&body);
        let mut cmd = vec![b'W'];
        cmd.extend_from_slice(&body);
        cmd.push(cs);
        cmd.push(0x06); // trailing byte the radio requires
        m.write_all(&cmd).unwrap();
        let mut ack = [0u8; 1];
        m.read_exact(&mut ack).unwrap();
        assert_eq!(ack[0], 0x06);
        assert_eq!(&m.memory()[0x20..0x30], &data);
    }

    #[test]
    fn mock_rejects_bad_write_checksum_with_nak() {
        let mut m = MockTransport::new(0x100, "ID878UV");
        let data = [0x11u8; MOCK_BLOCK_SIZE];
        let addr: u32 = 0x00;
        let mut cmd = vec![b'W'];
        cmd.extend_from_slice(&addr.to_be_bytes());
        cmd.push(MOCK_BLOCK_SIZE as u8);
        cmd.extend_from_slice(&data);
        cmd.push(0x00); // deliberately wrong checksum
        cmd.push(0x06); // trailing byte the radio requires
        m.write_all(&cmd).unwrap();
        let mut ack = [0u8; 1];
        m.read_exact(&mut ack).unwrap();
        assert_eq!(ack[0], 0x15);
    }
}
