import Foundation

/// Wire types for the repeater-database import (`core/src/geo`), plus the view
/// model behind `RepeaterImportSheet`. The selection rules live in the view
/// model rather than the view so they can be unit-tested without SwiftUI.

// MARK: - Wire types

/// Which database a repeater came from.
enum RepeaterSource: String, Codable, CaseIterable, Hashable {
    case hearham, radioid, repeaterbook

    var label: String {
        switch self {
        case .hearham: return "Hearham"
        case .radioid: return "RadioID"
        case .repeaterbook: return "RepeaterBook"
        }
    }
}

/// How a repeater is worked.
enum RepeaterMode: String, Codable, Hashable {
    case analog, dmr

    var label: String { self == .analog ? "Analog" : "DMR" }
}

/// One repeater as the core reports it. Frequencies are from the radio's side:
/// `rxHz` is the repeater's output, `txHz` its input. Every field round-trips,
/// because the picked rows are sent back to `codeplugme_geo_build` unchanged.
struct Repeater: Codable, Hashable {
    var source: RepeaterSource
    var callsign: String
    var location: String
    var rxHz: UInt32
    var txHz: UInt32
    var mode: RepeaterMode
    var txTone: Tone
    var rxTone: Tone
    var colorCode: Int?
    var lat: Double?
    var lon: Double?
    var distanceKm: Double?
    var duplicate: Bool

    enum CodingKeys: String, CodingKey {
        case source, callsign, location, mode, lat, lon, duplicate
        case rxHz = "rx_hz"
        case txHz = "tx_hz"
        case txTone = "tx_tone"
        case rxTone = "rx_tone"
        case colorCode = "color_code"
        case distanceKm = "distance_km"
    }

    /// The access tone for an analog repeater, or the color code for a DMR one.
    var toneLabel: String {
        if mode == .dmr { return colorCode.map { "CC \($0)" } ?? "CC ?" }
        return txTone.label
    }
}

/// RepeaterBook credentials from Settings.
struct RepeaterBookAuth: Codable, Hashable {
    var token: String
    var contact: String
}

/// What `codeplugme_geo_fetch_json` should look for. `country` and `state` are
/// full names ("United States", "Colorado"), which is how RadioID and
/// RepeaterBook are searched.
struct GeoQuery: Codable, Hashable {
    var lat: Double
    var lon: Double
    var radiusKm: Double
    var country: String?
    var state: String?
    var sources: [RepeaterSource]
    var repeaterbook: RepeaterBookAuth?

    enum CodingKeys: String, CodingKey {
        case lat, lon, country, state, sources, repeaterbook
        case radiusKm = "radius_km"
    }
}

/// Whether a build keeps the base codeplug's channels.
enum BuildMode: String, Codable, CaseIterable, Identifiable, Hashable {
    /// Keep settings and contacts but replace every channel, zone and scan list.
    case clean
    /// Add into the free slots of the existing codeplug.
    case merge

    var id: String { rawValue }
    var label: String { self == .clean ? "Clean" : "Merge" }
}

/// How many repeaters fit, from `codeplugme_geo_capacity_json`.
struct GeoCapacity: Codable, Hashable {
    var available: Int
    var maxSelectable: Int
    var zoneLimit: Int?
    var scanListLimit: Int?
    var blockedReason: String?

    enum CodingKeys: String, CodingKey {
        case available
        case maxSelectable = "max_selectable"
        case zoneLimit = "zone_limit"
        case scanListLimit = "scan_list_limit"
        case blockedReason = "blocked_reason"
    }
}

/// Options for `codeplugme_geo_build`.
struct GeoBuildOptions: Codable, Hashable {
    var mode: BuildMode
    var zoneName: String

    enum CodingKeys: String, CodingKey {
        case mode
        case zoneName = "zone_name"
    }
}

// MARK: - Filters

/// Which amateur band to show.
enum BandFilter: String, CaseIterable, Identifiable {
    case all, twoMeters, seventyCm

    var id: String { rawValue }

    var label: String {
        switch self {
        case .all: return "All Bands"
        case .twoMeters: return "2 m"
        case .seventyCm: return "70 cm"
        }
    }

