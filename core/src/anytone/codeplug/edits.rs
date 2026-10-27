//! JSON-driven batch editing of a [`Codeplug`], shared by the C FFI
//! (`codeplugme_apply_edits`) and the CLI (`edit`).
//!
//! An edit batch carries, for each entity family, an update array (index +
//! optional fields), an `add_*` array (field objects with no index — a record
//! is created and the fields applied), and a `remove_*` array of indices.
//! Operations run in the order update → remove → add. The result is verified by
//! re-parsing, so only intended changes reach the output.

use serde::Deserialize;

use super::channel::{Admit, OptSignaling, SignalingMode, SquelchMode};
use super::{Bandwidth, CallType, Channel, ChannelMode, Codeplug, Power};

/// Maximum name length for the 16-byte Latin1 name slots.
const NAME_LEN: usize = 16;

/// A batch of edits. All arrays default to empty; within each edit, absent
/// fields are left untouched.
#[derive(Deserialize, Default)]
pub struct EditSpec {
    #[serde(default)]
    channels: Vec<ChannelEdit>,
    #[serde(default)]
    add_channels: Vec<ChannelFields>,
    #[serde(default)]
    remove_channels: Vec<usize>,
    #[serde(default)]
    move_channels: Vec<MoveOp>,
    #[serde(default)]
    zones: Vec<ZoneEdit>,
    #[serde(default)]
    add_zones: Vec<ZoneFields>,
    #[serde(default)]
    remove_zones: Vec<usize>,
    #[serde(default)]
    move_zones: Vec<MoveOp>,
    #[serde(default)]
    scan_lists: Vec<ScanListEdit>,
    #[serde(default)]
    add_scan_lists: Vec<ScanListFields>,
    #[serde(default)]
    remove_scan_lists: Vec<usize>,
    #[serde(default)]
    move_scan_lists: Vec<MoveOp>,
    #[serde(default)]
    contacts: Vec<ContactEdit>,
    #[serde(default)]
    add_contacts: Vec<ContactFields>,
    #[serde(default)]
    remove_contacts: Vec<usize>,
    #[serde(default)]
    move_contacts: Vec<MoveOp>,
    #[serde(default)]
    group_lists: Vec<GroupListEdit>,
    #[serde(default)]
    add_group_lists: Vec<GroupListFields>,
    #[serde(default)]
    remove_group_lists: Vec<usize>,
    #[serde(default)]
    move_group_lists: Vec<MoveOp>,
    #[serde(default)]
    radio_ids: Vec<RadioIdEdit>,
    #[serde(default)]
    add_radio_ids: Vec<RadioIdFields>,
    #[serde(default)]
    remove_radio_ids: Vec<usize>,
    #[serde(default)]
    move_radio_ids: Vec<MoveOp>,
}

/// Relocate the record at `from` to the free slot `to` (backs the editable "#"
/// column in the GUI).
#[derive(Deserialize)]
struct MoveOp {
    from: usize,
    to: usize,
}

