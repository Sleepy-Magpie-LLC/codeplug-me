import SwiftUI

/// The set of field changes to apply across multiple channels. `nil` means
/// "leave unchanged". The `indices` array lists which channel slots are affected.
struct BulkChannelUpdate {
    var indices: [Int]
    var mode: String?
    var power: String?
    var bandwidth: String?
    var colorCode: Int?
    var timeSlot: Int?
    var contactIndex: UInt32?
    var groupListIndex: Int?
    var radioIdIndex: Int?
}

/// Bulk editor sheet: lets the user pick which fields to change and what value
/// to set, then applies the update to every selected channel at once.
struct BulkChannelEditorSheet: View {
    let channels: [DumpChannel]
    let contacts: [DumpContact]
    let groupLists: [DumpGroupList]
    let radioIds: [DumpRadioID]
    let onCommit: (BulkChannelUpdate) -> Void

    // Which fields the user has opted to change.
    @State private var changeMode = false
    @State private var changePower = false
    @State private var changeBandwidth = false
    @State private var changeColorCode = false
    @State private var changeTimeSlot = false
    @State private var changeContact = false
    @State private var changeGroupList = false
    @State private var changeRadioId = false

    // Draft values for each field.
    @State private var draftMode = "Digital"
    @State private var draftPower = "High"
    @State private var draftBandwidth = "Wide"
    @State private var draftColorCode = 1
    @State private var draftTimeSlot = 1
    @State private var draftContactIndex: UInt32 = 0
    @State private var draftGroupListIndex = 255 // 255 = None
    @State private var draftRadioIdIndex = 0

    @Environment(\.dismiss) private var dismiss

    /// 0xff in the group-list index means "no RX group list".
    private static let groupListNone = 255

