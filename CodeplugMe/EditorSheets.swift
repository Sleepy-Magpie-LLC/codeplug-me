import AppKit
import SwiftUI

/// Shared chrome for every record editor: a title, the fields, and Cancel/Done.
///
/// Cancel carries `.cancelAction` so Escape closes the sheet, and Done carries
/// `.defaultAction` so Return commits — the two things the old inline editors
/// had no way to do.
struct EditorSheet<Content: View>: View {
    let title: String
    /// Sheets built around a `MemberTransferField` need room for two side-by-side
    /// lists; the plain form sheets don't.
    var width: CGFloat = 520
    let onCancel: () -> Void
    let onDone: () -> Void
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title)
                .font(.headline)
                .padding(.horizontal, Spacing.section)
                .padding(.top, Spacing.section)
                .padding(.bottom, Spacing.stack)

            content
                .padding(.horizontal, Spacing.section)

            Divider()
                .padding(.top, Spacing.stack)

            HStack(spacing: Spacing.stack) {
                Spacer()
                Button("Cancel", role: .cancel, action: onCancel)
                    .keyboardShortcut(.cancelAction)
                Button("Done", action: onDone)
                    .keyboardShortcut(.defaultAction)
                    .buttonStyle(.borderedProminent)
            }
            .padding(.horizontal, Spacing.section)
            .padding(.vertical, Spacing.stack)
        }
        .frame(width: width)
        // A sheet takes its content's ideal height, and a long form's ideal
        // height can exceed the display. When that happened the Cancel/Done row
        // sat below the bottom of the screen with no way to reach it, and AppKit
        // shoved the parent window down to make room — which is what made the
        // main window jump when the sheet closed. Capping the height here keeps
        // the buttons on screen; the grouped `Form` inside scrolls to make up
        // the difference.
        .frame(maxHeight: EditorSheet.maxSheetHeight)
    }

    /// The tallest a sheet may be: the usable height of the screen it will open
    /// on, less the parent window's title bar and a margin so the sheet does not
    /// sit flush against the menu bar and the Dock.
    ///
    /// Read at layout time rather than cached, since the window can be moved to
    /// a different display between edits. Falls back to a laptop-sized screen
    /// when AppKit reports none.
    static var maxSheetHeight: CGFloat {
        let usable = NSScreen.main?.visibleFrame.height ?? 800
        return max(360, usable - 120)
    }
}

/// A field whose title sits above its control, left-aligned.
///
/// The grouped `Form` style puts labels in a right-aligned column beside the
/// field, which pushes a lone Name box far off to the right and reads as an
/// afterthought. Anything the user types a name into gets this instead.
struct StackedField<Content: View>: View {
    let label: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: Spacing.tight) {
            Text(label)
                .font(.subheadline.weight(.medium))
            content
                .labelsHidden()
                .textFieldStyle(.roundedBorder)
        }
    }
}

// MARK: - Channel

/// Editor for one channel: name, RX/TX, mode/power/bandwidth, color code, time
/// slot, and the DMR contact / group-list / radio-ID references.
struct ChannelEditorSheet: View {
    /// Referenced entities, so the DMR fields are name pickers instead of raw
    /// index boxes.
    let contacts: [DumpContact]
    let groupLists: [DumpGroupList]
    let radioIds: [DumpRadioID]
    let scanLists: [DumpScanList]
    let onCommit: (DumpChannel) -> Void

    @State private var draft: DumpChannel
    @Environment(\.dismiss) private var dismiss

    /// `0xff` in the group-list index means "no RX group list".
    private static let groupListNone = 255
    /// `0xff` in the scan-list index means "no scan list".
    private static let scanListNone = 255

