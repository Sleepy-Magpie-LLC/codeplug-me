import XCTest
import CryptoKit
@testable import CodeplugMe

/// Decoding the QYT half of the dump envelope, and encoding the edits that go
/// back. No FFI and no files: these pin the wire shapes the core defines in
/// `core/src/qyt/codeplug/`.
final class QytDumpTests: XCTestCase {

    /// A QYT envelope with one channel per tone kind.
    private let envelopeJSON = """
    {
      "model": "qyt-kt5000",
      "display_name": "QYT KT-5000",
      "capabilities": {
        "sections": ["qyt_channels", "qyt_settings", "qyt_bands"],
        "channel_capacity": 200, "channel_name_length": 7,
        "supports_dmr": false, "tone_encoding": "hz_tenths",
        "power_options": [{"tag": "High", "label": "High (25 W)"},
                          {"tag": "Low", "label": "Low (5 W)"}],
        "bandwidth_options": [{"tag": "Wide", "label": "Wide"},
                              {"tag": "Narrow", "label": "Narrow"}]
      },
      "qyt": {
        "channels": [
          {"index": 0, "name": "FRS-01", "rx_frequency_hz": 462562500,
           "tx_frequency_hz": 462562500,
           "rx_tone": {"kind": "none"}, "tx_tone": {"kind": "none"},
           "scode": 0, "speaker_mute": "Tone/DTCS", "optional_signaling": "OFF",
           "scramble": false, "power": "High", "bandwidth": "Wide",
           "busy_lockout": false, "scan_add": true, "ptt_id": "OFF",
           "flags_raw": [0, 0, 0, 36]},
          {"index": 1, "name": "N7FYZ", "rx_frequency_hz": 147320000,
           "tx_frequency_hz": 147920000,
           "rx_tone": {"kind": "none"}, "tx_tone": {"kind": "ctcss", "hz_tenths": 1000},
           "scode": 0, "speaker_mute": "Tone/DTCS", "optional_signaling": "OFF",
           "scramble": false, "power": "Low", "bandwidth": "Narrow",
           "busy_lockout": true, "scan_add": false, "ptt_id": "BOT",
           "flags_raw": [0, 0, 1, 4]},
          {"index": 2, "name": "RX ONLY", "rx_frequency_hz": 162550000,
           "tx_frequency_hz": null,
           "rx_tone": {"kind": "dcs", "code": 23, "inverted": false},
           "tx_tone": {"kind": "dcs", "code": 606, "inverted": true},
           "scode": 3, "speaker_mute": "Tone/DTCS or Optsig",
           "optional_signaling": "DTMF", "scramble": true, "power": "High",
           "bandwidth": "Wide", "busy_lockout": false, "scan_add": true,
           "ptt_id": "BOTH", "flags_raw": [3, 2, 16, 4]}
        ],
        "settings": {"tmr": 1, "squelch": 6, "tot": 3, "tot_seconds": 60,
                     "volume": 0, "backlight_timer": 0, "beep": false, "scan_rev": "CO",
                     "remember_power": false},
        "bands": [
          {"index": 0, "programmed": true, "frequency_hz": 462562500,
           "offset_hz": 10000, "shift": "Off",
           "rx_tone": {"kind": "none"}, "tx_tone": {"kind": "none"},
           "scode": 0, "speaker_mute": "Tone/DTCS", "optional_signaling": "OFF",
           "scramble": false, "bandwidth": "Wide", "power": "High",
           "step_hz": 5000},
          {"index": 1, "programmed": false, "frequency_hz": null,
           "offset_hz": null, "shift": "Off",
           "rx_tone": {"kind": "none"}, "tx_tone": {"kind": "none"},
           "scode": 0, "speaker_mute": "Tone/DTCS", "optional_signaling": "OFF",
           "scramble": false, "bandwidth": "Wide", "power": "High",
           "step_hz": null}
        ],
        "static_message": "Model:KT-5000"
      }
    }
    """

    private func decodeEnvelope() throws -> CodeplugDump {
        try JSONDecoder().decode(CodeplugDump.self, from: Data(envelopeJSON.utf8))
    }

    /// All three tone cases arrive as the core's tagged objects, and the "none"
    /// tag maps to `Tone.off` rather than to a missing value.
    func testQytChannelDecodesTaggedTones() throws {
        let qyt = try XCTUnwrap(decodeEnvelope().qyt)

        XCTAssertEqual(qyt.channels[0].rxTone, .off)
        XCTAssertEqual(qyt.channels[0].txTone, .off)
        XCTAssertEqual(qyt.channels[1].txTone, .ctcss(hzTenths: 1000))
        XCTAssertEqual(qyt.channels[2].rxTone, .dcs(code: 23, inverted: false))
        XCTAssertEqual(qyt.channels[2].txTone, .dcs(code: 606, inverted: true))

        // And the labels the table cells show.
        XCTAssertEqual(qyt.channels[0].rxTone.label, "Off")
        XCTAssertEqual(qyt.channels[1].txTone.label, "100.0 Hz")
        XCTAssertEqual(qyt.channels[2].rxTone.label, "D023N")
        XCTAssertEqual(qyt.channels[2].txTone.label, "D606I")
    }

    /// A tone kind the core doesn't emit today must fail loudly rather than
    /// decode as "no tone", which would look like a channel that lost its tone.
    func testAnUnknownToneKindIsRejected() {
        let json = #"{"kind":"nxdn","hz_tenths":885}"#
        XCTAssertThrowsError(try JSONDecoder().decode(Tone.self, from: Data(json.utf8)))
    }

