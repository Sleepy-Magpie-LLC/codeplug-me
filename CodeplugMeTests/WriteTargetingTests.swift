import XCTest
@testable import CodeplugMe

/// Which connected ports a codeplug may be written to.
///
/// The Rust core refuses a cross-model write by handshaking the radio before it
/// emits a byte, and that is the real guard. These cover the layer in front of
/// it: the app must not offer a destructive action it already knows will fail,
/// and when it somehow gets asked to anyway it must explain rather than try.
final class WriteTargetingTests: XCTestCase {

    /// A port the core recognized as an AnyTone by its USB VID/PID.
    private let anyTonePort = PortEntry(
        name: "/dev/cu.usbmodem14201", vid: 0x28e9, pid: 0x018a,
        product: "AnyTone", model: .anyToneD878UV2)

    /// A generic USB-serial adapter. This is how a QYT KT-5000 appears today:
    /// its bridge's VID/PID isn't in the core's table, so nothing attributes it.
    private let unknownPort = PortEntry(
        name: "/dev/cu.usbserial-1420", vid: 0x1a86, pid: 0x7523,
        product: "USB-SERIAL CH340", model: nil)

    func testWriteMenuOffersOnlyPortsMatchingTheOpenDocument() {
        let ports = [anyTonePort, unknownPort]
        let offered = writableTargets(among: ports, for: .anyToneD878UV2, manual: nil)
        XCTAssertEqual(offered.map(\.name), [anyTonePort.name],
                       "only the recognized AnyTone may take an AnyTone codeplug")

        // The same two ports, with a QYT document open: the AnyTone drops out.
        XCTAssertEqual(writableTargets(among: ports, for: .qytKt5000, manual: nil).map(\.name), [],
                       "an AnyTone port must never be offered a QYT codeplug")
    }

    func testUnknownPortRequiresAnExplicitModelSelection() {
        // Nobody has said what is behind the adapter, so it isn't a target.
        XCTAssertNil(effectiveModel(of: unknownPort, manual: nil))
        XCTAssertEqual(writableTargets(among: [unknownPort], for: .qytKt5000, manual: nil).count, 0)

        // The user picks QYT for that exact port, and it becomes writable. This
        // is the whole path by which a KT-5000 is reachable today.
        let manual = ManualModelSelection(port: unknownPort.name, model: .qytKt5000)
        XCTAssertEqual(effectiveModel(of: unknownPort, manual: manual), .qytKt5000)
        XCTAssertEqual(
            writableTargets(among: [unknownPort], for: .qytKt5000, manual: manual).map(\.name),
            [unknownPort.name])
    }

    /// A pick made for one adapter says nothing about a different one.
    func testAModelPickedForAnotherPortDoesNotApply() {
        let manual = ManualModelSelection(port: "/dev/cu.some-other-port", model: .qytKt5000)
        XCTAssertNil(effectiveModel(of: unknownPort, manual: manual))
        XCTAssertEqual(writableTargets(among: [unknownPort], for: .qytKt5000, manual: manual), [])
    }

    /// The case the whole stage exists to prevent: a QYT image aimed at an
    /// AnyTone. It has to stop as an explanation, not as a write attempt.
    func testQytDocumentOnAnAnyTonePortIsAMismatchError() throws {
        let message = try XCTUnwrap(
            writeRefusalReason(port: anyTonePort, documentModel: .qytKt5000, manual: nil),
            "a cross-model write must be refused, not attempted")
        XCTAssertTrue(message.contains("QYT KT-5000"),
                      "the error names the document's radio: \(message)")
        XCTAssertTrue(message.contains("AnyTone AT-D878UVII Plus"),
                      "the error names the connected radio: \(message)")
        XCTAssertFalse(writableTargets(among: [anyTonePort], for: .qytKt5000, manual: nil)
            .contains(anyTonePort),
            "and it is never offered as a target in the first place")
    }

    /// An explicit pick can't be used to force a mismatch either.
    func testAnExplicitPickCannotOverrideARecognizedRadio() {
        // Calling the AnyTone a QYT: its USB identity wins, and the mismatch stands.
        let manual = ManualModelSelection(port: anyTonePort.name, model: .qytKt5000)
        XCTAssertEqual(effectiveModel(of: anyTonePort, manual: manual), .anyToneD878UV2)
        XCTAssertNotNil(writeRefusalReason(port: anyTonePort, documentModel: .qytKt5000,
                                           manual: manual))
    }

    func testAMatchingPortIsNotRefused() {
        XCTAssertNil(writeRefusalReason(port: anyTonePort, documentModel: .anyToneD878UV2,
                                        manual: nil))
    }

