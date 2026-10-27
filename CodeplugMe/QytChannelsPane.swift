import SwiftUI

/// The QYT KT-5000's channel table.
///
/// This is a separate pane from `ChannelsPane` rather than a conditional one
/// because on macOS 13 a `Table`'s column list cannot contain an `if`:
/// `TableColumnBuilder.buildIf` is macOS 14.4, and there is no `#available`
/// escape from a result builder. Each radio therefore gets its own statically
/// typed table. It reads better too, since the columns this radio has (tones as
/// values, two power levels, no DMR anything) are not the columns the AnyTone
/// has.
///
/// Everything shared is shared: `EntityToolbar`, `UnsavedDot`, `TableEmptyState`
/// and `MoveSheet` are the same views the AnyTone panes use.
struct QytChannelsPane: View {
    @EnvironmentObject private var store: CodeplugStore
    @State private var selected = Set<DumpQytChannel.ID>()
    @State private var editing: DumpQytChannel?
    @State private var moving: DumpQytChannel?
    @State private var bulkEditing = false
    @State private var query = ""
    @State private var duplicatesOnly = false
    @State private var sortOrder = [KeyPathComparator(\DumpQytChannel.index)]

    var body: some View {
        VStack(spacing: 0) {
            filterBar
            Divider()
            table
        }
        // The search field matches the name and both frequencies. There is
        // no mode / colour-code / slot filter here, since this radio is
        // analog-only and has none of those fields.
        .searchable(text: $query, prompt: "Filter channels by name or frequency")
        .toolbar {
            EntityToolbar(noun: "Channel", canEdit: !selected.isEmpty,
                          onAdd: {
                              // Open the new record straight away: a blank
                              // "NEW" channel is never what the user wants
                              // to keep.
                              if let s = store.addQytChannel() {
                                  selected = [s]
                                  editing = store.qytChannels.first { $0.id == s }
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
            QytChannelEditorSheet(channel: channel,
                                  nameLimit: store.capabilities.channelNameLength,
                                  powerOptions: store.capabilities.powerOptions,
                                  bandwidthOptions: store.capabilities.bandwidthOptions) {
                store.update($0)
            }
        }
        .sheet(item: $moving) { channel in
            MoveSheet(title: "Move Channel \(formatSlot(channel.index))",
                      currentIndex: channel.index) { target in
                store.moveQytChannel(channel.index, to: target)
                selected = store.qytChannels.contains { $0.id == target }
                    ? [target] : [channel.index]
            }
        }
        .sheet(isPresented: $bulkEditing) {
            BulkQytChannelEditorSheet(channels: selectedChannels) { update in
                store.bulkUpdateQytChannels(update)
            }
        }
    }

    /// The table itself, split out of `body` so the modifiers above stay legible.
    private var table: some View {
        // Over every channel, not the filtered rows, so a twin hidden by the
        // search still flags the visible one.
        let duplicates = duplicateQytChannelSlots(store.qytChannels)
        return Table(rows, selection: $selected, sortOrder: $sortOrder) {
            // The duplicate mark shares this column: the table is already at
            // the ten columns macOS 13 allows.
            TableColumn("") { row in
                HStack(spacing: 2) {
                    if store.isUnsaved(.qytChannels, slot: row.index) { UnsavedDot() }
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
            TableColumn("TX (MHz)", value: \.txSortKey) { row in
                Text(verbatim: formatOptionalMHz(row.txFrequencyHz)).monospacedDigit()
                    .foregroundStyle(duplicates[row.index] == nil ? Color.primary : Color.red)
            }
            TableColumn("RX tone", value: \.rxToneLabel) { Text(verbatim: $0.rxTone.label) }
                .width(min: 60, ideal: 80)
            TableColumn("TX tone", value: \.txToneLabel) { Text(verbatim: $0.txTone.label) }
                .width(min: 60, ideal: 80)
            TableColumn("Power", value: \.power) { Text($0.power) }
                .width(min: 46, ideal: 60, max: 80)
            TableColumn("BW", value: \.bandwidth) { Text($0.bandwidth) }
                .width(min: 46, ideal: 60, max: 80)
            // Only the exceptional state is drawn: a table of two hundred rows
            // all reading "Scan" says nothing, and the skipped few are what the
            // user is looking for.
            TableColumn("Scan", value: \.scanLabel) { row in
                if !row.scanAdd { Text("Skip").foregroundStyle(.secondary) }
            }
            .width(min: 40, ideal: 48, max: 64)
        }
        .contextMenu(forSelectionType: DumpQytChannel.ID.self) { ids in
            rowMenu(ids)
        } primaryAction: { ids in
            if ids.count == 1 { editing = record(ids.first) }
        }
        .overlay {
            if rows.isEmpty {
                TableEmptyState(noun: "channels", query: query, filtered: duplicatesOnly)
            }
        }
    }

    /// The duplicates-only switch, the one filter beyond the search field.
    private var filterBar: some View {
        HStack(spacing: Spacing.stack) {
            Toggle("Duplicates only", isOn: duplicatesOnlyBinding)
                .toggleStyle(.checkbox)
                .help("Show only channels that duplicate another, grouped by RX / TX frequency")
            Spacer()
        }
        .padding(.horizontal, Spacing.stack)
        .padding(.vertical, Spacing.inline)
    }

    /// Turning duplicates-only on sorts by RX then TX so each group of twins
    /// lands together; turning it off goes back to slot order. It sets the
    /// table's own sort order, so the column headers still re-sort.
    private var duplicatesOnlyBinding: Binding<Bool> {
        Binding(
            get: { duplicatesOnly },
            set: { on in
                duplicatesOnly = on
                sortOrder = on ? qytChannelFrequencySortOrder : [KeyPathComparator(\DumpQytChannel.index)]
            }
        )
    }

    private var rows: [DumpQytChannel] {
        let duplicates = duplicatesOnly ? duplicateQytChannelSlots(store.qytChannels) : [:]
        return store.qytChannels
            .filter {
                qytChannelMatchesFilter($0, query: query)
                    && (!duplicatesOnly || duplicates[$0.index] != nil)
            }
            .sorted(using: sortOrder)
    }

    /// The channels currently selected, for the bulk editor.
    private var selectedChannels: [DumpQytChannel] {
        store.qytChannels.filter { selected.contains($0.id) }
    }

    /// The one selected record, or nil when the selection isn't a single row.
    private var singleSelection: DumpQytChannel? {
        selected.count == 1 ? record(selected.first) : nil
    }

    private func record(_ id: DumpQytChannel.ID?) -> DumpQytChannel? {
        guard let id else { return nil }
        return store.qytChannels.first { $0.id == id }
    }

    @ViewBuilder
    private func rowMenu(_ ids: Set<DumpQytChannel.ID>) -> some View {
        if ids.count == 1, let channel = record(ids.first) {
            Button("Edit…") { editing = channel }
            Button("Move to Slot…") { moving = channel }
            Divider()
            Button("Remove", role: .destructive) {
                selected = []
                store.removeQytChannel(channel.index)
            }
        } else if ids.count > 1 {
            Button("Bulk Edit \(ids.count) Channels…") { bulkEditing = true }
            Divider()
            Button("Remove \(ids.count) Channels", role: .destructive) {
                let indices = ids.sorted()
                selected = []
                for i in indices.reversed() { store.removeQytChannel(i) }
            }
        }
    }

    private func remove() {
        let indices = selected.sorted()
        selected = []
        for i in indices.reversed() { store.removeQytChannel(i) }
    }
}
