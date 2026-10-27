import SwiftUI

/// Editor for one QYT KT-5000 channel.
///
/// Sectioned the same way `ChannelEditorSheet` is — Frequency, Channel, Analog,
/// Flags — so the two radios' sheets read as the same shape with the parts this
/// one doesn't have removed.
///
/// Scan is editable because its bit (byte 15 bit 2) was confirmed against a real
/// KT-5000 and the core has a setter for it. The rest of the Flags section is
/// read-only: power, bandwidth, BCL, PTT-ID, scrambler, optional signalling and
/// speaker mute are decoded out of the same flag bytes, but their assignments
/// are still CHIRP-derived, so a live control would take the user's change and
/// silently drop it on the next reload.
struct QytChannelEditorSheet: View {
    /// Longest name the radio can store, from `capabilities.channel_name_length`.
    let nameLimit: Int
    /// Power and bandwidth labels, from the core's capabilities rather than a
    /// table duplicated here.
    let powerOptions: [PickerOption]
    let bandwidthOptions: [PickerOption]
    let onCommit: (DumpQytChannel) -> Void

    @State private var draft: DumpQytChannel
    /// Whether the read-only flag rows are showing. Sheet-local, so it resets
    /// with each channel.
    @State private var flagsExpanded = false
    @Environment(\.dismiss) private var dismiss

    init(channel: DumpQytChannel, nameLimit: Int,
         powerOptions: [PickerOption], bandwidthOptions: [PickerOption],
         onCommit: @escaping (DumpQytChannel) -> Void) {
        self.nameLimit = nameLimit
        self.powerOptions = powerOptions
        self.bandwidthOptions = bandwidthOptions
        self.onCommit = onCommit
        _draft = State(initialValue: channel)
    }

