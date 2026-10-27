import XCTest
@testable import CodeplugMe

/// Number formatting. These pin two bugs that reached the UI: DMR IDs rendering
/// as "3,113,043" because a UInt32 interpolated into `Text("\(n)")` routes
/// through LocalizedStringKey, and the same frequency rendering as "145.50000"
/// in a table but "145.5" in its editor.
final class FormattingTests: XCTestCase {

    func testDMRIDHasNoGroupingSeparator() {
        XCTAssertEqual(formatDMRID(3_113_043), "3113043")
        XCTAssertEqual(formatDMRID(1), "1")
        XCTAssertEqual(formatDMRID(16_777_215), "16777215")
    }

    func testMHzUsesFiveDecimalsAndNoGrouping() {
        XCTAssertEqual(formatMHz(145_500_000), "145.50000")
        XCTAssertEqual(formatMHz(446_000_000), "446.00000")
    }

    /// A 1.2 GHz channel is where a grouping separator would appear.
    func testMHzDoesNotGroupAboveOneThousand() {
        XCTAssertEqual(formatMHz(1_296_000_000), "1296.00000")
    }

    /// The table and the editor share one format, so what is displayed parses
    /// back to the same value.
    func testMHzRoundTripsThroughItsFormat() throws {
        let text = formatMHz(145_512_500)
        let parsed = try mhzFormat.parseStrategy.parse(text)
        XCTAssertEqual(parsed, 145.5125, accuracy: 0.000_001)
    }

    /// Slots are displayed 1-based and reach the thousands, so they must not
    /// pick up a separator either.
    func testSlotIsOneBasedAndUngrouped() {
        XCTAssertEqual(formatSlot(0), "1")
        XCTAssertEqual(formatSlot(1233), "1234")
    }

    func testEditValueMapsDumpEnumsToEditAPIValues() {
        XCTAssertEqual(editValue("MixedAnalog"), "mixed_analog")
        XCTAssertEqual(editValue("MixedDigital"), "mixed_digital")
        XCTAssertEqual(editValue("Turbo"), "turbo")
        XCTAssertEqual(editValue("Analog"), "analog")
        XCTAssertEqual(editValue("Group"), "group")
    }
}

/// Channel mode semantics and the Channels-pane filter matching.
final class ChannelFilterTests: XCTestCase {

    private func channel(mode: String = "Digital", colorCode: Int = 1,
                         timeSlot: Int = 1, name: String = "Repeater") -> DumpChannel {
        DumpChannel(index: 0, name: name, rxFrequencyHz: 146_520_000,
                    txFrequencyHz: 146_520_000, mode: mode, power: "High",
                    bandwidth: "Wide", colorCode: colorCode, timeSlot: timeSlot,
                    contactIndex: 0, radioIdIndex: 0, groupListIndex: 255)
    }

    /// Only pure "Analog" lacks a digital component; the two mixed modes keep it.
    func testModeHasDigital() {
        XCTAssertFalse(modeHasDigital("Analog"))
        XCTAssertTrue(modeHasDigital("Digital"))
        XCTAssertTrue(modeHasDigital("MixedAnalog"))
        XCTAssertTrue(modeHasDigital("MixedDigital"))
    }

    func testNilFiltersMatchEverything() {
        let ch = channel(mode: "Analog", colorCode: 7, timeSlot: 2)
        XCTAssertTrue(channelMatchesFilter(ch, query: "", mode: nil, colorCode: nil, slot: nil))
    }

    func testEachFilterComponentNarrows() {
        let ch = channel(mode: "Digital", colorCode: 3, timeSlot: 2)
        XCTAssertTrue(channelMatchesFilter(ch, query: "", mode: "Digital", colorCode: nil, slot: nil))
        XCTAssertFalse(channelMatchesFilter(ch, query: "", mode: "Analog", colorCode: nil, slot: nil))
        XCTAssertTrue(channelMatchesFilter(ch, query: "", mode: nil, colorCode: 3, slot: nil))
        XCTAssertFalse(channelMatchesFilter(ch, query: "", mode: nil, colorCode: 4, slot: nil))
        XCTAssertTrue(channelMatchesFilter(ch, query: "", mode: nil, colorCode: nil, slot: 2))
        XCTAssertFalse(channelMatchesFilter(ch, query: "", mode: nil, colorCode: nil, slot: 1))
    }