    /// The rest of the channel record: the fields the core decodes for display,
    /// the receive-only case, and the verbatim flag bytes.
    func testQytChannelDecodesTheWholeRecord() throws {
        let qyt = try XCTUnwrap(decodeEnvelope().qyt)

        XCTAssertEqual(qyt.channels.count, 3)
        XCTAssertEqual(qyt.channels[0].name, "FRS-01")
        XCTAssertEqual(qyt.channels[0].rxFrequencyHz, 462_562_500)
        XCTAssertEqual(qyt.channels[0].flagsRaw, [0, 0, 0, 36])
        XCTAssertEqual(qyt.channels[0].flagsRawHex, "00 00 00 24")

        XCTAssertEqual(qyt.channels[1].power, "Low")
        XCTAssertEqual(qyt.channels[1].bandwidth, "Narrow")
        XCTAssertTrue(qyt.channels[1].busyLockout)
        XCTAssertFalse(qyt.channels[1].scanAdd)
        XCTAssertEqual(qyt.channels[1].pttId, "BOT")

        // Receive-only: the core sends an explicit null, not a zero.
        XCTAssertNil(qyt.channels[2].txFrequencyHz)
        XCTAssertEqual(formatOptionalMHz(qyt.channels[2].txFrequencyHz), "—")
        XCTAssertEqual(qyt.channels[2].speakerMute, "Tone/DTCS or Optsig")
        XCTAssertEqual(qyt.channels[2].optionalSignaling, "DTMF")
        XCTAssertEqual(qyt.channels[2].scode, 3)

        XCTAssertEqual(qyt.settings.squelch, 6)
        XCTAssertEqual(qyt.settings.totSeconds, 60)
        XCTAssertFalse(qyt.settings.beep)
        XCTAssertEqual(qyt.settings.scanRev, "CO")
        XCTAssertFalse(qyt.settings.rememberPower)
        XCTAssertEqual(qyt.bands.count, 2)
        XCTAssertEqual(qyt.bands[0].frequencyHz, 462_562_500)
        XCTAssertNil(qyt.bands[1].frequencyHz)
        XCTAssertFalse(qyt.bands[1].programmed)
        XCTAssertEqual(qyt.staticMessage, "Model:KT-5000")
    }

    /// The envelope names the QYT and carries its capabilities, including the
    /// three sections that decide the sidebar.
    func testQytEnvelopeCarriesModelAndCapabilities() throws {
        let envelope = try decodeEnvelope()
        XCTAssertEqual(envelope.model, .qytKt5000)
        XCTAssertEqual(envelope.displayName, "QYT KT-5000")
        XCTAssertNil(envelope.anytone, "a QYT dump carries no AnyTone payload")
        XCTAssertEqual(envelope.capabilities.sections, [.qytChannels, .qytSettings, .qytBands])
        XCTAssertEqual(envelope.capabilities.channelCapacity, 200)
        XCTAssertEqual(envelope.capabilities.channelNameLength, 7)
        XCTAssertFalse(envelope.capabilities.supportsDMR)
        XCTAssertEqual(envelope.capabilities.toneEncoding, "hz_tenths")
        XCTAssertEqual(pickerLabel("Low", in: envelope.capabilities.powerOptions), "Low (5 W)")
    }

    /// An envelope that names the QYT but carries no payload is a core bug, and
    /// must fail the decode rather than open a blank editor.
    func testAQytEnvelopeWithoutItsPayloadIsRejected() {
        let json = """
        {"model": "qyt-kt5000", "display_name": "QYT KT-5000",
         "capabilities": {"sections": ["qyt_channels"], "channel_capacity": 200,
                          "channel_name_length": 7, "supports_dmr": false,
                          "tone_encoding": "hz_tenths",
                          "power_options": [], "bandwidth_options": []}}
        """
        XCTAssertThrowsError(try JSONDecoder().decode(CodeplugDump.self, from: Data(json.utf8)))
    }

    // MARK: - Edit encoding

