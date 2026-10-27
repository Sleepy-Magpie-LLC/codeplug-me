//! The QYT KT-5000 codeplug: the typed model of its 12,800-byte image.
//!
//! # Memory map
//!
//! | Range | Contents |
//! |---|---|
//! | `0x0000`-`0x0C7F` | 200 channel records, 16 bytes each |
//! | `0x0E00`-`0x0E35` | The settings header this build models |
//! | `0x0E36`-`0x0EFF` | Settings whose real names are still unknown |
//! | `0x0F00`-`0x0F7F` | Four VFO-shaped slots; only A and B are real VFOs |
//! | `0x0FE0`-`0x0FEF` | ASCII static message shown on the radio's LCD |
//! | `0x1000`-`0x1C7F` | 200 channel name records, 16 bytes each |
//! | `0x2000`-`0x31FF` | Signaling tables and factory data, byte-preserved |
//!
//! Addresses are image offsets directly: the KT-5000 image is flat, so unlike
//! the AnyTone there is no region table and no address translation.
//!
//! # Losslessness
//!
//! [`QytCodeplug`] keeps the whole image in `raw` and writes only the records
//! it models back over a copy of it on [`serialize`](QytCodeplug::serialize).
//! Everything this build does not understand, which is most of the settings
//! block and all of the signaling tables, therefore round-trips untouched.

pub mod bands;
pub mod channel;
pub mod edits;
pub mod settings;

pub use bands::{Band, Shift, BAND_SIZE, NUM_BANDS, NUM_VFOS};
pub use channel::{
    Bandwidth, OptSignaling, Power, PttId, QytChannel, SpeakerMute, Tone, CHANNEL_SIZE, FILL,
    NAME_LEN, NAME_RECORD_SIZE,
};
pub use edits::apply_edits;
pub use settings::{QytSettings, ScanRev, SETTINGS_SIZE};

use serde::Serialize;

use crate::bytes::{read_ascii, write_ascii};
use crate::error::{Error, Result};
use crate::qyt::device::IMAGE_SIZE;

/// Number of channel slots. Confirmed twice: CHIRP declares `_upper = 199`, and
/// the vendor capture holds 159 programmed plus 41 empty slots.
pub const NUM_CHANNELS: usize = 200;

/// Offset of the channel record table.
pub const CHANNELS: usize = 0x0000;

/// Offset of the channel name table, index-parallel with [`CHANNELS`].
pub const CHANNEL_NAMES: usize = 0x1000;

/// Offset of the settings block.
pub const SETTINGS: usize = 0x0E00;

/// Offset of the VFO-shaped image slots (A–D; only A/B are editable).
pub const BANDS: usize = 0x0F00;

/// Offset of the ASCII static message the radio shows on its LCD. Ships from
/// the factory as `Model:KT-5000   `, which is why it reads like an identity
/// field, but it is CHIRP's `static_msg` and is user text.
pub const STATIC_MSG: usize = 0x0FE0;

/// Length of the static message field. Shorter messages are space-padded, the
/// same way the factory pads `Model:KT-5000`.
pub const STATIC_MSG_LEN: usize = 16;

/// How much of the field the radio actually draws on its LCD. The field is
/// [`STATIC_MSG_LEN`] bytes wide, but the display cuts off after 13, which is
/// exactly the width of the factory `Model:KT-5000`.
pub const STATIC_MSG_MAX_CHARS: usize = 13;

/// Byte the static message pads with, matching the factory image.
const STATIC_MSG_PAD: u8 = b' ';

/// Address of channel record `i`.
pub fn channel_addr(i: usize) -> usize {
    CHANNELS + i * CHANNEL_SIZE
}

/// Address of channel name record `i`.
pub fn channel_name_addr(i: usize) -> usize {
    CHANNEL_NAMES + i * NAME_RECORD_SIZE
}

/// Address of band/VFO record `i`.
pub fn band_addr(i: usize) -> usize {
    BANDS + i * BAND_SIZE
}

/// A parsed KT-5000 codeplug.
#[derive(Debug)]
pub struct QytCodeplug {
    /// The full image as parsed; the base every serialization starts from.
    raw: Vec<u8>,
    /// One slot per channel index; `None` when the record is the empty marker.
    channels: Vec<Option<QytChannel>>,
    /// The fifty-four settings-header bytes this build models.
    settings: QytSettings,
    /// All image VFO slots (including unused C/D), always present.
    bands: Vec<Band>,
}

