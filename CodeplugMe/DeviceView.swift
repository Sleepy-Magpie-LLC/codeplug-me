import SwiftUI
import AppKit
import UniformTypeIdentifiers

/// A finished read or write, surfaced as a modal so the result can't be missed
/// the way a one-line status message can. A completed read carries the file it
/// produced, so the dialog can offer to open it in the editor.
struct DeviceCompletion: Identifiable {
    enum Kind { case read, write }
    let id = UUID()
    let kind: Kind
    let title: String
    let message: String
    let fileURL: URL?
}

/// State and device operations for the Device tab. All radio I/O runs on a
/// background task; published state is only touched on the main actor.
@MainActor
final class DeviceStore: ObservableObject {
    @Published var ports: [PortEntry] = []
    @Published var selectedPort: String?
    @Published var model: String?

    /// The model the user picked by hand for a port the core couldn't attribute.
    /// This is the only way to reach a radio whose USB bridge isn't in the
    /// core's table — the QYT KT-5000 today — so it is a first-class control on
    /// the Device tab, not a fallback hidden behind an error.
    @Published var selectedModel: RadioModel?

    @Published var busy = false
    @Published var progressFraction: Double?
    @Published var progressText = ""
    @Published var statusMessage: String?
    @Published var errorMessage: String?

    /// The last completed read/write, shown as a modal. Cleared when dismissed.
    @Published var completion: DeviceCompletion?

    /// What the Device table lists: every port that could plausibly have a radio
    /// behind it, recognized ones first.
    ///
    /// A port with no USB identity at all is one of macOS's built-in Bluetooth
    /// or debug serial devices and is dropped. Unrecognized *USB* ports are kept
    /// on purpose: a radio whose bridge isn't in the core's table shows up as
    /// exactly that, and hiding it would leave the radio unreachable.
    var candidatePorts: [PortEntry] {
        let usable = ports.filter { $0.likelyRadio || $0.vid != nil }
        return usable.filter(\.likelyRadio) + usable.filter { !$0.likelyRadio }
    }

    /// The currently selected port's full entry.
    var selectedPortEntry: PortEntry? {
        ports.first { $0.name == selectedPort }
    }

    /// The user's hand-picked model, bound to the port it was picked for.
    var manualSelection: ManualModelSelection? {
        guard let port = selectedPort, let model = selectedModel else { return nil }
        return ManualModelSelection(port: port, model: model)
    }

    /// Which radio to speak to on the selected port: its USB attribution, or the
    /// model the user picked for it. Nil disables every device operation, since
    /// speaking the wrong protocol at an unknown radio is the thing to avoid.
    var resolvedModel: RadioModel? {
        guard let entry = selectedPortEntry else { return nil }
        return effectiveModel(of: entry, manual: manualSelection)
    }

    /// True when the selected port needs the user to say what is behind it.
    var needsModelSelection: Bool {
        selectedPortEntry != nil && selectedPortEntry?.model == nil
    }

