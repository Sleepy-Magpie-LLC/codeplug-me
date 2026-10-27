//! The per-model wire-protocol interface.
//!
//! Each supported radio implements [`RadioDriver`] once, over `&mut dyn
//! Transport` so the trait stays object-safe and the C ABI can hold a
//! `Box<dyn RadioDriver>` chosen at runtime by [`driver_for`].
//!
//! Entering and leaving program mode is deliberately folded inside
//! [`RadioDriver::read_image`] and [`RadioDriver::write_image`]. Callers must
//! not have to know that one radio has a program-mode handshake and another
//! does not, and folding it in means the "always attempt a clean exit, even on
//! failure" rule lives in exactly one place per model.

use crate::error::Result;
use crate::model::Model;
use crate::transport::Transport;

/// Everything the app needs to do to a physical radio.
pub trait RadioDriver {
    /// Which model this driver speaks for.
    fn model(&self) -> Model;

    /// Handshake with the radio and return its identity string. Leaves the
    /// radio back in its normal state.
    fn identify(&self, t: &mut dyn Transport) -> Result<String>;

    /// Whether an identity string reported by a radio belongs to this model.
    fn accepts_identity(&self, id: &str) -> bool;

    /// Read the complete image, reporting `(done, total)` block progress.
    fn read_image(
        &self,
        t: &mut dyn Transport,
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<Vec<u8>>;

    /// Write a complete image back, reporting `(done, total)` block progress.
    fn write_image(
        &self,
        t: &mut dyn Transport,
        data: &[u8],
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<()>;
}

/// The driver that speaks `model`'s protocol.
pub fn driver_for(model: Model) -> Box<dyn RadioDriver> {
    match model {
        Model::AnyToneD878UV2 => Box::new(crate::anytone::driver::AnyToneDriver),
        Model::QytKt5000 => Box::new(crate::qyt::driver::QytDriver),
    }
}

/// Write `data` to the radio on `t`, but only once the radio has confirmed it
/// is the `model` the image belongs to.
///
/// `model` comes from the image's own bytes, never from the caller's intent, so
/// a QYT codeplug can never reach an AnyTone or the reverse. **The check runs
/// before the first write block is emitted.** A partially written codeplug of
/// the wrong shape is the worst thing this crate can do to a radio, and it is
/// not something the app can undo; `port` appears in the message so the
/// operator knows which cable to look at.
pub fn write_verified_image(
    model: Model,
    t: &mut dyn Transport,
    data: &[u8],
    port: &str,
    progress: &mut dyn FnMut(usize, usize),
) -> std::result::Result<(), String> {
    let driver = driver_for(model);
    // The identity check happens inside write_image, in the same session as the
    // transfer. It cannot be done as a separate identify() call here: every
    // driver method is a complete session, and the QYT has no exit command and
    // rejects a second handshake on an open link, so identify-then-write breaks
    // on real hardware. See the note on QytDriver::write_image.
    driver.write_image(t, data, progress).map_err(|e| {
        format!(
            "refusing to write to {port}: this file is for a {}. {e}",
            model.display_name()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::anytone::mock::MockTransport;
    use crate::qyt::mock::QytMock;
    use crate::qyt::protocol::HANDSHAKE;

    #[test]
    fn driver_for_returns_a_driver_that_names_its_own_model() {
        for m in Model::ALL {
            assert_eq!(driver_for(*m).model(), *m);
        }
    }

    /// A QYT image with its model tag in place, so `Model::detect` finds it.
    fn qyt_image() -> Vec<u8> {
        crate::qyt::codeplug::test_support::synthetic_qyt_image()
    }

    /// An AnyTone-sized image. Contents do not matter: the guard must refuse
    /// before anything reads it.
    fn anytone_image() -> Vec<u8> {
        vec![0u8; Model::AnyToneD878UV2.image_size()]
    }

    #[test]
    fn restore_refuses_a_qyt_image_on_an_anytone_radio() {
        let data = qyt_image();
        let model = Model::detect(&data).unwrap();
        assert_eq!(model, Model::QytKt5000);

        let mut mock = MockTransport::new(1024, "ID878UV V100");
        let err = {
            let mut t: &mut dyn Transport = &mut mock;
            write_verified_image(model, &mut t, &data, "/dev/mock", &mut |_, _| {})
                .expect_err("a QYT image must never be written to an AnyTone")
        };
        assert!(!err.is_empty(), "the refusal must say something");
        // Not one write command reached the radio. 'W' is the AnyTone write
        // opcode; its absence is the proof no block was emitted.
        assert!(
            !mock.written.contains(&b'W'),
            "a write block was emitted before the model check: {:?}",
            mock.written
        );
    }

    #[test]
    fn restore_refuses_an_anytone_image_on_a_qyt_radio() {
        let data = anytone_image();
        let model = Model::detect(&data).unwrap();
        assert_eq!(model, Model::AnyToneD878UV2);

        let mut mock = QytMock::new();
        let err = {
            let mut t: &mut dyn Transport = &mut mock;
            write_verified_image(model, &mut t, &data, "/dev/mock", &mut |_, _| {})
                .expect_err("an AnyTone image must never be written to a QYT")
        };
        assert!(!err.is_empty(), "the refusal must say something");
        assert!(
            mock.writes.is_empty(),
            "a write block was emitted before the model check: {:?}",
            mock.writes
        );
        assert!(!mock.written.contains(&0x58), "no 0x58 write command may go out");
    }

    /// The guarded write must complete in a single session.
    ///
    /// Regression test for a bug real hardware found on 2026-08-06: this
    /// function used to call `identify()` and then `write_image()`, which is two
    /// complete sessions and therefore two handshakes. The KT-5000 has no exit
    /// command and refuses a second handshake on an open link, so every QYT
    /// write would have failed. The mock now enforces the one-handshake rule,
    /// which is what makes this test meaningful.
    #[test]
    fn a_guarded_qyt_write_uses_exactly_one_handshake() {
        let data = qyt_image();
        let model = Model::detect(&data).unwrap();
        assert_eq!(model, Model::QytKt5000);

        let mut mock = QytMock::new();
        {
            let mut t: &mut dyn Transport = &mut mock;
            write_verified_image(model, &mut t, &data, "/dev/mock", &mut |_, _| {})
                .expect("a matching radio must accept the write");
        }

        let handshakes = mock
            .written
            .windows(HANDSHAKE.len())
            .filter(|w| *w == HANDSHAKE)
            .count();
        assert_eq!(handshakes, 1, "expected exactly one handshake per session");
        assert!(!mock.writes.is_empty(), "the image should actually have been written");
    }

    #[test]
    fn a_matching_image_and_radio_are_written() {
        let data = qyt_image();
        let model = Model::detect(&data).unwrap();
        let mut mock = QytMock::new();
        let mut t: &mut dyn Transport = &mut mock;
        write_verified_image(model, &mut t, &data, "/dev/mock", &mut |_, _| {}).unwrap();
        assert!(!mock.writes.is_empty(), "the matching case must still write");
    }
}