    private func encoded(_ edit: QytChannelEdit) throws -> [String: Any] {
        let data = try JSONEncoder().encode(edit)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    /// Only what changed is sent; the core reads an absent field as "leave it".
    func testQytChannelEditOmitsUntouchedFields() throws {
        var edit = QytChannelEdit(index: 4)
        edit.name = "REPEAT"
        let object = try encoded(edit)

        XCTAssertEqual(Set(object.keys), ["index", "name"])
        XCTAssertEqual(object["index"] as? Int, 4)
    }

    /// Clearing the TX frequency has to be an explicit JSON null: that is what
    /// distinguishes "make this receive-only" from "don't touch the TX".
    func testClearingTheTxFrequencySendsAnExplicitNull() throws {
        var edit = QytChannelEdit(index: 0)
        edit.clearTxFrequency = true
        let object = try encoded(edit)

        XCTAssertTrue(object.keys.contains("tx_frequency_hz"))
        XCTAssertTrue(object["tx_frequency_hz"] is NSNull)
    }

    /// Scan skip is the one flag the core will write, and it travels under the
    /// core's own key and sense: `scan_add: false` means "skip this channel".
    func testScanAddEncodesUnderTheCoresKeyAndSense() throws {
        var edit = QytChannelEdit(index: 3)
        edit.scanAdd = false
        let object = try encoded(edit)

        XCTAssertEqual(Set(object.keys), ["index", "scan_add"])
        XCTAssertEqual(object["scan_add"] as? Bool, false)

        // An untouched scan flag stays out of the JSON, so applying an unrelated
        // edit never rewrites byte 15.
        var untouched = QytChannelEdit(index: 3)
        untouched.name = "KEEP"
        XCTAssertFalse(try encoded(untouched).keys.contains("scan_add"))
    }

    /// The editor toggle is phrased as "skip", which is the inverse of the
    /// stored bit. Getting that backwards would silently exclude every channel
    /// the user edited from scanning.
    func testScanSkipIsTheInverseOfTheStoredFlag() throws {
        let qyt = try XCTUnwrap(decodeEnvelope().qyt)

        XCTAssertTrue(qyt.channels[0].scanAdd)
        XCTAssertEqual(qyt.channels[0].scanLabel, "Scan")
        XCTAssertFalse(qyt.channels[1].scanAdd)
        XCTAssertEqual(qyt.channels[1].scanLabel, "Skip")
    }

    /// Tones go back in the same tagged shape they arrived in.
    func testToneEncodesTheCoresTaggedShape() throws {
        var edit = QytChannelEdit(index: 0)
        edit.rxTone = .off
        edit.txTone = .ctcss(hzTenths: 885)
        let object = try encoded(edit)

        XCTAssertEqual((object["rx_tone"] as? [String: Any])?["kind"] as? String, "none")
        let tx = try XCTUnwrap(object["tx_tone"] as? [String: Any])
        XCTAssertEqual(tx["kind"] as? String, "ctcss")
        XCTAssertEqual(tx["hz_tenths"] as? Int, 885)

        var dcs = QytChannelEdit()
        dcs.rxTone = .dcs(code: 23, inverted: true)
        let rx = try XCTUnwrap(try encoded(dcs)["rx_tone"] as? [String: Any])
        XCTAssertEqual(rx["kind"] as? String, "dcs")
        XCTAssertEqual(rx["code"] as? Int, 23)
        XCTAssertEqual(rx["inverted"] as? Bool, true)
    }

    /// The batch names the model it is addressed to; the core rejects a QYT
    /// batch sent against an AnyTone file precisely because of this key.
    func testTheEditEnvelopeNamesTheQyt() throws {
        var spec = QytEditSpec()
        spec.removeChannels = [2]
        spec.moveChannels = [MoveOp(from: 0, to: 5)]
        let data = try JSONEncoder().encode(QytEditEnvelope(qyt: spec))
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        let qyt = try XCTUnwrap(object["qyt"] as? [String: Any])

        XCTAssertEqual(qyt["remove_channels"] as? [Int], [2])
        XCTAssertNotNil(qyt["move_channels"])
        XCTAssertNotNil(qyt["add_channels"])
        XCTAssertFalse(spec.isEmpty)
        XCTAssertTrue(QytEditSpec().isEmpty)
    }

    /// The LCD line travels as a bare `static_message` string beside the
    /// settings object, and is left out entirely when it was not touched.
    func testStaticMessageEditUsesTheCoresKey() throws {
        var spec = QytEditSpec()
        spec.staticMessage = "N7FYZ MOBILE"
        let data = try JSONEncoder().encode(QytEditEnvelope(qyt: spec))
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        let qyt = try XCTUnwrap(object["qyt"] as? [String: Any])

        XCTAssertEqual(qyt["static_message"] as? String, "N7FYZ MOBILE")
        XCTAssertFalse(spec.isEmpty, "a message-only batch still has something to apply")

        let untouched = try JSONEncoder().encode(QytEditEnvelope(qyt: QytEditSpec()))
        let bare = try XCTUnwrap(JSONSerialization.jsonObject(with: untouched) as? [String: Any])
        let bareQyt = try XCTUnwrap(bare["qyt"] as? [String: Any])
        XCTAssertNil(bareQyt["static_message"])
    }

    /// The field is 16 bytes of printable ASCII on the radio but the LCD only
    /// draws 13, so the pane filters as you type rather than failing at Apply.
    func testStaticMessageFieldClampsToWhatTheRadioCanStore() {
        XCTAssertEqual(QytSettingsPane.clamp("N7FYZ"), "N7FYZ")
        XCTAssertEqual(QytSettingsPane.clamp("0123456789ABCDEFGHI"), "0123456789ABC")
        XCTAssertEqual(QytSettingsPane.clamp("caf\u{e9} \u{1F4FB}"), "caf ")
        XCTAssertEqual(QytSettingsPane.clamp(""), "")
    }

    /// Settings go out as one object and bands as an array keyed by index, which
    /// is the shape `core/src/qyt/codeplug/edits.rs` deserializes.
    func testSettingsAndBandEditsUseTheCoresKeys() throws {
        var spec = QytEditSpec()
        spec.settings = QytSettingsEdit(squelch: 9, backlightTimer: 5, beep: true,
                                        scanRev: "SE", rememberPower: true)
        spec.bands = [QytBandEdit(index: 1, frequencyHz: 147_320_000, power: "Low")]
        let data = try JSONEncoder().encode(QytEditEnvelope(qyt: spec))
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        let qyt = try XCTUnwrap(object["qyt"] as? [String: Any])

        let settings = try XCTUnwrap(qyt["settings"] as? [String: Any])
        XCTAssertEqual(Set(settings.keys),
                       ["squelch", "backlight_timer", "beep", "scan_rev", "remember_power"],
                       "untouched settings fields are left out entirely")
        XCTAssertEqual(settings["squelch"] as? Int, 9)
        XCTAssertEqual(settings["beep"] as? Bool, true)
        XCTAssertEqual(settings["scan_rev"] as? String, "SE")
        XCTAssertEqual(settings["remember_power"] as? Bool, true)

        let bands = try XCTUnwrap(qyt["bands"] as? [[String: Any]])
        XCTAssertEqual(bands.count, 1)
        XCTAssertEqual(Set(bands[0].keys), ["index", "frequency_hz", "power"])
        XCTAssertEqual(bands[0]["frequency_hz"] as? UInt32, 147_320_000)
    }

    /// An empty settings edit is never sent: it would be a no-op round trip and
    /// a no-op undo entry.
    func testAnEmptySettingsEditIsRecognised() {
        XCTAssertTrue(QytSettingsEdit().isEmpty)
        XCTAssertFalse(QytSettingsEdit(volume: 0).isEmpty)
        XCTAssertTrue(QytBandEdit(index: 1).isEmpty)
        XCTAssertFalse(QytBandEdit(index: 1, bandwidth: "Narrow").isEmpty)
    }
}

/// The pure helpers behind the QYT channels pane and editor, testable without
/// SwiftUI.
final class QytChannelHelperTests: XCTestCase {

    private func channel(index: Int = 0, name: String = "FRS-01",
                         rx: UInt32 = 462_562_500, tx: UInt32? = 462_562_500) -> DumpQytChannel {
        DumpQytChannel(index: index, name: name, rxFrequencyHz: rx, txFrequencyHz: tx,
                       rxTone: .off, txTone: .off)
    }

    /// Same RX and TX under different names flag each other; a different TX,
    /// or receive-only against a TX channel, doesn't.
    func testDuplicateQytChannelSlots() {
        let dupes = duplicateQytChannelSlots([
            channel(index: 0, name: "FRS-01"), channel(index: 3, name: "FAMILY"),
            channel(index: 4, tx: 467_562_500), channel(index: 5, tx: nil),
            channel(index: 6, name: "RX", tx: nil),
        ])
        XCTAssertEqual(dupes, [0: [3], 3: [0], 5: [6], 6: [5]])
    }

