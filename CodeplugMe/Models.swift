import Foundation

/// One serial port as reported by the core's port enumeration
/// (`codeplugme_ports_json`).
struct PortEntry: Codable, Identifiable, Hashable {
    let name: String
    let vid: UInt16?
    let pid: UInt16?
    let product: String?
    /// The radio this port's USB VID/PID names, or nil when the core doesn't
    /// recognize the adapter. Not every supported radio's bridge is known, so
    /// nil means "ask the user", never "not a radio".
    let model: RadioModel?
    /// Always exactly `model != nil`; the core guarantees it.
    let likelyRadio: Bool

    var id: String { name }

    enum CodingKeys: String, CodingKey {
        case name, vid, pid, product, model
        case likelyRadio = "likely_radio"
    }

    /// Decode `model` leniently: a core that grows a radio this build has no
    /// `RadioModel` case for should leave that port unattributed rather than
    /// make the whole port list fail to decode and hide every radio.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        vid = try c.decodeIfPresent(UInt16.self, forKey: .vid)
        pid = try c.decodeIfPresent(UInt16.self, forKey: .pid)
        product = try c.decodeIfPresent(String.self, forKey: .product)
        model = try c.decodeIfPresent(String.self, forKey: .model)
            .flatMap(RadioModel.init(rawValue:))
        likelyRadio = try c.decode(Bool.self, forKey: .likelyRadio)
    }

    /// Memberwise init, for tests. `likelyRadio` is derived so a fixture can't
    /// express a state the core never produces.
    init(name: String, vid: UInt16? = nil, pid: UInt16? = nil,
         product: String? = nil, model: RadioModel? = nil) {
        self.name = name
        self.vid = vid
        self.pid = pid
        self.product = product
        self.model = model
        self.likelyRadio = model != nil
    }
}

/// One decoded channel from `codeplugme_dump_json`. Field names/values match the
/// Rust `Channel` serialization (enum fields arrive capitalized, e.g. "Analog").
struct DumpChannel: Codable, Identifiable, Hashable {
    let index: Int
    var name: String
    var rxFrequencyHz: UInt32
    var txFrequencyHz: UInt32
    var mode: String
    var power: String
    var bandwidth: String
    var colorCode: Int
    var timeSlot: Int
    var contactIndex: UInt32
    var radioIdIndex: Int
    var groupListIndex: Int
    // --- Analog / signaling (defaults keep the memberwise init usable from
    // tests; the real dump always supplies every key). ---
    var rxSignalingMode: String = "None"
    var txSignalingMode: String = "None"
    var rxCtcss: Int = 0
    var txCtcss: Int = 0
    var rxDcs: Int = 0
    var txDcs: Int = 0
    var squelchMode: String = "Carrier"
    var optionalSignaling: String = "Off"
    var admit: String = "Always"
    var scanListIndex: Int = 255
    var dtmfIdIndex: Int = 0
    var twoToneIdIndex: Int = 0
    var fiveToneIdIndex: Int = 0
    var twoToneDecodeIndex: Int = 0
    // --- Flags ---
    var rxOnly: Bool = false
    var talkAround: Bool = false
    var callConfirm: Bool = false
    var workAlone: Bool = false
    var simplexTdma: Bool = false
    var rxAprs: Bool = false

    var id: Int { index }

