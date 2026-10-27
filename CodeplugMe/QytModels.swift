import Foundation

/// The QYT KT-5000 half of the dump envelope, plus the edit spec that goes back
/// the other way.
///
/// These types are deliberately separate from the AnyTone ones in `Models.swift`
/// rather than a shared union. The fields that look alike do not mean alike: an
/// AnyTone `rx_ctcss` is an index into the radio's tone table, while a QYT tone
/// is a tagged value in units of 0.1 Hz; power is four-valued there and
/// two-valued here; and this radio has no DMR fields at all. A shared record
/// whose meaning depended on a sibling `model` field is exactly the shape that
/// writes a tone index into a frequency.

// MARK: - Tone

/// A sub-audible squelch tone, matching the core's tagged JSON
/// (`{"kind":"none"}`, `{"kind":"ctcss","hz_tenths":885}`,
/// `{"kind":"dcs","code":23,"inverted":false}`).
///
/// The "no tone" case is spelled `off` rather than `none` so `Tone?` can never
/// be confused with `Optional.none` at a call site.
enum Tone: Hashable, Codable {
    /// No tone; the squelch is carrier-operated.
    case off
    /// CTCSS, in tenths of a hertz (885 = 88.5 Hz).
    case ctcss(hzTenths: UInt16)
    /// DCS / DTCS, by the three-digit code written on the radio.
    case dcs(code: UInt16, inverted: Bool)

    private enum CodingKeys: String, CodingKey {
        case kind, code, inverted
        case hzTenths = "hz_tenths"
    }

    /// Decode the core's tagged form. An unknown kind throws rather than
    /// defaulting to "off", which would silently drop a tone the radio has.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .kind) {
        case "none":
            self = .off
        case "ctcss":
            self = .ctcss(hzTenths: try c.decode(UInt16.self, forKey: .hzTenths))
        case "dcs":
            self = .dcs(code: try c.decode(UInt16.self, forKey: .code),
                        inverted: try c.decode(Bool.self, forKey: .inverted))
        case let other:
            throw DecodingError.dataCorruptedError(
                forKey: .kind, in: c, debugDescription: "unknown tone kind “\(other)”")
        }
    }

    /// Encode back into the core's tagged form, for an edit.
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .off:
            try c.encode("none", forKey: .kind)
        case .ctcss(let hzTenths):
            try c.encode("ctcss", forKey: .kind)
            try c.encode(hzTenths, forKey: .hzTenths)
        case .dcs(let code, let inverted):
            try c.encode("dcs", forKey: .kind)
            try c.encode(code, forKey: .code)
            try c.encode(inverted, forKey: .inverted)
        }
    }

    /// How the tone reads in a table cell: "Off", "88.5 Hz", "D023N".
    var label: String {
        switch self {
        case .off:
            return "Off"
        case .ctcss(let hzTenths):
            return "\(hzTenths / 10).\(hzTenths % 10) Hz"
        case .dcs(let code, let inverted):
            return String(format: "D%03d%@", Int(code), inverted ? "I" : "N")
        }
    }

    /// Which of the three cases this is, as a picker tag.
    var kind: ToneKind {
        switch self {
        case .off: return .off
        case .ctcss: return .ctcss
        case .dcs: return .dcs
        }
    }
}

/// The three tone cases as a plain value, so a picker can select between them
/// without carrying the associated values around.
enum ToneKind: String, CaseIterable, Identifiable, Hashable {
    case off, ctcss, dcs

    var id: String { rawValue }

    var label: String {
        switch self {
        case .off: return "Off"
        case .ctcss: return "CTCSS"
        case .dcs: return "DCS"
        }
    }

    /// A representative tone of this kind, used when the user switches the
    /// picker. 88.5 Hz and D023 are the first entries of their respective
    /// standard tables.
    var defaultTone: Tone {
        switch self {
        case .off: return .off
        case .ctcss: return .ctcss(hzTenths: 885)
        case .dcs: return .dcs(code: 23, inverted: false)
        }
    }
}

// MARK: - Dump payload