    /// A different RX or TX tone makes it a different channel.
    func testQytToneDifferenceIsNotDuplicate() {
        var toned = channel(index: 0), sameTone = channel(index: 1), otherTone = channel(index: 2)
        toned.txTone = .ctcss(hzTenths: 1000)
        sameTone.txTone = .ctcss(hzTenths: 1000)
        otherTone.txTone = .ctcss(hzTenths: 1035)
        var dcs = channel(index: 3)
        dcs.rxTone = .dcs(code: 23, inverted: false)
        XCTAssertEqual(duplicateQytChannelSlots([toned, sameTone, otherTone, dcs]), [0: [1], 1: [0]])
    }

    /// The duplicates-only sort orders by RX, then TX with receive-only first, then slot.
    func testQytFrequencySortOrderGroupsTwins() {
        let sorted = [channel(index: 4), channel(index: 2, tx: nil), channel(index: 1),
                      channel(index: 0, rx: 146_520_000, tx: 146_520_000)]
            .sorted(using: qytChannelFrequencySortOrder)
        XCTAssertEqual(sorted.map(\.index), [0, 2, 1, 4])
    }

    /// The filter matches the name or either frequency, and nothing narrows the
    /// list when the query is empty.
    func testQytChannelMatchesFilter() {
        let ch = channel(name: "N7FYZ", rx: 147_320_000, tx: 147_920_000)

        XCTAssertTrue(qytChannelMatchesFilter(ch, query: ""))
        XCTAssertTrue(qytChannelMatchesFilter(ch, query: "n7f"), "the name match is case-insensitive")
        XCTAssertTrue(qytChannelMatchesFilter(ch, query: "147.32"), "the RX frequency matches")
        XCTAssertTrue(qytChannelMatchesFilter(ch, query: "147.92"), "the TX frequency matches")
        XCTAssertFalse(qytChannelMatchesFilter(ch, query: "146.52"))
        XCTAssertFalse(qytChannelMatchesFilter(ch, query: "simplex"))
    }

    /// A receive-only channel has no TX frequency to match against, and must not
    /// crash or match everything.
    func testFilteringAReceiveOnlyChannel() {
        let ch = channel(name: "WX-01", rx: 162_550_000, tx: nil)
        XCTAssertTrue(qytChannelMatchesFilter(ch, query: "162.55"))
        XCTAssertFalse(qytChannelMatchesFilter(ch, query: "147.32"))
    }

    /// Names are cut to what the radio stores, and the limit comes from the
    /// core's capabilities rather than a constant in the app.
    func testQytChannelNameIsLimitedToSevenCharacters() throws {
        let qyt = try XCTUnwrap(RadioCore.supportedModels.first { $0.id == .qytKt5000 })
        XCTAssertEqual(qyt.capabilities.channelNameLength, 7)

        let limit = qyt.capabilities.channelNameLength
        XCTAssertEqual(limitedName("REPEATER", to: limit), "REPEATE")
        XCTAssertEqual(limitedName("FRS-01", to: limit), "FRS-01", "a short name is untouched")
        XCTAssertEqual(limitedName("", to: limit), "")
        // The AnyTone's longer limit is the same helper with a different bound.
        XCTAssertEqual(limitedName("REPEATER", to: 16), "REPEATER")
    }

    /// A receive-only channel sorts with the other zero-TX rows rather than
    /// breaking the TX column's sort, which `Optional` can't provide.
    func testTxSortKeyGroupsReceiveOnlyChannels() {
        XCTAssertEqual(channel(tx: 147_920_000).txSortKey, 147_920_000)
        XCTAssertEqual(channel(tx: nil).txSortKey, 0)
    }

    /// The MHz field rounds to the 10 Hz resolution the record stores.
    func testHzFromMHzRoundsToTheRecordResolution() {
        XCTAssertEqual(hzFromMHz(146.52), 146_520_000)
        XCTAssertEqual(hzFromMHz(462.56250), 462_562_500)
        XCTAssertEqual(hzFromMHz(-1), 0, "a negative entry clamps rather than wrapping")
    }

    /// Switching a tone picker seeds a usable value of the new kind.
    func testToneKindDefaults() {
        XCTAssertEqual(ToneKind.off.defaultTone, .off)
        XCTAssertEqual(ToneKind.ctcss.defaultTone, .ctcss(hzTenths: 885))
        XCTAssertEqual(ToneKind.dcs.defaultTone, .dcs(code: 23, inverted: false))
        XCTAssertEqual(Tone.ctcss(hzTenths: 1000).kind, .ctcss)
        XCTAssertEqual(Tone.off.kind, .off)
    }

    /// The settings pane sends only what the user changed. This is the whole
    /// safety argument for a pane over a partly-unconfirmed memory map: an
    /// untouched field is never written, not even to the value it already holds.
    func testSettingsEditCarriesOnlyChangedFields() {
        let old = DumpQytSettings(tmr: 1, squelch: 6, tot: 3, totSeconds: 60,
                                  volume: 0, backlightTimer: 0, beep: false, scanRev: "CO",
                                  rememberPower: false)
        var new = old
        XCTAssertTrue(qytSettingsEdit(from: old, to: new).isEmpty)

        new.squelch = 9
        new.beep = true
        new.scanRev = "SE"
        new.rememberPower = true
        let edit = qytSettingsEdit(from: old, to: new)
        XCTAssertEqual(edit.squelch, 9)
        XCTAssertEqual(edit.beep, true)
        XCTAssertEqual(edit.scanRev, "SE")
        XCTAssertEqual(edit.rememberPower, true)
        XCTAssertNil(edit.tmr)
        XCTAssertNil(edit.tot)
        XCTAssertNil(edit.volume)
        XCTAssertNil(edit.backlightTimer)
    }