    /// The text query and the dropdown filters combine with AND.
    func testQueryAndFiltersCombine() {
        let ch = channel(mode: "Digital", colorCode: 1, timeSlot: 1, name: "Local Repeater")
        XCTAssertTrue(channelMatchesFilter(ch, query: "repeater", mode: "Digital",
                                           colorCode: 1, slot: 1))
        XCTAssertFalse(channelMatchesFilter(ch, query: "simplex", mode: "Digital",
                                            colorCode: 1, slot: 1))
    }
}

/// Duplicate-channel detection for the Channels pane highlight.
final class ChannelDuplicateTests: XCTestCase {

    private func channel(_ index: Int, name: String = "Repeater", rx: UInt32 = 442_000_000,
                         tx: UInt32 = 447_000_000, mode: String = "Digital",
                         colorCode: Int = 1, timeSlot: Int = 1) -> DumpChannel {
        DumpChannel(index: index, name: name, rxFrequencyHz: rx, txFrequencyHz: tx,
                    mode: mode, power: "High", bandwidth: "Narrow", colorCode: colorCode,
                    timeSlot: timeSlot, contactIndex: 0, radioIdIndex: 0, groupListIndex: 255)
    }

    /// Same pair, CC and slot under different names: both rows flag each other.
    func testDigitalTwinsWithDifferentNamesAreDuplicates() {
        let dupes = duplicateChannelSlots([channel(0, name: "A"), channel(5, name: "B"),
                                           channel(9, rx: 146_520_000, tx: 146_520_000)])
        XCTAssertEqual(dupes, [0: [5], 5: [0]])
    }

    /// A different color code or time slot makes it a different DMR channel.
    func testDigitalColorCodeOrSlotDifferenceIsNotDuplicate() {
        XCTAssertTrue(duplicateChannelSlots([channel(0), channel(1, colorCode: 2)]).isEmpty)
        XCTAssertTrue(duplicateChannelSlots([channel(0), channel(1, timeSlot: 2)]).isEmpty)
    }

    /// Analog channels ignore the (meaningless) CC / slot fields.
    func testAnalogIgnoresColorCodeAndSlot() {
        let dupes = duplicateChannelSlots([channel(0, mode: "Analog", colorCode: 1, timeSlot: 1),
                                           channel(1, mode: "Analog", colorCode: 7, timeSlot: 2)])
        XCTAssertEqual(dupes, [0: [1], 1: [0]])
    }

    /// An analog and a DMR channel on the same pair are not the same channel.
    func testAnalogAndDigitalOnSamePairAreNotDuplicates() {
        XCTAssertTrue(duplicateChannelSlots([channel(0, mode: "Analog"), channel(1)]).isEmpty)
    }

    /// A mixed mode is its own channel type: it doesn't match the pure mode
    /// on the same pair, CC and slot.
    func testMixedModeIsNotDuplicateOfPureMode() {
        XCTAssertTrue(duplicateChannelSlots([channel(0), channel(1, mode: "MixedDigital")]).isEmpty)
        XCTAssertTrue(duplicateChannelSlots([channel(0, mode: "Analog"),
                                             channel(1, mode: "MixedAnalog")]).isEmpty)
        XCTAssertEqual(duplicateChannelSlots([channel(0, mode: "MixedDigital"),
                                              channel(1, mode: "MixedDigital")]), [0: [1], 1: [0]])
    }

    /// Only RX matching isn't enough; TX must match too.
    func testDifferentTxIsNotDuplicate() {
        XCTAssertTrue(duplicateChannelSlots([channel(0), channel(1, tx: 442_000_000)]).isEmpty)
    }