/// One decoded QYT channel from `codeplugme_dump_json`.
///
/// Everything from `scode` down is read-only: the core decodes those bits from
/// the record's flag bytes but has no setters for them yet, because their
/// meanings come from CHIRP and have not been confirmed against this radio one
/// field at a time. `flagsRaw` carries bytes 12-15 verbatim so the unmodelled
/// bits are visible and provably round-trip.
struct DumpQytChannel: Codable, Identifiable, Hashable {
    let index: Int
    var name: String
    var rxFrequencyHz: UInt32
    /// Nil when the record stores no TX frequency, making the channel
    /// receive-only.
    var txFrequencyHz: UInt32?
    var rxTone: Tone
    var txTone: Tone
    /// PTT-ID signal code index (the low nibble of flag byte 12).
    var scode: Int = 0
    /// "Tone/DTCS" | "Tone/DTCS and Optsig" | "Tone/DTCS or Optsig".
    var speakerMute: String = "Tone/DTCS"
    /// "OFF" | "DTMF" | "2TONE" | "5TONE".
    var optionalSignaling: String = "OFF"
    var scramble: Bool = false
    /// "High" | "Low", matching `capabilities.power_options`.
    var power: String = "High"
    /// "Wide" | "Narrow", matching `capabilities.bandwidth_options`.
    var bandwidth: String = "Wide"
    var busyLockout: Bool = false
    /// Included in scan (the core's `scan_add`; false means skipped).
    var scanAdd: Bool = true
    /// "OFF" | "BOT" | "EOT" | "BOTH".
    var pttId: String = "OFF"
    /// Flag bytes 12-15 verbatim.
    var flagsRaw: [UInt8] = [0, 0, 0, 0]

    var id: Int { index }

    enum CodingKeys: String, CodingKey {
        case index, name, scode, scramble, power, bandwidth
        case rxFrequencyHz = "rx_frequency_hz"
        case txFrequencyHz = "tx_frequency_hz"
        case rxTone = "rx_tone"
        case txTone = "tx_tone"
        case speakerMute = "speaker_mute"
        case optionalSignaling = "optional_signaling"
        case busyLockout = "busy_lockout"
        case scanAdd = "scan_add"
        case pttId = "ptt_id"
        case flagsRaw = "flags_raw"
    }

    /// Sort key for the TX column. `Optional` isn't `Comparable`, so a
    /// receive-only channel sorts as if it transmitted at 0 Hz, which puts the
    /// whole group together at one end rather than scattering it.
    var txSortKey: UInt32 { txFrequencyHz ?? 0 }

    /// Sort keys for the two tone columns: the same text the cell shows.
    var rxToneLabel: String { rxTone.label }
    var txToneLabel: String { txTone.label }

    /// Sort key for the Scan column. `Bool` isn't `Comparable`, and the cell
    /// draws nothing for an included channel, so this sorts the skipped ones
    /// together at one end.
    var scanLabel: String { scanAdd ? "Scan" : "Skip" }

    /// Flag bytes 12-15 as hex, for the read-only row in the editor.
    var flagsRawHex: String {
        flagsRaw.map { String(format: "%02X", $0) }.joined(separator: " ")
    }
}

/// The KT-5000 settings header the core models (`0x0E00`-`0x0E35`). Everything
/// else in the settings block is byte-preserved and not modelled.
struct DumpQytSettings: Codable, Hashable {
    /// A stand-in for "no document open", so a pane's draft is never optional.
    /// The values are the header's own zeroes, not defaults anyone would write:
    /// nothing is sent to the core unless the draft differs from a real dump.
    static let empty = DumpQytSettings(tmr: 0, squelch: 0, tot: 0, totSeconds: 15,
                                       volume: 0, backlightTimer: 0, beep: false,
                                       scanRev: "TO", rememberPower: false)

    /// Dual-band waiting (Menu 0 TMR): 0 = Off, 1 = On.
    var tmr: Int
    /// Squelch level, 0-9.
    var squelch: Int
    /// Transmit timeout as a step index.
    var tot: Int
    /// The same timeout in seconds, derived by the core.
    var totSeconds: Int
    /// Power-on volume.
    var volume: Int
    /// Backlight timer: 0 is off, otherwise seconds.
    var backlightTimer: Int
    /// Key beep on/off.
    var beep: Bool
    /// Scan resume method: `"TO"` | `"CO"` | `"SE"`. Nil when the stored byte
    /// is outside that set; the picker then shows an "Unknown" entry.
    var scanRev: String?
    /// When on, applying power restores the last on/off state; when off,
    /// applying power always leaves the radio off.
    var rememberPower: Bool