    init(channel: DumpChannel, contacts: [DumpContact], groupLists: [DumpGroupList],
         radioIds: [DumpRadioID], scanLists: [DumpScanList] = [],
         onCommit: @escaping (DumpChannel) -> Void) {
        self.contacts = contacts
        self.groupLists = groupLists
        self.radioIds = radioIds
        self.scanLists = scanLists
        self.onCommit = onCommit
        _draft = State(initialValue: channel)
    }

    var body: some View {
        EditorSheet(title: "Channel \(formatSlot(draft.index))",
                    onCancel: { dismiss() },
                    onDone: { onCommit(draft); dismiss() }) {
            StackedField(label: "Name") {
                TextField("Name", text: $draft.name)
            }
            Form {
                Section("Frequency") {
                    TextField("Receive (MHz)", value: mhzBinding($draft.rxFrequencyHz), format: mhzFormat)
                    TextField("Transmit (MHz)", value: mhzBinding($draft.txFrequencyHz), format: mhzFormat)
                }

                Section("Channel") {
                    Picker("Mode", selection: $draft.mode) {
                        ForEach(channelModeOptions, id: \.tag) { Text($0.label).tag($0.tag) }
                    }
                    Picker("Power", selection: $draft.power) {
                        ForEach(channelPowerOptions, id: \.tag) { Text($0.label).tag($0.tag) }
                    }
                    Picker("Bandwidth", selection: $draft.bandwidth) {
                        ForEach(channelBandwidthOptions, id: \.tag) { Text($0.label).tag($0.tag) }
                    }

                    Picker("Scan list", selection: $draft.scanListIndex) {
                        Text("None").tag(Self.scanListNone)
                        if draft.scanListIndex != Self.scanListNone
                            && !scanLists.contains(where: { $0.index == draft.scanListIndex }) {
                            Text("#\(formatSlot(draft.scanListIndex)) (missing)").tag(draft.scanListIndex)
                        }
                        ForEach(scanLists) { s in Text(s.name).tag(s.index) }
                    }

                    Picker("TX permit", selection: $draft.admit) {
                        ForEach(admitOptions, id: \.tag) { Text($0.label).tag($0.tag) }
                    }
                }

                // One block, disabled as a unit, rather than five rows each
                // carrying their own `!isDigital`: this is the part of the sheet
                // a radio without DMR simply doesn't have.
                Section("Digital (DMR)") {
                    Stepper("Color code: \(String(draft.colorCode))",
                            value: $draft.colorCode, in: 0...15)
                    Picker("Time slot", selection: $draft.timeSlot) {
                        Text("1").tag(1)
                        Text("2").tag(2)
                    }
                    .pickerStyle(.segmented)

                    Picker("Contact", selection: $draft.contactIndex) {
                        // A channel can reference a contact that was removed; without
                        // a matching tag the picker renders blank and can write back a
                        // wrong value on interaction.
                        if !contacts.contains(where: { UInt32($0.index) == draft.contactIndex }) {
                            Text("#\(formatSlot(Int(draft.contactIndex))) (missing)")
                                .tag(draft.contactIndex)
                        }
                        ForEach(contacts) { c in
                            Text(verbatim: "\(c.name) (\(formatDMRID(c.number)))").tag(UInt32(c.index))
                        }
                    }
                    .disabled(contacts.isEmpty)

                    Picker("RX group list", selection: $draft.groupListIndex) {
                        Text("None").tag(Self.groupListNone)
                        if draft.groupListIndex != Self.groupListNone
                            && !groupLists.contains(where: { $0.index == draft.groupListIndex }) {
                            Text("#\(formatSlot(draft.groupListIndex)) (missing)").tag(draft.groupListIndex)
                        }
                        ForEach(groupLists) { g in Text(g.name).tag(g.index) }
                    }

                    Picker("Radio ID", selection: $draft.radioIdIndex) {
                        if !radioIds.contains(where: { $0.index == draft.radioIdIndex }) {
                            Text("#\(formatSlot(draft.radioIdIndex)) (missing)").tag(draft.radioIdIndex)
                        }
                        ForEach(radioIds) { r in
                            Text(verbatim: "\(r.name) (\(formatDMRID(r.number)))").tag(r.index)
                        }
                    }
                    .disabled(radioIds.isEmpty)
                }
                .disabled(!isDigital)

                Section("Analog") {
                    Picker("RX signaling", selection: $draft.rxSignalingMode) {
                        ForEach(signalingModeOptions, id: \.tag) { Text($0.label).tag($0.tag) }
                    }
                    Picker("TX signaling", selection: $draft.txSignalingMode) {
                        ForEach(signalingModeOptions, id: \.tag) { Text($0.label).tag($0.tag) }
                    }
                    Stepper("RX CTCSS index: \(String(draft.rxCtcss))", value: $draft.rxCtcss, in: 0...255)
                    Stepper("TX CTCSS index: \(String(draft.txCtcss))", value: $draft.txCtcss, in: 0...255)
                    Stepper("RX DCS code: \(String(draft.rxDcs))", value: $draft.rxDcs, in: 0...65535)
                    Stepper("TX DCS code: \(String(draft.txDcs))", value: $draft.txDcs, in: 0...65535)
                    Picker("Squelch mode", selection: $draft.squelchMode) {
                        ForEach(squelchModeOptions, id: \.tag) { Text($0.label).tag($0.tag) }
                    }
                    Picker("Optional signal", selection: $draft.optionalSignaling) {
                        ForEach(optionalSignalingOptions, id: \.tag) { Text($0.label).tag($0.tag) }
                    }
                    Stepper("DTMF ID index: \(String(draft.dtmfIdIndex))", value: $draft.dtmfIdIndex, in: 0...255)
                    Stepper("2-Tone ID index: \(String(draft.twoToneIdIndex))", value: $draft.twoToneIdIndex, in: 0...255)
                    Stepper("5-Tone ID index: \(String(draft.fiveToneIdIndex))", value: $draft.fiveToneIdIndex, in: 0...255)
                }
                .disabled(!hasAnalog)

                Section("Flags") {
                    Toggle("PTT prohibit (RX only)", isOn: $draft.rxOnly)
                    Toggle("Talk around (simplex)", isOn: $draft.talkAround)
                    Toggle("Work alone", isOn: $draft.workAlone)
                    Toggle("Call confirmation", isOn: $draft.callConfirm)
                    Toggle("APRS RX", isOn: $draft.rxAprs)
                    Toggle("Simplex TDMA", isOn: $draft.simplexTdma)
                }
            }
            .formStyle(.grouped)
        }
    }

