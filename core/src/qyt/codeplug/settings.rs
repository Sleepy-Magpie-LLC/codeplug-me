//! The KT-5000 settings header at `0x0E00`.
//!
//! # Why only fifty-four bytes
//!
//! CHIRP parses a 58-byte `settings` struct here, borrowed wholesale from the
//! WP-9900/KT-WP12, but it then sets `has_settings = False` for the KT-5000 and
//! its own source carries the comment "memory map is not EXACTLY the same as in
//! WP12. E.g. setting 'C display mode' overwrites VOX setting". Decoding our
//! capture backs that up: three fields (`pttid`, `anil`, `tmrtx`) hold values
//! out of range for the lists CHIRP pairs them with, which CHIRP silently
//! clamps away. Several confirmed KT-5000 offsets also disagree with CHIRP
//! (`scan_rev` one byte early; `remember_power` where CHIRP puts `reps`).
//!
//! So this module models exactly the bytes confirmed against real KT-5000
//! one-field diffs (`0x0E00`-`0x0E35`): the early header, `beep`, `scan_rev`,
//! and `remember_power`. **Everything from `0x0E36` to `0x0FFF` is
//! byte-preserved and unmodelled.** Real names for the rest have to come from
//! further one-field diffs.
//!
//! Even inside the modelled range, most bytes are unnamed; they live in `raw`
//! and are never rewritten.

use serde::Serialize;

use crate::error::{Error, Result};

/// Length of the settings header this build models, `0x0E00`-`0x0E35`.
pub const SETTINGS_SIZE: usize = 0x36;

/// Offset of `tmr` (dual-band waiting / Menu 0 TMR) within the header.
const TMR: usize = 0x00;
/// Offset of `sql` (squelch level) within the header.
const SQL: usize = 0x02;
/// Offset of `tot` (transmit timeout) within the header.
const TOT: usize = 0x05;
/// Offset of `volume` within the header.
const VOLUME: usize = 0x06;
/// Offset of `abr` (backlight timer) within the header.
const ABR: usize = 0x08;
/// Offset of the key-beep byte within the header.
const BEEP: usize = 0x09;
/// Bit within [`BEEP`] that turns the key beep on. CHIRP declares the rest of
/// the byte unused; our captures only ever store `0x00` / `0x01`.
const BEEP_BIT: u8 = 0;
/// Offset of `scan_rev` (scan resume method) within the header.
///
/// Confirmed on hardware at `0x0E11`. CHIRP's WP-9900 map puts `screv` at
/// `0x0E12`; that offset is wrong for the KT-5000.
const SCAN_REV: usize = 0x11;
/// Offset of `remember_power` (restore last on/off state when power returns).
///
/// Confirmed on hardware at `0x0E35`. CHIRP's WP-9900 map puts `reps` (tone
/// burst frequency) here; that label is wrong for the KT-5000.
const REMEMBER_POWER: usize = 0x35;

/// Highest `tmr` value. The KT-5000 manual Menu 0 is Off/On only; CHIRP's
/// WP-9900 `LIST_TMR7` (M+A…M+ABC) does not apply to this radio.
const TMR_MAX: u8 = 1;
/// Highest squelch level.
const SQL_MAX: u8 = 9;
/// Highest `tot` index: CHIRP's `LIST_TOT` has 39 entries, 15 s apart.
const TOT_MAX: u8 = 38;
/// Highest `abr` value: CHIRP's `LIST_OFF1TO50`.
const ABR_MAX: u8 = 50;

/// Seconds per `tot` step; index 0 is the first step, not zero.
const TOT_STEP_SECONDS: u32 = 15;

/// Scan resume method (Menu SCAN_REV / CHIRP `LIST_SCREV`).
///
/// Values confirmed against vendor CPS writes on a real KT-5000: base = CO,
/// then one-field writes to SE and TO.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ScanRev {
    /// Resume scanning after a timeout (TO).
    TO,
    /// Resume when the carrier drops (CO).
    CO,
    /// Stop on the first busy channel (SE).
    SE,
}

impl ScanRev {
    /// Decode the scan-resume byte. Unknown values come back as [`None`] so a
    /// dump never invents a label for a byte the radio actually holds.
    pub fn from_byte(v: u8) -> Option<ScanRev> {
        match v {
            0 => Some(ScanRev::TO),
            1 => Some(ScanRev::CO),
            2 => Some(ScanRev::SE),
            _ => None,
        }
    }

    /// Encode as the byte the radio stores.
    pub fn to_byte(self) -> u8 {
        match self {
            ScanRev::TO => 0,
            ScanRev::CO => 1,
            ScanRev::SE => 2,
        }
    }