    var body: some View {
        EditorSheet(title: "Bulk Edit \(channels.count) Channels",
                    onCancel: { dismiss() },
                    onDone: { commit() }) {
            Form {
                // Mode
                bulkFieldToggle("Mode", isOn: $changeMode) {
                    Picker("Mode", selection: $draftMode) {
                        ForEach(channelModeOptions, id: \.tag) { Text($0.label).tag($0.tag) }
                    }
                    .labelsHidden()
                    .disabled(!changeMode)
                }

                // Power
                bulkFieldToggle("Power", isOn: $changePower) {
                    Picker("Power", selection: $draftPower) {
                        ForEach(channelPowerOptions, id: \.tag) { Text($0.label).tag($0.tag) }
                    }
                    .labelsHidden()
                    .disabled(!changePower)
                }

                // Bandwidth
                bulkFieldToggle("Bandwidth", isOn: $changeBandwidth) {
                    Picker("Bandwidth", selection: $draftBandwidth) {
                        ForEach(channelBandwidthOptions, id: \.tag) { Text($0.label).tag($0.tag) }
                    }
                    .labelsHidden()
                    .disabled(!changeBandwidth)
                }

                // Color code
                bulkFieldToggle("Color code", isOn: $changeColorCode) {
                    Stepper("Color code: \(String(draftColorCode))",
                            value: $draftColorCode, in: 0...15)
                        .labelsHidden()
                        .disabled(!changeColorCode)
                }

                // Time slot
                bulkFieldToggle("Time slot", isOn: $changeTimeSlot) {
                    Picker("Time slot", selection: $draftTimeSlot) {
                        Text("1").tag(1)
                        Text("2").tag(2)
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                    .disabled(!changeTimeSlot)
                }

                // Contact
                bulkFieldToggle("Contact", isOn: $changeContact) {
                    Picker("Contact", selection: $draftContactIndex) {
                        ForEach(contacts) { c in
                            Text(verbatim: "\(c.name) (\(formatDMRID(c.number)))").tag(UInt32(c.index))
                        }
                    }
                    .labelsHidden()
                    .disabled(!changeContact || contacts.isEmpty)
                }

                // RX group list
                bulkFieldToggle("RX group list", isOn: $changeGroupList) {
                    Picker("RX group list", selection: $draftGroupListIndex) {
                        Text("None").tag(Self.groupListNone)
                        ForEach(groupLists) { g in Text(g.name).tag(g.index) }
                    }
                    .labelsHidden()
                    .disabled(!changeGroupList)
                }

                // Radio ID
                bulkFieldToggle("Radio ID", isOn: $changeRadioId) {
                    Picker("Radio ID", selection: $draftRadioIdIndex) {
                        ForEach(radioIds) { r in
                            Text(verbatim: "\(r.name) (\(formatDMRID(r.number)))").tag(r.index)
                        }
                    }
                    .labelsHidden()
                    .disabled(!changeRadioId || radioIds.isEmpty)
                }
            }
            .formStyle(.grouped)

            Text("Only checked fields will be updated. Unchecked fields keep "
                + "their current per-channel values.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .padding(.top, Spacing.tight)
        }
    }

    /// A row with a toggle, a label, and a control. The toggle gates whether the
    /// field will be included in the bulk update.
    private func bulkFieldToggle<C: View>(_ label: String, isOn: Binding<Bool>,
                                          @ViewBuilder control: () -> C) -> some View {
        HStack(spacing: Spacing.inline) {
            Toggle(isOn: isOn) {
                Text(label)
                    .frame(width: 100, alignment: .leading)
            }
            .toggleStyle(.checkbox)
            control()
        }
    }

    /// Build the update from the toggled-on fields and hand it to the caller.
    private func commit() {
        let hasAnyChange = changeMode || changePower || changeBandwidth
            || changeColorCode || changeTimeSlot || changeContact
            || changeGroupList || changeRadioId
        guard hasAnyChange else { dismiss(); return }

        var update = BulkChannelUpdate(indices: channels.map(\.index))
        if changeMode { update.mode = draftMode }
        if changePower { update.power = draftPower }
        if changeBandwidth { update.bandwidth = draftBandwidth }
        if changeColorCode { update.colorCode = draftColorCode }
        if changeTimeSlot { update.timeSlot = draftTimeSlot }
        if changeContact { update.contactIndex = draftContactIndex }
        if changeGroupList { update.groupListIndex = draftGroupListIndex }
        if changeRadioId { update.radioIdIndex = draftRadioIdIndex }

        onCommit(update)
        dismiss()
    }
}

/// Field changes to apply across multiple QYT channels. `nil` means leave
/// unchanged. Only writable shared settings belong here; name, frequencies and
/// tones stay single-channel edits, and the unconfirmed flag bits stay out.
struct BulkQytChannelUpdate {
    var indices: [Int]
    /// Core sense: `true` includes the channel in scan, `false` skips it.
    var scanAdd: Bool? = nil
}

/// Bulk editor for QYT KT-5000 channels. Same opt-in checkbox pattern as the
/// AnyTone sheet, scoped to scan skip (the one confirmed writable shared flag).
struct BulkQytChannelEditorSheet: View {
    let channels: [DumpQytChannel]
    let onCommit: (BulkQytChannelUpdate) -> Void

    @State private var changeScanSkip = false
    /// UI sense: on means skip when scanning. Mapped to `scanAdd = !draftSkip`
    /// on commit so the store/core keep their stored bit polarity.
    @State private var draftSkip = true

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        EditorSheet(title: "Bulk Edit \(channels.count) Channels",
                    onCancel: { dismiss() },
                    onDone: { commit() }) {
            Form {
                HStack(spacing: Spacing.inline) {
                    Toggle(isOn: $changeScanSkip) {
                        Text("Skip when scanning")
                            .frame(width: 140, alignment: .leading)
                    }
                    .toggleStyle(.checkbox)
                    Toggle("", isOn: $draftSkip)
                        .labelsHidden()
                        .disabled(!changeScanSkip)
                }
            }
            .formStyle(.grouped)

            Text("Only checked fields will be updated. Unchecked fields keep "
                + "their current per-channel values.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .padding(.top, Spacing.tight)
        }
    }

    /// Build the update from the toggled-on fields and hand it to the caller.
    private func commit() {
        guard changeScanSkip else { dismiss(); return }

        var update = BulkQytChannelUpdate(indices: channels.map(\.index))
        // Invert: the toggle is "skip", the core stores "add to scan".
        update.scanAdd = !draftSkip

        onCommit(update)
        dismiss()
    }
}
