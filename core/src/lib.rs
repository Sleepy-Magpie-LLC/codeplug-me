//! `codeplugme-core`: protocol, device, and codeplug layers for programming
//! amateur radios over their USB serial links.
//!
//! The crate is organised per radio model. Everything specific to one radio,
//! its wire protocol, its memory layout, and the fake radio its tests drive,
//! lives in that model's module. [`anytone`] is the AnyTone AT-D878UVII Plus;
//! [`qyt`] is the QYT KT-5000.
//!
//! Model-agnostic pieces sit at the top level: the byte [`transport`] trait,
//! the byte-encoding helpers in [`bytes`], the shared [`error`] type, and the
//! [`model`] identity and capability tables.
//!
//! # Platform features
//!
//! Everything above [`transport::Transport`] is portable. The two host-facing
//! surfaces are not, so each sits behind a feature:
//!
//! - `desktop` (default) brings in `serialport` and with it
//!   [`transport::SerialTransport`], the macOS port enumeration in [`ports`],
//!   and the C ABI in [`ffi`] that the CodeplugMe Swift app links against.
//! - `uniffi-api` brings in [`api`], the UniFFI surface the Android app calls.
//!   There the transport is implemented in Kotlin over the USB host API and
//!   passed in, because `serialport` needs libudev, which Android does not have.
//!
//! Both also turn on `net`, the HTTP behind the repeater import in [`geo`].
//! Its parsers and builder compile without it.
//!
//! The firmware UPDATE path is intentionally out of scope.

#![deny(clippy::all)]

#[cfg(feature = "uniffi-api")]
uniffi::setup_scaffolding!();

pub mod anytone;
#[cfg(feature = "uniffi-api")]
pub mod api;
pub mod bytes;
pub mod document;
pub mod driver;
pub mod error;
#[cfg(feature = "desktop")]
pub mod ffi;
pub mod geo;
pub mod model;
#[cfg(feature = "desktop")]
pub mod ports;
pub mod qyt;
pub mod transport;
pub mod usb_ids;

// The AnyTone types are re-exported flat at the crate root because they were
// the crate's entire public surface before it grew a per-model structure.
// Keeping them here means `cli` and the C ABI compile unchanged.
pub use anytone::codeplug::{
    apply_edits, CallType, Channel, Codeplug, Contact, GroupList, RadioId, Zone,
};
pub use anytone::device::{codeplug_size, is_supported_model, Radio, Region, REGIONS};
pub use anytone::mock::MockTransport;
pub use anytone::protocol::BLOCK_SIZE;
pub use error::{Error, Result};
#[cfg(feature = "desktop")]
pub use ports::{autodetect_radio, list_ports, PortInfo};
#[cfg(feature = "desktop")]
pub use transport::SerialTransport;
pub use transport::Transport;