    /// Analog-only channels carry no DMR data, so the digital fields are disabled.
    private var isDigital: Bool { modeHasDigital(draft.mode) }

    /// Pure-digital channels carry no analog signaling, so those rows are disabled.
    private var hasAnalog: Bool { draft.mode != "Digital" }

    /// Bridge a Hz field to an editable MHz value, rounded to the codeplug's
    /// 10 Hz BCD resolution.
    private func mhzBinding(_ hz: Binding<UInt32>) -> Binding<Double> {
        Binding(
            get: { Double(hz.wrappedValue) / 1_000_000 },
            set: { mhz in
                let clamped = min(max(mhz, 0), 999.999_99)
                hz.wrappedValue = UInt32((clamped * 100_000).rounded()) * 10
            }
        )
    }
}

// MARK: - Zone

/// Editor for one zone: its name and its channel membership.
struct ZoneEditorSheet: View {
    let channels: [DumpChannel]
    let onCommit: (DumpZone) -> Void

    @State private var draft: DumpZone
    @Environment(\.dismiss) private var dismiss

    init(zone: DumpZone, channels: [DumpChannel], onCommit: @escaping (DumpZone) -> Void) {
        self.channels = channels
        self.onCommit = onCommit
        _draft = State(initialValue: zone)
    }