    enum CodingKeys: String, CodingKey {
        case tmr, squelch, tot, volume, beep
        case totSeconds = "tot_seconds"
        case backlightTimer = "backlight_timer"
        case scanRev = "scan_rev"
        case rememberPower = "remember_power"
    }
}

/// One of the two VFO channels (A or B). The offset and shift are read-only in
/// the core: their encoding is assumed rather than confirmed.
struct DumpQytBand: Codable, Identifiable, Hashable {
    let index: Int
    /// False when the record holds no decodable frequency (an unused VFO).
    var programmed: Bool
    var frequencyHz: UInt32?
    var offsetHz: UInt32?
    /// "Off" | "Plus" | "Minus".
    var shift: String
    var rxTone: Tone
    var txTone: Tone
    var scode: Int
    var speakerMute: String
    var optionalSignaling: String
    var scramble: Bool
    var bandwidth: String
    var power: String
    var stepHz: UInt32?

    var id: Int { index }

    enum CodingKeys: String, CodingKey {
        case index, programmed, shift, scode, scramble, bandwidth, power
        case frequencyHz = "frequency_hz"
        case offsetHz = "offset_hz"
        case rxTone = "rx_tone"
        case txTone = "tx_tone"
        case speakerMute = "speaker_mute"
        case optionalSignaling = "optional_signaling"
        case stepHz = "step_hz"
    }
}

/// The QYT payload of `codeplugme_dump_json`, carried under the envelope's `"qyt"`
/// key. Non-optional for the same reason `AnyToneDump`'s families are: a core
/// bug that dropped one should fail the decode loudly.
struct QytDump: Codable {
    let channels: [DumpQytChannel]
    let settings: DumpQytSettings
    let bands: [DumpQytBand]
    /// The ASCII line at `0x0FE0` that the radio shows on its LCD. Ships as
    /// `Model:KT-5000`, which is why it reads like an identity field.
    let staticMessage: String

    enum CodingKeys: String, CodingKey {
        case channels, settings, bands
        case staticMessage = "static_message"
    }
}

// MARK: - Edit spec (encoded to `codeplugme_apply_edits`)

/// One QYT channel update or add. `index` is omitted for adds, and nil fields
/// are left out by the encoder so only what changed is applied.
///
/// `scanAdd` is the only flag field here. Its bit was confirmed against a real
/// KT-5000, so the core has a setter for it; power, bandwidth, BCL, PTT-ID and
/// the rest are still decoded for display only, and offering a control for them
/// would silently discard the user's change on the next reload.
struct QytChannelEdit: Encodable {
    var index: Int?
    var name: String?
    var rxFrequencyHz: UInt32?
    var txFrequencyHz: UInt32?
    /// Send an explicit `null` for the TX frequency, which is how the core is
    /// told to make the channel receive-only. Absent means "leave it alone", so
    /// clearing needs its own flag rather than just a nil `txFrequencyHz`.
    var clearTxFrequency = false
    var rxTone: Tone?
    var txTone: Tone?
    /// Include the channel in scanning; false skips it.
    var scanAdd: Bool?

    enum CodingKeys: String, CodingKey {
        case index, name
        case rxFrequencyHz = "rx_frequency_hz"
        case txFrequencyHz = "tx_frequency_hz"
        case rxTone = "rx_tone"
        case txTone = "tx_tone"
        case scanAdd = "scan_add"
    }

    /// Hand-written so `clearTxFrequency` can emit a real JSON null; the
    /// synthesized encoder would drop the key instead.
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encodeIfPresent(index, forKey: .index)
        try c.encodeIfPresent(name, forKey: .name)
        try c.encodeIfPresent(rxFrequencyHz, forKey: .rxFrequencyHz)
        if clearTxFrequency {
            try c.encodeNil(forKey: .txFrequencyHz)
        } else {
            try c.encodeIfPresent(txFrequencyHz, forKey: .txFrequencyHz)
        }
        try c.encodeIfPresent(rxTone, forKey: .rxTone)
        try c.encodeIfPresent(txTone, forKey: .txTone)
        try c.encodeIfPresent(scanAdd, forKey: .scanAdd)
    }
}