    enum CodingKeys: String, CodingKey {
        case index, name, mode, power, bandwidth, admit
        case rxFrequencyHz = "rx_frequency_hz"
        case txFrequencyHz = "tx_frequency_hz"
        case colorCode = "color_code"
        case timeSlot = "time_slot"
        case contactIndex = "contact_index"
        case radioIdIndex = "radio_id_index"
        case groupListIndex = "group_list_index"
        case rxSignalingMode = "rx_signaling_mode"
        case txSignalingMode = "tx_signaling_mode"
        case rxCtcss = "rx_ctcss"
        case txCtcss = "tx_ctcss"
        case rxDcs = "rx_dcs"
        case txDcs = "tx_dcs"
        case squelchMode = "squelch_mode"
        case optionalSignaling = "optional_signaling"
        case scanListIndex = "scan_list_index"
        case dtmfIdIndex = "dtmf_id_index"
        case twoToneIdIndex = "two_tone_id_index"
        case fiveToneIdIndex = "five_tone_id_index"
        case twoToneDecodeIndex = "two_tone_decode_index"
        case rxOnly = "rx_only"
        case talkAround = "talk_around"
        case callConfirm = "call_confirm"
        case workAlone = "work_alone"
        case simplexTdma = "simplex_tdma"
        case rxAprs = "rx_aprs"
    }
}

/// One decoded zone from `codeplugme_dump_json`.
struct DumpZone: Codable, Identifiable, Hashable {
    let index: Int
    var name: String
    var channels: [Int]

    var id: Int { index }
}

/// One decoded scan list from `codeplugme_dump_json`. Field names match the Rust
/// `ScanList` serialization.
struct DumpScanList: Codable, Identifiable, Hashable {
    let index: Int
    var name: String
    var members: [Int]
    var priorityChannelSelect: Int
    var priorityChannel1: Int
    var priorityChannel2: Int
    var lookBackA: Int
    var lookBackB: Int
    var dropoutDelay: Int
    var dwellTime: Int
    var revertChannel: Int

    var id: Int { index }

    enum CodingKeys: String, CodingKey {
        case index, name, members
        case priorityChannelSelect = "priority_channel_select"
        case priorityChannel1 = "priority_channel_1"
        case priorityChannel2 = "priority_channel_2"
        case lookBackA = "look_back_a"
        case lookBackB = "look_back_b"
        case dropoutDelay = "dropout_delay"
        case dwellTime = "dwell_time"
        case revertChannel = "revert_channel"
    }
}

/// One decoded DMR contact / talk group from `codeplugme_dump_json`.
struct DumpContact: Codable, Identifiable, Hashable {
    let index: Int
    var name: String
    var number: UInt32
    /// "Private" | "Group" | "All" (talk groups are Group calls).
    var callType: String

    var id: Int { index }

    enum CodingKeys: String, CodingKey {
        case index, name, number
        case callType = "call_type"
    }
}

/// One decoded RX group list from `codeplugme_dump_json`.
struct DumpGroupList: Codable, Identifiable, Hashable {
    let index: Int
    var name: String
    var members: [Int]

    var id: Int { index }
}

/// One decoded radio ID from `codeplugme_dump_json`.
struct DumpRadioID: Codable, Identifiable, Hashable {
    let index: Int
    var name: String
    var number: UInt32

    var id: Int { index }
}

/// The AnyTone payload of `codeplugme_dump_json`, carried under the envelope's
/// `"anytone"` key.
///
/// Every entity family is a non-optional `let` on purpose. A core bug that
/// dropped a whole family would then fail the decode loudly instead of quietly
/// rendering an empty pane, and nesting the payload is what lets this stay
/// strict as more models are added.
struct AnyToneDump: Codable {
    let channels: [DumpChannel]
    let zones: [DumpZone]
    var scanLists: [DumpScanList] = []
    let contacts: [DumpContact]
    let groupLists: [DumpGroupList]
    let radioIds: [DumpRadioID]
    var aprs: DumpAprs?

    enum CodingKeys: String, CodingKey {
        case channels, zones, contacts, aprs
        case scanLists = "scan_lists"
        case groupLists = "group_lists"
        case radioIds = "radio_ids"
    }
}

/// Top-level payload of `codeplugme_dump_json`: the model tag, its capabilities,
/// and exactly one per-model document payload.
struct CodeplugDump: Codable {
    let model: RadioModel
    let displayName: String
    let capabilities: Capabilities
    let anytone: AnyToneDump?
    let qyt: QytDump?

    enum CodingKeys: String, CodingKey {
        case model, capabilities, anytone, qyt
        case displayName = "display_name"
    }