/// Serializable projection of a [`QytCodeplug`], the `qyt` payload of the JSON
/// envelope.
#[derive(Debug, Serialize)]
pub struct QytCodeplugJson<'a> {
    /// The programmed channels, in ascending slot order.
    pub channels: Vec<&'a QytChannel>,
    /// The modelled settings header.
    pub settings: &'a QytSettings,
    /// The radio's real VFOs (A and B). Slots C/D stay in the image only.
    pub bands: &'a [Band],
    /// The static message shown on the radio's LCD.
    pub static_message: String,
}

impl QytCodeplug {
    /// Parse a full KT-5000 image. `raw.len()` must equal
    /// [`crate::qyt::device::IMAGE_SIZE`].
    ///
    /// A channel slot is empty when its first byte is `0xFF`, which is CHIRP's
    /// own test and matches the vendor capture, where the 41 unprogrammed slots
    /// are `0xFF` throughout.
    pub fn parse(raw: &[u8]) -> Result<QytCodeplug> {
        if raw.len() != IMAGE_SIZE {
            return Err(Error::Parse(format!(
                "QYT image is {} bytes, expected {IMAGE_SIZE}",
                raw.len()
            )));
        }
        let mut channels = Vec::with_capacity(NUM_CHANNELS);
        for i in 0..NUM_CHANNELS {
            let rec = &raw[channel_addr(i)..channel_addr(i) + CHANNEL_SIZE];
            let name = &raw[channel_name_addr(i)..channel_name_addr(i) + NAME_RECORD_SIZE];
            channels.push(if rec[0] == FILL {
                None
            } else {
                Some(QytChannel::parse(i, rec, name)?)
            });
        }
        let settings = QytSettings::parse(&raw[SETTINGS..SETTINGS + SETTINGS_SIZE])?;
        let mut bands = Vec::with_capacity(NUM_BANDS);
        for i in 0..NUM_BANDS {
            bands.push(Band::parse(i, &raw[band_addr(i)..band_addr(i) + BAND_SIZE])?);
        }
        Ok(QytCodeplug {
            raw: raw.to_vec(),
            channels,
            settings,
            bands,
        })
    }