    /// An out-of-range value the radio itself holds is left alone rather than
    /// clamped into the control's range and written back.
    func testAnUntouchedOutOfRangeSettingIsNotRewritten() {
        // 255 is what an unprogrammed 0xFF byte decodes to, and it is out of
        // range for every one of these lists. scan_rev is nil for the same
        // reason: an unrecognised byte must not be rewritten to TO/CO/SE.
        let old = DumpQytSettings(tmr: 255, squelch: 255, tot: 255, totSeconds: 3840,
                                  volume: 255, backlightTimer: 255, beep: true, scanRev: nil,
                                  rememberPower: true)
        var new = old
        new.volume = 3
        let edit = qytSettingsEdit(from: old, to: new)

        XCTAssertEqual(edit.volume, 3)
        XCTAssertNil(edit.tmr)
        XCTAssertNil(edit.squelch)
        XCTAssertNil(edit.tot)
        XCTAssertNil(edit.backlightTimer)
        XCTAssertNil(edit.beep)
        XCTAssertNil(edit.scanRev)
        XCTAssertNil(edit.rememberPower)
    }

    /// An unprogrammed band shows 0 MHz; leaving it there must not write a
    /// 0 Hz frequency into a record the radio never programmed.
    func testBandEditSkipsAZeroFrequencyAndUnsettableFields() {
        let old = DumpQytBand(index: 1, programmed: false, frequencyHz: nil, offsetHz: nil,
                              shift: "Off", rxTone: .off, txTone: .off, scode: 0,
                              speakerMute: "Tone/DTCS", optionalSignaling: "OFF",
                              scramble: false, bandwidth: "Wide", power: "High", stepHz: nil)
        var new = old
        new.frequencyHz = 0
        XCTAssertTrue(qytBandEdit(from: old, to: new).isEmpty)

        new.frequencyHz = 147_320_000
        new.power = "Low"
        // Read-only in the core: an offset or shift change is never sent.
        new.offsetHz = 600_000
        new.shift = "Minus"
        let edit = qytBandEdit(from: old, to: new)

        XCTAssertEqual(edit.index, 1, "the edit is addressed to the band it came from")
        XCTAssertEqual(edit.frequencyHz, 147_320_000)
        XCTAssertEqual(edit.power, "Low")
        XCTAssertNil(edit.bandwidth)
        XCTAssertNil(edit.stepHz)
    }

    /// The KT-5000 has two VFOs, A and B. The image carries two more slots in
    /// the same shape, but the radio has no letter for them, so they fall back
    /// to their index rather than inventing a C and a D.
    func testBandNamesAreTheRadiosTwoVfoLetters() {
        XCTAssertEqual((0..<2).map(qytBandName), ["A", "B"])
        XCTAssertEqual(qytBandName(2), "2")
        XCTAssertEqual(qytBandName(9), "9")
    }
}

/// Store behavior for a QYT document, driven from a synthetic dump so it needs
/// neither a radio nor a file.
@MainActor
final class QytStoreTests: XCTestCase {

    private func makeQytDump(sections: [CodeplugSection] = [.qytChannels, .qytSettings, .qytBands])
        -> CodeplugDump {
        CodeplugDump(
            model: .qytKt5000,
            capabilities: Capabilities(
                sections: sections,
                channelCapacity: 200,
                channelNameLength: 7,
                supportsDMR: false,
                toneEncoding: "hz_tenths",
                powerOptions: [PickerOption(tag: "High", label: "High (25 W)")],
                bandwidthOptions: [PickerOption(tag: "Wide", label: "Wide")]
            ),
            anytone: nil,
            qyt: QytDump(
                channels: [DumpQytChannel(index: 0, name: "FRS-01",
                                          rxFrequencyHz: 462_562_500,
                                          txFrequencyHz: 462_562_500,
                                          rxTone: .off, txTone: .ctcss(hzTenths: 885))],
                settings: DumpQytSettings(tmr: 1, squelch: 6, tot: 3, totSeconds: 60,
                                          volume: 0, backlightTimer: 0, beep: false,
                                          scanRev: "CO", rememberPower: false),
                bands: [],
                staticMessage: "Model:KT-5000"
            )
        )
    }

    private func makeAnyToneDump() -> CodeplugDump {
        CodeplugDump(
            model: .anyToneD878UV2,
            capabilities: .anyTonePlaceholder,
            anytone: AnyToneDump(
                channels: [DumpChannel(index: 0, name: "Simplex", rxFrequencyHz: 146_520_000,
                                       txFrequencyHz: 146_520_000, mode: "Analog", power: "High",
                                       bandwidth: "Wide", colorCode: 1, timeSlot: 1,
                                       contactIndex: 0, radioIdIndex: 0, groupListIndex: 255)],
                zones: [DumpZone(index: 0, name: "Local", channels: [0])],
                contacts: [],
                groupLists: [],
                radioIds: []
            )
        )
    }

    /// A QYT document's sidebar is its three sections, in the core's order.
    func testSectionsForQytAreChannelsSettingsBands() {
        let store = CodeplugStore()
        store.load(makeQytDump(), url: URL(fileURLWithPath: "/tmp/kt5000.bin"))

        XCTAssertEqual(store.model, .qytKt5000)
        XCTAssertEqual(store.sections, [.qytChannels, .qytSettings, .qytBands])
        XCTAssertFalse(store.capabilities.supportsDMR)
        XCTAssertEqual(store.qytChannels.count, 1)
        XCTAssertEqual(store.qytChannels.first?.txTone, .ctcss(hzTenths: 885))
        XCTAssertEqual(store.qytSettings?.squelch, 6)
    }