    /// Decode the envelope and require the payload its `model` names to be
    /// present. A dump that says "anytone" but carries no `anytone` object is a
    /// core bug, and failing here surfaces it instead of opening a blank editor.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        model = try c.decode(RadioModel.self, forKey: .model)
        displayName = try c.decodeIfPresent(String.self, forKey: .displayName) ?? model.displayName
        capabilities = try c.decode(Capabilities.self, forKey: .capabilities)
        anytone = try c.decodeIfPresent(AnyToneDump.self, forKey: .anytone)
        qyt = try c.decodeIfPresent(QytDump.self, forKey: .qyt)
        let payloadKey: CodingKeys
        let hasPayload: Bool
        switch model {
        case .anyToneD878UV2:
            (payloadKey, hasPayload) = (.anytone, anytone != nil)
        case .qytKt5000:
            (payloadKey, hasPayload) = (.qyt, qyt != nil)
        }
        guard hasPayload else {
            throw DecodingError.keyNotFound(payloadKey, DecodingError.Context(
                codingPath: c.codingPath,
                debugDescription: "dump names model \(model.rawValue) but carries no payload"))
        }
    }

    /// Memberwise init, so tests can build a synthetic envelope.
    init(model: RadioModel, displayName: String? = nil,
         capabilities: Capabilities, anytone: AnyToneDump?, qyt: QytDump? = nil) {
        self.model = model
        self.displayName = displayName ?? model.displayName
        self.capabilities = capabilities
        self.anytone = anytone
        self.qyt = qyt
    }
}

/// Read-only APRS settings from `codeplugme_dump_json` (the core exposes a focused
/// subset; there is no edit path — this block is never written to the radio).
struct DumpAprs: Codable, Hashable {
    var sourceCall: String
    var sourceSsid: Int
    var destinationCall: String
    var destinationSsid: Int
    var symbolTable: Int
    var symbol: Int
    var manualTxInterval: Int
    var autoTxInterval: Int
    var fmPower: Int
    var fmTxFrequencyHz: UInt32

    enum CodingKeys: String, CodingKey {
        case sourceCall = "source_call"
        case sourceSsid = "source_ssid"
        case destinationCall = "destination_call"
        case destinationSsid = "destination_ssid"
        case symbolTable = "symbol_table"
        case symbol
        case manualTxInterval = "manual_tx_interval"
        case autoTxInterval = "auto_tx_interval"
        case fmPower = "fm_power"
        case fmTxFrequencyHz = "fm_tx_frequency_hz"
    }
}

// MARK: - Edit spec (encoded to `codeplugme_apply_edits`)

/// One channel update or add. `index` is omitted for adds. Nil fields are
/// omitted by the encoder, so only present fields are applied.
struct ChannelEdit: Encodable {
    var index: Int?
    var name: String?
    var rxFrequencyHz: UInt32?
    var txFrequencyHz: UInt32?
    var mode: String?
    var power: String?
    var bandwidth: String?
    var colorCode: Int?
    var timeSlot: Int?
    var contactIndex: UInt32?
    var radioIdIndex: Int?
    var groupListIndex: Int?
    var rxSignalingMode: String?
    var txSignalingMode: String?
    var rxCtcss: Int?
    var txCtcss: Int?
    var rxDcs: Int?
    var txDcs: Int?
    var squelchMode: String?
    var optionalSignaling: String?
    var admit: String?
    var scanListIndex: Int?
    var dtmfIdIndex: Int?
    var twoToneIdIndex: Int?
    var fiveToneIdIndex: Int?
    var twoToneDecodeIndex: Int?
    var rxOnly: Bool?
    var talkAround: Bool?
    var callConfirm: Bool?
    var workAlone: Bool?
    var simplexTdma: Bool?
    var rxAprs: Bool?

