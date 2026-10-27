//! Everything specific to the QYT KT-5000: its byte-level programming protocol
//! ([`protocol`]), the whole-image transfer built on top of it ([`device`]),
//! and the in-memory fake radio the tests drive ([`mock`]).
//!
//! The KT-5000 is a far simpler machine than the AnyTone: analog only, 200
//! channels, a flat 12,800-byte image, no checksums, and 16-bit addresses. The
//! two protocols share nothing but "push bytes, pull bytes", which is why they
//! sit in sibling modules rather than behind a common abstraction.
//!
//! Everything here was decoded from the vendor CPS captures in
//! `docs/reference/QYT/`; `core/tests/qyt_capture.rs` replays them and asserts
//! our host byte stream is identical to the vendor's.

pub mod codeplug;
pub mod device;
pub mod driver;
pub mod mock;
pub mod protocol;
pub mod rescue;