    /// A different RX or TX tone makes it a different analog channel; a CTCSS
    /// index left behind under DCS doesn't, since the radio ignores it.
    func testAnalogToneDifferenceIsNotDuplicate() {
        var ctcss = channel(0, mode: "Analog")
        ctcss.txSignalingMode = "Ctcss"
        ctcss.txCtcss = 12
        var otherTone = channel(1, mode: "Analog")
        otherTone.txSignalingMode = "Ctcss"
        otherTone.txCtcss = 13
        XCTAssertTrue(duplicateChannelSlots([ctcss, otherTone]).isEmpty)
        XCTAssertTrue(duplicateChannelSlots([ctcss, channel(1, mode: "Analog")]).isEmpty,
                      "a toned channel isn't a twin of a carrier-squelch one")

        var dcsA = channel(0, mode: "Analog"), dcsB = channel(1, mode: "Analog")
        dcsA.rxSignalingMode = "Dcs"; dcsA.rxDcs = 23; dcsA.rxCtcss = 4
        dcsB.rxSignalingMode = "Dcs"; dcsB.rxDcs = 23; dcsB.rxCtcss = 9
        XCTAssertEqual(duplicateChannelSlots([dcsA, dcsB]), [0: [1], 1: [0]])
    }

    /// Pure digital has no analog side, so stale tone fields don't split twins.
    func testDigitalIgnoresTones() {
        var a = channel(0), b = channel(1)
        a.txSignalingMode = "Ctcss"; a.txCtcss = 12
        b.txSignalingMode = "Dcs"; b.txDcs = 23
        XCTAssertEqual(duplicateChannelSlots([a, b]), [0: [1], 1: [0]])
    }

    /// Three-way groups list every other member, in slot order.
    func testThreeWayGroupListsOthersSorted() {
        let dupes = duplicateChannelSlots([channel(8), channel(2), channel(4)])
        XCTAssertEqual(dupes[2], [4, 8])
        XCTAssertEqual(dupes[8], [2, 4])
        XCTAssertEqual(duplicateLabel([4, 8]), "Duplicate of channel \(formatSlot(4)), \(formatSlot(8))")
    }

    /// The duplicates-only sort groups twins by RX, then TX, then slot.
    func testFrequencySortOrderGroupsTwins() {
        let sorted = [channel(9, rx: 146_520_000, tx: 146_520_000), channel(7),
                      channel(1, tx: 446_000_000), channel(3)]
            .sorted(using: channelFrequencySortOrder)
        XCTAssertEqual(sorted.map(\.index), [9, 1, 3, 7])
    }
}

/// `EditSpec` encoding. This is the highest-consequence pure logic in the Swift
/// layer: the core matches on snake_case keys, so one wrong key silently drops
/// an edit to a radio's configuration with no error anywhere.
final class EditSpecTests: XCTestCase {

    private func encode(_ spec: EditSpec) throws -> [String: Any] {
        let data = try JSONEncoder().encode(spec)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    /// Absent fields must be omitted, not sent as null: the core leaves out
    /// fields alone, so an unintended key would overwrite a value the user
    /// never touched.
    func testChannelEditOmitsUntouchedFields() throws {
        var edit = ChannelEdit(index: 4)
        edit.name = "Simplex"
        let json = try encode(EditSpec(channels: [edit]))

        let channels = try XCTUnwrap(json["channels"] as? [[String: Any]])
        XCTAssertEqual(channels.count, 1)
        XCTAssertEqual(channels[0]["index"] as? Int, 4)
        XCTAssertEqual(channels[0]["name"] as? String, "Simplex")
        XCTAssertEqual(channels[0].count, 2, "only the touched fields should be encoded")
    }

    func testChannelEditUsesSnakeCaseKeys() throws {
        var edit = ChannelEdit(index: 0)
        edit.rxFrequencyHz = 145_500_000
        edit.txFrequencyHz = 145_500_000
        edit.colorCode = 1
        edit.timeSlot = 2
        edit.contactIndex = 3
        edit.radioIdIndex = 4
        edit.groupListIndex = 5
        let channel = try XCTUnwrap(try encode(EditSpec(channels: [edit]))["channels"]
            as? [[String: Any]])[0]

        XCTAssertEqual(channel["rx_frequency_hz"] as? UInt32, 145_500_000)
        XCTAssertEqual(channel["tx_frequency_hz"] as? UInt32, 145_500_000)
        XCTAssertEqual(channel["color_code"] as? Int, 1)
        XCTAssertEqual(channel["time_slot"] as? Int, 2)
        XCTAssertEqual(channel["contact_index"] as? Int, 3)
        XCTAssertEqual(channel["radio_id_index"] as? Int, 4)
        XCTAssertEqual(channel["group_list_index"] as? Int, 5)
    }

    func testStructuralArraysUseSnakeCaseKeys() throws {
        var spec = EditSpec()
        spec.addZones = [ZoneEdit()]
        spec.removeZones = [2]
        spec.moveZones = [MoveOp(from: 1, to: 7)]
        spec.addRadioIds = [RadioIDEdit()]
        spec.removeGroupLists = [3]
        let json = try encode(spec)

        XCTAssertNotNil(json["add_zones"])
        XCTAssertEqual(json["remove_zones"] as? [Int], [2])
        XCTAssertNotNil(json["add_radio_ids"])
        XCTAssertEqual(json["remove_group_lists"] as? [Int], [3])

        let moves = try XCTUnwrap(json["move_zones"] as? [[String: Any]])
        XCTAssertEqual(moves[0]["from"] as? Int, 1)
        XCTAssertEqual(moves[0]["to"] as? Int, 7)
    }

    func testContactEditUsesCallTypeKey() throws {
        var edit = ContactEdit(index: 1)
        edit.callType = "group"
        let contact = try XCTUnwrap(try encode(EditSpec(contacts: [edit]))["contacts"]
            as? [[String: Any]])[0]
        XCTAssertEqual(contact["call_type"] as? String, "group")
    }

    func testEmptySpecIsEmpty() {
        XCTAssertTrue(EditSpec().isEmpty)
    }

    func testSpecWithAnyOperationIsNotEmpty() {
        var spec = EditSpec()
        spec.moveRadioIds = [MoveOp(from: 0, to: 1)]
        XCTAssertFalse(spec.isEmpty, "isEmpty must account for every operation array")
    }
}

/// Dump decoding. The core emits snake_case and capitalized enum values; a
/// mismatch here means the app silently shows an empty codeplug.
final class CodeplugDumpTests: XCTestCase {

