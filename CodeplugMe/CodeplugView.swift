import SwiftUI

/// Which entity family the Codeplug editor is currently showing.
///
/// The raw values are the section names the core reports in a document's
/// `capabilities.sections`, so they are snake_case JSON keys, not display text.
/// `title` is what the user reads.
enum CodeplugSection: String, Codable, CaseIterable, Identifiable {
    case channels = "channels"
    case zones = "zones"
    case scanLists = "scan_lists"
    case contacts = "contacts"
    case groupLists = "group_lists"
    case radioIds = "radio_ids"
    case aprs = "aprs"
    // The QYT sections carry their own prefix because a QYT channel and an
    // AnyTone channel are different record shapes with different field meanings
    // and get different panes; sharing one `channels` case would invite a pane
    // to render the wrong one.
    case qytChannels = "qyt_channels"
    case qytSettings = "qyt_settings"
    case qytBands = "qyt_bands"

    var id: String { rawValue }

    /// The AnyTone's sections, in sidebar order. Only a placeholder for the
    /// sidebar before any document is open — once one is, the core's
    /// capabilities are what decide.
    static let anyToneSections: [CodeplugSection] = [
        .channels, .zones, .scanLists, .contacts, .groupLists, .radioIds, .aprs,
    ]

    var title: String {
        switch self {
        case .channels, .qytChannels: return "Channels"
        case .zones: return "Zones"
        case .scanLists: return "Scan Lists"
        case .contacts: return "Contacts"
        case .groupLists: return "Group Lists"
        case .radioIds: return "Radio IDs"
        case .aprs: return "APRS"
        case .qytSettings: return "Settings"
        case .qytBands: return "VFO Channels"
        }
    }

    var symbol: String {
        switch self {
        case .channels, .qytChannels: return "list.bullet"
        case .zones: return "square.grid.2x2"
        case .scanLists: return "antenna.radiowaves.left.and.right"
        case .contacts: return "person.2"
        case .groupLists: return "person.3"
        case .radioIds: return "number"
        case .aprs: return "location.circle"
        case .qytSettings: return "gearshape"
        case .qytBands: return "slider.horizontal.3"
        }
    }
}

/// Codeplug detail: the pane for one entity family, or the empty state when no
/// file is open.
struct CodeplugView: View {
    let section: CodeplugSection
    @EnvironmentObject private var store: CodeplugStore
    @EnvironmentObject private var device: DeviceStore

    var body: some View {
        Group {
            if store.fileURL == nil {
                noFileState
            } else {
                switch section {
                case .channels: ChannelsPane()
                case .zones: ZonesPane()
                case .scanLists: ScanListsPane()
                case .contacts: ContactsPane()
                case .groupLists: GroupListsPane()
                case .radioIds: RadioIDsPane()
                case .aprs: APRSPane()
                case .qytChannels: QytChannelsPane()
                case .qytSettings: QytSettingsPane()
                case .qytBands: QytBandsPane()
                }
            }
        }
        .toolbar { CodeplugFileToolbar() }
        // Populate the Write-to-Radio dropdown without a trip to the Device tab.
        .onAppear { device.refreshPorts() }
    }