/// Editable channel fields (no index); shared by add and update.
#[derive(Deserialize, Default)]
struct ChannelFields {
    name: Option<String>,
    rx_frequency_hz: Option<u32>,
    tx_frequency_hz: Option<u32>,
    /// "analog" | "digital" | "mixed_analog" | "mixed_digital".
    mode: Option<String>,
    /// "low" | "mid" | "high" | "turbo".
    power: Option<String>,
    /// "narrow" | "wide".
    bandwidth: Option<String>,
    color_code: Option<u8>,
    time_slot: Option<u8>,
    contact_index: Option<u32>,
    radio_id_index: Option<u8>,
    group_list_index: Option<u8>,
    /// RX-only / "PTT prohibit".
    rx_only: Option<bool>,
    /// Talk-around / simplex.
    talk_around: Option<bool>,
    /// Call confirmation.
    call_confirm: Option<bool>,
    /// Lone-worker / "work alone".
    work_alone: Option<bool>,
    /// Simplex TDMA.
    simplex_tdma: Option<bool>,
    /// Receive APRS.
    rx_aprs: Option<bool>,
    /// "TX permit": "always" | "channel_free" | "different_color_code" | "same_color_code".
    admit: Option<String>,
    /// "carrier" | "tone".
    squelch_mode: Option<String>,
    /// "off" | "dtmf" | "two_tone" | "five_tone".
    optional_signaling: Option<String>,
    /// RX signaling type: "none" | "ctcss" | "dcs".
    rx_signaling_mode: Option<String>,
    /// TX signaling type: "none" | "ctcss" | "dcs".
    tx_signaling_mode: Option<String>,
    /// RX CTCSS tone index.
    rx_ctcss: Option<u8>,
    /// TX CTCSS tone index.
    tx_ctcss: Option<u8>,
    /// RX DCS code word.
    rx_dcs: Option<u16>,
    /// TX DCS code word.
    tx_dcs: Option<u16>,
    /// Scan-list index (`0xff` = none).
    scan_list_index: Option<u8>,
    /// DTMF ID index.
    dtmf_id_index: Option<u8>,
    /// 2-tone ID index.
    two_tone_id_index: Option<u8>,
    /// 5-tone ID index.
    five_tone_id_index: Option<u8>,
    /// 2-tone decode index.
    two_tone_decode_index: Option<u16>,
}

/// Update to a single active channel, addressed by its index.
#[derive(Deserialize)]
struct ChannelEdit {
    index: usize,
    #[serde(flatten)]
    fields: ChannelFields,
}

/// Editable zone fields (no index); shared by add and update.
#[derive(Deserialize, Default)]
struct ZoneFields {
    name: Option<String>,
    /// Full replacement member channel-index list.
    members: Option<Vec<u16>>,
}

/// Update to a single active zone, addressed by its index.
#[derive(Deserialize)]
struct ZoneEdit {
    index: usize,
    #[serde(flatten)]
    fields: ZoneFields,
}

/// Editable scan-list fields (no index); shared by add and update. Absent
/// fields are left untouched.
#[derive(Deserialize, Default)]
struct ScanListFields {
    name: Option<String>,
    /// Full replacement member channel-index list.
    members: Option<Vec<u16>>,
    priority_channel_select: Option<u8>,
    priority_channel_1: Option<u16>,
    priority_channel_2: Option<u16>,
    look_back_a: Option<u16>,
    look_back_b: Option<u16>,
    dropout_delay: Option<u16>,
    dwell_time: Option<u16>,
    revert_channel: Option<u8>,
}

/// Update to a single active scan list, addressed by its index.
#[derive(Deserialize)]
struct ScanListEdit {
    index: usize,
    #[serde(flatten)]
    fields: ScanListFields,
}

/// Editable contact fields (no index); shared by add and update.
#[derive(Deserialize, Default)]
struct ContactFields {
    name: Option<String>,
    number: Option<u32>,
    /// "private" | "group" | "all".
    call_type: Option<String>,
}

/// Update to a single active contact, addressed by its index.
#[derive(Deserialize)]
struct ContactEdit {
    index: usize,
    #[serde(flatten)]
    fields: ContactFields,
}

/// Editable group-list fields (no index); shared by add and update.
#[derive(Deserialize, Default)]
struct GroupListFields {
    name: Option<String>,
    /// Full replacement member contact-index list.
    members: Option<Vec<u32>>,
}

/// Update to a single active group list, addressed by its index.
#[derive(Deserialize)]
struct GroupListEdit {
    index: usize,
    #[serde(flatten)]
    fields: GroupListFields,
}

/// Editable radio-ID fields (no index); shared by add and update.
#[derive(Deserialize, Default)]
struct RadioIdFields {
    name: Option<String>,
    number: Option<u32>,
}

/// Update to a single active radio ID, addressed by its index.
#[derive(Deserialize)]
struct RadioIdEdit {
    index: usize,
    #[serde(flatten)]
    fields: RadioIdFields,
}