    func testDecodesCoreJSON() throws {
        let json = """
        {
          "model": "anytone-d878uv2",
          "display_name": "AnyTone AT-D878UVII Plus",
          "capabilities": {
            "sections": ["channels", "zones", "scan_lists", "contacts",
                         "group_lists", "radio_ids", "aprs", "future_section"],
            "channel_capacity": 4000, "channel_name_length": 16,
            "supports_dmr": true, "tone_encoding": "table_index",
            "power_options": [{"tag": "Low", "label": "Low"},
                              {"tag": "Turbo", "label": "Turbo"}],
            "bandwidth_options": [{"tag": "Narrow", "label": "Narrow"},
                                  {"tag": "Wide", "label": "Wide"}]
          },
          "anytone": {
          "channels": [{
            "index": 0, "name": "Simplex", "rx_frequency_hz": 146520000,
            "tx_frequency_hz": 146520000, "mode": "Analog", "power": "High",
            "bandwidth": "Wide", "color_code": 1, "time_slot": 1,
            "contact_index": 0, "radio_id_index": 0, "group_list_index": 255,
            "rx_signaling_mode": "Ctcss", "tx_signaling_mode": "None",
            "rx_ctcss": 12, "tx_ctcss": 0, "rx_dcs": 0, "tx_dcs": 0,
            "squelch_mode": "Carrier", "optional_signaling": "Off", "admit": "Always",
            "scan_list_index": 255, "dtmf_id_index": 0, "two_tone_id_index": 0,
            "five_tone_id_index": 0, "two_tone_decode_index": 0,
            "rx_only": true, "talk_around": false, "call_confirm": false,
            "work_alone": false, "simplex_tdma": false, "rx_aprs": false
          }],
          "zones": [{"index": 0, "name": "Local", "channels": [0]}],
          "scan_lists": [{
            "index": 0, "name": "Personal", "members": [0], "priority_channel_select": 3,
            "priority_channel_1": 0, "priority_channel_2": 65535, "look_back_a": 20,
            "look_back_b": 30, "dropout_delay": 31, "dwell_time": 31, "revert_channel": 2
          }],
          "contacts": [{"index": 0, "name": "Parrot", "number": 9990, "call_type": "Private"}],
          "group_lists": [{"index": 0, "name": "Wide", "members": [0]}],
          "radio_ids": [{"index": 0, "name": "Me", "number": 3113043}],
          "aprs": {
            "source_call": "K7AOS", "source_ssid": 7, "destination_call": "APAT81",
            "destination_ssid": 0, "symbol_table": 47, "symbol": 60,
            "manual_tx_interval": 5, "auto_tx_interval": 45, "fm_power": 3,
            "fm_tx_frequency_hz": 144390000
          }
          }
        }
        """
        let envelope = try JSONDecoder().decode(CodeplugDump.self, from: Data(json.utf8))
        XCTAssertEqual(envelope.model, .anyToneD878UV2)
        XCTAssertEqual(envelope.displayName, "AnyTone AT-D878UVII Plus")
        XCTAssertEqual(envelope.capabilities.channelCapacity, 4000)
        XCTAssertEqual(envelope.capabilities.powerOptions.last?.label, "Turbo")
        // "future_section" is a section this build has no pane for. Dropping it
        // rather than throwing is what keeps an older app usable against a newer
        // core.
        XCTAssertEqual(envelope.capabilities.sections, CodeplugSection.anyToneSections)

        let dump = try XCTUnwrap(envelope.anytone)
        XCTAssertEqual(dump.channels.first?.rxFrequencyHz, 146_520_000)
        XCTAssertEqual(dump.channels.first?.groupListIndex, 255)
        XCTAssertEqual(dump.channels.first?.rxSignalingMode, "Ctcss")
        XCTAssertEqual(dump.channels.first?.rxCtcss, 12)
        XCTAssertEqual(dump.channels.first?.rxOnly, true)
        XCTAssertEqual(dump.channels.first?.admit, "Always")
        XCTAssertEqual(dump.scanLists.first?.name, "Personal")
        XCTAssertEqual(dump.scanLists.first?.members, [0])
        XCTAssertEqual(dump.scanLists.first?.priorityChannel2, 65535)
        XCTAssertEqual(dump.contacts.first?.callType, "Private")
        XCTAssertEqual(dump.groupLists.first?.members, [0])
        XCTAssertEqual(dump.radioIds.first?.number, 3_113_043)
        XCTAssertEqual(dump.aprs?.sourceCall, "K7AOS")
        XCTAssertEqual(dump.aprs?.fmTxFrequencyHz, 144_390_000)
    }
}

/// Store behavior that doesn't need the radio or a real .bin. `load` is split
/// out of `open` precisely so a synthetic dump can drive it.
@MainActor
final class CodeplugStoreTests: XCTestCase {