    var body: some View {
        EditorSheet(title: "Zone \(formatSlot(draft.index))",
                    width: 760,
                    onCancel: { dismiss() },
                    onDone: { onCommit(draft); dismiss() }) {
            VStack(alignment: .leading, spacing: Spacing.stack) {
                StackedField(label: "Name") {
                    TextField("Name", text: $draft.name)
                }
                MemberTransferField(label: "Channels", ownerNoun: "zone",
                                    candidates: candidates, members: $draft.channels)
            }
        }
    }

    /// Channels as pickable members, labeled by slot, name, and RX frequency.
    private var candidates: [MemberCandidate] {
        channels.map {
            MemberCandidate(index: $0.index,
                            label: "\(formatSlot($0.index)) — \($0.name) (\(formatMHz($0.rxFrequencyHz)))")
        }
    }
}

// MARK: - Scan List

/// Editor for one scan list: name, member channels, priority/revert channels,
/// and the scan timing parameters (look-back, dropout, dwell — raw values).
struct ScanListEditorSheet: View {
    let channels: [DumpChannel]
    let onCommit: (DumpScanList) -> Void

    @State private var draft: DumpScanList
    @Environment(\.dismiss) private var dismiss

    init(scanList: DumpScanList, channels: [DumpChannel], onCommit: @escaping (DumpScanList) -> Void) {
        self.channels = channels
        self.onCommit = onCommit
        _draft = State(initialValue: scanList)
    }

    var body: some View {
        EditorSheet(title: "Scan List \(formatSlot(draft.index))",
                    width: 760,
                    onCancel: { dismiss() },
                    onDone: { onCommit(draft); dismiss() }) {
            VStack(alignment: .leading, spacing: Spacing.stack) {
                StackedField(label: "Name") {
                    TextField("Name", text: $draft.name)
                }
                MemberTransferField(label: "Channels", ownerNoun: "scan list",
                                    candidates: candidates, members: $draft.members)
                Form {
                    Picker("Priority channel select", selection: $draft.priorityChannelSelect) {
                        ForEach(priorityChannelSelectOptions, id: \.tag) { Text($0.label).tag($0.tag) }
                    }
                    priorityPicker("Priority channel 1", selection: $draft.priorityChannel1)
                    priorityPicker("Priority channel 2", selection: $draft.priorityChannel2)
                    Stepper("Revert channel (raw): \(String(draft.revertChannel))",
                            value: $draft.revertChannel, in: 0...255)
                    Stepper("Look back A (raw): \(String(draft.lookBackA))",
                            value: $draft.lookBackA, in: 0...65535)
                    Stepper("Look back B (raw): \(String(draft.lookBackB))",
                            value: $draft.lookBackB, in: 0...65535)
                    Stepper("Dropout delay (raw): \(String(draft.dropoutDelay))",
                            value: $draft.dropoutDelay, in: 0...65535)
                    Stepper("Dwell time (raw): \(String(draft.dwellTime))",
                            value: $draft.dwellTime, in: 0...65535)
                }
                .formStyle(.grouped)
            }
        }
    }

    /// Channels as pickable members, labeled by slot, name, and RX frequency.
    private var candidates: [MemberCandidate] {
        channels.map {
            MemberCandidate(index: $0.index,
                            label: "\(formatSlot($0.index)) — \($0.name) (\(formatMHz($0.rxFrequencyHz)))")
        }
    }

    /// A channel picker with an "Off" (0xffff) option, tolerant of a stale index.
    @ViewBuilder
    private func priorityPicker(_ label: String, selection: Binding<Int>) -> some View {
        Picker(label, selection: selection) {
            Text("Off").tag(channelRefNone)
            if selection.wrappedValue != channelRefNone
                && !channels.contains(where: { $0.index == selection.wrappedValue }) {
                Text("#\(formatSlot(selection.wrappedValue)) (missing)").tag(selection.wrappedValue)
            }
            ForEach(channels) { c in Text(verbatim: "\(formatSlot(c.index)) — \(c.name)").tag(c.index) }
        }
    }
}

