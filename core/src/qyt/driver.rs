//! [`RadioDriver`] implementation for the QYT KT-5000.
//!
//! Thinner than the AnyTone driver because this radio has no program-mode
//! bracket to hold: the handshake is the whole entry sequence, and there is no
//! exit command at all. The radio resets itself when the port closes.

use crate::driver::RadioDriver;
use crate::error::{Error, Result};
use crate::model::Model;
use crate::qyt::device::{read_image, write_image};
use crate::qyt::protocol::{handshake, FINGERPRINT, MODEL_TAG};
use crate::transport::Transport;

/// The QYT KT-5000 driver. Stateless; the transport carries the session.
pub struct QytDriver;

impl RadioDriver for QytDriver {
    /// The model this driver speaks for.
    fn model(&self) -> Model {
        Model::QytKt5000
    }

    /// Handshake and report what the radio calls itself. There is nothing to
    /// exit: the handshake leaves the radio ready for the next command and the
    /// reset happens on port close.
    fn identify(&self, t: &mut dyn Transport) -> Result<String> {
        let id = handshake(t)?;
        Ok(format!("{} {}", id.model_tag, id.fingerprint))
    }

    /// Whether an identity string names this radio.
    ///
    /// Matches on either half, since the radio reports the model as `KT-500`
    /// (six characters, no trailing digit) while `WP9114` is the firmware
    /// fingerprint. Either alone identifies the KT-5000.
    fn accepts_identity(&self, id: &str) -> bool {
        let model = std::str::from_utf8(MODEL_TAG).expect("the model tag is ASCII");
        let fp = std::str::from_utf8(FINGERPRINT).expect("the fingerprint is ASCII");
        id.contains(model) || id.contains(fp)
    }

    /// Read the full `0x3200`-byte image.
    fn read_image(
        &self,
        t: &mut dyn Transport,
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<Vec<u8>> {
        handshake(t)?;
        read_image(t, progress)
    }

    /// Write the writable part of the image back.
    ///
    /// No read-back verification, deliberately: the radio reboots after a
    /// transfer, so an in-session read-back would time out. The vendor CPS does
    /// not attempt one either. Verification is a separate post-reboot session.
    /// Handshake, confirm the radio is a KT-5000, then write the image.
    ///
    /// The identity check is in-session because it has to be: the KT-5000 has no
    /// exit command and will not accept a second handshake on an open link, so a
    /// caller cannot identify first and write second. Confirmed on hardware
    /// 2026-08-06, where exactly that sequence failed.
    fn write_image(
        &self,
        t: &mut dyn Transport,
        data: &[u8],
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<()> {
        let identity = handshake(t)?;
        if !identity.is_kt5000() {
            return Err(Error::Protocol(format!(
                "radio identifies as {:?} {:?}, which is not a QYT KT-5000",
                identity.model_tag, identity.fingerprint
            )));
        }
        write_image(t, data, progress)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qyt::codeplug::QytCodeplug;
    use crate::qyt::device::{IMAGE_SIZE, WRITE_LIMIT};
    use crate::qyt::mock::QytMock;

    #[test]
    fn identify_reports_the_model_tag_and_fingerprint() {
        let mut mock = QytMock::new();
        let mut t: &mut dyn Transport = &mut mock;
        let id = QytDriver.identify(&mut t).unwrap();
        assert_eq!(id, "KT-500 WP9114");
        assert!(QytDriver.accepts_identity(&id));
    }

    #[test]
    fn accepts_identity_matches_either_half_and_nothing_else() {
        assert!(QytDriver.accepts_identity("KT-500 WP9114"));
        assert!(QytDriver.accepts_identity("WP9114"));
        assert!(QytDriver.accepts_identity("KT-500"));
        assert!(!QytDriver.accepts_identity("ID878UV V100"));
    }

    #[test]
    fn read_image_returns_a_full_size_image() {
        let mut mock = QytMock::new();
        let mut t: &mut dyn Transport = &mut mock;
        let mut seen = (0usize, 0usize);
        let data = QytDriver
            .read_image(&mut t, &mut |done, total| seen = (done, total))
            .unwrap();
        assert_eq!(data.len(), Model::QytKt5000.image_size());
        assert_eq!(data.len(), IMAGE_SIZE);
        assert_eq!(seen.0, seen.1);
        assert!(seen.1 > 0);
    }

    #[test]
    fn write_image_then_read_image_round_trips_a_codeplug() {
        let img = crate::qyt::codeplug::test_support::synthetic_qyt_image();
        let mut cp = QytCodeplug::parse(&img).unwrap();
        cp.channel_mut(0).unwrap().set_name("DRIVER");
        let mut image = cp.serialize();
        // The tail above the write limit is never sent, so an honest round trip
        // expects back whatever the radio already holds there.
        for b in &mut image[WRITE_LIMIT as usize..] {
            *b = 0xFF;
        }

        let mut mock = QytMock::new();
        {
            let mut t: &mut dyn Transport = &mut mock;
            QytDriver.write_image(&mut t, &image, &mut |_, _| {}).unwrap();
        }
        // The radio reboots when the port closes, and only then will it accept
        // another handshake. Reading back without this is a sequence the real
        // hardware refuses.
        mock.reboot();
        let mut t: &mut dyn Transport = &mut mock;
        let back = QytDriver.read_image(&mut t, &mut |_, _| {}).unwrap();
        assert_eq!(back, image);
        assert_eq!(
            QytCodeplug::parse(&back).unwrap().channels().next().unwrap().name,
            "DRIVER"
        );
    }

    /// A completely full codeplug survives a real write and read back.
    ///
    /// 200 is the radio's maximum, and the last channel's name record sits at
    /// `0x1C70`, far from the addresses the other tests exercise. This is what
    /// proves the transfer covers the whole populated range rather than just the
    /// early blocks.
    #[test]
    fn a_full_two_hundred_channel_codeplug_survives_a_write_and_read_back() {
        let img = crate::qyt::codeplug::test_support::synthetic_qyt_image();
        let mut cp = QytCodeplug::parse(&img).unwrap();
        while cp.add_channel().is_some() {}
        for i in 0..crate::qyt::codeplug::NUM_CHANNELS {
            let ch = cp.channel_mut(i).unwrap();
            ch.set_name(&format!("CH{i:03}"));
            ch.set_rx_frequency(144_000_000 + (i as u32) * 12_500);
        }

        let mut image = cp.serialize();
        // Above the write limit the radio keeps whatever it already held, so an
        // honest comparison expects the mock's erased tail there.
        for b in &mut image[WRITE_LIMIT as usize..] {
            *b = 0xFF;
        }

        let mut mock = QytMock::new();
        {
            let mut t: &mut dyn Transport = &mut mock;
            QytDriver.write_image(&mut t, &image, &mut |_, _| {}).unwrap();
        }
        mock.reboot();
        let mut t: &mut dyn Transport = &mut mock;
        let back = QytDriver.read_image(&mut t, &mut |_, _| {}).unwrap();
        assert_eq!(back, image, "a full codeplug must survive the round trip");

        let restored = QytCodeplug::parse(&back).unwrap();
        let names: Vec<_> = restored.channels().map(|c| c.name.clone()).collect();
        assert_eq!(names.len(), crate::qyt::codeplug::NUM_CHANNELS);
        assert_eq!(names.first().unwrap(), "CH000");
        assert_eq!(names.last().unwrap(), "CH199");
    }
}