    private func makeAnyToneDump() -> AnyToneDump {
        AnyToneDump(
            channels: [DumpChannel(index: 0, name: "Simplex", rxFrequencyHz: 146_520_000,
                                   txFrequencyHz: 146_520_000, mode: "Analog", power: "High",
                                   bandwidth: "Wide", colorCode: 1, timeSlot: 1,
                                   contactIndex: 0, radioIdIndex: 0, groupListIndex: 255)],
            zones: [DumpZone(index: 0, name: "Local", channels: [0])],
            contacts: [DumpContact(index: 0, name: "Parrot", number: 9990, callType: "Private")],
            groupLists: [DumpGroupList(index: 0, name: "Wide", members: [0])],
            radioIds: [DumpRadioID(index: 0, name: "Me", number: 3_113_043)]
        )
    }

    /// A full envelope with whatever section list the caller wants, so a
    /// document whose radio lacks a section can be simulated without a .bin.
    private func makeDump(sections: [CodeplugSection] = CodeplugSection.anyToneSections) -> CodeplugDump {
        CodeplugDump(
            model: .anyToneD878UV2,
            capabilities: Capabilities(
                sections: sections,
                channelCapacity: 4000,
                channelNameLength: 16,
                supportsDMR: true,
                toneEncoding: "table_index",
                powerOptions: [PickerOption(tag: "Low", label: "Low")],
                bandwidthOptions: [PickerOption(tag: "Wide", label: "Wide")]
            ),
            anytone: makeAnyToneDump()
        )
    }

    func testLoadPopulatesRecordsAndIsNotDirty() {
        let store = CodeplugStore()
        store.load(makeDump(), url: URL(fileURLWithPath: "/tmp/codeplug.bin"))

        XCTAssertEqual(store.channels.count, 1)
        XCTAssertEqual(store.zones.first?.name, "Local")
        XCTAssertEqual(store.radioIds.first?.number, 3_113_043)
        XCTAssertEqual(store.fileURL?.lastPathComponent, "codeplug.bin")
        XCTAssertFalse(store.isDirty, "a freshly loaded codeplug has no staged changes")
    }