    enum CodingKeys: String, CodingKey {
        case index, name, mode, power, bandwidth, admit
        case rxFrequencyHz = "rx_frequency_hz"
        case txFrequencyHz = "tx_frequency_hz"
        case colorCode = "color_code"
        case timeSlot = "time_slot"
        case contactIndex = "contact_index"
        case radioIdIndex = "radio_id_index"
        case groupListIndex = "group_list_index"
        case rxSignalingMode = "rx_signaling_mode"
        case txSignalingMode = "tx_signaling_mode"
        case rxCtcss = "rx_ctcss"
        case txCtcss = "tx_ctcss"
        case rxDcs = "rx_dcs"
        case txDcs = "tx_dcs"
        case squelchMode = "squelch_mode"
        case optionalSignaling = "optional_signaling"
        case scanListIndex = "scan_list_index"
        case dtmfIdIndex = "dtmf_id_index"
        case twoToneIdIndex = "two_tone_id_index"
        case fiveToneIdIndex = "five_tone_id_index"
        case twoToneDecodeIndex = "two_tone_decode_index"
        case rxOnly = "rx_only"
        case talkAround = "talk_around"
        case callConfirm = "call_confirm"
        case workAlone = "work_alone"
        case simplexTdma = "simplex_tdma"
        case rxAprs = "rx_aprs"
    }
}

/// One zone update or add (`index` omitted for adds).
struct ZoneEdit: Encodable {
    var index: Int?
    var name: String?
    var members: [Int]?
}

/// One scan-list update or add (`index` omitted for adds). Nil fields are
/// omitted by the encoder, so only present fields are applied.
struct ScanListEdit: Encodable {
    var index: Int?
    var name: String?
    var members: [Int]?
    var priorityChannelSelect: Int?
    var priorityChannel1: Int?
    var priorityChannel2: Int?
    var lookBackA: Int?
    var lookBackB: Int?
    var dropoutDelay: Int?
    var dwellTime: Int?
    var revertChannel: Int?

    enum CodingKeys: String, CodingKey {
        case index, name, members
        case priorityChannelSelect = "priority_channel_select"
        case priorityChannel1 = "priority_channel_1"
        case priorityChannel2 = "priority_channel_2"
        case lookBackA = "look_back_a"
        case lookBackB = "look_back_b"
        case dropoutDelay = "dropout_delay"
        case dwellTime = "dwell_time"
        case revertChannel = "revert_channel"
    }
}

/// One contact update or add (`index` omitted for adds).
struct ContactEdit: Encodable {
    var index: Int?
    var name: String?
    var number: UInt32?
    var callType: String?

    enum CodingKeys: String, CodingKey {
        case index, name, number
        case callType = "call_type"
    }
}

/// One group-list update or add (`index` omitted for adds).
struct GroupListEdit: Encodable {
    var index: Int?
    var name: String?
    var members: [Int]?
}

/// One radio-ID update or add (`index` omitted for adds).
struct RadioIDEdit: Encodable {
    var index: Int?
    var name: String?
    var number: UInt32?
}

/// Relocate the record at `from` to the free slot `to` (backs the editable "#"
/// column). Encoded into the `move_*` arrays of `EditSpec`.
struct MoveOp: Encodable {
    let from: Int
    let to: Int
}

/// Full batch passed to `codeplugme_apply_edits`. Empty arrays are fine; the core
/// applies updates, then removals, then additions.
struct EditSpec: Encodable {
    var channels: [ChannelEdit] = []
    var addChannels: [ChannelEdit] = []
    var removeChannels: [Int] = []
    var moveChannels: [MoveOp] = []
    var zones: [ZoneEdit] = []
    var addZones: [ZoneEdit] = []
    var removeZones: [Int] = []
    var moveZones: [MoveOp] = []
    var scanLists: [ScanListEdit] = []
    var addScanLists: [ScanListEdit] = []
    var removeScanLists: [Int] = []
    var moveScanLists: [MoveOp] = []
    var contacts: [ContactEdit] = []
    var addContacts: [ContactEdit] = []
    var removeContacts: [Int] = []
    var moveContacts: [MoveOp] = []
    var groupLists: [GroupListEdit] = []
    var addGroupLists: [GroupListEdit] = []
    var removeGroupLists: [Int] = []
    var moveGroupLists: [MoveOp] = []
    var radioIds: [RadioIDEdit] = []
    var addRadioIds: [RadioIDEdit] = []
    var removeRadioIds: [Int] = []
    var moveRadioIds: [MoveOp] = []

