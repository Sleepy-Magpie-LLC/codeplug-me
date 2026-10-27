//! Everything specific to the AnyTone AT-D878UVII Plus: its byte-level
//! programming protocol ([`protocol`]), the high-level device operations built
//! on top of it ([`device`]), its codeplug binary model ([`codeplug`]), and the
//! in-memory fake radio the tests drive ([`mock`]).
//!
//! Nothing outside this module encodes AnyTone command grammar or memory
//! layout; the generic byte helpers live in [`crate::bytes`] and the serial
//! transport in [`crate::transport`].

pub mod codeplug;
pub mod device;
pub mod driver;
pub mod mock;
pub mod protocol;
