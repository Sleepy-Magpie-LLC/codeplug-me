//! Serial port enumeration and radio detection on macOS.
//!
//! A radio enumerates as a standard USB serial device (`/dev/cu.usbmodem*` or
//! `/dev/cu.usbserial*`). We list all ports and attribute any whose USB VID/PID
//! is in [`KNOWN_USB_IDS`] to the model that bridge belongs to, but attribution
//! is confirmed later via the identify handshake, so a non-matching VID/PID is
//! not a hard failure: the caller can name the model explicitly instead.

use crate::error::Result;
use crate::model::Model;
use std::collections::HashSet;

// The USB identity table lives in `usb_ids` so Android can reach it without
// pulling in `serialport`. Re-exported here because this was its original home
// and callers (the C ABI, tests) still name it through this module.
pub use crate::usb_ids::{is_known_radio, model_for_usb_id, KNOWN_USB_IDS};

/// A discovered serial port and which radio, if any, its USB identity names.
#[derive(Debug, Clone)]
pub struct PortInfo {
    /// Device path, e.g. `/dev/cu.usbmodem1234`.
    pub name: String,
    /// USB vendor ID, if this is a USB serial port.
    pub vid: Option<u16>,
    /// USB product ID, if this is a USB serial port.
    pub pid: Option<u16>,
    /// USB product string, if the OS reported one.
    pub product: Option<String>,
    /// The model this port's VID/PID names, or `None` when unrecognized.
    pub model: Option<Model>,
    /// True when the VID/PID matches a known radio bridge. Always exactly
    /// `model.is_some()`; kept as its own field because callers that only ask
    /// "is this worth showing as a radio" should not have to care which model.
    pub likely_radio: bool,
}

impl PortInfo {
    /// Build a port entry, deriving [`model`](PortInfo::model) and
    /// [`likely_radio`](PortInfo::likely_radio) from the USB identity so the two
    /// can never disagree.
    pub fn new(
        name: String,
        vid: Option<u16>,
        pid: Option<u16>,
        product: Option<String>,
    ) -> PortInfo {
        let model = match (vid, pid) {
            (Some(v), Some(d)) => model_for_usb_id(v, d),
            _ => None,
        };
        PortInfo {
            name,
            vid,
            pid,
            product,
            model,
            likely_radio: model.is_some(),
        }
    }
}

/// macOS exposes every serial device twice: once as a callout device
/// (`/dev/cu.*`) and once as a dial-in device (`/dev/tty.*`). Both names refer
/// to the same physical port, so listing both shows every radio twice and makes
/// [`autodetect_radio`] report an ambiguity that doesn't exist.
///
/// Drop the `tty.` half of any such pair. Only the callout device is usable
/// here anyway: the dial-in device blocks on open waiting for carrier detect,
/// which a USB CDC-ACM bridge never asserts. A `tty.` port with no `cu.` twin
/// is left alone, since it is then the only handle on that device.
fn dedupe_callout_ports(ports: &mut Vec<PortInfo>) {
    let callouts: HashSet<&str> = ports
        .iter()
        .filter_map(|p| p.name.strip_prefix("/dev/cu."))
        .collect();
    let shadowed: HashSet<String> = ports
        .iter()
        .filter(|p| {
            p.name
                .strip_prefix("/dev/tty.")
                .is_some_and(|suffix| callouts.contains(suffix))
        })
        .map(|p| p.name.clone())
        .collect();
    ports.retain(|p| !shadowed.contains(&p.name));
}

/// Enumerate the serial ports, attributing each to a model by VID/PID where we
/// can and collapsing each macOS `cu.`/`tty.` pair down to its callout half.
pub fn list_ports() -> Result<Vec<PortInfo>> {
    let ports = serialport::available_ports()?;
    let mut out = Vec::with_capacity(ports.len());
    for p in ports {
        let (vid, pid, product) = match &p.port_type {
            serialport::SerialPortType::UsbPort(info) => {
                (Some(info.vid), Some(info.pid), info.product.clone())
            }
            _ => (None, None, None),
        };
        out.push(PortInfo::new(p.port_name, vid, pid, product));
    }
    dedupe_callout_ports(&mut out);
    Ok(out)
}