    /// True when there is nothing to apply.
    var isEmpty: Bool {
        channels.isEmpty && addChannels.isEmpty && removeChannels.isEmpty && moveChannels.isEmpty
            && zones.isEmpty && addZones.isEmpty && removeZones.isEmpty && moveZones.isEmpty
            && scanLists.isEmpty && addScanLists.isEmpty && removeScanLists.isEmpty && moveScanLists.isEmpty
            && contacts.isEmpty && addContacts.isEmpty && removeContacts.isEmpty && moveContacts.isEmpty
            && groupLists.isEmpty && addGroupLists.isEmpty && removeGroupLists.isEmpty
            && moveGroupLists.isEmpty
            && radioIds.isEmpty && addRadioIds.isEmpty && removeRadioIds.isEmpty && moveRadioIds.isEmpty
    }

    enum CodingKeys: String, CodingKey {
        case channels, zones, contacts
        case addChannels = "add_channels"
        case removeChannels = "remove_channels"
        case moveChannels = "move_channels"
        case addZones = "add_zones"
        case removeZones = "remove_zones"
        case moveZones = "move_zones"
        case scanLists = "scan_lists"
        case addScanLists = "add_scan_lists"
        case removeScanLists = "remove_scan_lists"
        case moveScanLists = "move_scan_lists"
        case addContacts = "add_contacts"
        case removeContacts = "remove_contacts"
        case moveContacts = "move_contacts"
        case groupLists = "group_lists"
        case addGroupLists = "add_group_lists"
        case removeGroupLists = "remove_group_lists"
        case moveGroupLists = "move_group_lists"
        case radioIds = "radio_ids"
        case addRadioIds = "add_radio_ids"
        case removeRadioIds = "remove_radio_ids"
        case moveRadioIds = "move_radio_ids"
    }
}

// MARK: - Display / value helpers

/// Picker options for channel mode. `tag` is the capitalized value as it appears
/// in a dump (so it binds directly to `DumpChannel.mode`); `label` is friendly.
let channelModeOptions: [(tag: String, label: String)] = [
    ("Analog", "Analog"), ("Digital", "Digital"),
    ("MixedAnalog", "Mixed (A)"), ("MixedDigital", "Mixed (D)"),
]

/// Whether a channel mode carries a digital (DMR) component. Only pure "Analog"
/// has none, so its color code / time slot / contact / group list / radio ID
/// fields are meaningless and get disabled in the editor. The two mixed modes
/// still carry digital, so those fields stay live for them.
func modeHasDigital(_ mode: String) -> Bool {
    mode != "Analog"
}

/// Whether a channel mode carries analog signaling (CTCSS / DCS). Only pure
/// "Digital" has none.
func modeHasAnalog(_ mode: String) -> Bool {
    mode != "Digital"
}

/// Whether a channel passes the Channels-pane filters. Kept out of the view so
/// the mode / color-code / slot matching can be tested without SwiftUI. A nil
/// component means "no constraint on that field".
func channelMatchesFilter(_ channel: DumpChannel, query: String,
                          mode: String?, colorCode: Int?, slot: Int?) -> Bool {
    (query.isEmpty || channel.name.localizedCaseInsensitiveContains(query))
        && (mode == nil || channel.mode == mode)
        && (colorCode == nil || channel.colorCode == colorCode)
        && (slot == nil || channel.timeSlot == slot)
}