    /// Whether a receive frequency falls in this band.
    func contains(_ hz: UInt32) -> Bool {
        switch self {
        case .all: return true
        case .twoMeters: return (144_000_000...148_000_000).contains(hz)
        case .seventyCm: return (420_000_000...450_000_000).contains(hz)
        }
    }
}

/// Which repeater mode to show.
enum ModeFilter: String, CaseIterable, Identifiable {
    case all, analog, dmr

    var id: String { rawValue }

    var label: String {
        switch self {
        case .all: return "All Modes"
        case .analog: return "Analog"
        case .dmr: return "DMR"
        }
    }

    /// Whether a repeater of `mode` passes this filter.
    func contains(_ mode: RepeaterMode) -> Bool {
        switch self {
        case .all: return true
        case .analog: return mode == .analog
        case .dmr: return mode == .dmr
        }
    }
}

// MARK: - View model

/// One fetched repeater with a stable id. Records carry no id of their own and
/// two can legitimately share a callsign, so rows are keyed by fetch position.
struct RepeaterRow: Identifiable, Hashable {
    let id: Int
    let repeater: Repeater
}

/// How the search center is given.
enum LocationInput: String, CaseIterable, Identifiable {
    case place, coordinates
    var id: String { rawValue }
    var label: String { self == .place ? "Place or ZIP" : "Coordinates" }
}

/// Everything the import sheet shows: the search form, the fetched rows, which
/// are checked, and whether that set fits the codeplug.
///
/// The store owns one for the whole app session, so closing the sheet and
/// opening it again brings back the same search and results. Results belong to
/// the codeplug they were fetched against (`resultsFile`): their duplicate
/// flags are only true for that file, so they are dropped when another opens.
///
/// Rows start unchecked. In merge mode a row the codeplug already has
/// (`duplicate`) can't be checked, because the core rejects it; in clean mode
/// the base channels are going away, so it can.
@MainActor
final class RepeaterImportModel: ObservableObject {
    // Search form.
    @Published var locationInput: LocationInput = .place
    @Published var place = ""
    @Published var latText = ""
    @Published var lonText = ""
    @Published var radiusMiles = 25.0
    @Published var useHearham = true
    @Published var useRadioID = true
    @Published var useRepeaterBook = false
    @Published var zoneName = "Nearby"

    /// The codeplug the current rows were fetched against.
    @Published private(set) var resultsFile: URL?
    @Published private(set) var hasFetched = false
    @Published private(set) var rows: [RepeaterRow] = []
    @Published private(set) var selection: Set<Int> = []
    @Published var capacity: GeoCapacity?
    @Published var bandFilter: BandFilter = .all
    @Published var modeFilter: ModeFilter = .all

    /// Switching to merge drops any checked duplicates, since they no longer
    /// qualify. The caller re-fetches `capacity` for the new mode.
    @Published var mode: BuildMode = .merge {
        didSet {
            if mode == .merge {
                selection = selection.filter { id in rows.first { $0.id == id }.map(isSelectable) ?? false }
            }
        }
    }

    /// Replace the results with a fresh fetch for `file`, clearing the
    /// selection.
    func setResults(_ repeaters: [Repeater], for file: URL? = nil) {
        rows = repeaters.enumerated().map { RepeaterRow(id: $0.offset, repeater: $0.element) }
        selection = []
        resultsFile = file
        hasFetched = true
    }

    /// Drop results fetched against a codeplug other than `file`. The search
    /// form is kept.
    func discardResults(unlessFor file: URL?) {
        guard resultsFile != file else { return }
        rows = []
        selection = []
        resultsFile = nil
        hasFetched = false
    }

    /// After the checked rows were added to the open codeplug, they are in it
    /// now: flag them as duplicates and uncheck them, so they can't be added
    /// twice. A clean build also removed every channel that was there before,
    /// so then only the rows just added count as duplicates.
    func markSelectionAdded() {
        let replacedAll = mode == .clean
        rows = rows.map { row in
            var r = row.repeater
            let added = selection.contains(row.id)
            r.duplicate = replacedAll ? added : (r.duplicate || added)
            return RepeaterRow(id: row.id, repeater: r)
        }
        selection = []
    }

