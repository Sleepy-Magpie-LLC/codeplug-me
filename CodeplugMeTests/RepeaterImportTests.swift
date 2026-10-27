import XCTest
@testable import CodeplugMe

/// Wire shapes for the repeater import (`core/src/geo`). No FFI and no files.
final class RepeaterWireTests: XCTestCase {

    /// One record per tone kind, with and without coordinates, as the core
    /// returns them from `codeplugme_geo_fetch_json`.
    private let fetchJSON = """
    [
      {"source": "hearham", "callsign": "W0ABC", "location": "Denver",
       "rx_hz": 146940000, "tx_hz": 146340000, "mode": "analog",
       "tx_tone": {"kind": "ctcss", "hz_tenths": 1000}, "rx_tone": {"kind": "none"},
       "color_code": null, "lat": 39.7, "lon": -104.9, "distance_km": 3.2,
       "duplicate": false},
      {"source": "radioid", "callsign": "K0DMR", "location": "Boulder",
       "rx_hz": 446500000, "tx_hz": 441500000, "mode": "dmr",
       "tx_tone": {"kind": "none"}, "rx_tone": {"kind": "none"},
       "color_code": 1, "lat": null, "lon": null, "distance_km": null,
       "duplicate": true},
      {"source": "repeaterbook", "callsign": "N0DCS", "location": "Golden",
       "rx_hz": 447000000, "tx_hz": 442000000, "mode": "analog",
       "tx_tone": {"kind": "dcs", "code": 23, "inverted": true},
       "rx_tone": {"kind": "dcs", "code": 23, "inverted": true},
       "color_code": null, "lat": 39.75, "lon": -105.2, "distance_km": 20.0,
       "duplicate": false}
    ]
    """

    /// Every field survives decode, encode, decode, since the picks go back to
    /// the core unchanged.
    func testRepeaterJSONRoundTrips() throws {
        let decoded = try JSONDecoder().decode([Repeater].self, from: Data(fetchJSON.utf8))
        XCTAssertEqual(decoded.count, 3)
        XCTAssertEqual(decoded[0].txTone, .ctcss(hzTenths: 1000))
        XCTAssertEqual(decoded[1].colorCode, 1)
        XCTAssertTrue(decoded[1].duplicate)
        XCTAssertNil(decoded[1].lat)
        XCTAssertEqual(decoded[2].rxTone, .dcs(code: 23, inverted: true))

        let again = try JSONDecoder().decode([Repeater].self, from: JSONEncoder().encode(decoded))
        XCTAssertEqual(again, decoded)
    }

    /// The encoded repeater uses the core's snake_case keys.
    func testRepeaterEncodesSnakeCaseKeys() throws {
        let decoded = try JSONDecoder().decode([Repeater].self, from: Data(fetchJSON.utf8))
        let object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(decoded[0])) as? [String: Any]
        for key in ["rx_hz", "tx_hz", "tx_tone", "rx_tone", "distance_km", "duplicate"] {
            XCTAssertNotNil(object?[key], "missing \(key)")
        }
    }

    /// The query's keys are snake_case and an absent RepeaterBook login is left
    /// out rather than sent as an empty object.
    func testQueryEncoding() throws {
        let query = GeoQuery(lat: 39.7, lon: -104.9, radiusKm: 40, country: "United States",
                             state: "Colorado", sources: [.hearham, .radioid], repeaterbook: nil)
        let object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(query)) as? [String: Any]
        XCTAssertEqual(object?["radius_km"] as? Double, 40)
        XCTAssertEqual(object?["sources"] as? [String], ["hearham", "radioid"])
        XCTAssertEqual(object?["state"] as? String, "Colorado")
        XCTAssertNil(object?["radiusKm"])
        XCTAssertTrue(object?["repeaterbook"] == nil || object?["repeaterbook"] is NSNull)

        let withAuth = GeoQuery(lat: 0, lon: 0, radiusKm: 10, country: nil, state: nil,
                                sources: [.repeaterbook],
                                repeaterbook: RepeaterBookAuth(token: "t", contact: "c"))
        let authObject = try JSONSerialization.jsonObject(with: JSONEncoder().encode(withAuth)) as? [String: Any]
        let auth = authObject?["repeaterbook"] as? [String: String]
        XCTAssertEqual(auth, ["token": "t", "contact": "c"])
    }

    /// Capacity decodes with and without its optional fields.
    func testCapacityDecodes() throws {
        let json = #"{"available":12,"max_selectable":10,"zone_limit":250,"scan_list_limit":50,"blocked_reason":null}"#
        let cap = try JSONDecoder().decode(GeoCapacity.self, from: Data(json.utf8))
        XCTAssertEqual(cap, GeoCapacity(available: 12, maxSelectable: 10, zoneLimit: 250,
                                        scanListLimit: 50, blockedReason: nil))
    }

    /// US abbreviations expand; anything else passes through.
    func testStateNames() {
        XCTAssertEqual(USStates.fullName("CO"), "Colorado")
        XCTAssertEqual(USStates.fullName("DC"), "District of Columbia")
        XCTAssertEqual(USStates.fullName("Ontario"), "Ontario")
        XCTAssertNil(USStates.fullName(nil))
    }
}