/// What makes two channels "the same channel" for duplicate highlighting.
/// The mode is part of it, so channels of different types (analog, digital,
/// or either mixed mode) on the same pair never match each other. Color code
/// and time slot are nil for an analog channel. Mode is nil for a radio with
/// only one channel type. The tones are nil when the channel has no tone, or
/// no analog side for one to apply to. There is no offset field: both radios
/// derive TX from RX plus the offset, so matching TX already matches it.
struct ChannelFrequencyKey: Hashable {
    var rxFrequencyHz: UInt32
    var txFrequencyHz: UInt32?
    var mode: String?
    var rxTone: String? = nil
    var txTone: String? = nil
    var colorCode: Int?
    var timeSlot: Int?
}

/// Map each slot that shares its key with at least one other record to the
/// other slots in its group (ascending). Slots with no twin are left out.
func duplicateSlots<T>(_ items: [T], slot: (T) -> Int,
                       key: (T) -> ChannelFrequencyKey) -> [Int: [Int]] {
    let groups = Dictionary(grouping: items.map { (slot($0), key($0)) }, by: \.1)
    var out: [Int: [Int]] = [:]
    for members in groups.values where members.count > 1 {
        let slots = members.map(\.0).sorted()
        for s in slots { out[s] = slots.filter { $0 != s } }
    }
    return out
}

/// AnyTone duplicates: same RX, TX and mode, for any mode with an analog side
/// also the same RX / TX tones, and for any mode with a DMR component also the
/// same color code and time slot. Names are ignored on purpose.
func duplicateChannelSlots(_ channels: [DumpChannel]) -> [Int: [Int]] {
    duplicateSlots(channels, slot: \.index) { c in
        let digital = modeHasDigital(c.mode)
        let analog = modeHasAnalog(c.mode)
        return ChannelFrequencyKey(rxFrequencyHz: c.rxFrequencyHz, txFrequencyHz: c.txFrequencyHz,
                                   mode: c.mode,
                                   rxTone: analog ? anytoneToneKey(c.rxSignalingMode, ctcss: c.rxCtcss, dcs: c.rxDcs) : nil,
                                   txTone: analog ? anytoneToneKey(c.txSignalingMode, ctcss: c.txCtcss, dcs: c.txDcs) : nil,
                                   colorCode: digital ? c.colorCode : nil,
                                   timeSlot: digital ? c.timeSlot : nil)
    }
}

/// One side's tone as a duplicate-key value: nil for no tone, otherwise the
/// signaling type with the one value that type uses. The CTCSS index is
/// ignored under DCS and the reverse, since the radio ignores it too.
func anytoneToneKey(_ signalingMode: String, ctcss: Int, dcs: Int) -> String? {
    switch signalingMode {
    case "Ctcss": return "ctcss \(ctcss)"
    case "Dcs": return "dcs \(dcs)"
    default: return nil
    }
}

/// Tooltip / caption for a duplicated row: "Duplicate of channel 0012, 0040".
func duplicateLabel(_ others: [Int]) -> String {
    "Duplicate of channel " + others.map(formatSlot).joined(separator: ", ")
}

/// Table sort for the duplicates-only view: RX, then TX, then slot, so each
/// group of twins sits together in slot order.
let channelFrequencySortOrder = [
    KeyPathComparator(\DumpChannel.rxFrequencyHz),
    KeyPathComparator(\DumpChannel.txFrequencyHz),
    KeyPathComparator(\DumpChannel.index),
]

/// Picker options for transmit power (dump-valued tags).
let channelPowerOptions: [(tag: String, label: String)] = [
    ("Low", "Low"), ("Mid", "Mid"), ("High", "High"), ("Turbo", "Turbo"),
]

/// Picker options for channel bandwidth (dump-valued tags).
let channelBandwidthOptions: [(tag: String, label: String)] = [
    ("Narrow", "Narrow"), ("Wide", "Wide"),
]

/// Picker options for contact call type (dump-valued tags).
let callTypeOptions: [(tag: String, label: String)] = [
    ("Group", "Group (talk group)"), ("Private", "Private"), ("All", "All"),
]