/// Settings-header fields to change, all optional so an untouched field is left
/// out of the JSON entirely and the core never writes it.
///
/// The field list is exactly what `core/src/qyt/codeplug/settings.rs` models.
/// Everything else in the settings block is byte-preserved and deliberately
/// absent here; see that module's docs for why.
struct QytSettingsEdit: Encodable, Equatable {
    var tmr: Int?
    var squelch: Int?
    var tot: Int?
    var volume: Int?
    var backlightTimer: Int?
    var beep: Bool?
    var scanRev: String?
    var rememberPower: Bool?

    /// True when the edit would change nothing, so the caller can skip the whole
    /// round trip rather than stage a no-op undo entry.
    var isEmpty: Bool { self == QytSettingsEdit() }

    enum CodingKeys: String, CodingKey {
        case tmr, squelch, tot, volume, beep
        case backlightTimer = "backlight_timer"
        case scanRev = "scan_rev"
        case rememberPower = "remember_power"
    }
}

/// One VFO channel's changes, addressed by index 0 (A) or 1 (B).
///
/// The repeater offset and shift direction are absent on purpose: the core has
/// no setters for them because their encoding is assumed rather than confirmed
/// against the radio.
struct QytBandEdit: Encodable, Equatable {
    let index: Int
    var frequencyHz: UInt32?
    var rxTone: Tone?
    var txTone: Tone?
    /// "Wide" | "Narrow", from `capabilities.bandwidth_options`.
    var bandwidth: String?
    /// "High" | "Low", from `capabilities.power_options`.
    var power: String?
    var stepHz: UInt32?

    /// True when only the index is set, meaning this record needs no round trip.
    var isEmpty: Bool { self == QytBandEdit(index: index) }

    enum CodingKeys: String, CodingKey {
        case index, bandwidth, power
        case frequencyHz = "frequency_hz"
        case rxTone = "rx_tone"
        case txTone = "tx_tone"
        case stepHz = "step_hz"
    }
}

/// A batch of QYT edits, the `qyt` half of the edit envelope.
struct QytEditSpec: Encodable {
    var channels: [QytChannelEdit] = []
    var addChannels: [QytChannelEdit] = []
    var removeChannels: [Int] = []
    var moveChannels: [MoveOp] = []
    /// Nil when the batch changes no settings; the core takes a single object
    /// here rather than an array, since there is one header.
    var settings: QytSettingsEdit?
    var bands: [QytBandEdit] = []
    /// The LCD static message. Nil leaves it alone; the core space-pads it out
    /// to the field's 16 bytes.
    var staticMessage: String?

    /// True when there is nothing to apply.
    var isEmpty: Bool {
        channels.isEmpty && addChannels.isEmpty && removeChannels.isEmpty
            && moveChannels.isEmpty && settings == nil && bands.isEmpty
            && staticMessage == nil
    }

    enum CodingKeys: String, CodingKey {
        case channels, settings, bands
        case addChannels = "add_channels"
        case removeChannels = "remove_channels"
        case moveChannels = "move_channels"
        case staticMessage = "static_message"
    }
}

/// The model-tagged envelope an edit batch travels in. The AnyTone side still
/// sends its spec bare, which the core accepts for backward compatibility, but
/// a QYT batch has to name itself or the core would read it as an AnyTone spec.
struct QytEditEnvelope: Encodable {
    let qyt: QytEditSpec
}

// MARK: - Display / value helpers

/// Format an optional frequency as MHz, or an em-space dash for a receive-only
/// channel's absent TX frequency.
func formatOptionalMHz(_ hz: UInt32?) -> String {
    guard let hz else { return "—" }
    return formatMHz(hz)
}

/// Convert a MHz value typed into a field to the Hz a record stores, rounded to
/// the codeplug's 10 Hz BCD resolution and clamped to what the field can hold.
func hzFromMHz(_ mhz: Double) -> UInt32 {
    let clamped = min(max(mhz, 0), 999.999_99)
    return UInt32((clamped * 100_000).rounded()) * 10
}

/// The human label for a `tag` in a set of core-supplied picker options, or the
/// tag itself if the core doesn't list it.
func pickerLabel(_ tag: String, in options: [PickerOption]) -> String {
    options.first { $0.tag == tag }?.label ?? tag
}