    /// Records are keyed by slot, not array position — that identity is what
    /// lets a sorted table commit an edit to the right record.
    func testRecordIDIsItsSlot() {
        let store = CodeplugStore()
        store.load(makeDump(), url: URL(fileURLWithPath: "/tmp/codeplug.bin"))
        XCTAssertEqual(store.channels.first?.id, store.channels.first?.index)
    }

    func testSectionsForAnyToneAreTheFullSet() {
        let store = CodeplugStore()
        // Before anything is open the sidebar still shows the AnyTone shape, so
        // it doesn't collapse and then jump back when a file arrives.
        XCTAssertEqual(store.sections, CodeplugSection.anyToneSections)

        store.load(makeDump(), url: URL(fileURLWithPath: "/tmp/codeplug.bin"))
        XCTAssertEqual(store.model, .anyToneD878UV2)
        XCTAssertEqual(store.sections, CodeplugSection.anyToneSections)
        XCTAssertTrue(store.capabilities.supportsDMR)
    }

    /// The sidebar's selection is invalidated by `ContentView`'s onChange when a
    /// document's radio has no such section. This drives the store side of that:
    /// the section really does disappear from `sections`.
    func testSidebarSelectionResetsWhenItsSectionDisappears() {
        let store = CodeplugStore()
        var selection = SidebarItem.codeplug(.aprs)

        let withoutAPRS = CodeplugSection.anyToneSections.filter { $0 != .aprs }
        store.load(makeDump(sections: withoutAPRS),
                   url: URL(fileURLWithPath: "/tmp/codeplug.bin"))
        XCTAssertFalse(store.sections.contains(.aprs))

        // The exact expression ContentView applies on a capabilities change.
        if case .codeplug(let section) = selection,
           !store.capabilities.sections.contains(section) {
            selection = .device
        }
        XCTAssertEqual(selection, .device)
    }

    func testIncompatibilityReasonNamesTheDetectedModel() throws {
        let store = CodeplugStore()
        let dir = URL(fileURLWithPath: NSTemporaryDirectory())

        // A real AnyTone-sized image is detected, so there is no complaint.
        let good = dir.appendingPathComponent("compat-\(UUID().uuidString).bin")
        try Data(count: RadioCore.expectedCodeplugSize).write(to: good)
        defer { try? FileManager.default.removeItem(at: good) }
        XCTAssertNil(store.incompatibilityReason(for: good))
        XCTAssertEqual(try RadioCore.detectModel(binPath: good.path), .anyToneD878UV2)

        // A file matching no radio is named against every model the core knows,
        // not against a hardcoded AnyTone.
        let bad = dir.appendingPathComponent("incompat-\(UUID().uuidString).bin")
        try Data(count: 12_800).write(to: bad)
        defer { try? FileManager.default.removeItem(at: bad) }
        let reason = try XCTUnwrap(store.incompatibilityReason(for: bad))
        XCTAssertTrue(reason.contains("12800"), reason)
        for model in RadioCore.supportedModels {
            XCTAssertTrue(reason.contains(model.displayName), reason)
            XCTAssertTrue(reason.contains("\(model.imageSize)"), reason)
        }
    }
}

/// The model layer the app reads off the core, rather than off a parallel Swift
/// table that could drift from it.
final class RadioModelTests: XCTestCase {

    func testSupportedModelsComeFromTheCore() throws {
        let models = RadioCore.supportedModels
        XCTAssertEqual(models.map(\.id), [.anyToneD878UV2, .qytKt5000])
        let anytone = try XCTUnwrap(models.first)
        XCTAssertEqual(anytone.imageSize, RadioCore.expectedCodeplugSize)
        XCTAssertEqual(anytone.capabilities.sections, CodeplugSection.anyToneSections)
    }

    func testCodeplugSizeForMatchesTheLegacyConstant() throws {
        XCTAssertEqual(try RadioCore.codeplugSize(for: .anyToneD878UV2),
                       RadioCore.expectedCodeplugSize)
    }