/// Map a capitalized enum string from a dump ("Analog", "Turbo", "Group") to the
/// lowercase snake_case value the edit API expects ("analog", "turbo", "group").
func editValue(_ dumped: String) -> String {
    switch dumped {
    case "MixedAnalog": return "mixed_analog"
    case "MixedDigital": return "mixed_digital"
    default: return dumped.lowercased()
    }
}

/// Convert a CamelCase dump enum value ("ChannelFree", "TwoTone", "Dcs") to the
/// snake_case string the edit API parses ("channel_free", "two_tone", "dcs").
/// An underscore is inserted before each interior capital, then lowercased.
func snakeEditValue(_ dumped: String) -> String {
    var out = ""
    for (i, ch) in dumped.enumerated() {
        if i > 0 && ch.isUppercase { out.append("_") }
        out.append(Character(ch.lowercased()))
    }
    return out
}

/// Picker options for a channel's signaling type (RX/TX), dump-valued tags.
let signalingModeOptions: [(tag: String, label: String)] = [
    ("None", "None"), ("Ctcss", "CTCSS"), ("Dcs", "DCS"),
]

/// Picker options for squelch mode (dump-valued tags).
let squelchModeOptions: [(tag: String, label: String)] = [
    ("Carrier", "Carrier"), ("Tone", "Tone (CTCSS/DCS)"),
]

/// Picker options for the TX-permit / admit criterion (dump-valued tags).
let admitOptions: [(tag: String, label: String)] = [
    ("Always", "Always"), ("ChannelFree", "Channel Free"),
    ("DifferentColorCode", "Different CC"), ("SameColorCode", "Same CC"),
]

/// Picker options for optional signaling (dump-valued tags).
let optionalSignalingOptions: [(tag: String, label: String)] = [
    ("Off", "Off"), ("Dtmf", "DTMF"), ("TwoTone", "2-Tone"), ("FiveTone", "5-Tone"),
]

/// Picker options for a scan list's priority-channel-select (raw index values).
let priorityChannelSelectOptions: [(tag: Int, label: String)] = [
    (0, "Off"), (1, "Primary"), (2, "Secondary"), (3, "Both"),
]

/// Sentinel for an unset channel reference (priority channel, etc.) — matches the
/// core's `0xffff`.
let channelRefNone = 65535

/// Shared MHz display/edit format: fixed 5 decimals (the codeplug's 10 Hz BCD
/// resolution), no grouping, dot decimal separator. Pinned to POSIX because
/// these are hardware values rather than prose numbers, and because the table
/// cell and the editor field must round-trip identically.
let mhzFormat = FloatingPointFormatStyle<Double>.number
    .precision(.fractionLength(5))
    .grouping(.never)
    .locale(Locale(identifier: "en_US_POSIX"))

/// Format a frequency in Hz as a MHz string.
func formatMHz(_ hz: UInt32) -> String {
    (Double(hz) / 1_000_000).formatted(mhzFormat)
}

/// Format a DMR ID as plain digits. DMR IDs are identifiers, never grouped:
/// `Text("\(id)")` would route a UInt32 through LocalizedStringKey and render
/// 3113043 as "3,113,043".
func formatDMRID(_ id: UInt32) -> String {
    String(id)
}

/// Format a 0-based slot as its 1-based display number, ungrouped (slot counts
/// reach the thousands, so interpolation would introduce a separator).
func formatSlot(_ index: Int) -> String {
    String(index + 1)
}

// MARK: - Layout

/// The app's spacing scale. Every gap between views comes from here so the
/// rhythm stays consistent instead of drifting into ad-hoc 4/6/8/10/12 values.
enum Spacing {
    /// Between tightly related items (a label and the control it names).
    static let tight: CGFloat = 4
    /// Between controls within a row or group.
    static let inline: CGFloat = 8
    /// Between distinct rows or stacked groups.
    static let stack: CGFloat = 12
    /// Between major sections, and around window content.
    static let section: CGFloat = 20
}