    /// Re-enumerate serial ports, keeping the selection on a still-connected
    /// port and otherwise falling to the first candidate found.
    func refreshPorts() {
        do {
            ports = try RadioCore.listPorts()
            if selectedPort == nil || !candidatePorts.contains(where: { $0.name == selectedPort }) {
                selectedPort = candidatePorts.first?.name
                // A hand-picked model belongs to the port it was picked for.
                selectedModel = nil
            }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    /// Identify the selected radio and show its model string.
    func identify() {
        guard let port = selectedPort, let model = resolvedModel else { return }
        run(label: "Identifying…") { [weak self] in
            let identity = try RadioCore.identity(port: port, model: model)
            await MainActor.run {
                self?.model = identity.identity
                self?.statusMessage = "Identified: \(identity.identity)"
            }
        }
    }

    /// Read the full codeplug from the radio into `url`.
    func backup(to url: URL) {
        guard let port = selectedPort, let model = resolvedModel else { return }
        run(label: "Reading codeplug…") { [weak self] in
            try RadioCore.backup(port: port, model: model, to: url.path) { done, total in
                Task { @MainActor in self?.setProgress(done: done, total: total) }
            }
            await MainActor.run {
                self?.statusMessage = "Read complete — saved to \(url.lastPathComponent)"
                self?.completion = DeviceCompletion(
                    kind: .read,
                    title: "Read Complete",
                    message: "The radio's codeplug was read and saved to "
                        + "\(url.lastPathComponent). Open it in the editor to view or change it.",
                    fileURL: url)
            }
        }
    }

    /// Write the codeplug file at `url` back to the radio. Only called after
    /// the user confirmed the destructive dialog, hence force: true.
    ///
    /// `displayName` names the codeplug in the status message. The Codeplug tab
    /// writes the staging work file, whose own filename ("work.bin") would mean
    /// nothing to the user.
    func restore(from url: URL, displayName: String? = nil) {
        guard let port = selectedPort else { return }
        let name = displayName ?? url.lastPathComponent
        run(label: "Writing codeplug… do not disconnect!") { [weak self] in
            try RadioCore.restore(port: port, from: url.path, force: true) { done, total in
                Task { @MainActor in self?.setProgress(done: done, total: total) }
            }
            await MainActor.run {
                self?.statusMessage = "Write complete and verified (\(name))"
                self?.completion = DeviceCompletion(
                    kind: .write,
                    title: "Write Complete",
                    message: "\(name) was written to the radio and verified. "
                        + "You can safely disconnect the radio now.",
                    fileURL: nil)
            }
        }
    }

    /// Update the progress bar from a block-progress callback.
    private func setProgress(done: Int, total: Int) {
        progressFraction = total > 0 ? Double(done) / Double(total) : nil
        progressText = "\(done)/\(total) blocks"
    }

    /// Run one device operation on a background task, tracking busy/error
    /// state on the main actor.
    private func run(label: String, _ work: @escaping () async throws -> Void) {
        busy = true
        errorMessage = nil
        statusMessage = label
        progressFraction = nil
        progressText = ""
        Task.detached(priority: .userInitiated) { [weak self] in
            // Rebind to a constant: the store must outlive the operation so
            // completion state lands, and weak vars can't cross actors.
            guard let store = self else { return }
            do {
                try await work()
            } catch {
                await MainActor.run {
                    store.errorMessage = error.localizedDescription
                    store.statusMessage = nil
                }
            }
            await MainActor.run {
                store.busy = false
                store.progressFraction = nil
            }
        }
    }
}

/// Device tab: port list, identify, backup, and (gated) restore.
struct DeviceView: View {
    @EnvironmentObject private var store: DeviceStore
    @State private var showRestoreConfirm = false
    @State private var pendingRestoreURL: URL?

    var body: some View {
        VStack(alignment: .leading, spacing: Spacing.stack) {
            Table(store.candidatePorts, selection: $store.selectedPort) {
                TableColumn("Radio") { port in
                    Label(port.product ?? (port.likelyRadio ? "Radio" : "USB serial adapter"),
                          systemImage: port.likelyRadio
                              ? "antenna.radiowaves.left.and.right"
                              : "cable.connector")
                        .foregroundStyle(port.likelyRadio ? Color.green : Color.secondary)
                }
                TableColumn("Model") { port in
                    if let model = port.model {
                        Text(model.displayName)
                    } else {
                        Text("Unknown").foregroundStyle(.secondary).italic()
                    }
                }
                TableColumn("Port") { port in
                    Text(port.name).foregroundStyle(.secondary)
                }
            }
            .overlay { if store.candidatePorts.isEmpty { noRadioState } }

            if store.needsModelSelection { modelPicker }

            if let model = store.model {
                Text("Model: \(model)").font(.callout).textSelection(.enabled)
            }
            // In-flight progress is shown as a centered overlay on the whole
            // window (see ContentView.busyOverlay), since a read/write can be
            // started from a Codeplug pane too.
        }
        .padding(Spacing.section)
        .onAppear { store.refreshPorts() }
        // A hand-picked model describes one adapter, so moving to another port
        // drops it rather than silently reapplying it to different hardware.
        .onChange(of: store.selectedPort) { _ in store.selectedModel = nil }
        .toolbar {
            ToolbarItemGroup {
                Button { store.refreshPorts() } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                }
                .help("Re-scan for connected radios")

                Button("Identify") { store.identify() }
                    .help("Ask the radio what model it is")
                    .disabled(store.resolvedModel == nil || store.busy)

                Button("Read from Radio…") { chooseBackupDestination() }
                    .help("Read the radio's codeplug to a file")
                    .disabled(store.resolvedModel == nil || store.busy)

                Button("Write to Radio…") { chooseRestoreSource() }
                    .help("Overwrite the radio's codeplug from a file")
                    .disabled(store.selectedPort == nil || store.busy)
            }
        }
        // The device error alert lives on ContentView, not here: a write started
        // from the Codeplug tab has to be able to report a failure too.
        .alert("Write codeplug to radio?", isPresented: $showRestoreConfirm, presenting: pendingRestoreURL) { url in
            Button("Write to Radio", role: .destructive) { store.restore(from: url) }
            Button("Cancel", role: .cancel) {}
        } message: { url in
            Text("""
            This OVERWRITES the radio's entire configuration with \
            "\(url.lastPathComponent)". Take a fresh backup first. \
            Do not disconnect the cable or power off the radio during the write.
            """)
        }
    }

    /// Shown when no radio is connected — the normal state with the cable
    /// unplugged, which is worth saying rather than showing bare headers. The
    /// Refresh button is here as well as in the toolbar because this is where the
    /// user is looking when the list is empty.
    private var noRadioState: some View {
        VStack(spacing: Spacing.inline) {
            Image(systemName: "cable.connector.slash")
                .font(.system(size: 32))
                .foregroundStyle(.secondary)
            Text("No radio connected")
                .font(.title3.weight(.medium))
            Text("Connect the radio over USB and turn it on. A cable this app "
                + "doesn’t recognize still works: it appears here as an adapter "
                + "with an unknown model, and you choose the radio yourself.")
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 380)
            Button("Refresh") { store.refreshPorts() }
                .padding(.top, Spacing.tight)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.background)
    }