    /// The rows the band and mode filters let through, in fetch (nearest first)
    /// order.
    var visibleRows: [RepeaterRow] {
        rows.filter { bandFilter.contains($0.repeater.rxHz) && modeFilter.contains($0.repeater.mode) }
    }

    /// Whether `row` may be checked in the current mode.
    func isSelectable(_ row: RepeaterRow) -> Bool {
        mode == .clean || !row.repeater.duplicate
    }

    /// Whether `row` is checked.
    func isSelected(_ row: RepeaterRow) -> Bool {
        selection.contains(row.id)
    }

    /// Check or uncheck `row`. Checking an unselectable row is ignored.
    func setSelected(_ row: RepeaterRow, _ selected: Bool) {
        if selected {
            guard isSelectable(row) else { return }
            selection.insert(row.id)
        } else {
            selection.remove(row.id)
        }
    }

    /// Check every visible row that may be checked. Rows hidden by a filter
    /// keep whatever state they had.
    func selectAll() {
        selection.formUnion(visibleRows.filter(isSelectable).map(\.id))
    }

    /// Uncheck everything, hidden rows included.
    func selectNone() {
        selection = []
    }

    var selectedCount: Int { selection.count }
    var maxSelectable: Int { capacity?.maxSelectable ?? 0 }
    var isOverLimit: Bool { selectedCount > maxSelectable }

    /// How many to uncheck to fit, zero when the selection already fits.
    var excess: Int { max(0, selectedCount - maxSelectable) }

    /// Build needs a known capacity, at least one pick, a selection that fits,
    /// and no reason from the core that the codeplug can't take any.
    var canBuild: Bool {
        guard let capacity, capacity.blockedReason == nil else { return false }
        return selectedCount > 0 && !isOverLimit
    }

    /// The checked repeaters, nearest first, exactly as fetched.
    var selectedRepeaters: [Repeater] {
        rows.filter { selection.contains($0.id) }.map(\.repeater)
    }

    /// Whether any result came from RepeaterBook, whose terms require credit.
    var hasRepeaterBookResults: Bool {
        rows.contains { $0.repeater.source == .repeaterbook }
    }
}

// MARK: - US states

/// Full US state names keyed by the postal abbreviation CLGeocoder reports as
/// `administrativeArea`. RadioID and RepeaterBook match on the full name.
enum USStates {
    static let names: [String: String] = [
        "AL": "Alabama", "AK": "Alaska", "AZ": "Arizona", "AR": "Arkansas",
        "CA": "California", "CO": "Colorado", "CT": "Connecticut", "DE": "Delaware",
        "FL": "Florida", "GA": "Georgia", "HI": "Hawaii", "ID": "Idaho",
        "IL": "Illinois", "IN": "Indiana", "IA": "Iowa", "KS": "Kansas",
        "KY": "Kentucky", "LA": "Louisiana", "ME": "Maine", "MD": "Maryland",
        "MA": "Massachusetts", "MI": "Michigan", "MN": "Minnesota", "MS": "Mississippi",
        "MO": "Missouri", "MT": "Montana", "NE": "Nebraska", "NV": "Nevada",
        "NH": "New Hampshire", "NJ": "New Jersey", "NM": "New Mexico", "NY": "New York",
        "NC": "North Carolina", "ND": "North Dakota", "OH": "Ohio", "OK": "Oklahoma",
        "OR": "Oregon", "PA": "Pennsylvania", "RI": "Rhode Island", "SC": "South Carolina",
        "SD": "South Dakota", "TN": "Tennessee", "TX": "Texas", "UT": "Utah",
        "VT": "Vermont", "VA": "Virginia", "WA": "Washington", "WV": "West Virginia",
        "WI": "Wisconsin", "WY": "Wyoming", "DC": "District of Columbia", "PR": "Puerto Rico",
    ]

    /// The full name for `area` when it is a US abbreviation, else `area`
    /// unchanged (other countries' geocoders already report full names).
    static func fullName(_ area: String?) -> String? {
        guard let area else { return nil }
        return names[area.uppercased()] ?? area
    }
}