    /// Re-encode to a full image.
    ///
    /// Starts from the image as parsed and overwrites only the two record
    /// tables this build models, so an unedited codeplug serializes back
    /// byte-identically and every unmodelled byte survives an edit.
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = self.raw.clone();
        for (i, slot) in self.channels.iter().enumerate() {
            let (c, n) = (channel_addr(i), channel_name_addr(i));
            match slot {
                Some(ch) => {
                    out[c..c + CHANNEL_SIZE].copy_from_slice(ch.encode());
                    out[n..n + NAME_RECORD_SIZE].copy_from_slice(ch.encode_name());
                }
                None => {
                    out[c..c + CHANNEL_SIZE].fill(FILL);
                    out[n..n + NAME_RECORD_SIZE].fill(FILL);
                }
            }
        }
        out[SETTINGS..SETTINGS + SETTINGS_SIZE].copy_from_slice(self.settings.encode());
        for (i, band) in self.bands.iter().enumerate() {
            out[band_addr(i)..band_addr(i) + BAND_SIZE].copy_from_slice(band.encode());
        }
        out
    }

    /// The programmed channels, in ascending slot order.
    pub fn channels(&self) -> impl Iterator<Item = &QytChannel> {
        self.channels.iter().flatten()
    }

    /// Mutable access to channel `index`, or `None` if the slot is empty.
    pub fn channel_mut(&mut self, index: usize) -> Option<&mut QytChannel> {
        self.channels.get_mut(index)?.as_mut()
    }

    /// Program the first free channel slot; returns its index, or `None` when
    /// all [`NUM_CHANNELS`] slots are in use.
    pub fn add_channel(&mut self) -> Option<usize> {
        let i = self.channels.iter().position(|s| s.is_none())?;
        self.channels[i] = Some(QytChannel::default_record(i));
        Some(i)
    }

    /// Clear channel `index`, blanking both its record and its name record on
    /// the next serialization. Returns false if the slot was already empty.
    ///
    /// Nothing else in this radio references a channel by index (there are no
    /// zones or scan lists), so removal has no fixups to do.
    pub fn remove_channel(&mut self, index: usize) -> bool {
        match self.channels.get_mut(index) {
            Some(slot @ Some(_)) => {
                *slot = None;
                true
            }
            _ => false,
        }
    }

    /// Move the channel at `from` into the free slot `to`, name record and all.
    /// Errors, leaving the codeplug untouched, if `from` is empty, `to` is out
    /// of range or occupied, or the two are the same slot.
    pub fn move_channel(&mut self, from: usize, to: usize) -> std::result::Result<(), String> {
        if from == to {
            return Err(format!("channel {from} is already at slot {to}"));
        }
        if to >= self.channels.len() {
            return Err(format!(
                "channel slot {to} is out of range (max {})",
                self.channels.len() - 1
            ));
        }
        if self.channels.get(from).map(|s| s.is_none()).unwrap_or(true) {
            return Err(format!("channel {from} is not active"));
        }
        if self.channels[to].is_some() {
            return Err(format!("channel slot {to} is already occupied"));
        }
        let mut rec = self.channels[from].take().unwrap();
        rec.index = to;
        self.channels[to] = Some(rec);
        Ok(())
    }

    /// The modelled settings header.
    pub fn settings(&self) -> &QytSettings {
        &self.settings
    }

    /// Mutable access to the settings header.
    pub fn settings_mut(&mut self) -> &mut QytSettings {
        &mut self.settings
    }

    /// Every VFO-shaped image slot, including unused C/D.
    pub fn bands(&self) -> &[Band] {
        &self.bands
    }

    /// The KT-5000's real VFOs (A and B).
    pub fn vfos(&self) -> &[Band] {
        &self.bands[..NUM_VFOS]
    }

    /// Mutable access to an image VFO slot. Edits should use indices
    /// `0..NUM_VFOS` only; higher slots exist so serialize can preserve them.
    pub fn band_mut(&mut self, index: usize) -> Option<&mut Band> {
        self.bands.get_mut(index)
    }

    /// The static message at `0x0FE0`, trailing padding trimmed.
    pub fn static_message(&self) -> String {
        read_ascii(&self.raw[STATIC_MSG..STATIC_MSG + STATIC_MSG_LEN], 0xFF)
    }

    /// Set the static message the radio shows on its LCD.
    ///
    /// Rejects anything the display cannot render rather than writing bytes the
    /// firmware would draw as garbage: printable ASCII only, at most
    /// [`STATIC_MSG_MAX_CHARS`] characters. An empty message blanks the field
    /// to spaces, which is what the factory pad byte already is.
    pub fn set_static_message(&mut self, msg: &str) -> Result<()> {
        if msg.chars().count() > STATIC_MSG_MAX_CHARS {
            return Err(Error::Parse(format!(
                "static message is {} characters, at most {STATIC_MSG_MAX_CHARS} fit",
                msg.chars().count()
            )));
        }
        if let Some(bad) = msg.chars().find(|c| !matches!(c, ' '..='~')) {
            return Err(Error::Parse(format!(
                "static message character {bad:?} is not printable ASCII"
            )));
        }
        write_ascii(
            &mut self.raw[STATIC_MSG..STATIC_MSG + STATIC_MSG_LEN],
            msg,
            STATIC_MSG_PAD,
        );
        Ok(())
    }

    /// Build the serializable projection.
    pub fn to_json(&self) -> QytCodeplugJson<'_> {
        QytCodeplugJson {
            channels: self.channels().collect(),
            settings: &self.settings,
            bands: self.vfos(),
            static_message: self.static_message(),
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    /// A hand-written KT-5000 image, built from raw bytes rather than through
    /// our own encoders so the tests cannot pass by agreeing with themselves.
    ///
    /// The base fill is `0xFF`, never zero: a zero-filled image decodes as 200
    /// perfectly valid 0 Hz channels, which is the fixture trap this radio
    /// sets. The three programmed channels and the band record below are the
    /// literal bytes from the vendor capture.
    pub(crate) fn synthetic_qyt_image() -> Vec<u8> {
        let mut img = vec![0xFFu8; IMAGE_SIZE];

        // Channel 0: FRS-01, 462.56250 simplex, no tones, tail 0x24.
        img[0x0000..0x0010].copy_from_slice(&[
            0x50, 0x62, 0x25, 0x46, 0x50, 0x62, 0x25, 0x46, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x24,
        ]);
        img[0x1000..0x1010].copy_from_slice(&[
            b'F', b'R', b'S', b'-', b'0', b'1', 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF,
        ]);

        // Channel 1: FRS-02, with the unmodelled 0x05 at name byte 7.
        img[0x0010..0x0020].copy_from_slice(&[
            0x50, 0x87, 0x25, 0x46, 0x50, 0x87, 0x25, 0x46, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x04,
        ]);
        img[0x1010..0x1020].copy_from_slice(&[
            b'F', b'R', b'S', b'-', b'0', b'2', 0xFF, 0x05, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF,
        ]);

        // Channel 2: N7FYZ, 147.32000/147.92000 with a 100.0 Hz TX tone.
        img[0x0020..0x0030].copy_from_slice(&[
            0x00, 0x20, 0x73, 0x14, 0x00, 0x20, 0x79, 0x14, 0x00, 0x00, 0xE8, 0x03, 0x00, 0x00,
            0x00, 0x04,
        ]);
        img[0x1020..0x1030].copy_from_slice(&[
            b'N', b'7', b'F', b'Y', b'Z', 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF,
        ]);

        // The settings header through remember_power at 0x0E35:
        // early bytes from base-config (tmr=1, sql=6, beep off, scan_rev=CO),
        // then 0x0E12-0x0E35 from the 2026-08-14 before dump (stored 0x00 at
        // 0x0E35, which is remember_power on after the inverted mapping).
        img[0x0E00..0x0E36].copy_from_slice(&[
            0x01, 0x03, 0x06, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x0A, 0x00, 0x02, 0x01, 0x00, 0x00, 0x02,
            0x02, 0x00, 0x00, 0x05, 0x05, 0x00, 0x00, 0x04, 0x05, 0x04, 0x06, 0x03, 0x01, 0x04,
            0x00, 0x00, 0x00, 0x02, 0x02, 0x00, 0x0A, 0x01, 0x05, 0x00, 0x14, 0x00,
        ]);

        // Band record 0, verbatim: 462.5625 MHz as one decimal digit per byte.
        img[0x0F00..0x0F20].copy_from_slice(&[
            0x04, 0x06, 0x02, 0x05, 0x06, 0x02, 0x05, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x24,
        ]);

        img[STATIC_MSG..STATIC_MSG + STATIC_MSG_LEN].copy_from_slice(b"Model:KT-5000   ");
        img
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::synthetic_qyt_image;
    use super::*;

    /// The offsets at which two images differ.
    fn diff(a: &[u8], b: &[u8]) -> Vec<usize> {
        (0..a.len()).filter(|&i| a[i] != b[i]).collect()
    }

    /// Fill every one of the 200 slots the radio supports, round trip the whole
    /// image, and confirm the last slot lands where the memory map says it
    /// should and inside the writable range.
    ///
    /// The per-slot tests all work near index 0, so nothing else here would
    /// catch an off-by-one that silently drops the final channel or pushes its
    /// name record past the write limit.
    #[test]
    fn all_two_hundred_channels_round_trip_and_stay_writable() {
        let mut cp = QytCodeplug::parse(&test_support::synthetic_qyt_image()).unwrap();

        // Claim every free slot, so the codeplug ends up completely full.
        while cp.add_channel().is_some() {}
        assert_eq!(cp.channels().count(), NUM_CHANNELS);
        assert!(
            cp.add_channel().is_none(),
            "the radio holds {NUM_CHANNELS} channels; a {}st must be refused",
            NUM_CHANNELS + 1
        );

        // Give each slot a distinct name and frequency so a misplaced record
        // shows up as wrong data rather than as a coincidence.
        for i in 0..NUM_CHANNELS {
            let ch = cp.channel_mut(i).expect("every slot was just filled");
            ch.set_name(&format!("CH{i:03}"));
            ch.set_rx_frequency(144_000_000 + (i as u32) * 12_500);
        }

        let image = cp.serialize();
        let back = QytCodeplug::parse(&image).unwrap();
        let restored: Vec<_> = back.channels().collect();
        assert_eq!(restored.len(), NUM_CHANNELS);
        // Every slot is occupied, so iteration order is slot order.
        for (i, ch) in restored.iter().enumerate() {
            assert_eq!(ch.index, i);
            assert_eq!(ch.name, format!("CH{i:03}"));
            assert_eq!(ch.rx_frequency_hz, 144_000_000 + (i as u32) * 12_500);
        }

        // The last slot is the one an off-by-one would strand.
        let last = NUM_CHANNELS - 1;
        assert_eq!(channel_addr(last), 0x0C70);
        assert_eq!(channel_name_addr(last), 0x1C70);
        let end = channel_name_addr(last) + NAME_RECORD_SIZE;
        assert!(
            end <= crate::qyt::device::WRITE_LIMIT as usize,
            "channel {last}'s name ends at {end:#06x}, past the {:#06x} write limit, \
             so it would never reach the radio",
            crate::qyt::device::WRITE_LIMIT
        );
    }

    #[test]
    fn channel_capacity_fits_the_memory_map() {
        assert_eq!(NUM_CHANNELS, 200);
        // Const blocks so raising the capacity past what the map allows is a
        // build failure rather than a test failure.
        const {
            assert!(
                CHANNELS + NUM_CHANNELS * CHANNEL_SIZE <= SETTINGS,
                "the channel table overruns the settings block"
            )
        };
        const {
            assert!(
                CHANNEL_NAMES + NUM_CHANNELS * NAME_RECORD_SIZE <= 0x2000,
                "the name table overruns the signaling tables"
            )
        };
        // Every parsed image really does yield exactly NUM_CHANNELS slots.
        let mut cp = QytCodeplug::parse(&synthetic_qyt_image()).unwrap();
        while cp.add_channel().is_some() {}
        assert_eq!(cp.channels().count(), NUM_CHANNELS);
    }

    #[test]
    fn parse_finds_only_the_programmed_slots() {
        let cp = QytCodeplug::parse(&synthetic_qyt_image()).unwrap();
        let names: Vec<&str> = cp.channels().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["FRS-01", "FRS-02", "N7FYZ"]);
        assert_eq!(cp.channels().count(), 3);
        assert_eq!(cp.static_message(), "Model:KT-5000");
    }

    #[test]
    fn setting_the_static_message_pads_to_the_factory_shape() {
        let mut cp = QytCodeplug::parse(&synthetic_qyt_image()).unwrap();
        cp.set_static_message("N7FYZ").unwrap();
        assert_eq!(cp.static_message(), "N7FYZ");
        // Space-padded to all 16 bytes, exactly how the factory writes it, so
        // no 0xFF ever lands in a field the radio draws.
        let out = cp.serialize();
        assert_eq!(
            &out[STATIC_MSG..STATIC_MSG + STATIC_MSG_LEN],
            b"N7FYZ           "
        );
    }

    #[test]
    fn a_full_width_static_message_survives_a_round_trip() {
        let mut cp = QytCodeplug::parse(&synthetic_qyt_image()).unwrap();
        cp.set_static_message("0123456789ABC").unwrap();
        let reparsed = QytCodeplug::parse(&cp.serialize()).unwrap();
        assert_eq!(reparsed.static_message(), "0123456789ABC");
        // The three bytes past the LCD's 13 columns stay factory spaces.
        assert_eq!(
            &cp.serialize()[STATIC_MSG..STATIC_MSG + STATIC_MSG_LEN],
            b"0123456789ABC   "
        );
    }

    #[test]
    fn a_static_message_past_the_lcd_width_is_rejected() {
        let mut cp = QytCodeplug::parse(&synthetic_qyt_image()).unwrap();
        // Fits the 16-byte field, but the radio would only draw 13 of it.
        assert!(cp.set_static_message("0123456789ABCD").is_err());
        assert_eq!(cp.static_message(), "Model:KT-5000");
    }

    #[test]
    fn an_empty_static_message_blanks_the_field() {
        let mut cp = QytCodeplug::parse(&synthetic_qyt_image()).unwrap();
        cp.set_static_message("").unwrap();
        assert_eq!(cp.static_message(), "");
        assert_eq!(
            &cp.serialize()[STATIC_MSG..STATIC_MSG + STATIC_MSG_LEN],
            b"                "
        );
    }

    #[test]
    fn the_static_message_rejects_overlong_and_unprintable_text() {
        let mut cp = QytCodeplug::parse(&synthetic_qyt_image()).unwrap();
        let err = cp
            .set_static_message("0123456789ABCDEFG")
            .unwrap_err()
            .to_string();
        assert!(err.contains("at most 13"), "{err}");
        let err = cp.set_static_message("hi\u{e9}").unwrap_err().to_string();
        assert!(err.contains("printable ASCII"), "{err}");
        // A rejected write leaves the field as it was.
        assert_eq!(cp.static_message(), "Model:KT-5000");
    }

    #[test]
    fn editing_the_static_message_leaves_the_rest_of_the_image_alone() {
        let img = synthetic_qyt_image();
        let mut cp = QytCodeplug::parse(&img).unwrap();
        cp.set_static_message("KD7ABC 146.52").unwrap();
        let out = cp.serialize();
        assert_eq!(out[..STATIC_MSG], img[..STATIC_MSG]);
        assert_eq!(
            out[STATIC_MSG + STATIC_MSG_LEN..],
            img[STATIC_MSG + STATIC_MSG_LEN..]
        );
    }

    #[test]
    fn parse_rejects_a_wrongly_sized_image() {
        let err = QytCodeplug::parse(&[0xFFu8; 128]).unwrap_err().to_string();
        assert!(err.contains("expected 12800"), "{err}");
    }

    #[test]
    fn serialize_is_byte_identical_for_an_unmodified_image() {
        let img = synthetic_qyt_image();
        let cp = QytCodeplug::parse(&img).unwrap();
        assert_eq!(cp.serialize(), img);
    }

    #[test]
    fn editing_a_channel_touches_only_its_own_two_records() {
        let img = synthetic_qyt_image();
        let mut cp = QytCodeplug::parse(&img).unwrap();
        let ch = cp.channel_mut(2).unwrap();
        ch.set_name("EDITED");
        ch.set_rx_frequency(146_520_000);
        let out = cp.serialize();

        // Subset, not equality: two nearby BCD frequencies share most bytes.
        let touched = diff(&img, &out);
        assert!(!touched.is_empty());
        assert!(
            touched.iter().all(|&i| (channel_addr(2)
                ..channel_addr(2) + CHANNEL_SIZE)
                .contains(&i)
                || (channel_name_addr(2)..channel_name_addr(2) + NAME_LEN).contains(&i)),
            "an edit to channel 2 reached {touched:?}"
        );
    }

    #[test]
    fn unknown6_bits_survive_an_unrelated_edit() {
        let mut cp = QytCodeplug::parse(&synthetic_qyt_image()).unwrap();
        cp.channel_mut(1).unwrap().set_name("OTHER");
        let out = cp.serialize();
        // Channel 0's tail, untouched by an edit two slots away.
        assert_eq!(out[channel_addr(0) + 15], 0x24);
        // Channel 1's own unmodelled name-record byte.
        assert_eq!(out[channel_name_addr(1) + 7], 0x05);
    }

    #[test]
    fn add_channel_fills_the_first_free_slot() {
        let mut cp = QytCodeplug::parse(&synthetic_qyt_image()).unwrap();
        assert_eq!(cp.add_channel(), Some(3));
        assert_eq!(cp.channel_mut(3).unwrap().name, "NEW");

        let out = cp.serialize();
        let cp = QytCodeplug::parse(&out).unwrap();
        assert_eq!(cp.channels().count(), 4);
        assert_eq!(cp.channels().last().unwrap().rx_frequency_hz, 146_520_000);
    }

    #[test]
    fn add_channel_returns_none_when_every_slot_is_used() {
        let mut cp = QytCodeplug::parse(&synthetic_qyt_image()).unwrap();
        while cp.add_channel().is_some() {}
        assert_eq!(cp.channels().count(), NUM_CHANNELS);
        assert_eq!(cp.add_channel(), None);
    }

    #[test]
    fn remove_channel_blanks_both_records() {
        let mut cp = QytCodeplug::parse(&synthetic_qyt_image()).unwrap();
        assert!(cp.remove_channel(1));
        assert!(!cp.remove_channel(1));
        assert!(!cp.remove_channel(199));

        let out = cp.serialize();
        assert_eq!(&out[channel_addr(1)..channel_addr(1) + CHANNEL_SIZE], &[FILL; 16]);
        assert_eq!(
            &out[channel_name_addr(1)..channel_name_addr(1) + NAME_RECORD_SIZE],
            &[FILL; 16]
        );
        let cp = QytCodeplug::parse(&out).unwrap();
        let names: Vec<&str> = cp.channels().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["FRS-01", "N7FYZ"]);
    }

    #[test]
    fn move_channel_carries_the_name_record_with_it() {
        let mut cp = QytCodeplug::parse(&synthetic_qyt_image()).unwrap();
        cp.move_channel(2, 50).unwrap();
        let out = cp.serialize();

        let cp = QytCodeplug::parse(&out).unwrap();
        let moved = cp.channels().find(|c| c.index == 50).unwrap();
        assert_eq!(moved.name, "N7FYZ");
        assert_eq!(moved.rx_frequency_hz, 147_320_000);
        assert!(cp.channels().all(|c| c.index != 2));
        assert_eq!(&out[channel_addr(2)..channel_addr(2) + CHANNEL_SIZE], &[FILL; 16]);
    }

    #[test]
    fn move_channel_rejects_bad_slots() {
        let mut cp = QytCodeplug::parse(&synthetic_qyt_image()).unwrap();
        assert!(cp.move_channel(0, 0).is_err());
        assert!(cp.move_channel(0, 1).is_err(), "slot 1 is occupied");
        assert!(cp.move_channel(0, NUM_CHANNELS).is_err());
        assert!(cp.move_channel(9, 100).is_err(), "slot 9 is empty");
        // None of the failures may have disturbed anything.
        assert_eq!(cp.serialize(), synthetic_qyt_image());
    }

    #[test]
    fn settings_and_bands_parse_out_of_the_image() {
        let cp = QytCodeplug::parse(&synthetic_qyt_image()).unwrap();
        assert_eq!(cp.settings().squelch, 6);
        assert_eq!(cp.settings().tot_seconds, 60);
        assert!(!cp.settings().beep);
        assert_eq!(cp.settings().scan_rev, Some(ScanRev::CO));
        assert!(cp.settings().remember_power);
        assert_eq!(cp.bands().len(), NUM_BANDS);
        assert_eq!(cp.bands()[0].frequency_hz, Some(462_562_500));
        // The synthetic image only programs band 0; the rest are 0xFF-filled.
        assert!(!cp.bands()[3].programmed);
    }

    #[test]
    fn a_settings_edit_touches_only_the_modelled_header() {
        let img = synthetic_qyt_image();
        let mut cp = QytCodeplug::parse(&img).unwrap();
        cp.settings_mut().set_squelch(2).unwrap();
        let out = cp.serialize();
        assert_eq!(diff(&img, &out), vec![SETTINGS + 0x02]);
        // Everything past the modelled header is byte-preserved.
        assert_eq!(&out[SETTINGS + SETTINGS_SIZE..BANDS], &img[SETTINGS + SETTINGS_SIZE..BANDS]);
    }

    #[test]
    fn a_band_edit_touches_only_that_band_record() {
        let img = synthetic_qyt_image();
        let mut cp = QytCodeplug::parse(&img).unwrap();
        cp.band_mut(0).unwrap().set_power(Power::Low);
        let out = cp.serialize();
        assert_eq!(diff(&img, &out), vec![band_addr(0) + 25]);
        // The unnamed 0x24 tail of band 0 survives.
        assert_eq!(out[band_addr(0) + 31], 0x24);
    }

    #[test]
    fn band_mut_is_bounded_to_the_four_records() {
        let mut cp = QytCodeplug::parse(&synthetic_qyt_image()).unwrap();
        assert!(cp.band_mut(3).is_some());
        assert!(cp.band_mut(4).is_none());
    }

    #[test]
    fn to_json_reports_channels_settings_bands_and_the_static_message() {
        let cp = QytCodeplug::parse(&synthetic_qyt_image()).unwrap();
        let v = serde_json::to_value(cp.to_json()).unwrap();
        assert_eq!(v["static_message"], "Model:KT-5000");
        assert_eq!(v["settings"]["squelch"], 6);
        assert_eq!(v["settings"]["tot_seconds"], 60);
        assert_eq!(v["bands"].as_array().unwrap().len(), 2);
        assert_eq!(v["bands"][0]["frequency_hz"], 462_562_500u32);
        assert!(v["bands"][1]["frequency_hz"].is_null());
        assert_eq!(v["channels"][0]["name"], "FRS-01");
        assert_eq!(v["channels"][0]["rx_frequency_hz"], 462_562_500u32);
        assert_eq!(v["channels"][2]["tx_tone"]["kind"], "ctcss");
        assert_eq!(v["channels"][2]["tx_tone"]["hz_tenths"], 1000);
        assert_eq!(v["channels"][0]["flags_raw"][3], 0x24);
        // The raw records must never leak into the projection.
        assert!(v["channels"][0].get("raw").is_none());
    }
}