/// Selection rules of the import sheet's view model.
@MainActor
final class RepeaterImportModelTests: XCTestCase {

    /// A minimal repeater for selection tests.
    private func repeater(_ call: String, rxHz: UInt32 = 146_940_000, mode: RepeaterMode = .analog,
                          duplicate: Bool = false) -> Repeater {
        Repeater(source: .hearham, callsign: call, location: "", rxHz: rxHz, txHz: rxHz,
                 mode: mode, txTone: .off, rxTone: .off, colorCode: nil, lat: nil, lon: nil,
                 distanceKm: nil, duplicate: duplicate)
    }

    /// A model holding `repeaters` with room for `max` picks.
    private func makeModel(_ repeaters: [Repeater], max: Int = 10,
                           blocked: String? = nil) -> RepeaterImportModel {
        let model = RepeaterImportModel()
        model.setResults(repeaters)
        model.capacity = GeoCapacity(available: max, maxSelectable: max, zoneLimit: nil,
                                     scanListLimit: nil, blockedReason: blocked)
        return model
    }

    func testRowsStartUnchecked() {
        let model = makeModel([repeater("A"), repeater("B")])
        XCTAssertEqual(model.selectedCount, 0)
        XCTAssertFalse(model.canBuild, "nothing picked, nothing to build")
    }

    // MARK: - Session state

    /// The search form and results survive while the open codeplug stays the
    /// same; another codeplug drops the results but keeps the form.
    func testResultsAreKeptForTheSameFileAndDroppedForAnother() {
        let file = URL(fileURLWithPath: "/tmp/a.bin")
        let model = RepeaterImportModel()
        model.place = "Denver, CO"
        model.radiusMiles = 40
        model.setResults([repeater("W0A"), repeater("W0B")], for: file)
        model.setSelected(model.rows[0], true)

        model.discardResults(unlessFor: file)
        XCTAssertEqual(model.rows.count, 2)
        XCTAssertEqual(model.selectedCount, 1)

        model.discardResults(unlessFor: URL(fileURLWithPath: "/tmp/b.bin"))
        XCTAssertTrue(model.rows.isEmpty)
        XCTAssertEqual(model.selectedCount, 0)
        XCTAssertFalse(model.hasFetched)
        XCTAssertEqual(model.place, "Denver, CO", "the form is kept")
        XCTAssertEqual(model.radiusMiles, 40)
    }

    /// After a merge add, the added rows become duplicates and can't be picked
    /// again; the others keep their flags.
    func testMergeAddMarksPicksAsAlreadyInTheCodeplug() {
        let model = makeModel([repeater("W0A"), repeater("W0B"), repeater("W0C", duplicate: true)])
        model.mode = .merge
        model.setSelected(model.rows[0], true)
        model.markSelectionAdded()
        XCTAssertEqual(model.rows.map(\.repeater.duplicate), [true, false, true])
        XCTAssertEqual(model.selectedCount, 0)
        XCTAssertFalse(model.isSelectable(model.rows[0]))
    }

    /// A clean add removed every earlier channel, so only the picks are
    /// duplicates afterwards.
    func testCleanAddLeavesOnlyThePicksAsDuplicates() {
        let model = makeModel([repeater("W0A"), repeater("W0B", duplicate: true)])
        model.mode = .clean
        model.setSelected(model.rows[0], true)
        model.markSelectionAdded()
        XCTAssertEqual(model.rows.map(\.repeater.duplicate), [true, false])
    }

    func testOverLimitDisablesBuild() {
        let model = makeModel([repeater("A"), repeater("B"), repeater("C")], max: 2)
        model.selectAll()
        XCTAssertEqual(model.selectedCount, 3)
        XCTAssertTrue(model.isOverLimit)
        XCTAssertEqual(model.excess, 1)
        XCTAssertFalse(model.canBuild)

        model.setSelected(model.rows[2], false)
        XCTAssertFalse(model.isOverLimit)
        XCTAssertTrue(model.canBuild)
    }