/// Parse `edits_json`, apply it to a fresh parse of `data`, verify by
/// re-parsing, and return the serialized result. Returns a human-readable error
/// string on any failure (invalid JSON, inactive/missing index, bad value, or a
/// verification mismatch).
pub fn apply_edits(data: &[u8], edits_json: &str) -> Result<Vec<u8>, String> {
    let spec: EditSpec =
        serde_json::from_str(edits_json).map_err(|e| format!("invalid edits JSON: {e}"))?;
    let mut cp = Codeplug::parse(data).map_err(|e| e.to_string())?;

    // --- 1. Per-index updates on the currently-active records. ---
    for e in &spec.channels {
        let ch = cp
            .channel_mut(e.index)
            .ok_or_else(|| format!("channel {} is not active in this codeplug", e.index))?;
        apply_channel_fields(ch, e.index, &e.fields)?;
    }
    for z in &spec.zones {
        let zone = cp
            .zone_mut(z.index)
            .ok_or_else(|| format!("zone {} is not active in this codeplug", z.index))?;
        apply_zone_fields(zone, z.index, &z.fields)?;
    }
    for s in &spec.scan_lists {
        let sl = cp
            .scan_list_mut(s.index)
            .ok_or_else(|| format!("scan list {} is not active in this codeplug", s.index))?;
        apply_scan_list_fields(sl, s.index, &s.fields)?;
    }
    for c in &spec.contacts {
        apply_contact_fields(&mut cp, c.index, &c.fields)?;
    }
    for g in &spec.group_lists {
        let gl = cp
            .group_list_mut(g.index)
            .ok_or_else(|| format!("group list {} is not active in this codeplug", g.index))?;
        apply_group_list_fields(gl, g.index, &g.fields)?;
    }
    for r in &spec.radio_ids {
        let rid = cp
            .radio_id_mut(r.index)
            .ok_or_else(|| format!("radio ID {} is not active in this codeplug", r.index))?;
        apply_radio_id_fields(rid, r.index, &r.fields)?;
    }

    // --- 2. Moves. Run after index-addressed updates (so those still target
    // the pre-move slots) and before removals/adds. ---
    for m in &spec.move_channels {
        cp.move_channel(m.from, m.to)?;
    }
    for m in &spec.move_zones {
        cp.move_zone(m.from, m.to)?;
    }
    for m in &spec.move_scan_lists {
        cp.move_scan_list(m.from, m.to)?;
    }
    for m in &spec.move_contacts {
        cp.move_contact(m.from, m.to)?;
    }
    for m in &spec.move_group_lists {
        cp.move_group_list(m.from, m.to)?;
    }
    for m in &spec.move_radio_ids {
        cp.move_radio_id(m.from, m.to)?;
    }

    // --- 3. Removals. ---
    for &i in &spec.remove_channels {
        if !cp.remove_channel(i) {
            return Err(format!("cannot remove channel {i}: not active"));
        }
    }
    for &i in &spec.remove_zones {
        if !cp.remove_zone(i) {
            return Err(format!("cannot remove zone {i}: not active"));
        }
    }
    for &i in &spec.remove_scan_lists {
        if !cp.remove_scan_list(i) {
            return Err(format!("cannot remove scan list {i}: not active"));
        }
    }
    for &i in &spec.remove_contacts {
        if !cp.remove_contact(i) {
            return Err(format!("cannot remove contact {i}: not active"));
        }
    }
    for &i in &spec.remove_group_lists {
        if !cp.remove_group_list(i) {
            return Err(format!("cannot remove group list {i}: not active"));
        }
    }
    for &i in &spec.remove_radio_ids {
        if !cp.remove_radio_id(i) {
            return Err(format!("cannot remove radio ID {i}: not active"));
        }
    }

    // --- 4. Additions: create a default record, then apply provided fields. ---
    for f in &spec.add_channels {
        let i = cp.add_channel().ok_or("no free channel slots")?;
        apply_channel_fields(cp.channel_mut(i).unwrap(), i, f)?;
    }
    for f in &spec.add_zones {
        let i = cp.add_zone().ok_or("no free zone slots")?;
        apply_zone_fields(cp.zone_mut(i).unwrap(), i, f)?;
    }
    for f in &spec.add_scan_lists {
        let i = cp.add_scan_list().ok_or("no free scan-list slots")?;
        apply_scan_list_fields(cp.scan_list_mut(i).unwrap(), i, f)?;
    }
    for f in &spec.add_contacts {
        let i = cp.add_contact().ok_or("no free contact slots")?;
        apply_contact_fields(&mut cp, i, f)?;
    }
    for f in &spec.add_group_lists {
        let i = cp.add_group_list().ok_or("no free group-list slots")?;
        apply_group_list_fields(cp.group_list_mut(i).unwrap(), i, f)?;
    }
    for f in &spec.add_radio_ids {
        let i = cp.add_radio_id().ok_or("no free radio-ID slots")?;
        apply_radio_id_fields(cp.radio_id_mut(i).unwrap(), i, f)?;
    }

    let out = cp.serialize();

    // Round-trip verification: the output must re-parse, per-index channel
    // updates must decode back, and removed slots must be inactive.
    let check = Codeplug::parse(&out).map_err(|e| e.to_string())?;
    for e in &spec.channels {
        let ch = check
            .channels()
            .find(|c| c.index == e.index)
            .ok_or_else(|| format!("channel {} missing after edit", e.index))?;
        if let Some(name) = &e.fields.name {
            if ch.name != name.trim_end_matches(' ') {
                return Err(format!(
                    "channel {} name verification failed: wrote {name:?}, read back {:?}",
                    e.index, ch.name
                ));
            }
        }
        if let Some(hz) = e.fields.rx_frequency_hz {
            if ch.rx_frequency_hz != hz {
                return Err(format!(
                    "channel {} RX verification failed: wrote {hz}, read back {}",
                    e.index, ch.rx_frequency_hz
                ));
            }
        }
        if let Some(hz) = e.fields.tx_frequency_hz {
            if ch.tx_frequency_hz != hz {
                return Err(format!(
                    "channel {} TX verification failed: wrote {hz}, read back {}",
                    e.index, ch.tx_frequency_hz
                ));
            }
        }
    }
    for &i in &spec.remove_channels {
        if check.channels().any(|c| c.index == i) {
            return Err(format!("channel {i} still active after removal"));
        }
    }
    for &i in &spec.remove_contacts {
        if check.contacts().any(|c| c.index == i) {
            return Err(format!("contact {i} still active after removal"));
        }
    }
    verify_moved("channel", &spec.move_channels, |i| {
        check.channels().any(|c| c.index == i)
    })?;
    verify_moved("zone", &spec.move_zones, |i| {
        check.zones().any(|z| z.index == i)
    })?;
    verify_moved("scan list", &spec.move_scan_lists, |i| {
        check.scan_lists().any(|s| s.index == i)
    })?;
    verify_moved("contact", &spec.move_contacts, |i| {
        check.contacts().any(|c| c.index == i)
    })?;
    verify_moved("group list", &spec.move_group_lists, |i| {
        check.group_lists().any(|g| g.index == i)
    })?;
    verify_moved("radio ID", &spec.move_radio_ids, |i| {
        check.radio_ids().any(|r| r.index == i)
    })?;

    Ok(out)
}