    /// The explicit model picker for a port whose USB identity names no radio.
    ///
    /// This is the only way to reach a radio the core can't recognize on sight
    /// (the QYT KT-5000, whose VID/PID we don't have yet), so it sits inline
    /// under the port list where the selection was made rather than behind a
    /// menu or a preference.
    private var modelPicker: some View {
        VStack(alignment: .leading, spacing: Spacing.tight) {
            Label("This adapter doesn’t say which radio is behind it.",
                  systemImage: "questionmark.circle")
                .font(.callout.weight(.medium))
            Text("Choose the model so the app knows which protocol to speak. "
                + "Picking the wrong one means the radio simply won’t answer.")
                .font(.caption)
                .foregroundStyle(.secondary)
            Picker("Radio model", selection: $store.selectedModel) {
                Text("Choose…").tag(RadioModel?.none)
                ForEach(RadioModel.allCases, id: \.self) { model in
                    Text(model.displayName).tag(RadioModel?.some(model))
                }
            }
            .pickerStyle(.menu)
            .frame(maxWidth: 320)
            .disabled(store.busy)
        }
        .padding(Spacing.inline)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
    }

    /// Ask where to save the codeplug read from the radio, then start the read.
    private func chooseBackupDestination() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "bin") ?? .data]
        panel.nameFieldStringValue = "codeplug.bin"
        panel.title = "Save Codeplug Read from Radio"
        if panel.runModal() == .OK, let url = panel.url {
            store.backup(to: url)
        }
    }

    /// Ask which .bin to restore, then show the destructive confirmation.
    private func chooseRestoreSource() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "bin") ?? .data]
        panel.allowsMultipleSelection = false
        panel.title = "Choose Codeplug to Write to Radio"
        if panel.runModal() == .OK, let url = panel.url {
            pendingRestoreURL = url
            showRestoreConfirm = true
        }
    }
}
