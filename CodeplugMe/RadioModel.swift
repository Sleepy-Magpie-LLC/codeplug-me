import Foundation

/// A radio model, matching the `id` strings the Rust core reports
/// (`core/src/model.rs`). The raw values are a stable wire contract: saved
/// documents and preferences hold them, so never rename one.
enum RadioModel: String, Codable, CaseIterable, Hashable {
    case anyToneD878UV2 = "anytone-d878uv2"
    case qytKt5000 = "qyt-kt5000"

    /// Fallback display name, used only if a payload arrives without the
    /// `display_name` the core normally supplies.
    var displayName: String {
        switch self {
        case .anyToneD878UV2: return "AnyTone AT-D878UVII Plus"
        case .qytKt5000: return "QYT KT-5000"
        }
    }
}

/// One entry of `codeplugme_models_json`: a radio this build can program.
struct SupportedModel: Codable, Equatable {
    let id: RadioModel
    let displayName: String
    let imageSize: Int
    let capabilities: Capabilities

    enum CodingKeys: String, CodingKey {
        case id, capabilities
        case displayName = "display_name"
        case imageSize = "image_size"
    }
}

/// What `codeplugme_identify_json` reports about a connected radio.
struct RadioIdentity: Codable, Hashable {
    let model: RadioModel
    let identity: String
}

/// One `(tag, label)` choice for a picker. `tag` is the value the core emits in
/// a dump and accepts in an edit; `label` is what the user reads.
struct PickerOption: Codable, Hashable {
    let tag: String
    let label: String
}

/// What a radio model can hold, as reported by the core. The app never keeps a
/// parallel table of this: Rust is the single source of truth, so adding a
/// model needs no Swift-side capability edit.
struct Capabilities: Codable, Equatable {
    /// Document sections this model has, in sidebar order.
    let sections: [CodeplugSection]
    let channelCapacity: Int
    let channelNameLength: Int
    let supportsDMR: Bool
    /// How this model encodes CTCSS/DCS: a table index, or a frequency.
    let toneEncoding: String
    let powerOptions: [PickerOption]
    let bandwidthOptions: [PickerOption]

    enum CodingKeys: String, CodingKey {
        case sections
        case channelCapacity = "channel_capacity"
        case channelNameLength = "channel_name_length"
        case supportsDMR = "supports_dmr"
        case toneEncoding = "tone_encoding"
        case powerOptions = "power_options"
        case bandwidthOptions = "bandwidth_options"
    }

    /// Decode leniently in exactly one place: `sections` arrives as raw strings
    /// and unknown ones are dropped, so a core that grows a section this build
    /// has no pane for degrades to hiding it rather than failing the whole
    /// document. Every other field stays strict.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        sections = try c.decode([String].self, forKey: .sections)
            .compactMap(CodeplugSection.init(rawValue:))
        channelCapacity = try c.decode(Int.self, forKey: .channelCapacity)
        channelNameLength = try c.decode(Int.self, forKey: .channelNameLength)
        supportsDMR = try c.decode(Bool.self, forKey: .supportsDMR)
        toneEncoding = try c.decode(String.self, forKey: .toneEncoding)
        powerOptions = try c.decode([PickerOption].self, forKey: .powerOptions)
        bandwidthOptions = try c.decode([PickerOption].self, forKey: .bandwidthOptions)
    }

    /// Memberwise init, for the placeholder below and for tests.
    init(sections: [CodeplugSection], channelCapacity: Int, channelNameLength: Int,
         supportsDMR: Bool, toneEncoding: String,
         powerOptions: [PickerOption], bandwidthOptions: [PickerOption]) {
        self.sections = sections
        self.channelCapacity = channelCapacity
        self.channelNameLength = channelNameLength
        self.supportsDMR = supportsDMR
        self.toneEncoding = toneEncoding
        self.powerOptions = powerOptions
        self.bandwidthOptions = bandwidthOptions
    }

    /// What the sidebar shows before any document is open. Using the AnyTone
    /// shape rather than an empty one keeps the sidebar from collapsing and then
    /// jumping back when a file opens.
    static let anyTonePlaceholder = Capabilities(
        sections: CodeplugSection.anyToneSections,
        channelCapacity: 4000,
        channelNameLength: 16,
        supportsDMR: true,
        toneEncoding: "table_index",
        powerOptions: channelPowerOptions.map { PickerOption(tag: $0.tag, label: $0.label) },
        bandwidthOptions: channelBandwidthOptions.map { PickerOption(tag: $0.tag, label: $0.label) }
    )
}