    func testBlockedReasonDisablesBuild() {
        let model = makeModel([repeater("A")], blocked: "no room")
        model.selectAll()
        XCTAssertFalse(model.canBuild)
    }

    func testDuplicatesAreUnselectableInMerge() {
        let model = makeModel([repeater("A"), repeater("DUP", duplicate: true)])
        model.mode = .merge
        model.setSelected(model.rows[1], true)
        XCTAssertEqual(model.selectedCount, 0)

        model.selectAll()
        XCTAssertEqual(model.selectedRepeaters.map(\.callsign), ["A"])
    }

    func testDuplicatesAreSelectableInClean() {
        let model = makeModel([repeater("A"), repeater("DUP", duplicate: true)])
        model.mode = .clean
        model.selectAll()
        XCTAssertEqual(model.selectedCount, 2)

        // Back to merge: the duplicate no longer qualifies and is dropped.
        model.mode = .merge
        XCTAssertEqual(model.selectedRepeaters.map(\.callsign), ["A"])
    }

    func testSelectAllRespectsFilters() {
        let model = makeModel([repeater("TWO"), repeater("SEVENTY", rxHz: 446_000_000),
                               repeater("DMR", rxHz: 145_000_000, mode: .dmr)])
        model.bandFilter = .twoMeters
        model.modeFilter = .analog
        XCTAssertEqual(model.visibleRows.map(\.repeater.callsign), ["TWO"])
        model.selectAll()
        XCTAssertEqual(model.selectedRepeaters.map(\.callsign), ["TWO"])

        model.selectNone()
        XCTAssertEqual(model.selectedCount, 0)
    }

    func testNewResultsClearTheSelection() {
        let model = makeModel([repeater("A")])
        model.selectAll()
        model.setResults([repeater("B")])
        XCTAssertEqual(model.selectedCount, 0)
    }
}

/// Capacity and build through the real core, against synthetic images. No
/// network: fetch is the only call that needs it, and build takes the records
/// directly.
final class RepeaterBuildTests: XCTestCase {

    private var tempDir: URL!

    /// The same all-zero AnyTone image `SaveModelTests` uses.
    private static let anyToneSize = 1_789_328

    /// A blank KT-5000: all `0xFF` with the model tag at `0x0FE0` (see
    /// `QytSaveModelTests.blankImage`).
    private static func blankQyt() -> Data {
        var bytes = [UInt8](repeating: 0xFF, count: 12_800)
        let tag = Array("Model:KT-5000   ".utf8)
        bytes.replaceSubrange(0x0FE0..<(0x0FE0 + tag.count), with: tag)
        return Data(bytes)
    }

    /// A 2 m analog repeater with a CTCSS access tone.
    private func analog(_ call: String, rxHz: UInt32 = 146_940_000) -> Repeater {
        Repeater(source: .hearham, callsign: call, location: "Denver", rxHz: rxHz,
                 txHz: rxHz - 600_000, mode: .analog, txTone: .ctcss(hzTenths: 1000),
                 rxTone: .off, colorCode: nil, lat: 39.7, lon: -104.9, distanceKm: 1,
                 duplicate: false)
    }