/// Truncate a channel name to what the radio can store. The KT-5000 holds 7
/// characters; the limit comes from `capabilities.channel_name_length` rather
/// than a constant here so it follows the core.
func limitedName(_ name: String, to limit: Int) -> String {
    name.count <= limit ? name : String(name.prefix(limit))
}

/// The letter on a VFO channel: 0 is A, 1 is B.
func qytBandName(_ index: Int) -> String {
    let letters = ["A", "B"]
    return letters.indices.contains(index) ? letters[index] : String(index)
}

/// Which settings fields the pane's draft changes, relative to the document.
///
/// Only differing fields go into the edit, so applying a squelch change never
/// rewrites the other four bytes — including bytes holding a value the radio put
/// there that is out of range for the control showing it.
func qytSettingsEdit(from old: DumpQytSettings, to new: DumpQytSettings) -> QytSettingsEdit {
    var e = QytSettingsEdit()
    if new.tmr != old.tmr { e.tmr = new.tmr }
    if new.squelch != old.squelch { e.squelch = new.squelch }
    if new.tot != old.tot { e.tot = new.tot }
    if new.volume != old.volume { e.volume = new.volume }
    if new.backlightTimer != old.backlightTimer { e.backlightTimer = new.backlightTimer }
    if new.beep != old.beep { e.beep = new.beep }
    if new.scanRev != old.scanRev { e.scanRev = new.scanRev }
    if new.rememberPower != old.rememberPower { e.rememberPower = new.rememberPower }
    return e
}

/// Which fields of one VFO the pane's draft changes.
///
/// A frequency is sent only when the draft actually has one: an unprogrammed
/// record edits from a displayed 0 MHz, and 0 is not a frequency anyone means to
/// write. The offset and shift direction are never sent — the core has no
/// setters for them.
func qytBandEdit(from old: DumpQytBand, to new: DumpQytBand) -> QytBandEdit {
    var e = QytBandEdit(index: old.index)
    if let hz = new.frequencyHz, hz > 0, hz != old.frequencyHz { e.frequencyHz = hz }
    if new.rxTone != old.rxTone { e.rxTone = new.rxTone }
    if new.txTone != old.txTone { e.txTone = new.txTone }
    if new.bandwidth != old.bandwidth { e.bandwidth = new.bandwidth }
    if new.power != old.power { e.power = new.power }
    if let hz = new.stepHz, hz != old.stepHz { e.stepHz = hz }
    return e
}

/// QYT duplicates: same RX, TX and tones (two receive-only channels on one RX
/// count). The radio is analog-only, so there is no color code or slot to match.
func duplicateQytChannelSlots(_ channels: [DumpQytChannel]) -> [Int: [Int]] {
    duplicateSlots(channels, slot: \.index) {
        ChannelFrequencyKey(rxFrequencyHz: $0.rxFrequencyHz, txFrequencyHz: $0.txFrequencyHz,
                            rxTone: $0.rxTone == .off ? nil : $0.rxTone.label,
                            txTone: $0.txTone == .off ? nil : $0.txTone.label)
    }
}

/// Table sort for the QYT duplicates-only view: RX, then TX (receive-only
/// first), then slot, so each group of twins sits together in slot order.
let qytChannelFrequencySortOrder = [
    KeyPathComparator(\DumpQytChannel.rxFrequencyHz),
    KeyPathComparator(\DumpQytChannel.txSortKey),
    KeyPathComparator(\DumpQytChannel.index),
]

/// Whether a QYT channel passes the channels-pane filter. Name and either
/// frequency are matched, so typing "462.5" finds a channel whose name says
/// nothing about its frequency. Kept out of the view so it can be tested
/// without SwiftUI.
///
/// There is deliberately no mode / colour-code / slot filter: this radio is
/// analog-only and has none of those fields.
func qytChannelMatchesFilter(_ channel: DumpQytChannel, query: String) -> Bool {
    guard !query.isEmpty else { return true }
    return channel.name.localizedCaseInsensitiveContains(query)
        || formatMHz(channel.rxFrequencyHz).contains(query)
        || formatOptionalMHz(channel.txFrequencyHz).contains(query)
}