    private var noFileState: some View {
        VStack(spacing: Spacing.stack) {
            Image(systemName: "doc.badge.plus")
                .font(.system(size: 40))
                .foregroundStyle(.secondary)
            Text("No codeplug open")
                .font(.title3.weight(.medium))
            Text("Open a .bin file to view and edit channels, zones, contacts, "
                + "group lists, and radio IDs. You can create one from the Device "
                + "tab with Read from Radio. Once one is open, Add Repeaters in "
                + "the toolbar fills it with nearby repeaters.")
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 380)
            Button("Open Codeplug…") { store.openWithPanel() }
                .keyboardShortcut("o")
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// Save/Write-to-radio, shared by every codeplug pane. Open and Close live in
/// `FileToolbar` at the window level, since with no file open these panes — and
/// this toolbar — aren't reachable.
struct CodeplugFileToolbar: ToolbarContent {
    @EnvironmentObject private var store: CodeplugStore
    @EnvironmentObject private var device: DeviceStore
    @State private var confirmWrite = false

    /// The connected ports this document may actually be written to. A radio of
    /// the wrong model is left out of the menu entirely rather than offered and
    /// then refused: one click from here is a confirmation dialog for a
    /// destructive, unrecoverable operation.
    private var targets: [PortEntry] {
        writableTargets(among: device.candidatePorts, for: store.model,
                        manual: device.manualSelection)
    }

    /// What to say when nothing is writable, which is a different situation from
    /// nothing being connected. An unattributed adapter is the QYT's normal
    /// state today, so that case points at the fix instead of stopping at "no".
    private var noTargetsReason: String {
        if device.candidatePorts.isEmpty { return "No radios connected" }
        let modelName = store.model?.displayName ?? "this radio"
        if device.candidatePorts.contains(where: { $0.model == nil }) {
            return "Choose \(modelName) for the connected adapter on the Device tab"
        }
        return "No connected \(modelName)"
    }

    /// Select `radio` and open the confirmation, unless it can't take this
    /// document — then say why, and never show the dialog at all. The core's
    /// identity check is the backstop; this is the sentence the user can act on.
    private func begin(write radio: PortEntry) {
        if let reason = writeRefusalReason(port: radio, documentModel: store.model,
                                           manual: device.manualSelection) {
            device.errorMessage = reason
            return
        }
        device.selectedPort = radio.name
        confirmWrite = true
    }

    var body: some ToolbarContent {
        ToolbarItem {
            // A split button: click Save, or pick Save As from the chevron. Save
            // As stays live with nothing staged, since saving an unmodified
            // codeplug under a new name is a reasonable thing to want.
            Menu {
                Button("Save") { store.save() }
                    .disabled(!store.isDirty)
                Button("Save As…") { store.saveAs() }
            } label: {
                Label("Save", systemImage: "square.and.arrow.down")
            } primaryAction: {
                store.save()
            }
            .help("Write staged changes to the file")
            // Without this the toolbar Menu takes its accessibility name from
            // the SF Symbol and announces itself as "download".
            .accessibilityLabel("Save")
            .disabled(store.fileURL == nil)
        }
        ToolbarItem {
            // A dropdown rather than a single dead button: it lists the connected
            // radios so the user picks the target directly, and stays live even
            // with nothing connected so Refresh is always reachable.
            Menu {
                if targets.isEmpty {
                    Text(noTargetsReason)
                } else {
                    ForEach(targets) { radio in
                        Button(radio.product ?? radio.name) { begin(write: radio) }
                            .disabled(device.busy)
                    }
                }
                Divider()
                Button("Refresh") { device.refreshPorts() }
            } label: {
                Label("Write to Radio", systemImage: "antenna.radiowaves.left.and.right")
            }
            .help("Write this codeplug, including unsaved changes, to a connected radio")
            .accessibilityLabel("Write to Radio")
            .disabled(store.fileURL == nil)
            .confirmationDialog("Write this codeplug to the radio?",
                                isPresented: $confirmWrite) {
                Button("Write to Radio", role: .destructive) {
                    // The work file, not the document: what gets written is what
                    // the user is looking at, staged changes included.
                    device.restore(from: Recovery.workURL,
                                   displayName: store.fileURL?.lastPathComponent ?? "codeplug")
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("""
                This OVERWRITES the radio's entire configuration with what you see \
                here\(store.isDirty ? ", including changes you haven't saved to the file yet" : ""). \
                Read the radio's current codeplug to a file first if you don't have a backup. \
                Do not disconnect the cable or power off the radio during the write.
                """)
            }
        }
    }
}

/// Open and Close a codeplug, always reachable from the window toolbar no matter
/// which pane is showing. This lives above the panes (attached in `ContentView`)
/// because the app starts with no file open, and the codeplug panes that carry
/// the rest of the file chrome are disabled until there is one — so without this
/// the only way to open a file was the File menu.
struct FileToolbar: ToolbarContent {
    @EnvironmentObject private var store: CodeplugStore
    @State private var confirmOpen = false
    @State private var confirmClose = false

    var body: some ToolbarContent {
        ToolbarItem(placement: .navigation) {
            Button {
                // Opening drops the staged work file, so ask before discarding it.
                if store.isDirty { confirmOpen = true } else { store.openWithPanel() }
            } label: {
                Label("Open", systemImage: "folder")
            }
            .help("Open a codeplug .bin")
            .confirmationDialog("Save changes before opening another codeplug?",
                                isPresented: $confirmOpen) {
                Button("Save") { store.save(); if !store.isDirty { store.openWithPanel() } }
                Button("Discard", role: .destructive) { store.openWithPanel() }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("Your staged changes haven't been written to "
                    + "\(store.fileURL?.lastPathComponent ?? "the file") yet.")
            }
        }
        if store.fileURL != nil {
            ToolbarItem(placement: .navigation) {
                Button {
                    if store.isDirty { confirmClose = true } else { store.close() }
                } label: {
                    Label("Close", systemImage: "xmark.circle")
                }
                .help("Close the open codeplug")
                .confirmationDialog("Save changes before closing?",
                                    isPresented: $confirmClose) {
                    // Only close if the save actually succeeded, so a failed write
                    // can't silently throw the changes away.
                    Button("Save") { store.save(); if !store.isDirty { store.close() } }
                    Button("Discard", role: .destructive) { store.close() }
                    Button("Cancel", role: .cancel) {}
                } message: {
                    Text("Your staged changes haven't been written to "
                        + "\(store.fileURL?.lastPathComponent ?? "the file") yet.")
                }
            }
            // Window level, like Open and Close, so it shows on every pane
            // (the app starts on Device) once there is a codeplug to add to.
            ToolbarItem(placement: .navigation) {
                Button {
                    store.showingRepeaterImport = true
                } label: {
                    Label("Add Repeaters", systemImage: "dot.radiowaves.up.forward")
                }
                .help("Add nearby repeaters from Hearham, RadioID or RepeaterBook")
            }
        }
    }
}

/// Add/Remove/Edit, shared by every entity pane. The pane owns the actions
/// because each entity family has its own store methods.
struct EntityToolbar: ToolbarContent {
    let noun: String
    let canEdit: Bool
    let onAdd: () -> Void
    let onEdit: () -> Void
    let onRemove: () -> Void

    var body: some ToolbarContent {
        ToolbarItemGroup {
            Button(action: onAdd) {
                Label("Add \(noun)", systemImage: "plus")
            }
            .help("Add a \(noun.lowercased())")

            Button(action: onEdit) {
                Label("Edit", systemImage: "pencil")
            }
            .help("Edit the selected \(noun.lowercased())")
            .disabled(!canEdit)

            Button(role: .destructive, action: onRemove) {
                Label("Remove", systemImage: "minus")
            }
            .help("Remove the selected \(noun.lowercased())")
            .disabled(!canEdit)
        }
    }
}

/// Marks something as carrying staged changes the file doesn't have yet: a
/// table row, or the sidebar section that row lives in.
struct UnsavedDot: View {
    var body: some View {
        Image(systemName: "circle.fill")
            .font(.system(size: 6))
            .foregroundStyle(.orange)
            .help("Unsaved changes")
            .accessibilityLabel("Unsaved changes")
    }
}

/// Marks a channel row whose frequencies (and DMR color code / slot) match
/// another channel's. The tooltip names the other slots; `DuplicateNameCell`
/// shows them on the row itself.
struct DuplicateMark: View {
    let others: [Int]

    var body: some View {
        Image(systemName: "exclamationmark.triangle.fill")
            .symbolRenderingMode(.multicolor)
            .help(duplicateLabel(others))
            .accessibilityLabel(duplicateLabel(others))
    }
}

/// A channel's name with, for a duplicate, the other slots it matches written
/// under it. It lives in the Name column because both channel tables are
/// short of room for a column of their own (macOS 13 caps a Table at ten).
struct DuplicateNameCell: View {
    let name: String
    let others: [Int]?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(name)
            if let others {
                Text(verbatim: duplicateLabel(others))
                    .font(.caption)
                    .foregroundStyle(.red)
            }
        }
    }
}

/// Stands in for a section the core reports but this build has no editor for
/// yet. The sidebar row comes from the radio's capabilities, so saying plainly
/// that the pane isn't built beats hiding a section the radio really has.
struct UnbuiltPane: View {
    let noun: String