    /// One document is open at a time, so opening one radio's file must leave
    /// none of the other radio's records on screen.
    func testOpeningOneModelClearsTheOtherModelsPayload() {
        let store = CodeplugStore()
        let url = URL(fileURLWithPath: "/tmp/codeplug.bin")

        store.load(makeAnyToneDump(), url: url)
        XCTAssertEqual(store.channels.count, 1)
        XCTAssertTrue(store.qytChannels.isEmpty)

        store.load(makeQytDump(), url: url)
        XCTAssertEqual(store.qytChannels.count, 1)
        XCTAssertTrue(store.channels.isEmpty, "the AnyTone channels are gone")
        XCTAssertTrue(store.zones.isEmpty)
        XCTAssertNotNil(store.qytSettings)

        store.load(makeAnyToneDump(), url: url)
        XCTAssertEqual(store.channels.count, 1)
        XCTAssertTrue(store.qytChannels.isEmpty, "and back the other way")
        XCTAssertNil(store.qytSettings)
    }
}

/// End-to-end save-model tests for a QYT document, driving the real Rust core
/// against a real file. Same discipline as `SaveModelTests`: SHA-256 before and
/// after, because "nothing touches your file until Save" is a claim about bytes
/// on disk.
@MainActor
final class QytSaveModelTests: XCTestCase {

    private var tempDir: URL!
    private var codeplug: URL!

    /// Image size of the KT-5000, `0x3200`.
    private static let imageSize = 12_800

    /// A blank KT-5000 image: all `0xFF` with the model tag at `0x0FE0`.
    ///
    /// The fill byte matters. `0xFF` is what an unprogrammed slot holds, so this
    /// parses as a radio with zero channels. An all-*zero* image of the same
    /// length would parse as 200 perfectly valid 0 Hz channels, which is the
    /// fixture trap this radio sets and the reason it is spelled out here.
    private static func blankImage() -> Data {
        var bytes = [UInt8](repeating: 0xFF, count: imageSize)
        let tag = Array("Model:KT-5000   ".utf8)
        bytes.replaceSubrange(0x0FE0..<(0x0FE0 + tag.count), with: tag)
        return Data(bytes)
    }

    override func setUpWithError() throws {
        try super.setUpWithError()
        tempDir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("qyt-save-tests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)

        setenv("CODEPLUGME_RECOVERY_DIR", tempDir.appendingPathComponent("recovery").path, 1)
        Recovery.clear()

        codeplug = tempDir.appendingPathComponent("kt5000.bin")
        try Self.blankImage().write(to: codeplug)
    }

    override func tearDownWithError() throws {
        Recovery.clear()
        unsetenv("CODEPLUGME_RECOVERY_DIR")
        try? FileManager.default.removeItem(at: tempDir)
        try super.tearDownWithError()
    }

    private func sha(_ url: URL) throws -> String {
        SHA256.hash(data: try Data(contentsOf: url)).map { String(format: "%02x", $0) }.joined()
    }

    /// The fixture really is a blank radio, not 200 phantom channels.
    func testTheBlankFixtureOpensAsAQytWithNoChannels() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)

        XCTAssertNil(store.errorMessage)
        XCTAssertEqual(store.model, .qytKt5000)
        XCTAssertEqual(store.qytChannels.count, 0, "0xFF slots are empty, not zero-Hz channels")
        XCTAssertEqual(store.qytBands.count, 2)
        XCTAssertEqual(store.sections, [.qytChannels, .qytSettings, .qytBands])
        XCTAssertFalse(store.isDirty)
    }

    /// The whole point of the work file: adding a channel stages, Save commits.
    func testAddingAQytChannelStagesThenPersists() throws {
        let before = try sha(codeplug)
        let store = CodeplugStore()
        store.open(url: codeplug)

        let slot = try XCTUnwrap(store.addQytChannel())

        XCTAssertEqual(slot, 0, "the core fills the first free slot")
        XCTAssertTrue(store.isDirty)
        XCTAssertEqual(store.qytChannels.count, 1)
        XCTAssertTrue(store.isUnsaved(.qytChannels, slot: 0))
        XCTAssertEqual(try sha(codeplug), before, "the document is byte-identical until Save")

        store.save()

        XCTAssertNil(store.errorMessage)
        XCTAssertFalse(store.isDirty)
        XCTAssertNotEqual(try sha(codeplug), before, "Save must write the document")

        let reopened = CodeplugStore()
        reopened.open(url: codeplug)
        XCTAssertEqual(reopened.qytChannels.count, 1)
        XCTAssertEqual(reopened.qytChannels.first?.name, "NEW")
        XCTAssertFalse(reopened.isDirty)
    }

    /// A field edit goes out through the tagged envelope and comes back through
    /// the dump, tones included.
    func testEditingAQytChannelStagesThenPersists() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addQytChannel()
        let before = try sha(codeplug)

        var channel = try XCTUnwrap(store.qytChannels.first)
        channel.name = "N7FYZ"
        channel.rxFrequencyHz = 147_320_000
        channel.txFrequencyHz = 147_920_000
        channel.txTone = .ctcss(hzTenths: 1000)
        channel.rxTone = .dcs(code: 23, inverted: false)
        store.update(channel)

        XCTAssertNil(store.errorMessage)
        XCTAssertEqual(store.qytChannels.first?.name, "N7FYZ")
        XCTAssertEqual(try sha(codeplug), before, "a field edit must not write the document either")