/// Confirm every moved record re-parsed as active at its destination slot.
/// (The vacated source slot is not asserted empty: a same-batch add fills the
/// first free slot, which may legitimately reuse it.)
fn verify_moved(
    kind: &str,
    moves: &[MoveOp],
    is_active: impl Fn(usize) -> bool,
) -> Result<(), String> {
    for m in moves {
        if !is_active(m.to) {
            return Err(format!(
                "{kind} move to slot {} failed verification: destination not active",
                m.to
            ));
        }
    }
    Ok(())
}

/// Validate a channel/zone/contact/etc. name for a 16-byte Latin1 name slot.
fn validate_name(kind: &str, index: usize, name: &str) -> Result<(), String> {
    if !name.is_ascii() {
        return Err(format!(
            "{kind} {index} name {name:?} contains non-ASCII characters"
        ));
    }
    if name.len() > NAME_LEN {
        return Err(format!(
            "{kind} {index} name {name:?} is longer than {NAME_LEN} characters"
        ));
    }
    Ok(())
}

/// Validate a frequency in Hz fits the radio's 8-digit BCD field (10 Hz units).
fn validate_freq(kind: &str, index: usize, hz: u32) -> Result<(), String> {
    if !hz.is_multiple_of(10) {
        return Err(format!(
            "channel {index} {kind} frequency {hz} Hz must be a multiple of 10 Hz"
        ));
    }
    if hz / 10 > 99_999_999 {
        return Err(format!(
            "channel {index} {kind} frequency {hz} Hz is out of range"
        ));
    }
    Ok(())
}

