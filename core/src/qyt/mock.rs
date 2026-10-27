//! In-memory fake QYT KT-5000 used by the test suite.
//!
//! Mirrors [`crate::anytone::mock::MockTransport`] in structure but speaks the
//! QYT grammar instead: the `55 20 …` handshake, `06 53` reads and `06 58`
//! writes, and the asymmetric leading `0x06` on read replies. Every byte the
//! host sends is recorded in [`QytMock::written`] so tests can assert the exact
//! request stream against the vendor CPS captures.

use std::collections::VecDeque;

use crate::error::{Error, Result};
use crate::qyt::protocol::{HANDSHAKE, IDENTITY_LEN, PROLOGUE_ADDR, PROLOGUE_LEN, STX};
use crate::transport::Transport;

/// Second byte of a host read command.
const CMD_READ: u8 = 0x53;

/// Second byte of a host write command.
const CMD_WRITE: u8 = 0x58;

/// Size of the mock's address space. The programmable image ends at `0x3200`;
/// the extra span exists so the `0x3DF0` prologue probe has somewhere to land.
pub const MOCK_MEMORY_SIZE: usize = PROLOGUE_ADDR as usize + PROLOGUE_LEN as usize;

/// The identity block a real KT-5000 returns, byte for byte from the capture in
/// `docs/reference/QYT/read-radio_read`.
const REAL_IDENTITY: [u8; IDENTITY_LEN] = [
    0xA1, 0x20, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x02, 0x00, 0x06, //
    0x01, 0x03, 0x00, 0x01, 0x07, 0x09, 0x00, 0x01, 0x00, 0x00, //
    b'K', b'T', b'-', b'5', b'0', b'0', //
    0x01, 0x03, 0x00, 0x01, 0x07, 0x09, 0x00, 0x01, 0x00, 0x00, //
    b'W', b'P', b'9', b'1', b'1', b'4', //
    0x04, 0x00, 0x00, 0x04, 0x08, 0x00, 0x00, 0x01, //
    0x04, 0x00, 0x00, 0x04, 0x08, 0x00, 0x00, 0x01, //
    0x55, 0x05,
];

/// The sixteen bytes the real radio returns from the prologue probe: twelve
/// spaces then four zeros.
const PROLOGUE_CONTENT: [u8; PROLOGUE_LEN as usize] = [
    0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x00, 0x00, 0x00, 0x00,
];

/// In-memory fake KT-5000. Holds a memory image initialised to `0xFF` (what an
/// erased radio reads back as) and answers the real command grammar.
pub struct QytMock {
    /// The radio's memory image; reads and writes address into this vector.
    memory: Vec<u8>,
    /// Bytes staged for the host to read back.
    out: VecDeque<u8>,
    /// Bytes written by the host that have not yet formed a complete command.
    pending_in: Vec<u8>,
    /// Every byte the host has written, in order, for exact-stream assertions.
    pub written: Vec<u8>,
    /// Every read command received, as `(address, length)`.
    pub reads: Vec<(u16, u8)>,
    /// Every write command received, as `(address, length)`.
    pub writes: Vec<(u16, usize)>,
    /// Identity block returned after the handshake. Public so a test can
    /// corrupt it.
    pub identity: [u8; IDENTITY_LEN],
    /// When true, data read replies drop their leading `0x06`, exercising the
    /// protocol layer's tolerance of the prologue reply's framing.
    pub omit_stx_on_data_replies: bool,
    /// Whether a handshake has already been accepted on this link. The real
    /// radio takes exactly one per open port.
    handshaken: bool,
    /// When true, read replies echo a deliberately wrong address.
    pub corrupt_read_addr: bool,
    /// When true, read replies echo a deliberately wrong length.
    pub corrupt_read_len: bool,
    /// When true, read replies are cut short so the host runs out of data.
    pub truncate_read_reply: bool,
    /// When true, writes are answered with `0x15` instead of an ACK.
    pub nak_writes: bool,
    /// When true, the second write of a session is acked but not stored, which
    /// is what the real radio does (measured 2026-09-09 and 2026-10-16).
    pub drop_second_write: bool,
    /// Write commands received since the last [`QytMock::reboot`].
    session_writes: usize,
}