    override func setUpWithError() throws {
        try super.setUpWithError()
        tempDir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("repeater-build-tests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: tempDir)
        try super.tearDownWithError()
    }

    /// A KT-5000 build writes the pick as a channel and leaves the base alone.
    func testQytBuildRoundTrip() throws {
        let base = tempDir.appendingPathComponent("kt5000.bin")
        let out = tempDir.appendingPathComponent("kt5000-repeaters.bin")
        try Self.blankQyt().write(to: base)
        let before = try Data(contentsOf: base)

        let cap = try RadioCore.geoCapacity(binURL: base, mode: .merge)
        XCTAssertGreaterThan(cap.maxSelectable, 0)
        XCTAssertNil(cap.blockedReason)

        try RadioCore.geoBuild(binURL: base, repeaters: [analog("W0ABC")],
                               options: GeoBuildOptions(mode: .merge, zoneName: "Nearby"),
                               outURL: out)
        XCTAssertEqual(try Data(contentsOf: base), before, "the base is only read")

        let channels = try XCTUnwrap(RadioCore.dump(binPath: out.path).qyt?.channels)
        XCTAssertEqual(channels.count, 1)
        XCTAssertEqual(channels[0].rxFrequencyHz, 146_940_000)
        XCTAssertEqual(channels[0].txFrequencyHz, 146_340_000)
        XCTAssertEqual(channels[0].txTone, .ctcss(hzTenths: 1000))
    }

    /// The KT-5000 has no DMR, so a DMR pick is refused.
    func testQytRefusesDMR() throws {
        let base = tempDir.appendingPathComponent("kt5000.bin")
        try Self.blankQyt().write(to: base)
        var dmr = analog("K0DMR", rxHz: 446_500_000)
        dmr.mode = .dmr
        dmr.colorCode = 1
        XCTAssertThrowsError(try RadioCore.geoBuild(
            binURL: base, repeaters: [dmr],
            options: GeoBuildOptions(mode: .merge, zoneName: "Nearby"),
            outURL: tempDir.appendingPathComponent("out.bin")))
    }

    /// An AnyTone build adds the channel and a zone with the requested name.
    func testAnyToneBuildRoundTrip() throws {
        let base = tempDir.appendingPathComponent("anytone.bin")
        let out = tempDir.appendingPathComponent("anytone-repeaters.bin")
        try Data(count: Self.anyToneSize).write(to: base)

        let cap = try RadioCore.geoCapacity(binURL: base, mode: .clean)
        XCTAssertGreaterThan(cap.maxSelectable, 1)

        try RadioCore.geoBuild(binURL: base, repeaters: [analog("W0ABC"), analog("W0XYZ", rxHz: 147_000_000)],
                               options: GeoBuildOptions(mode: .clean, zoneName: "Nearby"),
                               outURL: out)
        let dump = try XCTUnwrap(RadioCore.dump(binPath: out.path).anytone)
        XCTAssertEqual(Set(dump.channels.map(\.rxFrequencyHz)), [146_940_000, 147_000_000])
        let zone = try XCTUnwrap(dump.zones.first { $0.name == "Nearby" })
        XCTAssertEqual(zone.channels.count, 2)
    }
}

/// The settings warning for a value that isn't a RepeaterBook token.
final class RepeaterBookTokenWarningTests: XCTestCase {
    func testPrefixedTokensPassAndBareValuesWarn() {
        XCTAssertNil(repeaterBookTokenWarning(""))
        XCTAssertNil(repeaterBookTokenWarning("rbuapp_abc"))
        XCTAssertNil(repeaterBookTokenWarning(" app_abc "))
        XCTAssertNotNil(repeaterBookTokenWarning(String(repeating: "ad56", count: 16)))
    }
}

/// Adding picks to the open codeplug: staged in the work file, undoable, and
/// the document on disk untouched until Save.
@MainActor
final class RepeaterAddToCodeplugTests: XCTestCase {

    private var tempDir: URL!
    private var codeplug: URL!

    override func setUpWithError() throws {
        try super.setUpWithError()
        tempDir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("repeater-add-tests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
        setenv("CODEPLUGME_RECOVERY_DIR", tempDir.appendingPathComponent("recovery").path, 1)
        Recovery.clear()
        codeplug = tempDir.appendingPathComponent("codeplug.bin")
        try Data(count: 1_789_328).write(to: codeplug)
    }

    override func tearDownWithError() throws {
        Recovery.clear()
        unsetenv("CODEPLUGME_RECOVERY_DIR")
        try? FileManager.default.removeItem(at: tempDir)
        try super.tearDownWithError()
    }

    private func analog(_ call: String, rxHz: UInt32) -> Repeater {
        Repeater(source: .hearham, callsign: call, location: "Denver", rxHz: rxHz,
                 txHz: rxHz - 600_000, mode: .analog, txTone: .ctcss(hzTenths: 1000),
                 rxTone: .off, colorCode: nil, lat: 39.7, lon: -104.9, distanceKm: 1,
                 duplicate: false)
    }

    func testAddStagesIntoTheOpenCodeplugAndUndoes() throws {
        let before = try Data(contentsOf: codeplug)
        let store = CodeplugStore()
        store.open(url: codeplug)

        let ok = store.addRepeaters([analog("W0A", rxHz: 146_940_000), analog("W0B", rxHz: 147_000_000)],
                                    options: GeoBuildOptions(mode: .merge, zoneName: "Home"))

        XCTAssertTrue(ok, store.errorMessage ?? "")
        XCTAssertEqual(store.channels.count, 2)
        XCTAssertEqual(store.zones.map(\.name), ["Home"])
        XCTAssertTrue(store.isDirty)
        XCTAssertTrue(store.hasUnsavedChanges(.channels))
        XCTAssertEqual(store.fileURL, codeplug, "still the same open document")
        XCTAssertEqual(try Data(contentsOf: codeplug), before, "nothing written until Save")

        store.undo()
        XCTAssertEqual(store.channels.count, 0, "one undo removes the whole import")
        XCTAssertTrue(store.zones.isEmpty)
    }
}