        store.save()
        let reopened = CodeplugStore()
        reopened.open(url: codeplug)
        let saved = try XCTUnwrap(reopened.qytChannels.first)
        XCTAssertEqual(saved.name, "N7FYZ")
        XCTAssertEqual(saved.rxFrequencyHz, 147_320_000)
        XCTAssertEqual(saved.txFrequencyHz, 147_920_000)
        XCTAssertEqual(saved.txTone, .ctcss(hzTenths: 1000))
        XCTAssertEqual(saved.rxTone, .dcs(code: 23, inverted: false))
    }

    /// Scan skip survives the whole round trip: editor draft → tagged envelope →
    /// core setter → saved bytes → reopened dump. A new channel starts in the
    /// scan list, so this exercises both directions.
    func testScanSkipStagesPersistsAndComesBack() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addQytChannel()
        XCTAssertEqual(store.qytChannels.first?.scanAdd, true, "a new channel scans by default")

        var channel = try XCTUnwrap(store.qytChannels.first)
        channel.scanAdd = false
        store.update(channel)

        XCTAssertNil(store.errorMessage)
        XCTAssertEqual(store.qytChannels.first?.scanAdd, false)

        store.save()
        let reopened = CodeplugStore()
        reopened.open(url: codeplug)
        let saved = try XCTUnwrap(reopened.qytChannels.first)
        XCTAssertFalse(saved.scanAdd)
        // The other flag bits came back untouched: only byte 15 bit 2 moved.
        XCTAssertEqual(saved.flagsRaw[3] & ~UInt8(0x04), 0x00)
        XCTAssertEqual(saved.power, "High")
        XCTAssertFalse(saved.busyLockout)

        // And back into the scan list again.
        var again = saved
        again.scanAdd = true
        reopened.update(again)
        XCTAssertNil(reopened.errorMessage)
        XCTAssertEqual(reopened.qytChannels.first?.scanAdd, true)
    }

    /// Bulk scan-skip marks every touched slot, not just the first.
    func testBulkScanSkipMarksEveryTouchedChannel() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        let a = try XCTUnwrap(store.addQytChannel())
        let b = try XCTUnwrap(store.addQytChannel())
        store.save()

        store.bulkUpdateQytChannels(BulkQytChannelUpdate(indices: [a, b], scanAdd: false))

        XCTAssertNil(store.errorMessage)
        XCTAssertTrue(store.isUnsaved(.qytChannels, slot: a))
        XCTAssertTrue(store.isUnsaved(.qytChannels, slot: b))
        XCTAssertEqual(store.qytChannels.first(where: { $0.index == a })?.scanAdd, false)
        XCTAssertEqual(store.qytChannels.first(where: { $0.index == b })?.scanAdd, false)
    }

    /// Bulk scan-skip stages through the tagged envelope and survives a save.
    func testBulkScanSkipStagesThenPersists() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        let a = try XCTUnwrap(store.addQytChannel())
        let b = try XCTUnwrap(store.addQytChannel())
        let before = try sha(codeplug)

        store.bulkUpdateQytChannels(BulkQytChannelUpdate(indices: [a, b], scanAdd: false))

        XCTAssertNil(store.errorMessage)
        XCTAssertEqual(store.qytChannels.first(where: { $0.index == a })?.scanAdd, false)
        XCTAssertEqual(store.qytChannels.first(where: { $0.index == b })?.scanAdd, false)
        XCTAssertEqual(try sha(codeplug), before, "bulk edit must not write the document")

        store.save()
        let reopened = CodeplugStore()
        reopened.open(url: codeplug)
        XCTAssertEqual(reopened.qytChannels.first(where: { $0.index == a })?.scanAdd, false)
        XCTAssertEqual(reopened.qytChannels.first(where: { $0.index == b })?.scanAdd, false)
        // Other flag bits stay untouched on both channels.
        for ch in reopened.qytChannels where [a, b].contains(ch.index) {
            XCTAssertEqual(ch.flagsRaw[3] & ~UInt8(0x04), 0x00)
            XCTAssertEqual(ch.power, "High")
        }
    }

    /// Clearing the TX frequency makes the channel receive-only, which needs the
    /// explicit-null edit to reach the core intact.
    func testClearingTheTxFrequencyMakesTheChannelReceiveOnly() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addQytChannel()

        var channel = try XCTUnwrap(store.qytChannels.first)
        channel.txFrequencyHz = nil
        store.update(channel)

        XCTAssertNil(store.errorMessage)
        XCTAssertNil(store.qytChannels.first?.txFrequencyHz)

        store.save()
        let reopened = CodeplugStore()
        reopened.open(url: codeplug)
        XCTAssertNil(reopened.qytChannels.first?.txFrequencyHz)
    }

    /// Remove and move go through the same staging path, so undo covers them.
    func testRemovingAndMovingAQytChannel() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addQytChannel()
        _ = store.addQytChannel()
        XCTAssertEqual(store.qytChannels.map(\.index), [0, 1])

        store.moveQytChannel(1, to: 7)
        XCTAssertNil(store.errorMessage)
        XCTAssertEqual(store.qytChannels.map(\.index), [0, 7])
        XCTAssertTrue(store.isUnsaved(.qytChannels, slot: 7))

        store.removeQytChannel(0)
        XCTAssertEqual(store.qytChannels.map(\.index), [7])

        store.undo()
        XCTAssertEqual(store.qytChannels.map(\.index), [0, 7], "undo brings the channel back")
    }

    /// An edit rejected by the core leaves the staged state alone and reports
    /// why. A CTCSS tone below 60 Hz is one the radio cannot store.
    func testAnImpossibleToneIsRejectedWithoutStaging() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addQytChannel()
        store.save()

        var channel = try XCTUnwrap(store.qytChannels.first)
        channel.txTone = .ctcss(hzTenths: 100)
        store.update(channel)

        XCTAssertNotNil(store.errorMessage, "the core must refuse a 10.0 Hz CTCSS tone")
        XCTAssertEqual(store.qytChannels.first?.txTone, .off, "and the record is unchanged")
        XCTAssertFalse(store.isDirty)
    }

    /// Opening a QYT file after an AnyTone one swaps the whole document, through
    /// the real detect-by-bytes path rather than a synthetic dump.
    func testOpeningAQytFileAfterAnAnyToneFileSwapsThePayload() throws {
        let anytone = tempDir.appendingPathComponent("anytone.bin")
        try Data(count: RadioCore.expectedCodeplugSize).write(to: anytone)

        let store = CodeplugStore()
        store.open(url: anytone)
        XCTAssertEqual(store.model, .anyToneD878UV2)
        _ = store.addChannel()
        XCTAssertEqual(store.channels.count, 1)

        store.open(url: codeplug)

        XCTAssertNil(store.errorMessage)
        XCTAssertEqual(store.model, .qytKt5000)
        XCTAssertTrue(store.channels.isEmpty, "the AnyTone payload is cleared")
        XCTAssertTrue(store.zones.isEmpty)
        XCTAssertEqual(store.qytChannels.count, 0)
        XCTAssertEqual(store.qytBands.count, 2)

        store.open(url: anytone)
        XCTAssertEqual(store.model, .anyToneD878UV2)
        XCTAssertTrue(store.qytChannels.isEmpty, "and the QYT payload is cleared going back")
        XCTAssertNil(store.qytSettings)
    }

    /// A settings change stages against the work file and reaches disk only on
    /// Save, the same contract the channel edits keep.
    func testApplyingSettingsStagesAndPersists() throws {
        let before = try sha(codeplug)
        let store = CodeplugStore()
        store.open(url: codeplug)

        var draft = try XCTUnwrap(store.qytSettings)
        draft.squelch = 4
        draft.tot = 3
        store.updateQytSettings(draft, staticMessage: store.qytStaticMessage)

        XCTAssertNil(store.errorMessage)
        XCTAssertTrue(store.isDirty)
        XCTAssertEqual(store.qytSettings?.squelch, 4)
        XCTAssertEqual(store.qytSettings?.totSeconds, 60, "the core derives the seconds")
        XCTAssertTrue(store.hasUnsavedChanges(.qytSettings))
        XCTAssertEqual(try sha(codeplug), before, "the document is byte-identical until Save")

        store.save()

        XCTAssertNil(store.errorMessage)
        XCTAssertFalse(store.isDirty)
        XCTAssertNotEqual(try sha(codeplug), before, "Save must write the document")

        let reopened = CodeplugStore()
        reopened.open(url: codeplug)
        XCTAssertEqual(reopened.qytSettings?.squelch, 4)
        XCTAssertEqual(reopened.qytSettings?.tot, 3)
    }

    /// Undo has to reach the pane, not just the document. The pane keeps a local
    /// draft and re-seeds it from `store.qytSettings` whenever that changes, so
    /// this drives the same sequence: seed, edit, apply, re-seed, undo, re-seed.
    func testUndoRestoresTheSettingsDraft() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)

        let original = try XCTUnwrap(store.qytSettings)
        var draft = original
        draft.squelch = 7
        store.updateQytSettings(draft, staticMessage: store.qytStaticMessage)
        draft = try XCTUnwrap(store.qytSettings)
        XCTAssertEqual(draft.squelch, 7)

        store.undo()

        draft = try XCTUnwrap(store.qytSettings)
        XCTAssertEqual(draft, original, "undo re-seeds the pane's draft from the document")
        XCTAssertTrue(qytSettingsEdit(from: original, to: draft).isEmpty,
                      "so a stale draft can't re-apply the undone change")
    }

    /// The safety property behind a deliberately narrow settings pane: editing a
    /// modelled field leaves every unmodelled byte from `0x0E36` to `0x0FFF`
    /// exactly as the radio wrote it, VFO slots and static message included.
    func testUnmodelledSettingsBytesSurviveAnEdit() throws {
        let original = try Data(contentsOf: codeplug)
        let store = CodeplugStore()
        store.open(url: codeplug)

        var draft = try XCTUnwrap(store.qytSettings)
        draft.squelch = 2
        draft.volume = 5
        draft.backlightTimer = 10
        draft.beep = true
        draft.scanRev = "SE"
        draft.rememberPower = true
        store.updateQytSettings(draft, staticMessage: store.qytStaticMessage)
        store.save()

        XCTAssertNil(store.errorMessage)
        let saved = try Data(contentsOf: codeplug)
        let region = 0x0E36..<0x1000
        XCTAssertEqual(saved[region], original[region],
                       "the unmodelled settings region must be byte-identical")
        XCTAssertNotEqual(saved[0x0E00..<0x0E36], original[0x0E00..<0x0E36],
                          "and the modelled header really did change")
    }

    /// The display message reaches the image as 16 space-padded bytes at
    /// `0x0FE0`, and nothing else in the file moves with it.
    func testStaticMessageEditWritesOnlyItsOwnSixteenBytes() throws {
        let original = try Data(contentsOf: codeplug)
        let store = CodeplugStore()
        store.open(url: codeplug)

        let settings = try XCTUnwrap(store.qytSettings)
        store.updateQytSettings(settings, staticMessage: "N7FYZ MOBILE")
        store.save()

        XCTAssertNil(store.errorMessage)
        let saved = try Data(contentsOf: codeplug)
        let field = 0x0FE0..<0x0FF0
        XCTAssertEqual(String(decoding: saved[field], as: UTF8.self), "N7FYZ MOBILE    ")
        XCTAssertEqual(saved[..<field.lowerBound], original[..<field.lowerBound])
        XCTAssertEqual(saved[field.upperBound...], original[field.upperBound...])
        XCTAssertEqual(store.qytStaticMessage, "N7FYZ MOBILE",
                       "and the reloaded document shows the trimmed line back")
    }

    /// A VFO frequency is stored one decimal digit per byte, not as the packed
    /// BCD the channel table uses. The core owns that codec; this checks the
    /// value the pane sends comes back as the value the pane sent.
    func testBandFrequencyRoundTrips() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)

        var draft = store.qytBands
        XCTAssertEqual(draft.count, 2)
        XCTAssertNil(draft[0].frequencyHz, "the blank fixture programs no VFO")
        draft[0].frequencyHz = 147_320_000
        draft[1].frequencyHz = 462_562_500
        store.updateQytBands(draft)

        XCTAssertNil(store.errorMessage)
        XCTAssertEqual(store.qytBands[0].frequencyHz, 147_320_000)
        XCTAssertEqual(store.qytBands[1].frequencyHz, 462_562_500)
        XCTAssertTrue(store.isUnsaved(.qytBands, slot: 1))
        store.save()

        let reopened = CodeplugStore()
        reopened.open(url: codeplug)
        XCTAssertEqual(reopened.qytBands[0].frequencyHz, 147_320_000)
        XCTAssertEqual(reopened.qytBands[1].frequencyHz, 462_562_500)
    }

    /// A QYT image is a codeplug this build can open, and the size gate says so.
    func testAQytImageIsCompatible() throws {
        let store = CodeplugStore()
        XCTAssertNil(store.incompatibilityReason(for: codeplug))
        XCTAssertEqual(try RadioCore.detectModel(binPath: codeplug.path), .qytKt5000)
        XCTAssertEqual(try RadioCore.codeplugSize(for: .qytKt5000), Self.imageSize)
    }
}