impl Default for QytMock {
    /// Same as [`QytMock::new`].
    fn default() -> Self {
        Self::new()
    }
}

impl QytMock {
    /// Build a mock radio with an all-`0xFF` image and the prologue window
    /// seeded with what the real radio returns.
    pub fn new() -> Self {
        let mut memory = vec![0xFFu8; MOCK_MEMORY_SIZE];
        let p = PROLOGUE_ADDR as usize;
        memory[p..p + PROLOGUE_CONTENT.len()].copy_from_slice(&PROLOGUE_CONTENT);
        Self {
            memory,
            out: VecDeque::new(),
            pending_in: Vec::new(),
            written: Vec::new(),
            reads: Vec::new(),
            writes: Vec::new(),
            identity: REAL_IDENTITY,
            handshaken: false,
            omit_stx_on_data_replies: false,
            corrupt_read_addr: false,
            corrupt_read_len: false,
            truncate_read_reply: false,
            nak_writes: false,
            drop_second_write: false,
            session_writes: 0,
        }
    }

    /// Model the port closing and the radio rebooting, which is the only way a
    /// real KT-5000 becomes ready for another session. Memory survives; the
    /// session state does not.
    ///
    /// A test that drives two operations in a row must call this between them,
    /// exactly as a caller must close and reopen the port.
    pub fn reboot(&mut self) {
        self.handshaken = false;
        self.session_writes = 0;
        self.out.clear();
        self.pending_in.clear();
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
                // Handshake -> the 61-byte identity block.
                0x55 => {
                    if self.pending_in.len() < HANDSHAKE.len() {
                        return Ok(());
                    }
                    if self.pending_in[..HANDSHAKE.len()] != HANDSHAKE {
                        return Err(Error::Protocol("malformed QYT handshake".into()));
                    }
                    self.pending_in.drain(..HANDSHAKE.len());
                    // A real KT-5000 accepts exactly one handshake per open
                    // link. It has no exit command, so the only way to start a
                    // second session is to close the port and let it reboot.
                    // Modelling that is what makes an identify-then-write
                    // sequence fail here instead of only on hardware.
                    if self.handshaken {
                        return Err(Error::Protocol(
                            "second handshake on an open QYT link: each driver \
                             method is a whole session, so close the port and \
                             let the radio reboot between operations"
                                .into(),
                        ));
                    }
                    self.handshaken = true;
                    self.out.extend(self.identity.iter().copied());
                }
                // Every other host frame opens with 0x06. A lone trailing 0x06
                // is the end-of-read-session marker: it never completes a
                // command, so it simply sits in the buffer, exactly as it does
                // on the real link.
                STX => {
                    if self.pending_in.len() < 5 {
                        return Ok(());
                    }
                    let addr = u16::from_be_bytes([self.pending_in[2], self.pending_in[3]]);
                    let len = self.pending_in[4] as usize;
                    match self.pending_in[1] {
                        CMD_READ => {
                            self.pending_in.drain(..5);
                            self.reads.push((addr, len as u8));
                            self.stage_read_response(addr, len)?;
                        }
                        CMD_WRITE => {
                            if self.pending_in.len() < 5 + len {
                                return Ok(());
                            }
                            let data: Vec<u8> = self.pending_in[5..5 + len].to_vec();
                            self.pending_in.drain(..5 + len);
                            self.writes.push((addr, len));
                            let start = addr as usize;
                            if start + len > self.memory.len() {
                                return Err(Error::Protocol(format!(
                                    "write out of range: 0x{addr:04X}+{len}"
                                )));
                            }
                            self.session_writes += 1;
                            if self.nak_writes {
                                self.out.push_back(0x15);
                            } else if self.drop_second_write && self.session_writes == 2 {
                                self.out.push_back(STX);
                            } else {
                                self.memory[start..start + len].copy_from_slice(&data);
                                self.out.push_back(STX);
                            }
                        }
                        other => {
                            return Err(Error::Protocol(format!(
                                "unrecognized QYT command byte 0x{other:02X}"
                            )));
                        }
                    }
                }
                other => {
                    return Err(Error::Protocol(format!(
                        "unrecognized QYT frame opener 0x{other:02X}"
                    )));
                }
            }
        }
    }

    /// Stage a read response for the requested block. The prologue address is
    /// answered without the leading `0x06`, every other address with it, which
    /// is the asymmetry the real radio exhibits.
    fn stage_read_response(&mut self, addr: u16, len: usize) -> Result<()> {
        let start = addr as usize;
        if start + len > self.memory.len() {
            return Err(Error::Protocol(format!(
                "read out of range: 0x{addr:04X}+{len}"
            )));
        }

        let lead_stx = addr != PROLOGUE_ADDR && !self.omit_stx_on_data_replies;
        if lead_stx {
            self.out.push_back(STX);
        }
        self.out.push_back(CMD_WRITE);

        let echoed = if self.corrupt_read_addr {
            addr.wrapping_add(0x40)
        } else {
            addr
        };
        self.out.extend(echoed.to_be_bytes());
        self.out
            .push_back(if self.corrupt_read_len { 1 } else { len as u8 });

        // A truncated reply is how the "short reply must time out" test starves
        // the host of the tail of the block.
        let staged = if self.truncate_read_reply {
            len / 2
        } else {
            len
        };
        self.out
            .extend(self.memory[start..start + staged].iter().copied());
        Ok(())
    }
}