    /// Parse an edit-spec string (`"TO"` / `"CO"` / `"SE"`, case-insensitive).
    pub fn parse_label(s: &str) -> std::result::Result<ScanRev, String> {
        match s.to_ascii_uppercase().as_str() {
            "TO" => Ok(ScanRev::TO),
            "CO" => Ok(ScanRev::CO),
            "SE" => Ok(ScanRev::SE),
            _ => Err(format!("scan_rev must be TO, CO, or SE, got {s:?}")),
        }
    }
}

/// The modelled settings header. `raw` is authoritative so the unnamed bytes
/// inside the header survive any edit.
#[derive(Debug, Clone, Serialize)]
pub struct QytSettings {
    /// Dual-band waiting (Menu 0 TMR): 0 = Off, 1 = On.
    pub tmr: u8,
    /// Squelch level, 0-9.
    pub squelch: u8,
    /// Transmit timeout as a step index; see [`tot_seconds`](QytSettings::tot_seconds).
    pub tot: u8,
    /// Transmit timeout in seconds, derived from [`tot`](QytSettings::tot).
    pub tot_seconds: u32,
    /// Power-on volume level. The capture reads 0 and the legal range is not
    /// known, so no bound is enforced.
    pub volume: u8,
    /// Backlight timer, 0 = off, otherwise seconds (CHIRP `LIST_OFF1TO50`).
    pub backlight_timer: u8,
    /// Key beep on/off (bit 0 of `0x0E09`).
    pub beep: bool,
    /// Scan resume method. `None` when the stored byte is outside TO/CO/SE.
    pub scan_rev: Option<ScanRev>,
    /// When on, applying power restores the radio's last on/off state; when
    /// off, applying power always leaves the radio off.
    ///
    /// The EEPROM byte at `0x0E35` is inverted: API `true` ↔ stored `0x00`,
    /// API `false` ↔ stored `0x01`.
    pub remember_power: bool,
    /// Authoritative header bytes; skipped in JSON output.
    #[serde(skip)]
    raw: [u8; SETTINGS_SIZE],
}

impl QytSettings {
    /// Parse the modelled header. Returns [`Error::Parse`] on a wrong length.
    pub fn parse(rec: &[u8]) -> Result<QytSettings> {
        if rec.len() != SETTINGS_SIZE {
            return Err(Error::Parse(format!(
                "QYT settings header is {} bytes, expected {SETTINGS_SIZE}",
                rec.len()
            )));
        }
        let mut raw = [0u8; SETTINGS_SIZE];
        raw.copy_from_slice(rec);
        Ok(Self::from_raw(raw))
    }

    /// The raw header for serialization (lossless).
    pub fn encode(&self) -> &[u8; SETTINGS_SIZE] {
        &self.raw
    }

    /// Set dual-band waiting, 0 (Off) or 1 (On).
    pub fn set_tmr(&mut self, v: u8) -> std::result::Result<(), String> {
        self.set_checked(TMR, v, TMR_MAX, "tmr")
    }

    /// Set the squelch level, 0-9.
    pub fn set_squelch(&mut self, v: u8) -> std::result::Result<(), String> {
        self.set_checked(SQL, v, SQL_MAX, "squelch")
    }

    /// Set the transmit timeout step index, 0-38 (15 s through 585 s).
    pub fn set_tot(&mut self, v: u8) -> std::result::Result<(), String> {
        self.set_checked(TOT, v, TOT_MAX, "tot")
    }

    /// Set the power-on volume. Unbounded: the radio's legal range is unknown.
    pub fn set_volume(&mut self, v: u8) {
        self.raw[VOLUME] = v;
        *self = Self::from_raw(self.raw);
    }

    /// Set the backlight timer, 0-50.
    pub fn set_backlight_timer(&mut self, v: u8) -> std::result::Result<(), String> {
        self.set_checked(ABR, v, ABR_MAX, "backlight_timer")
    }

    /// Set the key beep. Touches only bit 0 of `0x0E09`.
    pub fn set_beep(&mut self, on: bool) {
        if on {
            self.raw[BEEP] |= 1 << BEEP_BIT;
        } else {
            self.raw[BEEP] &= !(1 << BEEP_BIT);
        }
        *self = Self::from_raw(self.raw);
    }

    /// Set the scan resume method.
    pub fn set_scan_rev(&mut self, v: ScanRev) {
        self.raw[SCAN_REV] = v.to_byte();
        *self = Self::from_raw(self.raw);
    }

    /// Set whether applying power restores the last on/off state.
    ///
    /// The stored byte is inverted relative to the UI/API value: `true` writes
    /// `0x00`, `false` writes `0x01`.
    pub fn set_remember_power(&mut self, on: bool) {
        self.raw[REMEMBER_POWER] = u8::from(!on);
        *self = Self::from_raw(self.raw);
    }