    var body: some View {
        EditorSheet(title: "Channel \(formatSlot(draft.index))",
                    onCancel: { dismiss() },
                    onDone: { onCommit(draft); dismiss() }) {
            StackedField(label: "Name") {
                TextField("Name", text: $draft.name)
                Text("Up to \(nameLimit) characters.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            // Truncate as the user types rather than at commit: the radio stores
            // seven characters, and silently losing the tail at Done would be a
            // surprise.
            .onChange(of: draft.name) { name in
                draft.name = limitedName(name, to: nameLimit)
            }

            Form {
                Section("Frequency") {
                    TextField("Receive (MHz)", value: rxMHz, format: mhzFormat)
                    TextField("Transmit (MHz)", value: txMHz, format: mhzFormat)
                        .disabled(draft.txFrequencyHz == nil)
                    Toggle("Receive only (no transmit)", isOn: receiveOnly)
                }

                Section("Analog") {
                    QytToneField(label: "RX tone", tone: $draft.rxTone)
                    QytToneField(label: "TX tone", tone: $draft.txTone)
                }

                Section("Scan") {
                    // Phrased as the radio's own menu does, so "on" is the
                    // exceptional state the user went looking for. The stored
                    // bit is the opposite sense (`scan_add`), inverted here
                    // rather than in the model, which follows the core.
                    Toggle("Skip when scanning", isOn: scanSkip)
                    Text("A skipped channel stays programmed but the scanner passes over it.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                Section("Flags") {
                    // Collapsed by default. Nine read-only rows made this sheet
                    // taller than a laptop screen, and none of them is something
                    // the user came here to change.
                    DisclosureGroup(isExpanded: $flagsExpanded) {
                        LabeledContent("Power", value: pickerLabel(draft.power, in: powerOptions))
                        LabeledContent("Bandwidth",
                                       value: pickerLabel(draft.bandwidth, in: bandwidthOptions))
                        LabeledContent("Busy channel lockout", value: yesNo(draft.busyLockout))
                        LabeledContent("Scrambler", value: yesNo(draft.scramble))
                        LabeledContent("PTT-ID", value: draft.pttId)
                        LabeledContent("Optional signalling", value: draft.optionalSignaling)
                        LabeledContent("Speaker mute", value: draft.speakerMute)
                        LabeledContent("Signal code", value: String(draft.scode))
                        LabeledContent("Flag bytes", value: draft.flagsRawHex)
                            .monospaced()
                        Text("These come from the record's flag bytes, which round-trip "
                            + "untouched. They stay read-only until their bit assignments "
                            + "are confirmed against the radio, which scan already is.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    } label: {
                        Text("Radio flags (read-only)")
                    }
                }
            }
            .formStyle(.grouped)
        }
    }

    /// "Yes" / "No" for a read-only boolean row.
    private func yesNo(_ value: Bool) -> String { value ? "Yes" : "No" }

    /// The RX frequency as an editable MHz value.
    private var rxMHz: Binding<Double> {
        Binding(get: { Double(draft.rxFrequencyHz) / 1_000_000 },
                set: { draft.rxFrequencyHz = hzFromMHz($0) })
    }

    /// The TX frequency as an editable MHz value. A receive-only channel has
    /// none, and the field is disabled in that case rather than showing 0.
    private var txMHz: Binding<Double> {
        Binding(get: { Double(draft.txFrequencyHz ?? 0) / 1_000_000 },
                set: { draft.txFrequencyHz = hzFromMHz($0) })
    }

    /// Scan skip, which is the stored `scanAdd` bit read the other way round.
    private var scanSkip: Binding<Bool> {
        Binding(get: { !draft.scanAdd }, set: { draft.scanAdd = !$0 })
    }

    /// Clearing the TX frequency is what makes the channel receive-only; turning
    /// it back on seeds TX from RX, which is the simplex case and the sane
    /// starting point.
    private var receiveOnly: Binding<Bool> {
        Binding(get: { draft.txFrequencyHz == nil },
                set: { draft.txFrequencyHz = $0 ? nil : draft.rxFrequencyHz })
    }
}

/// Editor for one sub-audible tone: the kind, then whatever that kind needs.
///
/// A DCS code is typed rather than picked from a list because the valid codes
/// are the core's table and the capabilities payload doesn't publish it;
/// duplicating 105 codes in Swift to save one round trip would be a second
/// source of truth. An unknown code comes back as a clear error from the core.
struct QytToneField: View {
    let label: String
    @Binding var tone: Tone

    var body: some View {
        Picker(label, selection: kind) {
            ForEach(ToneKind.allCases) { Text($0.label).tag($0) }
        }
        if case .ctcss = tone {
            TextField("\(label) frequency (Hz)", value: ctcssHz,
                      format: .number.precision(.fractionLength(1)).grouping(.never))
            Text("60.1 Hz and up; the radio stores tenths of a hertz.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        if case .dcs(_, let inverted) = tone {
            TextField("\(label) DCS code", value: dcsCode, format: .number.grouping(.never))
            Toggle("Inverted", isOn: dcsInverted)
                .help(inverted ? "Reversed polarity" : "Normal polarity")
        }
    }

    /// Switching kinds seeds a representative value for the new kind. Guarded
    /// against a same-value write so re-selecting the current kind doesn't throw
    /// away what the user typed.
    private var kind: Binding<ToneKind> {
        Binding(get: { tone.kind },
                set: { if $0 != tone.kind { tone = $0.defaultTone } })
    }

    /// The CTCSS frequency in whole hertz, stored as tenths.
    private var ctcssHz: Binding<Double> {
        Binding(
            get: {
                guard case .ctcss(let hzTenths) = tone else { return 0 }
                return Double(hzTenths) / 10
            },
            set: { hz in
                let clamped = min(max(hz, 0), 6_553.5)
                tone = .ctcss(hzTenths: UInt16((clamped * 10).rounded()))
            }
        )
    }

    private var dcsCode: Binding<Int> {
        Binding(
            get: {
                guard case .dcs(let code, _) = tone else { return 0 }
                return Int(code)
            },
            set: { code in
                guard case .dcs(_, let inverted) = tone else { return }
                tone = .dcs(code: UInt16(min(max(code, 0), 999)), inverted: inverted)
            }
        )
    }

    private var dcsInverted: Binding<Bool> {
        Binding(
            get: {
                guard case .dcs(_, let inverted) = tone else { return false }
                return inverted
            },
            set: { inverted in
                guard case .dcs(let code, _) = tone else { return }
                tone = .dcs(code: code, inverted: inverted)
            }
        )
    }
}