// MARK: - Contact

/// Editor for one DMR contact / talk group.
struct ContactEditorSheet: View {
    let onCommit: (DumpContact) -> Void

    @State private var draft: DumpContact
    @Environment(\.dismiss) private var dismiss

    init(contact: DumpContact, onCommit: @escaping (DumpContact) -> Void) {
        self.onCommit = onCommit
        _draft = State(initialValue: contact)
    }

    var body: some View {
        EditorSheet(title: "Contact \(formatSlot(draft.index))",
                    onCancel: { dismiss() },
                    onDone: { onCommit(draft); dismiss() }) {
            StackedField(label: "Name") {
                TextField("Name", text: $draft.name)
            }
            Form {
                TextField("DMR ID", value: dmrIDBinding($draft.number),
                          format: .number.grouping(.never))
                Picker("Call type", selection: $draft.callType) {
                    ForEach(callTypeOptions, id: \.tag) { Text($0.label).tag($0.tag) }
                }
            }
            .formStyle(.grouped)
        }
    }

    private func dmrIDBinding(_ v: Binding<UInt32>) -> Binding<Int> {
        Binding(get: { Int(v.wrappedValue) }, set: { v.wrappedValue = UInt32(max(0, $0)) })
    }
}

// MARK: - Group list

/// Editor for one RX group list: its name and its contact membership.
struct GroupListEditorSheet: View {
    let contacts: [DumpContact]
    let onCommit: (DumpGroupList) -> Void

    @State private var draft: DumpGroupList
    @Environment(\.dismiss) private var dismiss

    init(groupList: DumpGroupList, contacts: [DumpContact],
         onCommit: @escaping (DumpGroupList) -> Void) {
        self.contacts = contacts
        self.onCommit = onCommit
        _draft = State(initialValue: groupList)
    }

    var body: some View {
        EditorSheet(title: "Group List \(formatSlot(draft.index))",
                    width: 760,
                    onCancel: { dismiss() },
                    onDone: { onCommit(draft); dismiss() }) {
            VStack(alignment: .leading, spacing: Spacing.stack) {
                StackedField(label: "Name") {
                    TextField("Name", text: $draft.name)
                }
                MemberTransferField(label: "Contacts", ownerNoun: "group list",
                                    candidates: candidates, members: $draft.members)
            }
        }
    }

    /// Contacts as pickable members, labeled by name and DMR ID.
    private var candidates: [MemberCandidate] {
        contacts.map { MemberCandidate(index: $0.index, label: "\($0.name) (\(formatDMRID($0.number)))") }
    }
}

// MARK: - Radio ID

/// Editor for one radio ID.
struct RadioIDEditorSheet: View {
    let onCommit: (DumpRadioID) -> Void

    @State private var draft: DumpRadioID
    @Environment(\.dismiss) private var dismiss

    init(radioId: DumpRadioID, onCommit: @escaping (DumpRadioID) -> Void) {
        self.onCommit = onCommit
        _draft = State(initialValue: radioId)
    }

    var body: some View {
        EditorSheet(title: "Radio ID \(formatSlot(draft.index))",
                    onCancel: { dismiss() },
                    onDone: { onCommit(draft); dismiss() }) {
            StackedField(label: "Name") {
                TextField("Name", text: $draft.name)
            }
            Form {
                TextField("DMR ID", value: dmrIDBinding($draft.number),
                          format: .number.grouping(.never))
            }
            .formStyle(.grouped)
        }
    }

    private func dmrIDBinding(_ v: Binding<UInt32>) -> Binding<Int> {
        Binding(get: { Int(v.wrappedValue) }, set: { v.wrappedValue = UInt32(max(0, $0)) })
    }
}

// MARK: - Move

