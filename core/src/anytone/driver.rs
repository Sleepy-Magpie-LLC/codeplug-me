//! [`RadioDriver`] implementation for the AnyTone AT-D878UVII Plus.
//!
//! Thin adapter over [`Radio`]: it owns the program-mode bracketing that every
//! caller used to have to write by hand, and nothing else.

use crate::anytone::device::{is_supported_model, Radio};
use crate::driver::RadioDriver;
use crate::error::{Error, Result};
use crate::model::Model;
use crate::transport::Transport;

/// The AnyTone AT-D878UVII Plus driver. Stateless; the transport carries all
/// the session state.
pub struct AnyToneDriver;

impl RadioDriver for AnyToneDriver {
    /// The model this driver speaks for.
    fn model(&self) -> Model {
        Model::AnyToneD878UV2
    }

    /// Enter program mode, read the identity string, and exit.
    fn identify(&self, t: &mut dyn Transport) -> Result<String> {
        let mut radio = Radio::new(t);
        radio.enter()?;
        let id = radio.identify();
        // Always attempt a clean exit from program mode, even on failure.
        let exit = radio.exit();
        let id = id?;
        exit?;
        Ok(id)
    }

    /// Whether an identify string names a supported D878UV radio.
    fn accepts_identity(&self, id: &str) -> bool {
        is_supported_model(id)
    }

    /// Enter program mode, read the full codeplug, and exit.
    fn read_image(
        &self,
        t: &mut dyn Transport,
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<Vec<u8>> {
        let mut radio = Radio::new(t);
        radio.enter()?;
        let data = radio.read_codeplug(progress);
        let exit = radio.exit();
        let data = data?;
        exit?;
        Ok(data)
    }

    /// Enter program mode, confirm the radio is one we speak for, write the full
    /// codeplug (each block verified by the device layer), and exit.
    ///
    /// The identity check lives inside this session on purpose. Every method on
    /// this trait is a complete session, so a caller that identified first and
    /// wrote second would bracket program mode twice. That is merely wasteful
    /// here, but it is fatal on the QYT, which has no exit command and rejects a
    /// second handshake. Doing the check in-session keeps both drivers honest.
    fn write_image(
        &self,
        t: &mut dyn Transport,
        data: &[u8],
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<()> {
        let mut radio = Radio::new(t);
        radio.enter()?;
        let result = (|| {
            let id = radio.identify()?;
            if !is_supported_model(&id) {
                return Err(Error::Protocol(format!(
                    "radio identifies as {id:?}, which is not an AnyTone AT-D878UVII Plus"
                )));
            }
            radio.write_codeplug(data, progress)
        })();
        let exit = radio.exit();
        result?;
        exit?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anytone::codeplug::{global_offset, Codeplug, CONTACT_BITMAP};
    use crate::anytone::device::REGIONS;
    use crate::anytone::mock::MockTransport;

    /// A mock whose flat memory image covers the highest-addressed region.
    fn mock() -> MockTransport {
        let end = REGIONS
            .iter()
            .map(|r| r.address as usize + r.length)
            .max()
            .unwrap();
        MockTransport::new(end, "ID878UV V100")
    }

    #[test]
    fn identify_brackets_the_call_in_program_mode() {
        let mut mock = mock();
        let mut t: &mut dyn Transport = &mut mock;
        let id = AnyToneDriver.identify(&mut t).unwrap();
        assert!(
            AnyToneDriver.accepts_identity(&id),
            "unexpected identity {id:?}"
        );
    }

    #[test]
    fn read_image_returns_a_full_size_codeplug() {
        let mut mock = mock();
        let mut t: &mut dyn Transport = &mut mock;
        let mut seen = (0usize, 0usize);
        let data = AnyToneDriver
            .read_image(&mut t, &mut |done, total| seen = (done, total))
            .unwrap();
        assert_eq!(data.len(), Model::AnyToneD878UV2.image_size());
        assert_eq!(seen.0, seen.1);
        assert!(seen.1 > 0);
    }

    /// An image with no active records. The contact bitmap is inverted, so a
    /// bare zero-fill would read back as ten thousand live contacts.
    fn empty_image() -> Vec<u8> {
        let mut raw = vec![0u8; Model::AnyToneD878UV2.image_size()];
        let off = global_offset(CONTACT_BITMAP).unwrap();
        for b in raw[off..off + 0x500].iter_mut() {
            *b = 0xff;
        }
        raw
    }

    #[test]
    fn write_image_then_read_image_round_trips() {
        let mut cp = Codeplug::parse(&empty_image()).unwrap();
        let ci = cp.add_channel().unwrap();
        cp.channel_mut(ci).unwrap().set_name("REPEATER");
        let image = cp.serialize();

        let mut mock = mock();
        {
            let mut t: &mut dyn Transport = &mut mock;
            AnyToneDriver
                .write_image(&mut t, &image, &mut |_, _| {})
                .unwrap();
        }
        let mut t: &mut dyn Transport = &mut mock;
        let back = AnyToneDriver.read_image(&mut t, &mut |_, _| {}).unwrap();
        // Byte-level preservation is device.rs's job; what the driver has to
        // prove is that its enter/write/exit then enter/read/exit bracketing
        // leaves the radio holding the record we sent.
        assert_eq!(back.len(), image.len());
        let cp = Codeplug::parse(&back).unwrap();
        assert_eq!(cp.channels().next().unwrap().name, "REPEATER");
    }
}
