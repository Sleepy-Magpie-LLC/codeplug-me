import SwiftUI

/// A row in the sidebar: the radio itself, or one entity family in the open
/// codeplug.
enum SidebarItem: Hashable, Identifiable {
    case device
    case codeplug(CodeplugSection)

    var id: Self { self }

    var label: String {
        switch self {
        case .device: return "Device"
        case .codeplug(let section): return section.title
        }
    }

    var symbol: String {
        switch self {
        case .device: return "antenna.radiowaves.left.and.right"
        case .codeplug(let section): return section.symbol
        }
    }
}

/// Window shell: a sidebar selecting between the radio and the open codeplug's
/// entity families, over a status bar that reports the last action.
struct ContentView: View {
    @EnvironmentObject private var store: CodeplugStore
    @EnvironmentObject private var device: DeviceStore
    @State private var selection: SidebarItem? = .device

    var body: some View {
        NavigationSplitView {
            sidebar
        } detail: {
            detail
                .safeAreaInset(edge: .bottom, spacing: 0) { statusBar }
                // Open/Close ride here, above the panes, so they're reachable
                // even with no file open (when the codeplug panes are disabled).
                .toolbar { FileToolbar() }
        }
        .navigationTitle(store.fileURL?.lastPathComponent ?? "CodeplugMe")
        .navigationSubtitle(store.isDirty ? "Edited" : "")
        // A radio read/write can run for minutes and can be kicked off from any
        // pane, so its progress takes over the center of the window rather than
        // hiding in a corner.
        .overlay { if device.busy { busyOverlay } }
        .onAppear { store.checkForRecovery() }
        // A closed file can't have a section selected.
        .onChange(of: store.fileURL) { url in
            if url == nil, case .codeplug = selection { selection = .device }
        }
        // Opening a document for a radio without the selected section would
        // otherwise leave the sidebar pointing at a row that no longer exists.
        .onChange(of: store.capabilities) { caps in
            if case .codeplug(let section) = selection, !caps.sections.contains(section) {
                selection = .device
            }
        }
        .sheet(isPresented: $store.showingRepeaterImport) {
            RepeaterImportSheet(model: store.repeaterImport)
                .environmentObject(store)
        }
        .alert("Error", isPresented: errorBinding) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(store.errorMessage ?? "")
        }
        // Device errors surface here rather than on the Device pane: a write
        // started from a codeplug pane has to report its failure too.
        .alert("Radio Error", isPresented: deviceErrorBinding) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(device.errorMessage ?? "")
        }
        .alert("Restore unsaved changes?", isPresented: recoveryBinding,
               presenting: store.pendingRecovery) { manifest in
            Button("Restore") { store.restoreRecovery(manifest) }
            Button("Discard", role: .destructive) { store.discardRecovery() }
        } message: { manifest in
            Text("""
            Codeplug quit unexpectedly with unsaved changes to \
            "\(URL(fileURLWithPath: manifest.originalPath).lastPathComponent)". \
            You can pick up where you left off, or discard them and open the file as saved.
            """)
        }
        // Read/write completion. A finished read offers to load the file it just
        // produced straight into the editor.
        .alert(device.completion?.title ?? "", isPresented: completionBinding,
               presenting: device.completion) { completion in
            if completion.kind == .read, let url = completion.fileURL {
                Button("Open in Editor") {
                    store.open(url: url)
                    selection = store.sections.first.map(SidebarItem.codeplug) ?? .device
                }
                Button("Not Now", role: .cancel) {}
            } else {
                Button("OK", role: .cancel) {}
            }
        } message: { completion in
            Text(completion.message)
        }
    }

    /// Full-window scrim with a progress card, shown while a radio read or write
    /// is in flight.
    private var busyOverlay: some View {
        ZStack {
            Rectangle()
                .fill(.black.opacity(0.25))
                .ignoresSafeArea()
            VStack(spacing: Spacing.stack) {
                if let fraction = device.progressFraction {
                    ProgressView(value: fraction)
                        .frame(width: 220)
                    Text(device.progressText)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } else {
                    ProgressView()
                        .controlSize(.large)
                }
                if let status = device.statusMessage {
                    Text(status)
                        .font(.callout)
                        .multilineTextAlignment(.center)
                }
            }
            .padding(Spacing.section)
            .frame(minWidth: 260)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
            .shadow(radius: 20)
        }
    }

    private var sidebar: some View {
        List(selection: $selection) {
            Section("Radio") {
                row(.device)
            }
            Section("Codeplug") {
                ForEach(store.sections) { section in
                    row(.codeplug(section))
                        // Kept visible but inert with no file open: hiding these
                        // would make the sidebar jump on open and would hide what
                        // the app is for.
                        .disabled(store.fileURL == nil)
                }
            }
        }
        .listStyle(.sidebar)
        .navigationSplitViewColumnWidth(min: 180, ideal: 200, max: 260)
    }

    /// A sidebar row, with a dot when the section it names holds staged changes.
    private func row(_ item: SidebarItem) -> some View {
        HStack {
            Label(item.label, systemImage: item.symbol)
            Spacer()
            if case .codeplug(let section) = item, store.hasUnsavedChanges(section) {
                UnsavedDot()
            }
        }
        .tag(item)
    }

    @ViewBuilder
    private var detail: some View {
        switch selection {
        case .device:
            DeviceView()
        case .codeplug(let section):
            CodeplugView(section: section)
        case nil:
            Text("Select a section.")
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    /// Fixed-height footer. Always present so the layout doesn't shift when a
    /// message appears or clears.
    private var statusBar: some View {
        VStack(spacing: 0) {
            Divider()
            HStack(spacing: Spacing.inline) {
                Text(activeStatus ?? "")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer()
                if store.isDirty {
                    Label("Unsaved changes", systemImage: "pencil.circle.fill")
                        .font(.callout)
                        .foregroundStyle(.orange)
                }
            }
            .padding(.horizontal, Spacing.stack)
            .frame(height: 28)
        }
        .background(.bar)
    }

    /// The status of whatever the user is looking at. The two stores report
    /// independently, and showing a codeplug message while the Device pane is up
    /// (or vice versa) would just be confusing.
    ///
    /// The exception is a radio operation in flight: it can be started from a
    /// codeplug pane, and a multi-minute write with no visible progress would
    /// look like a hang.
    private var activeStatus: String? {
        if case .codeplug = selection, device.busy {
            return [device.statusMessage, device.progressText.isEmpty ? nil : device.progressText]
                .compactMap { $0 }
                .joined(separator: " — ")
        }
        switch selection {
        case .device: return device.statusMessage
        default: return store.statusMessage
        }
    }

    private var errorBinding: Binding<Bool> {
        Binding(get: { store.errorMessage != nil },
                set: { if !$0 { store.errorMessage = nil } })
    }

    private var deviceErrorBinding: Binding<Bool> {
        Binding(get: { device.errorMessage != nil },
                set: { if !$0 { device.errorMessage = nil } })
    }

    private var recoveryBinding: Binding<Bool> {
        Binding(get: { store.pendingRecovery != nil },
                set: { if !$0 { store.pendingRecovery = nil } })
    }

    private var completionBinding: Binding<Bool> {
        Binding(get: { device.completion != nil },
                set: { if !$0 { device.completion = nil } })
    }
}