/// Relocate a record to a different slot. The core rejects an occupied or
/// out-of-range target, which surfaces as an error.
struct MoveSheet: View {
    let title: String
    let currentIndex: Int
    let onCommit: (Int) -> Void

    @State private var slot: Int
    @Environment(\.dismiss) private var dismiss

    init(title: String, currentIndex: Int, onCommit: @escaping (Int) -> Void) {
        self.title = title
        self.currentIndex = currentIndex
        self.onCommit = onCommit
        _slot = State(initialValue: currentIndex + 1)
    }

    var body: some View {
        EditorSheet(title: title,
                    onCancel: { dismiss() },
                    onDone: {
                        let target = slot - 1
                        if target != currentIndex { onCommit(target) }
                        dismiss()
                    }) {
            Form {
                TextField("Slot number", value: $slot, format: .number.grouping(.never))
                Text("The slot must be free and within the radio's range.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .formStyle(.grouped)
        }
    }
}

// MARK: - Membership

/// One selectable member: its 0-based slot index and a human label. Identified
/// by `index` for `ForEach`.
struct MemberCandidate: Identifiable {
    let index: Int
    let label: String
    var id: Int { index }
}

/// The list math behind `MemberTransferField`, kept free of view state so the
/// membership rules can be tested without standing up a sheet.
enum MemberTransfer {
    /// Candidates not already in the membership list. These are the left column;
    /// filtering them out of it is what stops a member being added twice.
    static func available(candidates: [MemberCandidate], members: [Int]) -> [MemberCandidate] {
        let taken = Set(members)
        return candidates.filter { !taken.contains($0.index) }
    }

    /// The membership list as candidates, in membership order. A member whose
    /// record has since been deleted still gets a row, so it can be removed
    /// rather than being stranded in the zone invisibly.
    static func memberItems(candidates: [MemberCandidate], members: [Int]) -> [MemberCandidate] {
        members.map { index in
            candidates.first { $0.index == index }
                ?? MemberCandidate(index: index, label: "#\(formatSlot(index)) (missing)")
        }
    }

    /// `members` with `selection` appended, in candidate order rather than the
    /// arbitrary order of the selection set, so a multi-select add lands
    /// predictably.
    static func adding(_ selection: Set<Int>, candidates: [MemberCandidate],
                       to members: [Int]) -> [Int] {
        members + available(candidates: candidates, members: members)
            .filter { selection.contains($0.index) }
            .map(\.index)
    }

    /// `members` with `selection` removed, preserving the order of the rest.
    static func removing(_ selection: Set<Int>, from members: [Int]) -> [Int] {
        members.filter { !selection.contains($0) }
    }
}

/// Edits a membership list (zone channels, group-list contacts) as a two-column
/// transfer: everything available on the left, everything already a member on
/// the right, and buttons between them that move whole multi-selections at once.
///
/// Both lists filter, because the candidate side can run to thousands of
/// channels and picking them one at a time out of an unfiltered list was the
/// thing that made the old one-at-a-time menu unusable.
///
/// The bound `members` stays a list of 0-based indices in membership order; the
/// index bookkeeping is hidden from the user.
struct MemberTransferField: View {
    /// Plural noun for the things being picked, e.g. "Channels".
    let label: String
    /// Singular noun for the record they belong to, e.g. "zone".
    let ownerNoun: String
    let candidates: [MemberCandidate]
    @Binding var members: [Int]

    @State private var availableSelection = Set<Int>()
    @State private var memberSelection = Set<Int>()
    @State private var availableQuery = ""
    @State private var memberQuery = ""

    private static let listHeight: CGFloat = 260

    var body: some View {
        VStack(alignment: .leading, spacing: Spacing.tight) {
            Text(label)
                .font(.subheadline.weight(.medium))
            HStack(alignment: .center, spacing: Spacing.stack) {
                column(title: "Available",
                       count: available.count,
                       items: filtered(available, by: availableQuery),
                       query: $availableQuery,
                       selection: $availableSelection,
                       emptyText: "Nothing left to add",
                       primaryVerb: "Add",
                       onPrimary: add)
                transferButtons
                column(title: "In this \(ownerNoun)",
                       count: members.count,
                       items: filtered(memberItems, by: memberQuery),
                       query: $memberQuery,
                       selection: $memberSelection,
                       emptyText: "No \(label.lowercased()) yet",
                       primaryVerb: "Remove",
                       onPrimary: remove)
            }
        }
    }

    private var transferButtons: some View {
        VStack(spacing: Spacing.inline) {
            Button(action: addSelected) {
                Image(systemName: "chevron.right")
                    .frame(width: 20)
            }
            .disabled(availableSelection.isEmpty)
            .help("Add the selected \(label.lowercased())")

            Button(action: removeSelected) {
                Image(systemName: "chevron.left")
                    .frame(width: 20)
            }
            .disabled(memberSelection.isEmpty)
            .help("Remove the selected \(label.lowercased())")
        }
    }

    private func column(title: String, count: Int, items: [MemberCandidate],
                        query: Binding<String>, selection: Binding<Set<Int>>,
                        emptyText: String, primaryVerb: String,
                        onPrimary: @escaping (Set<Int>) -> Void) -> some View {
        VStack(alignment: .leading, spacing: Spacing.tight) {
            Text("\(title) (\(String(count)))")
                .font(.caption)
                .foregroundStyle(.secondary)
            TextField("Filter", text: query)
                .textFieldStyle(.roundedBorder)
            List(items, selection: selection) { item in
                Text(item.label)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    // The label takes itself out of hit testing so every click
                    // lands on the row itself. Clicking directly on the text was
                    // otherwise eaten before the list saw it, so only clicks on
                    // the blank part of a row changed the selection.
                    .allowsHitTesting(false)
            }
            // Double-click moves the clicked rows across. This has to go through
            // `primaryAction` rather than a `TapGesture(count: 2)` on the row: a
            // tap gesture inside the row competes with the list's own click
            // handling, which swallowed or delayed single-click selection.
            .contextMenu(forSelectionType: Int.self) { ids in
                if !ids.isEmpty {
                    Button("\(primaryVerb) \(ids.count) \(label.lowercased())") { onPrimary(ids) }
                }
            } primaryAction: { ids in
                onPrimary(ids)
            }
            .listStyle(.bordered(alternatesRowBackgrounds: true))
            .frame(height: Self.listHeight)
            .overlay {
                if items.isEmpty {
                    Text(query.wrappedValue.isEmpty ? emptyText : "No matches")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .frame(maxWidth: .infinity)
    }

    private var available: [MemberCandidate] {
        MemberTransfer.available(candidates: candidates, members: members)
    }

    private var memberItems: [MemberCandidate] {
        MemberTransfer.memberItems(candidates: candidates, members: members)
    }

    private func filtered(_ items: [MemberCandidate], by query: String) -> [MemberCandidate] {
        query.isEmpty ? items : items.filter { $0.label.localizedCaseInsensitiveContains(query) }
    }

    private func addSelected() {
        members = MemberTransfer.adding(availableSelection, candidates: candidates, to: members)
        availableSelection = []
    }

    private func removeSelected() {
        members = MemberTransfer.removing(memberSelection, from: members)
        memberSelection = []
    }

    /// Add specific candidates (the double-click / context-menu path on the
    /// Available column), independent of what the column has selected.
    private func add(_ ids: Set<Int>) {
        members = MemberTransfer.adding(ids, candidates: candidates, to: members)
        availableSelection.subtract(ids)
    }

    /// Remove specific members (the double-click / context-menu path on the
    /// members column).
    private func remove(_ ids: Set<Int>) {
        members = MemberTransfer.removing(ids, from: members)
        memberSelection.subtract(ids)
    }
}
