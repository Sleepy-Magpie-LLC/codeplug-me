//! Low-level AnyTone programming protocol: the PROGRAM/identify/read/write/END
//! handshake and the block checksum. All multi-byte addresses are big-endian.
//!
//! Every function here operates over a [`Transport`], so the entire protocol is
//! testable against [`crate::anytone::mock::MockTransport`] with no hardware.

use crate::error::{Error, Result};
use crate::transport::Transport;

/// The codeplug data path transfers 16 bytes per block.
pub const BLOCK_SIZE: usize = 16;

/// ASCII length byte sent with a read command and echoed in a write command.
const LEN_BYTE: u8 = BLOCK_SIZE as u8;

/// ACK byte the radio appends to most responses.
const ACK: u8 = 0x06;

/// Compute the 1-byte block checksum: the sum of every byte from the first
/// address byte through the last data byte, modulo 256. It deliberately
/// excludes the leading `'R'`/`'W'` command byte and the trailing ACK.
pub fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
}

/// Enter program mode: send `"PROGRAM"` and require the `"QX"` + ACK reply.
pub fn enter_program_mode(t: &mut dyn Transport) -> Result<()> {
    t.write_all(b"PROGRAM")?;
    let mut reply = [0u8; 3];
    t.read_exact(&mut reply)?;
    if &reply != b"QX\x06" {
        return Err(Error::Protocol(format!(
            "expected QX+ACK entering program mode, got {reply:02X?}"
        )));
    }
    Ok(())
}

/// Send the identify command (`0x02`) and return the model/version string the
/// radio reports. The reply is `'I'` + ASCII string + ACK; we read up to a
/// bounded number of bytes and stop at the trailing ACK.
pub fn identify(t: &mut dyn Transport) -> Result<String> {
    t.write_all(&[0x02])?;
    let mut lead = [0u8; 1];
    t.read_exact(&mut lead)?;
    if lead[0] != b'I' {
        return Err(Error::Protocol(format!(
            "expected 'I' leading identify reply, got 0x{:02X}",
            lead[0]
        )));
    }
    // Read one byte at a time until the ACK terminator. The identify string is
    // short (well under 64 bytes); cap the loop so a misbehaving link can't
    // spin forever.
    let mut model = Vec::new();
    for _ in 0..64 {
        let mut b = [0u8; 1];
        t.read_exact(&mut b)?;
        if b[0] == ACK {
            let s = String::from_utf8_lossy(&model).trim().to_string();
            return Ok(s);
        }
        model.push(b[0]);
    }
    Err(Error::Protocol(
        "identify reply exceeded 64 bytes without an ACK".into(),
    ))
}

/// Read a single 16-byte block at `addr`. Sends `'R'` + addr[4 BE] + len, then
/// validates the `'W'` + addr[4] + len + data[16] + checksum + ACK response,
/// including address echo and checksum.
pub fn read_block(t: &mut dyn Transport, addr: u32) -> Result<[u8; BLOCK_SIZE]> {
    let mut cmd = [0u8; 6];
    cmd[0] = b'R';
    cmd[1..5].copy_from_slice(&addr.to_be_bytes());
    cmd[5] = LEN_BYTE;
    t.write_all(&cmd)?;

    // Response layout: 'W'(1) addr(4) len(1) data(16) checksum(1) ACK(1).
    let mut resp = [0u8; 1 + 4 + 1 + BLOCK_SIZE + 1 + 1];
    t.read_exact(&mut resp)?;

    if resp[0] != b'W' {
        return Err(Error::Protocol(format!(
            "expected 'W' in read reply, got 0x{:02X}",
            resp[0]
        )));
    }
    let echoed = u32::from_be_bytes([resp[1], resp[2], resp[3], resp[4]]);
    if echoed != addr {
        return Err(Error::Protocol(format!(
            "read reply address 0x{echoed:08X} != requested 0x{addr:08X}"
        )));
    }
    if resp[5] as usize != BLOCK_SIZE {
        return Err(Error::Protocol(format!(
            "read reply length {} != {BLOCK_SIZE}",
            resp[5]
        )));
    }
    let ack = resp[resp.len() - 1];
    if ack != ACK {
        return Err(Error::Protocol(format!(
            "read reply missing ACK, got 0x{ack:02X}"
        )));
    }

    // Checksum covers addr[4] + len + data[16] (bytes 1..=21 of the response).
    let claimed = resp[resp.len() - 2];
    let actual = checksum(&resp[1..1 + 4 + 1 + BLOCK_SIZE]);
    if claimed != actual {
        return Err(Error::Checksum {
            addr,
            expected: claimed,
            actual,
        });
    }

    let mut data = [0u8; BLOCK_SIZE];
    data.copy_from_slice(&resp[6..6 + BLOCK_SIZE]);
    Ok(data)
}