/// Validate a DMR ID fits the radio's 8-digit BCD field.
fn validate_dmr_id(kind: &str, index: usize, number: u32) -> Result<(), String> {
    if number > 99_999_999 {
        return Err(format!(
            "{kind} {index} DMR number {number} is out of range (max 99999999)"
        ));
    }
    Ok(())
}

/// Parse the channel `mode` string into a [`ChannelMode`].
fn parse_mode(s: &str) -> Result<ChannelMode, String> {
    match s {
        "analog" => Ok(ChannelMode::Analog),
        "digital" => Ok(ChannelMode::Digital),
        "mixed_analog" => Ok(ChannelMode::MixedAnalog),
        "mixed_digital" => Ok(ChannelMode::MixedDigital),
        other => Err(format!("unknown channel mode {other:?}")),
    }
}

/// Parse the channel `power` string into a [`Power`].
fn parse_power(s: &str) -> Result<Power, String> {
    match s {
        "low" => Ok(Power::Low),
        "mid" => Ok(Power::Mid),
        "high" => Ok(Power::High),
        "turbo" => Ok(Power::Turbo),
        other => Err(format!("unknown power level {other:?}")),
    }
}

/// Parse the channel `bandwidth` string into a [`Bandwidth`].
fn parse_bandwidth(s: &str) -> Result<Bandwidth, String> {
    match s {
        "narrow" => Ok(Bandwidth::Narrow),
        "wide" => Ok(Bandwidth::Wide),
        other => Err(format!("unknown bandwidth {other:?}")),
    }
}

/// Parse the channel `admit` ("TX permit") string into an [`Admit`].
fn parse_admit(s: &str) -> Result<Admit, String> {
    match s {
        "always" => Ok(Admit::Always),
        "channel_free" => Ok(Admit::ChannelFree),
        "different_color_code" => Ok(Admit::DifferentColorCode),
        "same_color_code" => Ok(Admit::SameColorCode),
        other => Err(format!("unknown admit/TX-permit {other:?}")),
    }
}

/// Parse the channel `squelch_mode` string into a [`SquelchMode`].
fn parse_squelch_mode(s: &str) -> Result<SquelchMode, String> {
    match s {
        "carrier" => Ok(SquelchMode::Carrier),
        "tone" => Ok(SquelchMode::Tone),
        other => Err(format!("unknown squelch mode {other:?}")),
    }
}

/// Parse the channel `optional_signaling` string into an [`OptSignaling`].
fn parse_optional_signaling(s: &str) -> Result<OptSignaling, String> {
    match s {
        "off" => Ok(OptSignaling::Off),
        "dtmf" => Ok(OptSignaling::Dtmf),
        "two_tone" => Ok(OptSignaling::TwoTone),
        "five_tone" => Ok(OptSignaling::FiveTone),
        other => Err(format!("unknown optional signaling {other:?}")),
    }
}