    var body: some View {
        VStack(spacing: Spacing.inline) {
            Text("No editor for these \(noun) yet")
                .font(.title3.weight(.medium))
            Text("The values round-trip untouched when you save or write to the radio.")
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 380)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// Shown when a table has no rows at all, or when a filter matches nothing.
struct TableEmptyState: View {
    let noun: String
    let query: String
    /// True when filters beyond the text query are narrowing the list.
    var filtered = false

    var body: some View {
        VStack(spacing: Spacing.inline) {
            if query.isEmpty && !filtered {
                Text("No \(noun) yet")
                    .font(.title3.weight(.medium))
                Text("Use the + button to add one.")
                    .foregroundStyle(.secondary)
            } else if query.isEmpty {
                Text("No \(noun) match the current filters")
                    .font(.title3.weight(.medium))
            } else {
                Text("No \(noun) match “\(query)”")
                    .font(.title3.weight(.medium))
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.background)
    }
}

// MARK: - Channels

struct ChannelsPane: View {
    @EnvironmentObject private var store: CodeplugStore
    @State private var selected = Set<DumpChannel.ID>()
    @State private var editing: DumpChannel?
    @State private var moving: DumpChannel?
    @State private var bulkEditing = false
    @State private var query = ""
    @State private var filterMode: String?
    @State private var filterColorCode: Int?
    @State private var filterSlot: Int?
    @State private var duplicatesOnly = false
    @State private var sortOrder = [KeyPathComparator(\DumpChannel.index)]

    var body: some View {
        VStack(spacing: 0) {
            filterBar
            Divider()
            table
        }
        .searchable(text: $query, prompt: "Filter channels")
        .toolbar {
            EntityToolbar(noun: "Channel", canEdit: !selected.isEmpty,
                          onAdd: {
                              // Open the new record for editing right away — a
                              // blank "NEW" channel is never what the user wants
                              // to keep.
                              if let s = store.addChannel() {
                                  selected = [s]
                                  editing = store.channels.first { $0.id == s }
                              }
                          },
                          onEdit: {
                              if selected.count == 1 {
                                  editing = singleSelection
                              } else if selected.count > 1 {
                                  bulkEditing = true
                              }
                          },
                          onRemove: { remove() })
            ToolbarItem {
                Button {
                    bulkEditing = true
                } label: {
                    Label("Bulk Edit", systemImage: "square.and.pencil")
                }
                .help("Edit all selected channels at once")
                .disabled(selected.count < 2)
            }
        }
        .sheet(item: $editing) { channel in
            ChannelEditorSheet(channel: channel, contacts: store.contacts,
                               groupLists: store.groupLists, radioIds: store.radioIds,
                               scanLists: store.scanLists) {
                store.update($0)
            }
        }
        .sheet(item: $moving) { channel in
            MoveSheet(title: "Move Channel \(formatSlot(channel.index))",
                      currentIndex: channel.index) { target in
                store.moveChannel(channel.index, to: target)
                selected = store.channels.contains { $0.id == target } ? [target] : [channel.index]
            }
        }
        .sheet(isPresented: $bulkEditing) {
            BulkChannelEditorSheet(
                channels: selectedChannels,
                contacts: store.contacts,
                groupLists: store.groupLists,
                radioIds: store.radioIds
            ) { update in
                store.bulkUpdateChannels(update)
            }
        }
    }

    /// The channel table itself. Split out of `body` so the filter bar can sit
    /// above it in a stack while the search field, toolbar, and sheets stay on
    /// the enclosing view.
    private var table: some View {
        // Computed once per render over every channel, not just the filtered
        // rows, so a twin hidden by the filter still flags the visible one.
        let duplicates = duplicateChannelSlots(store.channels)
        return Table(rows, selection: $selected, sortOrder: $sortOrder) {
            // Shares the status column with the unsaved dot: a Table on
            // macOS 13 tops out at ten columns.
            TableColumn("") { row in
                HStack(spacing: 2) {
                    if store.isUnsaved(.channels, slot: row.index) { UnsavedDot() }
                    if let others = duplicates[row.index] { DuplicateMark(others: others) }
                }
            }
            .width(30)
            TableColumn("#", value: \.index) { Text(verbatim: formatSlot($0.index)) }
                .width(min: 40, ideal: 48, max: 64)
            TableColumn("Name", value: \.name) { row in
                DuplicateNameCell(name: row.name, others: duplicates[row.index])
            }
            TableColumn("RX (MHz)", value: \.rxFrequencyHz) { row in
                Text(verbatim: formatMHz(row.rxFrequencyHz)).monospacedDigit()
                    .foregroundStyle(duplicates[row.index] == nil ? Color.primary : Color.red)
            }
            TableColumn("TX (MHz)", value: \.txFrequencyHz) { row in
                Text(verbatim: formatMHz(row.txFrequencyHz)).monospacedDigit()
                    .foregroundStyle(duplicates[row.index] == nil ? Color.primary : Color.red)
            }
            TableColumn("Mode", value: \.mode) { Text($0.mode) }
                .width(min: 50, ideal: 80)
            TableColumn("Color code", value: \.colorCode) {
                Text(verbatim: String($0.colorCode))
            }
            .width(min: 40, ideal: 56, max: 80)
            TableColumn("Slot", value: \.timeSlot) { Text(verbatim: String($0.timeSlot)) }
                .width(min: 34, ideal: 44, max: 64)
        }
        .contextMenu(forSelectionType: DumpChannel.ID.self) { ids in
            rowMenu(ids)
        } primaryAction: { ids in
            if ids.count == 1 { editing = record(ids.first) }
        }
        .overlay {
            if rows.isEmpty {
                TableEmptyState(noun: "channels", query: query, filtered: hasActiveFilters)
            }
        }
    }

    /// Mode / color-code / slot filters, plus a Clear button once any is set.
    private var filterBar: some View {
        HStack(spacing: Spacing.stack) {
            Picker("Mode", selection: $filterMode) {
                Text("All modes").tag(String?.none)
                ForEach(channelModeOptions, id: \.tag) { Text($0.label).tag(Optional($0.tag)) }
            }
            .fixedSize()

            Picker("Color code", selection: $filterColorCode) {
                Text("All color codes").tag(Int?.none)
                ForEach(0...15, id: \.self) { Text(verbatim: "CC \($0)").tag(Optional($0)) }
            }
            .fixedSize()

            Picker("Slot", selection: $filterSlot) {
                Text("All slots").tag(Int?.none)
                Text("Slot 1").tag(Optional(1))
                Text("Slot 2").tag(Optional(2))
            }
            .fixedSize()

            Toggle("Duplicates only", isOn: duplicatesOnlyBinding)
                .toggleStyle(.checkbox)
                .help("Show only channels that duplicate another, grouped by RX / TX frequency")

            if hasActiveFilters {
                Button("Clear") {
                    filterMode = nil
                    filterColorCode = nil
                    filterSlot = nil
                    duplicatesOnlyBinding.wrappedValue = false
                }
                .buttonStyle(.borderless)
            }
            Spacer()
        }
        .padding(.horizontal, Spacing.stack)
        .padding(.vertical, Spacing.inline)
    }

    /// True when any of the mode / color-code / slot / duplicate filters is narrowing the list.
    private var hasActiveFilters: Bool {
        filterMode != nil || filterColorCode != nil || filterSlot != nil || duplicatesOnly
    }

    /// The duplicates-only switch. Turning it on sorts by RX then TX so each
    /// group of twins lands together; turning it off goes back to slot order.
    /// It sets the table's own sort order, so the column headers still re-sort.
    private var duplicatesOnlyBinding: Binding<Bool> {
        Binding(
            get: { duplicatesOnly },
            set: { on in
                duplicatesOnly = on
                sortOrder = on ? channelFrequencySortOrder : [KeyPathComparator(\DumpChannel.index)]
            }
        )
    }

    private var rows: [DumpChannel] {
        let duplicates = duplicatesOnly ? duplicateChannelSlots(store.channels) : [:]
        return store.channels
            .filter {
                channelMatchesFilter($0, query: query, mode: filterMode,
                                     colorCode: filterColorCode, slot: filterSlot)
                    && (!duplicatesOnly || duplicates[$0.index] != nil)
            }
            .sorted(using: sortOrder)
    }

    /// The channels currently selected, for the bulk editor.
    private var selectedChannels: [DumpChannel] {
        store.channels.filter { selected.contains($0.id) }
    }

    /// Convenience for the single-edit path: the one selected record, or nil.
    private var singleSelection: DumpChannel? {
        selected.count == 1 ? record(selected.first) : nil
    }

    private func record(_ id: DumpChannel.ID?) -> DumpChannel? {
        guard let id else { return nil }
        return store.channels.first { $0.id == id }
    }

    @ViewBuilder
    private func rowMenu(_ ids: Set<DumpChannel.ID>) -> some View {
        if ids.count == 1, let channel = record(ids.first) {
            Button("Edit…") { editing = channel }
            Button("Move to Slot…") { moving = channel }
            Divider()
            Button("Remove", role: .destructive) {
                selected = []
                store.removeChannel(channel.index)
            }
        } else if ids.count > 1 {
            Button("Bulk Edit \(ids.count) Channels…") { bulkEditing = true }
            Divider()
            Button("Remove \(ids.count) Channels", role: .destructive) {
                let indices = ids.sorted()
                selected = []
                for i in indices.reversed() { store.removeChannel(i) }
            }
        }
    }

    private func remove() {
        let indices = selected.sorted()
        selected = []
        for i in indices.reversed() { store.removeChannel(i) }
    }
}

// MARK: - Zones

struct ZonesPane: View {
    @EnvironmentObject private var store: CodeplugStore
    @State private var selected: DumpZone.ID?
    @State private var editing: DumpZone?
    @State private var moving: DumpZone?
    @State private var query = ""
    @State private var sortOrder = [KeyPathComparator(\DumpZone.index)]

    var body: some View {
        Table(rows, selection: $selected, sortOrder: $sortOrder) {
            TableColumn("") { row in
                if store.isUnsaved(.zones, slot: row.index) { UnsavedDot() }
            }
            .width(14)
            TableColumn("#", value: \.index) { Text(verbatim: formatSlot($0.index)) }
                .width(min: 40, ideal: 48, max: 64)
            TableColumn("Name", value: \.name) { Text($0.name) }
            TableColumn("Channels", value: \.channels.count) {
                Text(verbatim: String($0.channels.count))
            }
            .width(min: 60, ideal: 80)
        }
        .contextMenu(forSelectionType: DumpZone.ID.self) { ids in
            rowMenu(ids)
        } primaryAction: { ids in
            editing = record(ids.first)
        }
        .overlay { if rows.isEmpty { TableEmptyState(noun: "zones", query: query) } }
        .searchable(text: $query, prompt: "Filter zones")
        .toolbar {
            EntityToolbar(noun: "Zone", canEdit: selection != nil,
                          onAdd: {
                              if let s = store.addZone() {
                                  selected = s
                                  editing = record(s)
                              }
                          },
                          onEdit: { editing = selection },
                          onRemove: { remove() })
        }
        .sheet(item: $editing) { zone in
            ZoneEditorSheet(zone: zone, channels: store.channels) { store.update($0) }
        }
        .sheet(item: $moving) { zone in
            MoveSheet(title: "Move Zone \(formatSlot(zone.index))",
                      currentIndex: zone.index) { target in
                store.moveZone(zone.index, to: target)
                selected = store.zones.contains { $0.id == target } ? target : zone.index
            }
        }
    }

    private var rows: [DumpZone] {
        store.zones
            .filter { query.isEmpty || $0.name.localizedCaseInsensitiveContains(query) }
            .sorted(using: sortOrder)
    }

    private var selection: DumpZone? { record(selected) }

    private func record(_ id: DumpZone.ID?) -> DumpZone? {
        guard let id else { return nil }
        return store.zones.first { $0.id == id }
    }

    @ViewBuilder
    private func rowMenu(_ ids: Set<DumpZone.ID>) -> some View {
        if let zone = record(ids.first) {
            Button("Edit…") { editing = zone }
            Button("Move to Slot…") { moving = zone }
            Divider()
            Button("Remove", role: .destructive) {
                selected = nil
                store.removeZone(zone.index)
            }
        }
    }

    private func remove() {
        guard let zone = selection else { return }
        selected = nil
        store.removeZone(zone.index)
    }
}

// MARK: - Contacts / Talk Groups

struct ContactsPane: View {
    @EnvironmentObject private var store: CodeplugStore
    @State private var selected: DumpContact.ID?
    @State private var editing: DumpContact?
    @State private var moving: DumpContact?
    @State private var query = ""
    @State private var sortOrder = [KeyPathComparator(\DumpContact.index)]

    var body: some View {
        Table(rows, selection: $selected, sortOrder: $sortOrder) {
            TableColumn("") { row in
                if store.isUnsaved(.contacts, slot: row.index) { UnsavedDot() }
            }
            .width(14)
            TableColumn("#", value: \.index) { Text(verbatim: formatSlot($0.index)) }
                .width(min: 40, ideal: 48, max: 64)
            TableColumn("Name", value: \.name) { Text($0.name) }
            TableColumn("DMR ID", value: \.number) {
                Text(verbatim: formatDMRID($0.number)).monospacedDigit()
            }
            TableColumn("Type", value: \.callType) { Text($0.callType) }
                .width(min: 60, ideal: 80)
        }
        .contextMenu(forSelectionType: DumpContact.ID.self) { ids in
            rowMenu(ids)
        } primaryAction: { ids in
            editing = record(ids.first)
        }
        .overlay { if rows.isEmpty { TableEmptyState(noun: "contacts", query: query) } }
        .searchable(text: $query, prompt: "Filter contacts")
        .toolbar {
            EntityToolbar(noun: "Contact", canEdit: selection != nil,
                          onAdd: {
                              if let s = store.addContact() {
                                  selected = s
                                  editing = record(s)
                              }
                          },
                          onEdit: { editing = selection },
                          onRemove: { remove() })
        }
        .sheet(item: $editing) { contact in
            ContactEditorSheet(contact: contact) { store.update($0) }
        }
        .sheet(item: $moving) { contact in
            MoveSheet(title: "Move Contact \(formatSlot(contact.index))",
                      currentIndex: contact.index) { target in
                store.moveContact(contact.index, to: target)
                selected = store.contacts.contains { $0.id == target } ? target : contact.index
            }
        }
    }

    private var rows: [DumpContact] {
        store.contacts
            .filter {
                query.isEmpty
                    || $0.name.localizedCaseInsensitiveContains(query)
                    || formatDMRID($0.number).contains(query)
            }
            .sorted(using: sortOrder)
    }

    private var selection: DumpContact? { record(selected) }

    private func record(_ id: DumpContact.ID?) -> DumpContact? {
        guard let id else { return nil }
        return store.contacts.first { $0.id == id }
    }

    @ViewBuilder
    private func rowMenu(_ ids: Set<DumpContact.ID>) -> some View {
        if let contact = record(ids.first) {
            Button("Edit…") { editing = contact }
            Button("Move to Slot…") { moving = contact }
            Divider()
            Button("Remove", role: .destructive) {
                selected = nil
                store.removeContact(contact.index)
            }
        }
    }

    private func remove() {
        guard let contact = selection else { return }
        selected = nil
        store.removeContact(contact.index)
    }
}

// MARK: - Group lists

struct GroupListsPane: View {
    @EnvironmentObject private var store: CodeplugStore
    @State private var selected: DumpGroupList.ID?
    @State private var editing: DumpGroupList?
    @State private var moving: DumpGroupList?
    @State private var query = ""
    @State private var sortOrder = [KeyPathComparator(\DumpGroupList.index)]

    var body: some View {
        Table(rows, selection: $selected, sortOrder: $sortOrder) {
            TableColumn("") { row in
                if store.isUnsaved(.groupLists, slot: row.index) { UnsavedDot() }
            }
            .width(14)
            TableColumn("#", value: \.index) { Text(verbatim: formatSlot($0.index)) }
                .width(min: 40, ideal: 48, max: 64)
            TableColumn("Name", value: \.name) { Text($0.name) }
            TableColumn("Contacts", value: \.members.count) {
                Text(verbatim: String($0.members.count))
            }
            .width(min: 60, ideal: 80)
        }
        .contextMenu(forSelectionType: DumpGroupList.ID.self) { ids in
            rowMenu(ids)
        } primaryAction: { ids in
            editing = record(ids.first)
        }
        .overlay { if rows.isEmpty { TableEmptyState(noun: "group lists", query: query) } }
        .searchable(text: $query, prompt: "Filter group lists")
        .toolbar {
            EntityToolbar(noun: "Group List", canEdit: selection != nil,
                          onAdd: {
                              if let s = store.addGroupList() {
                                  selected = s
                                  editing = record(s)
                              }
                          },
                          onEdit: { editing = selection },
                          onRemove: { remove() })
        }
        .sheet(item: $editing) { groupList in
            GroupListEditorSheet(groupList: groupList, contacts: store.contacts) { store.update($0) }
        }
        .sheet(item: $moving) { groupList in
            MoveSheet(title: "Move Group List \(formatSlot(groupList.index))",
                      currentIndex: groupList.index) { target in
                store.moveGroupList(groupList.index, to: target)
                selected = store.groupLists.contains { $0.id == target } ? target : groupList.index
            }
        }
    }

    private var rows: [DumpGroupList] {
        store.groupLists
            .filter { query.isEmpty || $0.name.localizedCaseInsensitiveContains(query) }
            .sorted(using: sortOrder)
    }

    private var selection: DumpGroupList? { record(selected) }

    private func record(_ id: DumpGroupList.ID?) -> DumpGroupList? {
        guard let id else { return nil }
        return store.groupLists.first { $0.id == id }
    }

    @ViewBuilder
    private func rowMenu(_ ids: Set<DumpGroupList.ID>) -> some View {
        if let groupList = record(ids.first) {
            Button("Edit…") { editing = groupList }
            Button("Move to Slot…") { moving = groupList }
            Divider()
            Button("Remove", role: .destructive) {
                selected = nil
                store.removeGroupList(groupList.index)
            }
        }
    }

    private func remove() {
        guard let groupList = selection else { return }
        selected = nil
        store.removeGroupList(groupList.index)
    }
}

// MARK: - Scan Lists

struct ScanListsPane: View {
    @EnvironmentObject private var store: CodeplugStore
    @State private var selected: DumpScanList.ID?
    @State private var editing: DumpScanList?
    @State private var moving: DumpScanList?
    @State private var query = ""
    @State private var sortOrder = [KeyPathComparator(\DumpScanList.index)]

    var body: some View {
        Table(rows, selection: $selected, sortOrder: $sortOrder) {
            TableColumn("") { row in
                if store.isUnsaved(.scanLists, slot: row.index) { UnsavedDot() }
            }
            .width(14)
            TableColumn("#", value: \.index) { Text(verbatim: formatSlot($0.index)) }
                .width(min: 40, ideal: 48, max: 64)
            TableColumn("Name", value: \.name) { Text($0.name) }
            TableColumn("Channels", value: \.members.count) {
                Text(verbatim: String($0.members.count))
            }
            .width(min: 60, ideal: 80)
        }
        .contextMenu(forSelectionType: DumpScanList.ID.self) { ids in
            rowMenu(ids)
        } primaryAction: { ids in
            editing = record(ids.first)
        }
        .overlay { if rows.isEmpty { TableEmptyState(noun: "scan lists", query: query) } }
        .searchable(text: $query, prompt: "Filter scan lists")
        .toolbar {
            EntityToolbar(noun: "Scan List", canEdit: selection != nil,
                          onAdd: {
                              if let s = store.addScanList() {
                                  selected = s
                                  editing = record(s)
                              }
                          },
                          onEdit: { editing = selection },
                          onRemove: { remove() })
        }
        .sheet(item: $editing) { scanList in
            ScanListEditorSheet(scanList: scanList, channels: store.channels) { store.update($0) }
        }
        .sheet(item: $moving) { scanList in
            MoveSheet(title: "Move Scan List \(formatSlot(scanList.index))",
                      currentIndex: scanList.index) { target in
                store.moveScanList(scanList.index, to: target)
                selected = store.scanLists.contains { $0.id == target } ? target : scanList.index
            }
        }
    }

    private var rows: [DumpScanList] {
        store.scanLists
            .filter { query.isEmpty || $0.name.localizedCaseInsensitiveContains(query) }
            .sorted(using: sortOrder)
    }

    private var selection: DumpScanList? { record(selected) }

    private func record(_ id: DumpScanList.ID?) -> DumpScanList? {
        guard let id else { return nil }
        return store.scanLists.first { $0.id == id }
    }

    @ViewBuilder
    private func rowMenu(_ ids: Set<DumpScanList.ID>) -> some View {
        if let scanList = record(ids.first) {
            Button("Edit…") { editing = scanList }
            Button("Move to Slot…") { moving = scanList }
            Divider()
            Button("Remove", role: .destructive) {
                selected = nil
                store.removeScanList(scanList.index)
            }
        }
    }

    private func remove() {
        guard let scanList = selection else { return }
        selected = nil
        store.removeScanList(scanList.index)
    }
}

// MARK: - APRS (read-only)

/// Read-only view of the radio's APRS identity settings. This block lives in the
/// radio-settings region the tool must never write, so there is no edit path —
/// the values are shown for reference and captured in backups only.
struct APRSPane: View {
    @EnvironmentObject private var store: CodeplugStore

    var body: some View {
        Group {
            if let a = store.aprs {
                Form {
                    Section("Read-only") {
                        Text("APRS settings live in a protected region this tool does not write. "
                             + "These values are shown for reference only; edit them in the vendor CPS.")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                    }
                    Section("Identity") {
                        LabeledContent("My Call Sign", value: a.sourceCall.isEmpty ? "—" : a.sourceCall)
                        LabeledContent("My SSID", value: String(a.sourceSsid))
                        LabeledContent("Destination", value: a.destinationCall.isEmpty ? "—" : a.destinationCall)
                        LabeledContent("Destination SSID", value: String(a.destinationSsid))
                        LabeledContent("Symbol Table", value: symbolString(a.symbolTable))
                        LabeledContent("Map Icon", value: symbolString(a.symbol))
                    }
                    Section("Analog TX") {
                        LabeledContent("TX Frequency",
                                       value: a.fmTxFrequencyHz == 0 ? "—" : "\(formatMHz(a.fmTxFrequencyHz)) MHz")
                        LabeledContent("TX Power (raw)", value: String(a.fmPower))
                    }
                    Section("Timing (raw values)") {
                        LabeledContent("Manual TX Interval", value: String(a.manualTxInterval))
                        LabeledContent("Auto TX Interval", value: String(a.autoTxInterval))
                    }
                }
                .formStyle(.grouped)
            } else {
                VStack(spacing: Spacing.inline) {
                    Image(systemName: "location.slash")
                        .font(.largeTitle)
                        .foregroundStyle(.secondary)
                    Text("No APRS settings")
                        .font(.headline)
                    Text("This codeplug has no APRS block.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }

    /// Render a symbol-selector byte as its ASCII character when printable.
    private func symbolString(_ v: Int) -> String {
        if let scalar = Unicode.Scalar(v), v >= 0x20, v < 0x7f {
            return "\(Character(scalar)) (\(v))"
        }
        return String(v)
    }
}

// MARK: - Radio IDs

struct RadioIDsPane: View {
    @EnvironmentObject private var store: CodeplugStore
    @State private var selected: DumpRadioID.ID?
    @State private var editing: DumpRadioID?
    @State private var moving: DumpRadioID?
    @State private var query = ""
    @State private var sortOrder = [KeyPathComparator(\DumpRadioID.index)]

    var body: some View {
        Table(rows, selection: $selected, sortOrder: $sortOrder) {
            TableColumn("") { row in
                if store.isUnsaved(.radioIds, slot: row.index) { UnsavedDot() }
            }
            .width(14)
            TableColumn("#", value: \.index) { Text(verbatim: formatSlot($0.index)) }
                .width(min: 40, ideal: 48, max: 64)
            TableColumn("Name", value: \.name) { Text($0.name) }
            TableColumn("DMR ID", value: \.number) {
                Text(verbatim: formatDMRID($0.number)).monospacedDigit()
            }
        }
        .contextMenu(forSelectionType: DumpRadioID.ID.self) { ids in
            rowMenu(ids)
        } primaryAction: { ids in
            editing = record(ids.first)
        }
        .overlay { if rows.isEmpty { TableEmptyState(noun: "radio IDs", query: query) } }
        .searchable(text: $query, prompt: "Filter radio IDs")
        .toolbar {
            EntityToolbar(noun: "Radio ID", canEdit: selection != nil,
                          onAdd: {
                              if let s = store.addRadioId() {
                                  selected = s
                                  editing = record(s)
                              }
                          },
                          onEdit: { editing = selection },
                          onRemove: { remove() })
        }
        .sheet(item: $editing) { radioId in
            RadioIDEditorSheet(radioId: radioId) { store.update($0) }
        }
        .sheet(item: $moving) { radioId in
            MoveSheet(title: "Move Radio ID \(formatSlot(radioId.index))",
                      currentIndex: radioId.index) { target in
                store.moveRadioId(radioId.index, to: target)
                selected = store.radioIds.contains { $0.id == target } ? target : radioId.index
            }
        }
    }

    private var rows: [DumpRadioID] {
        store.radioIds
            .filter {
                query.isEmpty
                    || $0.name.localizedCaseInsensitiveContains(query)
                    || formatDMRID($0.number).contains(query)
            }
            .sorted(using: sortOrder)
    }

    private var selection: DumpRadioID? { record(selected) }

    private func record(_ id: DumpRadioID.ID?) -> DumpRadioID? {
        guard let id else { return nil }
        return store.radioIds.first { $0.id == id }
    }

    @ViewBuilder
    private func rowMenu(_ ids: Set<DumpRadioID.ID>) -> some View {
        if let radioId = record(ids.first) {
            Button("Edit…") { editing = radioId }
            Button("Move to Slot…") { moving = radioId }
            Divider()
            Button("Remove", role: .destructive) {
                selected = nil
                store.removeRadioId(radioId.index)
            }
        }
    }

    private func remove() {
        guard let radioId = selection else { return }
        selected = nil
        store.removeRadioId(radioId.index)
    }
}