/// Write a single 16-byte block at `addr`. Sends `'W'` + addr[4 BE] + len +
/// data[16] + checksum + a trailing `0x06`, and requires a bare ACK in reply.
///
/// The trailing `0x06` is mandatory: qdmr's `WriteRequest` frames the command
/// as `'W' addr size data sum ack(=0x06)` and sends the whole 24-byte struct.
/// Omitting it leaves the radio waiting for one more byte, so it never ACKs and
/// the read times out — unlike the read command, which has no trailing byte.
pub fn write_block(t: &mut dyn Transport, addr: u32, data: &[u8; BLOCK_SIZE]) -> Result<()> {
    let mut cmd = Vec::with_capacity(1 + 4 + 1 + BLOCK_SIZE + 1 + 1);
    cmd.push(b'W');
    cmd.extend_from_slice(&addr.to_be_bytes());
    cmd.push(LEN_BYTE);
    cmd.extend_from_slice(data);
    // Checksum covers addr[4] + len + data[16], i.e. everything after 'W'.
    let cs = checksum(&cmd[1..]);
    cmd.push(cs);
    cmd.push(ACK);
    t.write_all(&cmd)?;

    let mut ack = [0u8; 1];
    t.read_exact(&mut ack)?;
    if ack[0] != ACK {
        return Err(Error::Protocol(format!(
            "write not acknowledged, got 0x{:02X}",
            ack[0]
        )));
    }
    Ok(())
}

/// Exit program mode: send `"END"` and require the ACK reply.
pub fn exit_program_mode(t: &mut dyn Transport) -> Result<()> {
    t.write_all(b"END")?;
    let mut ack = [0u8; 1];
    t.read_exact(&mut ack)?;
    if ack[0] != ACK {
        return Err(Error::Protocol(format!(
            "END not acknowledged, got 0x{:02X}",
            ack[0]
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anytone::mock::MockTransport;

    #[test]
    fn checksum_matches_hand_computed_vectors() {
        // Empty -> 0.
        assert_eq!(checksum(&[]), 0);
        // Simple sum without wrap.
        assert_eq!(checksum(&[0x01, 0x02, 0x03]), 0x06);
        // Wraps modulo 256: 0xFF + 0x02 = 0x101 -> 0x01.
        assert_eq!(checksum(&[0xFF, 0x02]), 0x01);
        // A realistic read block: addr 0x00000010, len 0x10, 16 bytes of 0x01.
        // sum = 0x10 + 0x10 + 16*0x01 = 0x30.
        let mut v = vec![0x00, 0x00, 0x00, 0x10, 0x10];
        v.extend(std::iter::repeat_n(0x01u8, 16));
        assert_eq!(checksum(&v), 0x30);
    }

    #[test]
    fn enter_identify_exit_roundtrip() {
        let mut m = MockTransport::new(0x100, "ID878UV V100");
        enter_program_mode(&mut m).unwrap();
        let model = identify(&mut m).unwrap();
        assert_eq!(model, "ID878UV V100");
        exit_program_mode(&mut m).unwrap();
        // Exact request stream: PROGRAM, 0x02, END.
        let mut expected = b"PROGRAM".to_vec();
        expected.push(0x02);
        expected.extend_from_slice(b"END");
        assert_eq!(m.written, expected);
    }

    #[test]
    fn write_then_read_block_roundtrip() {
        let mut m = MockTransport::new(0x100, "ID878UV");
        let data = [0x5Au8; BLOCK_SIZE];
        write_block(&mut m, 0x40, &data).unwrap();
        let got = read_block(&mut m, 0x40).unwrap();
        assert_eq!(got, data);
    }

    #[test]
    fn read_block_rejects_bad_checksum() {
        let mut m = MockTransport::new(0x100, "ID878UV");
        m.corrupt_read_checksum = true;
        let err = read_block(&mut m, 0x00).unwrap_err();
        match err {
            Error::Checksum { addr, .. } => assert_eq!(addr, 0x00),
            other => panic!("expected checksum error, got {other:?}"),
        }
    }

    #[test]
    fn write_command_has_correct_framing_and_checksum() {
        let mut m = MockTransport::new(0x100, "ID878UV");
        let data = [0x01u8; BLOCK_SIZE];
        write_block(&mut m, 0x10, &data).unwrap();
        // 'W' + addr(4) + len + data(16) + checksum + trailing ACK = 24 bytes.
        assert_eq!(m.written.len(), 24);
        assert_eq!(m.written[0], b'W');
        assert_eq!(&m.written[1..5], &0x10u32.to_be_bytes());
        assert_eq!(m.written[5], BLOCK_SIZE as u8);
        // checksum over bytes 1..=21 (addr+len+data), computed independently.
        let cs = checksum(&m.written[1..22]);
        assert_eq!(m.written[22], cs);
        // Trailing byte the radio requires.
        assert_eq!(m.written[23], 0x06);
    }
}