    /// Write one header byte after a range check, rejecting rather than
    /// clamping: a clamped value looks like it was accepted and then writes
    /// something the operator never asked for.
    fn set_checked(
        &mut self,
        offset: usize,
        value: u8,
        max: u8,
        field: &str,
    ) -> std::result::Result<(), String> {
        if value > max {
            return Err(format!("{field} must be 0-{max}, got {value}"));
        }
        self.raw[offset] = value;
        *self = Self::from_raw(self.raw);
        Ok(())
    }

    /// Transmit timeout in seconds for a `tot` step index.
    fn tot_seconds(tot: u8) -> u32 {
        (tot as u32 + 1) * TOT_STEP_SECONDS
    }

    /// Rebuild the decoded fields from `raw`.
    fn from_raw(raw: [u8; SETTINGS_SIZE]) -> QytSettings {
        QytSettings {
            tmr: raw[TMR],
            squelch: raw[SQL],
            tot: raw[TOT],
            tot_seconds: Self::tot_seconds(raw[TOT]),
            volume: raw[VOLUME],
            backlight_timer: raw[ABR],
            beep: raw[BEEP] & (1 << BEEP_BIT) != 0,
            scan_rev: ScanRev::from_byte(raw[SCAN_REV]),
            // Stored byte is inverted: 0 means on, non-zero means off.
            remember_power: raw[REMEMBER_POWER] == 0,
            raw,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Settings header built from the base-config / power-remember captures:
    /// early header matches base-config (tmr=1, sql=6, beep off, scan_rev=CO),
    /// and `0x0E12`-`0x0E35` matches the 2026-08-14 before dump (stored
    /// `0x0E35=0x00`, which decodes as remember_power on because the byte is
    /// inverted).
    const CAPTURE: [u8; SETTINGS_SIZE] = [
        0x01, 0x03, 0x06, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, // 0x0E00-08
        0x00, // beep off
        0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, // 0x0E0A-10
        0x01, // scan_rev = CO
        0x00, 0x00, 0x00, 0x0A, 0x00, 0x02, 0x01, 0x00, // 0x0E12-19
        0x00, 0x02, 0x02, 0x00, 0x00, 0x05, 0x05, 0x00, // 0x0E1A-21
        0x00, 0x04, 0x05, 0x04, 0x06, 0x03, 0x01, 0x04, // 0x0E22-29
        0x00, 0x00, 0x00, 0x02, 0x02, 0x00, 0x0A, 0x01, // 0x0E2A-31
        0x05, 0x00, 0x14, 0x00, // 0x0E32-35; stored 0 = remember_power on
    ];

    #[test]
    fn the_capture_header_decodes_to_sane_values() {
        let s = QytSettings::parse(&CAPTURE).unwrap();
        assert_eq!(s.tmr, 1);
        assert_eq!(s.squelch, 6);
        assert_eq!(s.tot, 3);
        // Step index 3 is the fourth 15-second step: 60 seconds.
        assert_eq!(s.tot_seconds, 60);
        assert_eq!(s.volume, 0);
        assert_eq!(s.backlight_timer, 0);
        assert!(!s.beep);
        assert_eq!(s.scan_rev, Some(ScanRev::CO));
        assert!(s.remember_power);
    }

    #[test]
    fn encode_returns_an_untouched_header_verbatim() {
        assert_eq!(QytSettings::parse(&CAPTURE).unwrap().encode(), &CAPTURE);
    }

    #[test]
    fn each_setter_touches_only_its_own_byte() {
        /// One setter under test, paired with the single byte it may touch.
        type Case = (usize, fn(&mut QytSettings));

        let cases: [Case; 8] = [
            // Capture already has tmr=1 (On); set Off so the byte actually changes.
            (TMR, |s| s.set_tmr(0).unwrap()),
            (SQL, |s| s.set_squelch(2).unwrap()),
            (TOT, |s| s.set_tot(9).unwrap()),
            (VOLUME, |s| s.set_volume(12)),
            (ABR, |s| s.set_backlight_timer(7).unwrap()),
            (BEEP, |s| s.set_beep(true)),
            (SCAN_REV, |s| s.set_scan_rev(ScanRev::SE)),
            // Capture stores 0x00 (API on); set off so the byte actually changes.
            (REMEMBER_POWER, |s| s.set_remember_power(false)),
        ];
        for (offset, apply) in cases {
            let mut s = QytSettings::parse(&CAPTURE).unwrap();
            apply(&mut s);
            let touched: Vec<usize> = (0..SETTINGS_SIZE)
                .filter(|&i| s.encode()[i] != CAPTURE[i])
                .collect();
            assert_eq!(touched, vec![offset]);
        }
    }

    #[test]
    fn unnamed_header_bytes_are_never_rewritten() {
        let mut header = CAPTURE;
        // Sentinels in bytes the header does not name.
        header[0x01] = 0xA1;
        header[0x03] = 0xA3;
        header[0x04] = 0xA4;
        header[0x07] = 0xA7;
        header[0x0A] = 0xAA;
        header[0x0E] = 0xAE;
        header[0x10] = 0xB0;
        header[0x20] = 0xC0;
        header[0x34] = 0xD4; // CHIRP's anil; still unnamed here

        let mut s = QytSettings::parse(&header).unwrap();
        s.set_squelch(9).unwrap();
        s.set_tot(0).unwrap();
        s.set_volume(4);
        s.set_beep(true);
        s.set_scan_rev(ScanRev::TO);
        s.set_remember_power(false);
        let out = s.encode();
        assert_eq!(
            [
                out[0x01], out[0x03], out[0x04], out[0x07], out[0x0A], out[0x0E], out[0x10],
                out[0x20], out[0x34]
            ],
            [0xA1, 0xA3, 0xA4, 0xA7, 0xAA, 0xAE, 0xB0, 0xC0, 0xD4]
        );
    }

    #[test]
    fn beep_setter_preserves_other_bits_in_its_byte() {
        let mut header = CAPTURE;
        header[BEEP] = 0xFE; // every bit except beep
        let mut s = QytSettings::parse(&header).unwrap();
        assert!(!s.beep);
        s.set_beep(true);
        assert_eq!(s.encode()[BEEP], 0xFF);
        s.set_beep(false);
        assert_eq!(s.encode()[BEEP], 0xFE);
    }

    #[test]
    fn scan_rev_round_trips_all_three_values() {
        let mut s = QytSettings::parse(&CAPTURE).unwrap();
        for (label, expected) in [("TO", ScanRev::TO), ("CO", ScanRev::CO), ("SE", ScanRev::SE)] {
            s.set_scan_rev(ScanRev::parse_label(label).unwrap());
            assert_eq!(s.scan_rev, Some(expected));
            assert_eq!(s.encode()[SCAN_REV], expected.to_byte());
        }
    }

    #[test]
    fn hardware_cps_diffs_match_the_documented_offsets() {
        // Mirrors docs/reference/QYT/Configs/{beep-on,sacn_rev-*,squalch-3}_write
        // and power-remember/{before,after}.bin: each variant flips one byte.
        let mut beep = CAPTURE;
        beep[BEEP] = 0x01;
        let s = QytSettings::parse(&beep).unwrap();
        assert!(s.beep);
        assert_eq!(s.scan_rev, Some(ScanRev::CO));

        let mut se = CAPTURE;
        se[SCAN_REV] = 0x02;
        assert_eq!(
            QytSettings::parse(&se).unwrap().scan_rev,
            Some(ScanRev::SE)
        );

        let mut to = CAPTURE;
        to[SCAN_REV] = 0x00;
        assert_eq!(
            QytSettings::parse(&to).unwrap().scan_rev,
            Some(ScanRev::TO)
        );

        let mut sql = CAPTURE;
        sql[SQL] = 0x03;
        assert_eq!(QytSettings::parse(&sql).unwrap().squelch, 3);

        let mut rem = CAPTURE;
        rem[REMEMBER_POWER] = 0x01;
        assert!(!QytSettings::parse(&rem).unwrap().remember_power);
        rem[REMEMBER_POWER] = 0x00;
        assert!(QytSettings::parse(&rem).unwrap().remember_power);
    }

    #[test]
    fn out_of_range_values_are_rejected_not_clamped() {
        let mut s = QytSettings::parse(&CAPTURE).unwrap();
        for r in [
            // WP-9900 LIST_TMR7 values must not be accepted on this radio.
            s.set_tmr(2),
            s.set_squelch(10),
            s.set_tot(39),
            s.set_backlight_timer(51),
        ] {
            assert!(r.is_err());
        }
        assert!(ScanRev::parse_label("XX").is_err());
        // A rejected write leaves the header exactly as it was.
        assert_eq!(s.encode(), &CAPTURE);
    }

    #[test]
    fn parse_rejects_a_wrongly_sized_header() {
        assert!(QytSettings::parse(&[0u8; 9]).is_err());
        assert!(QytSettings::parse(&[0u8; 18]).is_err());
        assert!(QytSettings::parse(&[0u8; 53]).is_err());
    }
}