impl Transport for QytMock {
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
                "mock QYT radio has no more data to return",
            )));
        }
        for slot in buf.iter_mut() {
            *slot = self.out.pop_front().expect("length checked above");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_handshake_returns_the_identity_block() {
        let mut m = QytMock::new();
        m.write_all(&HANDSHAKE).unwrap();
        let mut buf = [0u8; IDENTITY_LEN];
        m.read_exact(&mut buf).unwrap();
        assert_eq!(buf, REAL_IDENTITY);
        assert_eq!(&buf[21..27], b"KT-500");
        assert_eq!(&buf[37..43], b"WP9114");
    }

    #[test]
    fn mock_data_read_reply_carries_a_leading_stx() {
        let mut m = QytMock::new();
        m.write_all(&[0x06, 0x53, 0x00, 0x00, 0x40]).unwrap();
        let mut head = [0u8; 5];
        m.read_exact(&mut head).unwrap();
        assert_eq!(head, [0x06, 0x58, 0x00, 0x00, 0x40]);
    }

    #[test]
    fn mock_prologue_reply_omits_the_leading_stx() {
        let mut m = QytMock::new();
        m.write_all(&[0x06, 0x53, 0x3D, 0xF0, 0x10]).unwrap();
        let mut head = [0u8; 4];
        m.read_exact(&mut head).unwrap();
        assert_eq!(head, [0x58, 0x3D, 0xF0, 0x10]);
    }

    #[test]
    fn mock_write_stores_the_block_and_acks() {
        let mut m = QytMock::new();
        let mut cmd = vec![0x06, 0x58, 0x01, 0x00, 0x10];
        cmd.extend_from_slice(&[0x7Eu8; 0x10]);
        m.write_all(&cmd).unwrap();
        let mut ack = [0u8; 1];
        m.read_exact(&mut ack).unwrap();
        assert_eq!(ack[0], 0x06);
        assert_eq!(&m.memory()[0x0100..0x0110], &[0x7Eu8; 0x10]);
        assert_eq!(m.writes, vec![(0x0100, 0x10)]);
    }

    #[test]
    fn mock_buffers_a_lone_end_of_session_stx_without_erroring() {
        let mut m = QytMock::new();
        m.write_all(&[0x06]).unwrap();
        assert_eq!(m.written, vec![0x06]);
    }

    #[test]
    fn mock_starts_out_erased() {
        let m = QytMock::new();
        assert!(m.memory()[..0x3200].iter().all(|&b| b == 0xFF));
    }
}