/// Parse a channel signaling-mode string into a [`SignalingMode`].
fn parse_signaling_mode(s: &str) -> Result<SignalingMode, String> {
    match s {
        "none" => Ok(SignalingMode::None),
        "ctcss" => Ok(SignalingMode::Ctcss),
        "dcs" => Ok(SignalingMode::Dcs),
        other => Err(format!("unknown signaling mode {other:?}")),
    }
}

/// Parse the contact `call_type` string into a [`CallType`].
fn parse_call_type(s: &str) -> Result<CallType, String> {
    match s {
        "private" => Ok(CallType::Private),
        "group" => Ok(CallType::Group),
        "all" => Ok(CallType::All),
        other => Err(format!("unknown call type {other:?}")),
    }
}

/// Apply channel field edits to a mutable channel record.
fn apply_channel_fields(ch: &mut Channel, index: usize, f: &ChannelFields) -> Result<(), String> {
    if let Some(name) = &f.name {
        validate_name("channel", index, name)?;
        ch.set_name(name);
    }
    if let Some(hz) = f.rx_frequency_hz {
        validate_freq("RX", index, hz)?;
        ch.set_rx_frequency(hz);
    }
    if let Some(hz) = f.tx_frequency_hz {
        validate_freq("TX", index, hz)?;
        ch.set_tx_frequency(hz);
    }
    if let Some(m) = &f.mode {
        ch.set_mode(parse_mode(m)?);
    }
    if let Some(p) = &f.power {
        ch.set_power(parse_power(p)?);
    }
    if let Some(b) = &f.bandwidth {
        ch.set_bandwidth(parse_bandwidth(b)?);
    }
    if let Some(cc) = f.color_code {
        if cc > 15 {
            return Err(format!(
                "channel {index} color code {cc} is out of range (0-15)"
            ));
        }
        ch.set_color_code(cc);
    }
    if let Some(ts) = f.time_slot {
        if ts != 1 && ts != 2 {
            return Err(format!("channel {index} time slot {ts} must be 1 or 2"));
        }
        ch.set_time_slot(ts);
    }
    if let Some(ci) = f.contact_index {
        ch.set_contact_index(ci);
    }
    if let Some(ri) = f.radio_id_index {
        ch.set_radio_id_index(ri);
    }
    if let Some(gi) = f.group_list_index {
        ch.set_group_list_index(gi);
    }
    if let Some(on) = f.rx_only {
        ch.set_rx_only(on);
    }
    if let Some(on) = f.talk_around {
        ch.set_talk_around(on);
    }
    if let Some(on) = f.call_confirm {
        ch.set_call_confirm(on);
    }
    if let Some(on) = f.work_alone {
        ch.set_work_alone(on);
    }
    if let Some(on) = f.simplex_tdma {
        ch.set_simplex_tdma(on);
    }
    if let Some(on) = f.rx_aprs {
        ch.set_rx_aprs(on);
    }
    if let Some(a) = &f.admit {
        ch.set_admit(parse_admit(a)?);
    }
    if let Some(sq) = &f.squelch_mode {
        ch.set_squelch_mode(parse_squelch_mode(sq)?);
    }
    if let Some(os) = &f.optional_signaling {
        ch.set_optional_signaling(parse_optional_signaling(os)?);
    }
    if let Some(m) = &f.rx_signaling_mode {
        ch.set_rx_signaling_mode(parse_signaling_mode(m)?);
    }
    if let Some(m) = &f.tx_signaling_mode {
        ch.set_tx_signaling_mode(parse_signaling_mode(m)?);
    }
    if let Some(t) = f.rx_ctcss {
        ch.set_rx_ctcss(t);
    }
    if let Some(t) = f.tx_ctcss {
        ch.set_tx_ctcss(t);
    }
    if let Some(c) = f.rx_dcs {
        ch.set_rx_dcs(c);
    }
    if let Some(c) = f.tx_dcs {
        ch.set_tx_dcs(c);
    }
    if let Some(si) = f.scan_list_index {
        ch.set_scan_list_index(si);
    }
    if let Some(di) = f.dtmf_id_index {
        ch.set_dtmf_id_index(di);
    }
    if let Some(ti) = f.two_tone_id_index {
        ch.set_two_tone_id_index(ti);
    }
    if let Some(fi) = f.five_tone_id_index {
        ch.set_five_tone_id_index(fi);
    }
    if let Some(di) = f.two_tone_decode_index {
        ch.set_two_tone_decode_index(di);
    }
    Ok(())
}

