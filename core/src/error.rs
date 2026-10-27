//! Error types for the codeplugme-core crate.

use thiserror::Error;

/// All fallible operations in this crate return this error type.
#[derive(Error, Debug)]
pub enum Error {
    /// A transport-level I/O failure (serial read/write, timeout, port open).
    #[error("transport I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The underlying `serialport` crate reported an error. Desktop-only:
    /// the Android build has no `serialport`, and its transport errors arrive
    /// as [`Error::Io`] instead.
    #[cfg(feature = "desktop")]
    #[error("serial port error: {0}")]
    Serial(#[from] serialport::Error),

    /// The radio sent a response that did not match the expected framing.
    #[error("unexpected response from radio: {0}")]
    Protocol(String),

    /// A read-block response carried a checksum that did not match the data.
    #[error("checksum mismatch at address 0x{addr:08X}: expected 0x{expected:02X}, got 0x{actual:02X}")]
    Checksum {
        /// Block address the mismatch was detected at.
        addr: u32,
        /// Checksum the radio claimed.
        expected: u8,
        /// Checksum we computed over the received data.
        actual: u8,
    },

    /// A write-then-read-back verification found differing bytes.
    #[error("read-back verification failed at address 0x{addr:08X}")]
    Verify {
        /// Block address where the written and read-back data diverged.
        addr: u32,
    },

    /// A block read/write failed while transferring a specific codeplug block;
    /// wraps the underlying cause with the failing radio address so an IO
    /// timeout (radio not answering for memory it doesn't map) is diagnosable.
    #[error("codeplug transfer failed at block address 0x{addr:08X}: {message}")]
    Transfer {
        /// Radio block address the transfer failed at.
        addr: u32,
        /// The underlying error, rendered.
        message: String,
    },

    /// The caller asked to operate on a region whose length is not a multiple
    /// of the 16-byte block size, or another invariant was violated.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    /// A codeplug byte buffer could not be parsed into the typed model (wrong
    /// size, an address that falls outside the known region map, etc.).
    #[error("codeplug parse error: {0}")]
    Parse(String),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;