    /// The raw values are the core's JSON keys, not display text. Getting this
    /// wrong makes `capabilities.sections` decode to an empty sidebar.
    func testSectionRawValuesMatchTheCoreJSONKeys() {
        XCTAssertEqual(CodeplugSection.scanLists.rawValue, "scan_lists")
        XCTAssertEqual(CodeplugSection.groupLists.rawValue, "group_lists")
        XCTAssertEqual(CodeplugSection.radioIds.rawValue, "radio_ids")
        XCTAssertEqual(CodeplugSection.scanLists.title, "Scan Lists")
    }
}

/// The membership rules behind the two-column transfer picker used to add
/// channels to a zone and contacts to a group list.
final class MemberTransferTests: XCTestCase {

    /// Five candidates at slots 0...4, labeled A...E.
    private let candidates = (0..<5).map {
        MemberCandidate(index: $0, label: String(UnicodeScalar(UInt8(65 + $0))))
    }

    private func labels(_ items: [MemberCandidate]) -> [String] { items.map(\.label) }

    /// The left column offers only what isn't already a member — that exclusion
    /// is the whole reason a member can't be added to a zone twice.
    func testAvailableExcludesCurrentMembers() {
        let available = MemberTransfer.available(candidates: candidates, members: [1, 3])
        XCTAssertEqual(labels(available), ["A", "C", "E"])
    }

    /// A multi-select add lands in candidate order, not the arbitrary order a
    /// Set iterates in — otherwise the same three picks could produce different
    /// channel orders in the zone from run to run.
    func testAddingAppendsInCandidateOrderNotSelectionOrder() {
        let result = MemberTransfer.adding([4, 0, 2], candidates: candidates, to: [])
        XCTAssertEqual(result, [0, 2, 4])
    }

    /// Adds append, so an existing zone keeps the order the user built.
    func testAddingAppendsAfterExistingMembers() {
        let result = MemberTransfer.adding([0, 2], candidates: candidates, to: [3])
        XCTAssertEqual(result, [3, 0, 2])
    }

    /// Selecting something already a member can't duplicate it: it isn't in the
    /// available column to begin with.
    func testAddingCannotDuplicateAnExistingMember() {
        let result = MemberTransfer.adding([1], candidates: candidates, to: [1])
        XCTAssertEqual(result, [1])
    }

    /// Double-clicking a single available row moves just that one across,
    /// appended after the existing members like any other add.
    func testDoubleClickAddMovesOneMemberAppended() {
        let result = MemberTransfer.adding([2], candidates: candidates, to: [3])
        XCTAssertEqual(result, [3, 2])
    }

    func testRemovingDropsOnlyTheSelectionAndKeepsOrder() {
        let result = MemberTransfer.removing([2], from: [3, 2, 0])
        XCTAssertEqual(result, [3, 0])
    }

    /// Double-clicking a single member row removes just that one.
    func testDoubleClickRemoveMovesOneMemberOut() {
        let result = MemberTransfer.removing([2], from: [3, 2, 0])
        XCTAssertEqual(result, [3, 0])
    }

    func testRemovingHandlesAMultiSelection() {
        let result = MemberTransfer.removing([3, 0], from: [3, 2, 0])
        XCTAssertEqual(result, [2])
    }

    /// The right column shows membership order, not slot order.
    func testMemberItemsFollowMembershipOrder() {
        let items = MemberTransfer.memberItems(candidates: candidates, members: [4, 1])
        XCTAssertEqual(labels(items), ["E", "B"])
    }

    /// A zone can reference a channel that was deleted. It still needs a row, or
    /// the user has no way to remove the dangling reference.
    func testMemberItemsKeepARowForAMissingRecord() {
        let items = MemberTransfer.memberItems(candidates: candidates, members: [9])
        XCTAssertEqual(items.count, 1)
        XCTAssertEqual(items.first?.index, 9)
        XCTAssertEqual(items.first?.label, "#10 (missing)", "labeled by 1-based slot")
    }

    /// Removing a dangling reference has to work, since that's the only way to
    /// clean one up.
    func testRemovingAMissingMemberWorks() {
        XCTAssertEqual(MemberTransfer.removing([9], from: [0, 9]), [0])
    }
}
