//! Byte transport abstraction over the radio's USB serial link.
//!
//! The [`Transport`] trait is the minimal surface a protocol layer needs: a way
//! to push a request out and to pull an exact number of response bytes back
//! (subject to a timeout). It is radio-model agnostic; the per-model fake
//! radios the tests drive live beside their protocols, e.g.
//! [`crate::anytone::mock::MockTransport`].

#[cfg(feature = "desktop")]
use std::io::{Read, Write};
#[cfg(feature = "desktop")]
use std::time::Duration;

#[cfg(feature = "desktop")]
use crate::error::Error;
use crate::error::Result;

/// The minimal I/O surface the protocol layer drives.
pub trait Transport {
    /// Write the entire buffer to the device, blocking until it is flushed.
    fn write_all(&mut self, buf: &[u8]) -> Result<()>;

    /// Read exactly `buf.len()` bytes into `buf`, or fail (e.g. on timeout).
    fn read_exact(&mut self, buf: &mut [u8]) -> Result<()>;
}

/// Forward the transport through a mutable borrow. This is what lets a driver
/// that receives `&mut dyn Transport` (object safety) hand it to the generic
/// `Radio<T: Transport>` without any wrapper type.
impl<T: Transport + ?Sized> Transport for &mut T {
    /// Delegate to the borrowed transport.
    fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        (**self).write_all(buf)
    }

    /// Delegate to the borrowed transport.
    fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        (**self).read_exact(buf)
    }
}

/// Real desktop serial transport backed by the `serialport` crate.
///
/// Desktop-only. Android supplies its own transport from Kotlin through the
/// UniFFI surface in [`crate::api`], because `serialport` needs libudev.
#[cfg(feature = "desktop")]
pub struct SerialTransport {
    port: Box<dyn serialport::SerialPort>,
}

#[cfg(feature = "desktop")]
impl SerialTransport {
    /// Open the serial port at `path` (e.g. `/dev/cu.usbmodem1234`) with 8-N-1
    /// framing and the given per-read timeout. Baud is largely ignored by the
    /// CDC-ACM driver but a sane value is still set.
    pub fn open(path: &str, baud: u32, timeout: Duration) -> Result<Self> {
        let port = serialport::new(path, baud)
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::One)
            .flow_control(serialport::FlowControl::None)
            .timeout(timeout)
            .open()?;
        Ok(Self { port })
    }

    /// Read whatever has already arrived, up to `buf.len()` bytes, returning 0
    /// when the read timeout expires instead of failing.
    ///
    /// [`Transport::read_exact`] is the wrong shape for listening to a radio
    /// that is talking on its own schedule rather than answering a command:
    /// a boot banner has no length known in advance, and silence is data too.
    /// The KT-5000 rescue watcher is the only caller.
    pub fn read_some(&mut self, buf: &mut [u8]) -> Result<usize> {
        match self.port.read(buf) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(0),
            Err(e) => Err(Error::Io(e)),
        }
    }
}

#[cfg(feature = "desktop")]
impl Transport for SerialTransport {
    /// Push the whole buffer out and flush the OS write buffer.
    fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        self.port.write_all(buf)?;
        self.port.flush()?;
        Ok(())
    }

    /// Fill `buf` completely, looping across partial reads until it is full or
    /// a read times out. A short read that returns zero bytes is treated as a
    /// timeout so callers get a deterministic error rather than spinning.
    fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        let mut filled = 0;
        while filled < buf.len() {
            let n = self.port.read(&mut buf[filled..])?;
            if n == 0 {
                return Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "serial read returned no data",
                )));
            }
            filled += n;
        }
        Ok(())
    }
}
