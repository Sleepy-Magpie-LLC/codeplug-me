//! USB identity attribution: which radio a VID/PID pair names.
//!
//! This is a plain lookup table with no OS dependency, so it lives apart from
//! [`crate::ports`] (which needs `serialport`, hence libudev on Linux and
//! IOKit on macOS, and is desktop-only). Android enumerates USB devices through
//! its own USB host API and then asks this module what it found, so both
//! platforms attribute radios from exactly one table.

use crate::model::Model;

/// Known (VID, PID, model) triples for USB bridges we can attribute on sight.
///
/// The two AnyTone entries are the AT-D878UVII Plus GD32 and STM32 bridge
/// variants, respectively.
///
/// There is deliberately no QYT KT-5000 entry, and there never will be.
///
/// Checked against the hardware on 2026-08-06: the KT-5000's programming cable
/// enumerates as `067b:2303`, a stock Prolific PL2303. That chip is in
/// thousands of unrelated adapters, so attributing it here would tell every
/// PL2303 owner their USB-to-RS232 cable is a radio, and would let a codeplug
/// be aimed at whatever else happened to be plugged in. The radio simply cannot
/// be identified from its USB descriptors.
///
/// A KT-5000 therefore appears as an unattributed port and is reached by the
/// caller naming the model explicitly. That is the correct design for this
/// radio, not a placeholder waiting on a capture.
pub const KNOWN_USB_IDS: &[(u16, u16, Model)] = &[
    (0x28e9, 0x018a, Model::AnyToneD878UV2),
    (0x2e3c, 0x5740, Model::AnyToneD878UV2),
];

/// The model this VID/PID pair belongs to, or `None` if we don't know it.
pub fn model_for_usb_id(vid: u16, pid: u16) -> Option<Model> {
    KNOWN_USB_IDS
        .iter()
        .find(|(v, p, _)| *v == vid && *p == pid)
        .map(|(_, _, m)| *m)
}

/// Return true if this VID/PID pair is a known radio USB bridge.
pub fn is_known_radio(vid: u16, pid: u16) -> bool {
    model_for_usb_id(vid, pid).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_ids_match_and_others_do_not() {
        assert!(is_known_radio(0x28e9, 0x018a));
        assert!(is_known_radio(0x2e3c, 0x5740));
        assert!(!is_known_radio(0x1234, 0x5678));
        assert!(!is_known_radio(0x28e9, 0x0000));
    }

    #[test]
    fn known_usb_ids_all_attribute_a_model() {
        // Both of today's entries are the AnyTone's two bridge variants. The
        // QYT is deliberately absent: it has no distinguishable USB identity.
        // See the note on KNOWN_USB_IDS.
        assert_eq!(KNOWN_USB_IDS.len(), 2);
        for (vid, pid, model) in KNOWN_USB_IDS {
            assert_eq!(model_for_usb_id(*vid, *pid), Some(*model));
        }
    }

    #[test]
    fn the_qyt_programming_cable_is_never_attributed() {
        // A stock Prolific PL2303, shared with thousands of unrelated adapters.
        // Attributing it would tell every PL2303 owner they own a radio.
        assert_eq!(model_for_usb_id(0x067b, 0x2303), None);
    }
}