    /// With no document open there is nothing to write, whatever is connected.
    func testNoOpenDocumentMeansNoTargets() {
        XCTAssertEqual(writableTargets(among: [anyTonePort, unknownPort], for: nil, manual: nil), [])
        XCTAssertNotNil(writeRefusalReason(port: anyTonePort, documentModel: nil, manual: nil))
    }
}

/// The port list the Device tab shows, and the decode that feeds it.
final class PortEntryTests: XCTestCase {

    /// `likely_radio` is `model != null` in the core, and the app relies on it.
    func testDecodesTheModelAlongsideLikelyRadio() throws {
        let json = """
        [{"name":"/dev/cu.usbmodem14201","vid":10473,"pid":394,"product":"AnyTone",
          "likely_radio":true,"model":"anytone-d878uv2"},
         {"name":"/dev/cu.usbserial-1420","vid":6790,"pid":29987,"product":"CH340",
          "likely_radio":false,"model":null}]
        """
        let ports = try JSONDecoder().decode([PortEntry].self, from: Data(json.utf8))
        XCTAssertEqual(ports.count, 2)
        XCTAssertEqual(ports[0].model, .anyToneD878UV2)
        XCTAssertTrue(ports[0].likelyRadio)
        XCTAssertNil(ports[1].model)
        XCTAssertFalse(ports[1].likelyRadio)
    }

    /// A core that grows a radio this build doesn't know must not take the whole
    /// port list down with it — that would hide every radio, including the ones
    /// this build handles fine.
    func testAnUnknownModelIdLeavesThePortUnattributed() throws {
        let json = """
        [{"name":"/dev/cu.usbmodem1","vid":1,"pid":2,"product":"Future",
          "likely_radio":true,"model":"some-future-radio"}]
        """
        let ports = try JSONDecoder().decode([PortEntry].self, from: Data(json.utf8))
        XCTAssertNil(ports[0].model, "an unrecognized id decodes as no attribution")
    }
}

/// `DeviceStore`'s port selection and model resolution. These touch no hardware:
/// the port list is assigned directly, which is what `refreshPorts` would do.
@MainActor
final class DeviceStoreModelTests: XCTestCase {

    private func store(ports: [PortEntry]) -> DeviceStore {
        let s = DeviceStore()
        s.ports = ports
        return s
    }

    private let anyTonePort = PortEntry(
        name: "/dev/cu.usbmodem14201", vid: 0x28e9, pid: 0x018a,
        product: "AnyTone", model: .anyToneD878UV2)
    private let unknownPort = PortEntry(
        name: "/dev/cu.usbserial-1420", vid: 0x1a86, pid: 0x7523,
        product: "USB-SERIAL CH340", model: nil)
    /// macOS reports these whether or not anything is plugged in.
    private let bluetoothPort = PortEntry(name: "/dev/cu.Bluetooth-Incoming-Port")

    /// The unattributed adapter must be listed, or the QYT is unreachable. The
    /// Bluetooth port must not be: nothing is ever behind it.
    func testCandidatePortsKeepUnknownAdaptersAndDropNonUsbPorts() {
        let s = store(ports: [bluetoothPort, unknownPort, anyTonePort])
        XCTAssertEqual(s.candidatePorts.map(\.name), [anyTonePort.name, unknownPort.name],
                       "recognized radios first, then unattributed adapters")
    }

    func testResolvedModelComesFromUsbAttributionWithoutAPick() {
        let s = store(ports: [anyTonePort])
        s.selectedPort = anyTonePort.name
        XCTAssertEqual(s.resolvedModel, .anyToneD878UV2)
        XCTAssertFalse(s.needsModelSelection)
    }

    /// The QYT path, end to end through the store: an unrecognized adapter is
    /// blocked until the user picks, then resolves to what they picked.
    func testAnUnknownPortNeedsAPickBeforeItResolves() {
        let s = store(ports: [unknownPort])
        s.selectedPort = unknownPort.name
        XCTAssertTrue(s.needsModelSelection)
        XCTAssertNil(s.resolvedModel, "no read or write may start on an unnamed radio")

        s.selectedModel = .qytKt5000
        XCTAssertEqual(s.resolvedModel, .qytKt5000)
        XCTAssertEqual(s.manualSelection,
                       ManualModelSelection(port: unknownPort.name, model: .qytKt5000))
    }

    /// A pick is bound to its port, so it can't follow the selection elsewhere.
    func testAPickDoesNotLeakToADifferentPort() {
        let s = store(ports: [unknownPort, anyTonePort])
        s.selectedPort = unknownPort.name
        s.selectedModel = .qytKt5000
        s.selectedPort = anyTonePort.name
        XCTAssertEqual(s.resolvedModel, .anyToneD878UV2,
                       "the AnyTone's own identity decides, not the earlier pick")
    }
}