/// Apply zone field edits to a mutable zone record.
fn apply_zone_fields(zone: &mut super::Zone, index: usize, f: &ZoneFields) -> Result<(), String> {
    if let Some(name) = &f.name {
        validate_name("zone", index, name)?;
        zone.set_name(name);
    }
    if let Some(members) = &f.members {
        zone.set_members(members);
    }
    Ok(())
}

/// Apply scan-list field edits to a mutable scan-list record. Absent fields are
/// left untouched.
fn apply_scan_list_fields(
    sl: &mut super::ScanList,
    index: usize,
    f: &ScanListFields,
) -> Result<(), String> {
    if let Some(name) = &f.name {
        validate_name("scan list", index, name)?;
        sl.set_name(name);
    }
    if let Some(members) = &f.members {
        sl.set_members(members);
    }
    if let Some(v) = f.priority_channel_select {
        sl.set_priority_channel_select(v);
    }
    if let Some(v) = f.priority_channel_1 {
        sl.set_priority_channel_1(v);
    }
    if let Some(v) = f.priority_channel_2 {
        sl.set_priority_channel_2(v);
    }
    if let Some(v) = f.look_back_a {
        sl.set_look_back_a(v);
    }
    if let Some(v) = f.look_back_b {
        sl.set_look_back_b(v);
    }
    if let Some(v) = f.dropout_delay {
        sl.set_dropout_delay(v);
    }
    if let Some(v) = f.dwell_time {
        sl.set_dwell_time(v);
    }
    if let Some(v) = f.revert_channel {
        sl.set_revert_channel(v);
    }
    Ok(())
}

/// Apply contact field edits by index through the codeplug (so the reverse
/// lookup tables are dirtied only when the number/type changes).
fn apply_contact_fields(cp: &mut Codeplug, index: usize, f: &ContactFields) -> Result<(), String> {
    if let Some(name) = &f.name {
        validate_name("contact", index, name)?;
        if !cp.set_contact_name(index, name) {
            return Err(format!("contact {index} is not active in this codeplug"));
        }
    }
    if let Some(number) = f.number {
        validate_dmr_id("contact", index, number)?;
        cp.set_contact_number(index, number);
    }
    if let Some(ct) = &f.call_type {
        cp.set_contact_call_type(index, parse_call_type(ct)?);
    }
    Ok(())
}

/// Apply group-list field edits to a mutable group-list record.
fn apply_group_list_fields(
    gl: &mut super::GroupList,
    index: usize,
    f: &GroupListFields,
) -> Result<(), String> {
    if let Some(name) = &f.name {
        validate_name("group list", index, name)?;
        gl.set_name(name);
    }
    if let Some(members) = &f.members {
        gl.set_members(members);
    }
    Ok(())
}

/// Apply radio-ID field edits to a mutable radio-ID record.
fn apply_radio_id_fields(
    rid: &mut super::RadioId,
    index: usize,
    f: &RadioIdFields,
) -> Result<(), String> {
    if let Some(name) = &f.name {
        validate_name("radio ID", index, name)?;
        rid.set_name(name);
    }
    if let Some(number) = f.number {
        validate_dmr_id("radio ID", index, number)?;
        rid.set_number(number);
    }
    Ok(())
}