/// Pick the single likely-radio port, if exactly one is present.
///
/// - `Ok(Some(path))` when exactly one likely radio was found.
/// - `Ok(None)` when none were found.
/// - `Err(..)` (ambiguous) when more than one likely radio was found; the
///   caller should ask the user to disambiguate with an explicit port.
pub fn autodetect_radio() -> Result<Option<String>> {
    let mut radios: Vec<PortInfo> = list_ports()?
        .into_iter()
        .filter(|p| p.likely_radio)
        .collect();
    match radios.len() {
        0 => Ok(None),
        1 => Ok(Some(radios.remove(0).name)),
        n => Err(crate::error::Error::InvalidArgument(format!(
            "{n} candidate radio ports found; pass --port to choose one"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A radio port with the given device path. VID/PID are a known AnyTone
    /// bridge so `model`/`likely_radio` reflect what the real enumeration would
    /// say.
    fn radio_at(name: &str) -> PortInfo {
        PortInfo::new(
            name.to_string(),
            Some(0x28e9),
            Some(0x018a),
            Some("AnyTone".to_string()),
        )
    }

    fn names(ports: &[PortInfo]) -> Vec<&str> {
        ports.iter().map(|p| p.name.as_str()).collect()
    }

    #[test]
    fn dedupe_drops_the_tty_half_of_a_callout_pair() {
        // The exact shape macOS reports for one plugged-in radio, and the reason
        // the device list showed every radio twice.
        let mut ports = vec![
            radio_at("/dev/cu.usbmodem14201"),
            radio_at("/dev/tty.usbmodem14201"),
        ];
        dedupe_callout_ports(&mut ports);
        assert_eq!(names(&ports), ["/dev/cu.usbmodem14201"]);
    }

    #[test]
    fn dedupe_keeps_a_tty_port_with_no_callout_twin() {
        // Hiding it would leave the device with no handle at all.
        let mut ports = vec![radio_at("/dev/tty.usbmodem14201")];
        dedupe_callout_ports(&mut ports);
        assert_eq!(names(&ports), ["/dev/tty.usbmodem14201"]);
    }

    #[test]
    fn dedupe_pairs_by_suffix_not_across_devices() {
        // Two radios plugged in at once must stay two radios.
        let mut ports = vec![
            radio_at("/dev/cu.usbmodem14201"),
            radio_at("/dev/tty.usbmodem14201"),
            radio_at("/dev/cu.usbmodem14301"),
            radio_at("/dev/tty.usbmodem14301"),
        ];
        dedupe_callout_ports(&mut ports);
        assert_eq!(
            names(&ports),
            ["/dev/cu.usbmodem14201", "/dev/cu.usbmodem14301"]
        );
    }

    #[test]
    fn dedupe_leaves_unrelated_ports_alone() {
        let mut ports = vec![
            radio_at("/dev/cu.usbmodem14201"),
            radio_at("/dev/tty.usbmodem14201"),
            radio_at("/dev/cu.Bluetooth-Incoming-Port"),
        ];
        dedupe_callout_ports(&mut ports);
        assert_eq!(
            names(&ports),
            ["/dev/cu.usbmodem14201", "/dev/cu.Bluetooth-Incoming-Port"]
        );
    }

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
            assert_eq!(*model, Model::AnyToneD878UV2);
        }
        assert_eq!(model_for_usb_id(0x1234, 0x5678), None);
    }

    #[test]
    fn port_info_model_matches_likely_radio() {
        // The invariant every consumer relies on: a port is "likely a radio"
        // exactly when we could name which radio it is.
        // Known AnyTone bridge, its other variant, a generic CH340 of the kind
        // a QYT cable uses, a non-USB port, and half an identity.
        let cases = [
            (Some(0x28e9), Some(0x018a)),
            (Some(0x2e3c), Some(0x5740)),
            (Some(0x1a86), Some(0x7523)),
            (None, None),
            (Some(0x28e9), None),
        ];
        for (vid, pid) in cases {
            let p = PortInfo::new("/dev/cu.test".to_string(), vid, pid, None);
            assert_eq!(
                p.likely_radio,
                p.model.is_some(),
                "likely_radio disagreed with model for {vid:?}/{pid:?}"
            );
        }
    }

    #[test]
    fn an_unrecognized_adapter_is_left_unattributed() {
        // The QYT's own bridge lands here today. It must not be guessed at.
        let p = PortInfo::new(
            "/dev/cu.usbserial-1420".to_string(),
            Some(0x1a86),
            Some(0x7523),
            Some("USB-SERIAL CH340".to_string()),
        );
        assert_eq!(p.model, None);
        assert!(!p.likely_radio);
    }

    #[test]
    fn list_ports_does_not_error() {
        // We can't assert contents without hardware, but enumeration itself
        // must succeed (or return an empty list) on any host.
        let _ = list_ports().expect("port enumeration should not error");
    }
}
