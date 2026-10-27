import SwiftUI
import AppKit
import UniformTypeIdentifiers

/// Where an unsaved session lives on disk, so a crash can't cost the user their
/// work. The work file holds every staged change; the manifest records which
/// document it belongs to.
///
/// This is a plain namespace rather than a type because there is exactly one
/// recovery slot: the app is effectively single-document.
enum Recovery {
    /// `~/Library/Application Support/CodeplugMe/Recovery`, unless
    /// `CODEPLUGME_RECOVERY_DIR` overrides it. The override exists so tests can
    /// exercise the real recovery lifecycle without touching — or destroying —
    /// a genuine unsaved session belonging to the installed app.
    static var directory: URL {
        if let override = ProcessInfo.processInfo.environment["CODEPLUGME_RECOVERY_DIR"] {
            return URL(fileURLWithPath: override, isDirectory: true)
        }
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return base.appendingPathComponent("CodeplugMe/Recovery", isDirectory: true)
    }

    /// Where builds from before the CodeplugMe rename kept their recovery slot.
    static var legacyDirectory: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return base.appendingPathComponent("AnyToneMac/Recovery", isDirectory: true)
    }

    /// One-time move of the pre-rename recovery slot into `directory`, so an
    /// unsaved session survives the upgrade. Skipped when the env override is
    /// set, so tests never touch the real slot.
    static func migrateLegacyIfNeeded() {
        guard ProcessInfo.processInfo.environment["CODEPLUGME_RECOVERY_DIR"] == nil else { return }
        migrate(from: legacyDirectory, to: directory)
    }

    /// Moves `legacy` to `current` when only the legacy folder exists. Never
    /// overwrites an existing `current`. Failures are logged, not thrown: a
    /// missed migration only loses the recovery prompt, not a saved file.
    static func migrate(from legacy: URL, to current: URL) {
        let fm = FileManager.default
        guard fm.fileExists(atPath: legacy.path), !fm.fileExists(atPath: current.path) else { return }
        do {
            try fm.createDirectory(at: current.deletingLastPathComponent(), withIntermediateDirectories: true)
            try fm.moveItem(at: legacy, to: current)
        } catch {
            NSLog("Recovery migration from \(legacy.path) failed: \(error)")
        }
    }

    static var workURL: URL { directory.appendingPathComponent("work.bin") }
    static var manifestURL: URL { directory.appendingPathComponent("manifest.json") }

    /// Points a recovered work file back at the document it was edited from.
    struct Manifest: Codable {
        let originalPath: String
        let modifiedAt: Date
    }

    /// Create the recovery directory if it isn't there yet.
    static func ensureDirectory() throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    }

    /// The pending session, if a previous run left one behind and its original
    /// document still exists. A manifest pointing at a deleted or moved file is
    /// treated as stale and cleared, since Save would have nowhere to go.
    static func pending() -> Manifest? {
        guard let data = try? Data(contentsOf: manifestURL),
              let manifest = try? JSONDecoder().decode(Manifest.self, from: data),
              FileManager.default.fileExists(atPath: workURL.path),
              FileManager.default.fileExists(atPath: manifest.originalPath)
        else {
            clear()
            return nil
        }
        return manifest
    }

    /// Record that `workURL` holds unsaved changes to `original`.
    static func mark(original: URL) throws {
        try ensureDirectory()
        let manifest = Manifest(originalPath: original.path, modifiedAt: Date())
        try JSONEncoder().encode(manifest).write(to: manifestURL, options: .atomic)
    }

    /// Drop the "there is unsaved work here" marker, keeping the work file.
    /// Used after open and save: the work file is the live staging buffer and
    /// must survive, but there is nothing to recover at that moment.
    static func clearManifest() {
        try? FileManager.default.removeItem(at: manifestURL)
    }

    /// Drop the pending session entirely, work file included. Only for
    /// discarding recovered work — during normal editing the work file is the
    /// staging buffer and deleting it would strand the session.
    static func clear() {
        clearManifest()
        try? FileManager.default.removeItem(at: workURL)
    }
}

/// State for the offline codeplug editor.
///
/// The user's document is never written until `save()`. Every staged change is
/// applied to a work file (`Recovery.workURL`) instead, and the in-memory
/// records are always a dump of that work file. This keeps the Rust core as the
/// single source of truth for slot allocation and record encoding — the Swift
/// side never has to model a half-applied edit.
///
/// Applying edits repeatedly to the work file is lossless: `Codeplug::serialize`
/// starts from the parsed raw image and patches only the slots it touched, so an
/// untouched record round-trips byte-for-byte.
@MainActor
final class CodeplugStore: ObservableObject {
    /// The user's document. Written only by `save()`.
    @Published private(set) var fileURL: URL?

    /// Which radio the open document belongs to, or nil with nothing open.
    @Published private(set) var model: RadioModel?

    /// What the open document's radio can hold. Falls back to the AnyTone shape
    /// with nothing open so the sidebar doesn't collapse and then jump when a
    /// file is opened.
    @Published private(set) var capabilities: Capabilities = .anyTonePlaceholder

    @Published private(set) var channels: [DumpChannel] = []
    @Published private(set) var zones: [DumpZone] = []
    @Published private(set) var scanLists: [DumpScanList] = []
    /// Read-only APRS settings (no edit path — the block is never written).
    @Published private(set) var aprs: DumpAprs?
    @Published private(set) var contacts: [DumpContact] = []
    @Published private(set) var groupLists: [DumpGroupList] = []
    @Published private(set) var radioIds: [DumpRadioID] = []

    // The QYT payload. Exactly one of the two families is populated at a time:
    // `load` fills the one the open document's model names and clears the other.
    @Published private(set) var qytChannels: [DumpQytChannel] = []
    @Published private(set) var qytSettings: DumpQytSettings?
    @Published private(set) var qytBands: [DumpQytBand] = []
    /// The LCD static message at `0x0FE0`, empty with no QYT document open.
    @Published private(set) var qytStaticMessage = ""

    /// True when the work file holds changes the document doesn't have yet.
    @Published private(set) var isDirty = false

    /// Slots holding staged, unsaved changes, per section. Drives the per-row
    /// indicator in the tables.
    @Published private(set) var unsavedSlots: [CodeplugSection: Set<Int>] = [:]

    /// Sections holding any staged change. This can't be derived from
    /// `unsavedSlots`: a removal is a real unsaved change but leaves no row
    /// behind to mark, and the sidebar still has to show the section as edited.
    @Published private(set) var unsavedSections: Set<CodeplugSection> = []

    @Published private(set) var statusMessage: String?
    @Published var errorMessage: String?

    /// Drives the "Add Repeaters from Database…" sheet, which the File menu
    /// opens from outside the view hierarchy.
    @Published var showingRepeaterImport = false
    /// The repeater import's search and results, kept for the app session so
    /// reopening the sheet picks up where it was left.
    let repeaterImport = RepeaterImportModel()

    /// Whether an undo or redo is currently available. Drives the Edit-menu
    /// items; the stacks themselves are private.
    @Published private(set) var canUndo = false
    @Published private(set) var canRedo = false

    /// Undo/redo history as full work-file snapshots. Each staged edit pushes the
    /// pre-edit state; undo/redo swap whole snapshots in and out. Snapshotting the
    /// bytes rather than inverting each operation keeps the Rust core the single
    /// source of truth and makes every edit — add, remove, move, bulk — reversible
    /// through one path.
    private var undoStack: [EditSnapshot] = []
    private var redoStack: [EditSnapshot] = []

    /// A previous session's unsaved work, found at launch. The UI offers to
    /// restore it; `nil` once handled.
    @Published var pendingRecovery: Recovery.Manifest?

    /// Cancels the previous auto-clear when a new status arrives, so messages
    /// don't wink out early.
    private var statusClearTask: Task<Void, Never>?

    // MARK: - Opening

    /// Look for unsaved work left by a previous run. Call once at launch.
    func checkForRecovery() {
        Recovery.migrateLegacyIfNeeded()
        pendingRecovery = Recovery.pending()
    }

    /// Adopt a recovered session: the work file already holds the changes, so
    /// dump it as-is and come up dirty.
    func restoreRecovery(_ manifest: Recovery.Manifest) {
        let original = URL(fileURLWithPath: manifest.originalPath)
        do {
            load(try RadioCore.dump(binPath: Recovery.workURL.path), url: original)
            isDirty = true
            // The work file records the result of the recovered edits, not which
            // slots they touched, so every section is flagged and no row is. The
            // alternative — showing nothing as edited — would be a lie.
            unsavedSlots = [:]
            unsavedSections = Set(sections)
            clearHistory()
            pendingRecovery = nil
            setStatus("Restored unsaved changes to \(original.lastPathComponent)")
        } catch {
            errorMessage = "Could not restore the recovered session: \(error.localizedDescription)"
            Recovery.clear()
            pendingRecovery = nil
        }
    }

    /// Throw away a recovered session.
    func discardRecovery() {
        Recovery.clear()
        pendingRecovery = nil
    }

    /// Ask which codeplug .bin to open, then open it. Lives on the store because
    /// the toolbar, the File menu, and the empty state all need it.
    func openWithPanel() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "bin") ?? .data]
        panel.allowsMultipleSelection = false
        panel.title = "Open Codeplug"
        if panel.runModal() == .OK, let url = panel.url {
            open(url: url)
        }
    }

    /// Open and parse a codeplug .bin (offline, no radio), seeding the work file
    /// with a pristine copy.
    func open(url: URL) {
        // Reject an incompatible file up front. A codeplug from a different radio
        // model or an older firmware has a different total size, so a length
        // mismatch is a clear "wrong version" signal — and catching it here gives
        // the user a plain-language reason instead of a low-level parse error
        // after the file has already been copied into the work slot.
        if let incompatibility = incompatibilityReason(for: url) {
            errorMessage = incompatibility
            return
        }
        do {
            try Recovery.ensureDirectory()
            try replaceFile(at: Recovery.workURL, withContentsOf: url)
            load(try RadioCore.dump(binPath: Recovery.workURL.path), url: url)
            isDirty = false
            clearUnsaved()
            clearHistory()
            Recovery.clearManifest()
            setStatus("Opened \(url.lastPathComponent): \(entitySummary)")
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    /// What the open document holds, counted per the radio it belongs to. The
    /// QYT has no zones, contacts, group lists or radio IDs, so reporting the
    /// AnyTone families for it would print five zeros.
    private var entitySummary: String {
        if model == .qytKt5000 {
            return "\(qytChannels.count) channels, \(qytBands.count) VFO channels"
        }
        return "\(channels.count) channels, \(zones.count) zones, "
            + "\(contacts.count) contacts, \(groupLists.count) group lists, "
            + "\(radioIds.count) radio IDs"
    }

    /// A plain-language reason `url` is not a codeplug this build can open, or
    /// nil if its size matches what the core expects. Split out of `open` so the
    /// version gate can be exercised in tests without a modal panel.
    func incompatibilityReason(for url: URL) -> String? {
        guard let size = try? FileManager.default
            .attributesOfItem(atPath: url.path)[.size] as? Int else {
            return nil // Unreadable size: let the normal open path report the I/O error.
        }
        // The core matches the file's bytes against every radio it supports, so
        // a file it recognizes is openable whichever radio it turns out to be for.
        if (try? RadioCore.detectModel(binPath: url.path)) != nil { return nil }
        let known = RadioCore.supportedModels
            .map { "\($0.displayName) (\($0.imageSize) bytes)" }
            .joined(separator: ", ")
        return "“\(url.lastPathComponent)” isn’t a compatible codeplug. It is "
            + "\(size) bytes, which matches no radio this app supports: \(known). It looks "
            + "like it came from a different radio model or an older, incompatible firmware "
            + "version."
    }

    /// The sections the open document's radio has, in sidebar order.
    var sections: [CodeplugSection] { capabilities.sections }

    /// Close the open codeplug and return to the empty state. Clears the staging
    /// work file and undo history too, so nothing from this document lingers to
    /// be picked up or half-applied later. Callers guard against discarding
    /// unsaved changes before calling this.
    func close() {
        fileURL = nil
        model = nil
        capabilities = .anyTonePlaceholder
        channels = []
        zones = []
        scanLists = []
        aprs = nil
        contacts = []
        groupLists = []
        radioIds = []
        qytChannels = []
        qytSettings = nil
        qytBands = []
        qytStaticMessage = ""
        isDirty = false
        errorMessage = nil
        clearUnsaved()
        clearHistory()
        Recovery.clear()
        setStatus("Closed codeplug")
    }

    /// Populate the records from a parsed dump. Split out of `open` so it can be
    /// driven from a synthetic dump without the FFI or a real file.
    ///
    /// A dump carries exactly one model's payload, so the other model's records
    /// are cleared here rather than left behind: a stale AnyTone channel list
    /// under a QYT document would be a different radio's data still on screen.
    func load(_ dump: CodeplugDump, url: URL) {
        model = dump.model
        capabilities = dump.capabilities
        let anytone = dump.anytone
        channels = anytone?.channels ?? []
        zones = anytone?.zones ?? []
        scanLists = anytone?.scanLists ?? []
        aprs = anytone?.aprs
        contacts = anytone?.contacts ?? []
        groupLists = anytone?.groupLists ?? []
        radioIds = anytone?.radioIds ?? []
        let qyt = dump.qyt
        qytChannels = qyt?.channels ?? []
        qytSettings = qyt?.settings
        qytBands = qyt?.bands ?? []
        qytStaticMessage = qyt?.staticMessage ?? ""
        fileURL = url
        errorMessage = nil
    }

    // MARK: - Saving

    /// Write the staged work file over the user's document.
    func save() {
        guard let url = fileURL, isDirty else { return }
        write(to: url)
    }

    /// Ask for a destination, write the staged state there, and continue editing
    /// that document instead of the old one.
    ///
    /// Unlike `save()` this runs even with nothing staged: "save this codeplug
    /// under a new name" is a reasonable thing to want from an unmodified file.
    func saveAs() {
        guard fileURL != nil else { return }
        let panel = NSSavePanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "bin") ?? .data]
        panel.nameFieldStringValue = fileURL?.lastPathComponent ?? "codeplug.bin"
        panel.title = "Save Codeplug As"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        write(to: url)
    }

    /// Write the work file's bytes to `url` and adopt it as the open document.
    ///
    /// Copying the bytes rather than `replaceItem`-ing the work file into place
    /// covers both callers with one path: Save As targets a file that may not
    /// exist yet, which `replaceItem` rejects. It also leaves the work file
    /// intact, so editing continues without re-seeding it.
    ///
    /// Internal rather than private so tests can drive a Save As without
    /// standing up a modal panel.
    func write(to url: URL) {
        do {
            try Data(contentsOf: Recovery.workURL).write(to: url, options: .atomic)
            fileURL = url
            isDirty = false
            clearUnsaved()
            Recovery.clearManifest()
            setStatus("Saved \(url.lastPathComponent)")
        } catch {
            errorMessage = "Could not save \(url.lastPathComponent): \(error.localizedDescription)"
        }
    }

    /// Throw away every staged change and reload the document from disk.
    func discardChanges() {
        guard let url = fileURL else { return }
        open(url: url)
        setStatus("Discarded unsaved changes to \(url.lastPathComponent)")
    }

    // MARK: - Field edits

    /// Stage an edited record. Each looks the record up by slot, not by array
    /// position, so a sorted or filtered table can't commit to the wrong row.

    func update(_ ch: DumpChannel) {
        guard let old = channels.first(where: { $0.index == ch.index }), old != ch else { return }
        var e = ChannelEdit(index: ch.index)
        if ch.name != old.name { e.name = ch.name }
        if ch.rxFrequencyHz != old.rxFrequencyHz { e.rxFrequencyHz = ch.rxFrequencyHz }
        if ch.txFrequencyHz != old.txFrequencyHz { e.txFrequencyHz = ch.txFrequencyHz }
        if ch.mode != old.mode { e.mode = editValue(ch.mode) }
        if ch.power != old.power { e.power = editValue(ch.power) }
        if ch.bandwidth != old.bandwidth { e.bandwidth = editValue(ch.bandwidth) }
        if ch.colorCode != old.colorCode { e.colorCode = ch.colorCode }
        if ch.timeSlot != old.timeSlot { e.timeSlot = ch.timeSlot }
        if ch.contactIndex != old.contactIndex { e.contactIndex = ch.contactIndex }
        if ch.radioIdIndex != old.radioIdIndex { e.radioIdIndex = ch.radioIdIndex }
        if ch.groupListIndex != old.groupListIndex { e.groupListIndex = ch.groupListIndex }
        if ch.rxSignalingMode != old.rxSignalingMode { e.rxSignalingMode = snakeEditValue(ch.rxSignalingMode) }
        if ch.txSignalingMode != old.txSignalingMode { e.txSignalingMode = snakeEditValue(ch.txSignalingMode) }
        if ch.rxCtcss != old.rxCtcss { e.rxCtcss = ch.rxCtcss }
        if ch.txCtcss != old.txCtcss { e.txCtcss = ch.txCtcss }
        if ch.rxDcs != old.rxDcs { e.rxDcs = ch.rxDcs }
        if ch.txDcs != old.txDcs { e.txDcs = ch.txDcs }
        if ch.squelchMode != old.squelchMode { e.squelchMode = snakeEditValue(ch.squelchMode) }
        if ch.optionalSignaling != old.optionalSignaling { e.optionalSignaling = snakeEditValue(ch.optionalSignaling) }
        if ch.admit != old.admit { e.admit = snakeEditValue(ch.admit) }
        if ch.scanListIndex != old.scanListIndex { e.scanListIndex = ch.scanListIndex }
        if ch.dtmfIdIndex != old.dtmfIdIndex { e.dtmfIdIndex = ch.dtmfIdIndex }
        if ch.twoToneIdIndex != old.twoToneIdIndex { e.twoToneIdIndex = ch.twoToneIdIndex }
        if ch.fiveToneIdIndex != old.fiveToneIdIndex { e.fiveToneIdIndex = ch.fiveToneIdIndex }
        if ch.twoToneDecodeIndex != old.twoToneDecodeIndex { e.twoToneDecodeIndex = ch.twoToneDecodeIndex }
        if ch.rxOnly != old.rxOnly { e.rxOnly = ch.rxOnly }
        if ch.talkAround != old.talkAround { e.talkAround = ch.talkAround }
        if ch.callConfirm != old.callConfirm { e.callConfirm = ch.callConfirm }
        if ch.workAlone != old.workAlone { e.workAlone = ch.workAlone }
        if ch.simplexTdma != old.simplexTdma { e.simplexTdma = ch.simplexTdma }
        if ch.rxAprs != old.rxAprs { e.rxAprs = ch.rxAprs }
        if stage({ $0.channels.append(e) }, "Updated channel \(formatSlot(ch.index))") {
            markUnsaved(.channels, slots: [ch.index])
        }
    }

    func update(_ sl: DumpScanList) {
        guard let old = scanLists.first(where: { $0.index == sl.index }), old != sl else { return }
        var e = ScanListEdit(index: sl.index)
        if sl.name != old.name { e.name = sl.name }
        if sl.members != old.members { e.members = sl.members }
        if sl.priorityChannelSelect != old.priorityChannelSelect { e.priorityChannelSelect = sl.priorityChannelSelect }
        if sl.priorityChannel1 != old.priorityChannel1 { e.priorityChannel1 = sl.priorityChannel1 }
        if sl.priorityChannel2 != old.priorityChannel2 { e.priorityChannel2 = sl.priorityChannel2 }
        if sl.lookBackA != old.lookBackA { e.lookBackA = sl.lookBackA }
        if sl.lookBackB != old.lookBackB { e.lookBackB = sl.lookBackB }
        if sl.dropoutDelay != old.dropoutDelay { e.dropoutDelay = sl.dropoutDelay }
        if sl.dwellTime != old.dwellTime { e.dwellTime = sl.dwellTime }
        if sl.revertChannel != old.revertChannel { e.revertChannel = sl.revertChannel }
        if stage({ $0.scanLists.append(e) }, "Updated scan list \(formatSlot(sl.index))") {
            markUnsaved(.scanLists, slots: [sl.index])
        }
    }

    func update(_ z: DumpZone) {
        guard let old = zones.first(where: { $0.index == z.index }), old != z else { return }
        var e = ZoneEdit(index: z.index)
        if z.name != old.name { e.name = z.name }
        if z.channels != old.channels { e.members = z.channels }
        if stage({ $0.zones.append(e) }, "Updated zone \(formatSlot(z.index))") {
            markUnsaved(.zones, slots: [z.index])
        }
    }

    func update(_ c: DumpContact) {
        guard let old = contacts.first(where: { $0.index == c.index }), old != c else { return }
        var e = ContactEdit(index: c.index)
        if c.name != old.name { e.name = c.name }
        if c.number != old.number { e.number = c.number }
        if c.callType != old.callType { e.callType = editValue(c.callType) }
        if stage({ $0.contacts.append(e) }, "Updated contact \(formatSlot(c.index))") {
            markUnsaved(.contacts, slots: [c.index])
        }
    }

    func update(_ g: DumpGroupList) {
        guard let old = groupLists.first(where: { $0.index == g.index }), old != g else { return }
        var e = GroupListEdit(index: g.index)
        if g.name != old.name { e.name = g.name }
        if g.members != old.members { e.members = g.members }
        if stage({ $0.groupLists.append(e) }, "Updated group list \(formatSlot(g.index))") {
            markUnsaved(.groupLists, slots: [g.index])
        }
    }

    func update(_ r: DumpRadioID) {
        guard let old = radioIds.first(where: { $0.index == r.index }), old != r else { return }
        var e = RadioIDEdit(index: r.index)
        if r.name != old.name { e.name = r.name }
        if r.number != old.number { e.number = r.number }
        if stage({ $0.radioIds.append(e) }, "Updated radio ID \(formatSlot(r.index))") {
            markUnsaved(.radioIds, slots: [r.index])
        }
    }

    /// Stage an edited QYT channel. Only the fields the core has setters for are
    /// diffed; power, bandwidth and the remaining flag bits are decoded for
    /// display but not yet writable, so they are not sent.
    func update(_ ch: DumpQytChannel) {
        guard let old = qytChannels.first(where: { $0.index == ch.index }), old != ch else { return }
        var e = QytChannelEdit(index: ch.index)
        if ch.name != old.name { e.name = ch.name }
        if ch.rxFrequencyHz != old.rxFrequencyHz { e.rxFrequencyHz = ch.rxFrequencyHz }
        if ch.txFrequencyHz != old.txFrequencyHz {
            // An absent TX frequency has to be sent as an explicit null, which
            // is how the core is told to make the channel receive-only.
            if let hz = ch.txFrequencyHz { e.txFrequencyHz = hz } else { e.clearTxFrequency = true }
        }
        if ch.rxTone != old.rxTone { e.rxTone = ch.rxTone }
        if ch.txTone != old.txTone { e.txTone = ch.txTone }
        if ch.scanAdd != old.scanAdd { e.scanAdd = ch.scanAdd }
        if stageQyt({ $0.channels.append(e) }, "Updated channel \(formatSlot(ch.index))") {
            markUnsaved(.qytChannels, slots: [ch.index])
        }
    }

    /// Stage the settings pane's draft. Only the fields that differ from the
    /// open document are sent, so the four unnamed bytes inside the header — and
    /// the whole unmodelled region above it — are never rewritten.
    ///
    /// This is one call per Apply rather than one per keystroke on purpose: each
    /// staging round trip is a full parse, edit, serialize and re-dump in the
    /// core, and it produces one undo entry.
    /// The LCD static message rides along because it lives on the same pane and
    /// a user changing both expects one Apply, not two undo entries.
    func updateQytSettings(_ s: DumpQytSettings, staticMessage: String) {
        guard let old = qytSettings else { return }
        let edit = qytSettingsEdit(from: old, to: s)
        let message = staticMessage == qytStaticMessage ? nil : staticMessage
        guard !edit.isEmpty || message != nil else { return }
        let noun = edit.isEmpty ? "Updated display message" : "Updated settings"
        if stageQyt({
            if !edit.isEmpty { $0.settings = edit }
            $0.staticMessage = message
        }, noun) {
            // The header has no slots, so only the section is marked.
            markUnsaved(.qytSettings, slots: [])
        }
    }

    /// Stage the VFO pane's draft: both slots diffed, one round trip.
    func updateQytBands(_ bands: [DumpQytBand]) {
        var edits: [QytBandEdit] = []
        for band in bands {
            guard let old = qytBands.first(where: { $0.index == band.index }) else { continue }
            let edit = qytBandEdit(from: old, to: band)
            if !edit.isEmpty { edits.append(edit) }
        }
        guard !edits.isEmpty else { return }
        let noun = edits.count == 1
            ? "VFO \(qytBandName(edits[0].index))"
            : "\(edits.count) VFO channels"
        if stageQyt({ $0.bands = edits }, "Updated \(noun)") {
            markUnsaved(.qytBands, slots: edits.map(\.index))
        }
    }

    /// Stage a bulk update to multiple channels at once. Each field in
    /// `update` that is non-nil is applied to every listed channel index.
    func bulkUpdateChannels(_ update: BulkChannelUpdate) {
        guard !update.indices.isEmpty else { return }
        var spec = EditSpec()
        for index in update.indices {
            var e = ChannelEdit(index: index)
            if let v = update.mode { e.mode = editValue(v) }
            if let v = update.power { e.power = editValue(v) }
            if let v = update.bandwidth { e.bandwidth = editValue(v) }
            if let v = update.colorCode { e.colorCode = v }
            if let v = update.timeSlot { e.timeSlot = v }
            if let v = update.contactIndex { e.contactIndex = v }
            if let v = update.groupListIndex { e.groupListIndex = v }
            if let v = update.radioIdIndex { e.radioIdIndex = v }
            spec.channels.append(e)
        }
        if bulkStage(spec, "Bulk-updated \(update.indices.count) channels") {
            markUnsaved(.channels, slots: update.indices)
        }
    }

    /// Stage a bulk update to multiple QYT channels. Only non-nil fields are
    /// sent, through the same tagged envelope a single-channel edit uses.
    func bulkUpdateQytChannels(_ update: BulkQytChannelUpdate) {
        guard !update.indices.isEmpty else { return }
        // Nothing opted in: leave the document alone rather than round-tripping
        // an empty channel list through the core.
        guard update.scanAdd != nil else { return }
        if stageQyt({ spec in
            for index in update.indices {
                var e = QytChannelEdit(index: index)
                if let v = update.scanAdd { e.scanAdd = v }
                spec.channels.append(e)
            }
        }, "Bulk-updated \(update.indices.count) channels") {
            markUnsaved(.qytChannels, slots: update.indices)
        }
    }

    // MARK: - Add / remove

    /// Each add returns the slot the core placed the record in, so the caller can
    /// select it. The FFI doesn't report the new index, so it's recovered by
    /// diffing the slot set across the reload.

    func addChannel() -> Int? {
        // A fresh channel points at RX group list "None" (0xff) rather than the
        // core's default of slot 0, which would silently attach group list #1.
        addedSlot(of: \.channels, section: .channels, noun: "channel") {
            $0.addChannels.append(ChannelEdit(groupListIndex: 255))
        }
    }

    func addZone() -> Int? {
        addedSlot(of: \.zones, section: .zones, noun: "zone") { $0.addZones.append(ZoneEdit()) }
    }

    func addScanList() -> Int? {
        addedSlot(of: \.scanLists, section: .scanLists, noun: "scan list") { $0.addScanLists.append(ScanListEdit()) }
    }

    func addContact() -> Int? {
        addedSlot(of: \.contacts, section: .contacts, noun: "contact") { $0.addContacts.append(ContactEdit(callType: "group")) }
    }

    func addGroupList() -> Int? {
        addedSlot(of: \.groupLists, section: .groupLists, noun: "group list") { $0.addGroupLists.append(GroupListEdit()) }
    }

    func addRadioId() -> Int? {
        addedSlot(of: \.radioIds, section: .radioIds, noun: "radio ID") { $0.addRadioIds.append(RadioIDEdit()) }
    }

    /// Program the first free QYT channel slot. Written out rather than routed
    /// through `addedSlot` because that helper stages an AnyTone `EditSpec`;
    /// the slot-diffing it does is the four lines below.
    func addQytChannel() -> Int? {
        let before = Set(qytChannels.map(\.id))
        guard stageQyt({ $0.addChannels.append(QytChannelEdit()) }, "Added channel") else {
            return nil
        }
        guard let slot = qytChannels.map(\.id).first(where: { !before.contains($0) }) else {
            return nil
        }
        markUnsaved(.qytChannels, slots: [slot])
        setStatus("Added channel \(formatSlot(slot)) — unsaved")
        return slot
    }

    func removeChannel(_ i: Int) {
        if stage({ $0.removeChannels.append(i) }, "Removed channel \(formatSlot(i))") {
            markRemoved(.channels, slot: i)
        }
    }

    func removeQytChannel(_ i: Int) {
        if stageQyt({ $0.removeChannels.append(i) }, "Removed channel \(formatSlot(i))") {
            markRemoved(.qytChannels, slot: i)
        }
    }

    func removeZone(_ i: Int) {
        if stage({ $0.removeZones.append(i) }, "Removed zone \(formatSlot(i))") {
            markRemoved(.zones, slot: i)
        }
    }

    func removeScanList(_ i: Int) {
        if stage({ $0.removeScanLists.append(i) }, "Removed scan list \(formatSlot(i))") {
            markRemoved(.scanLists, slot: i)
        }
    }

    func removeContact(_ i: Int) {
        if stage({ $0.removeContacts.append(i) }, "Removed contact \(formatSlot(i))") {
            markRemoved(.contacts, slot: i)
        }
    }

    func removeGroupList(_ i: Int) {
        if stage({ $0.removeGroupLists.append(i) }, "Removed group list \(formatSlot(i))") {
            markRemoved(.groupLists, slot: i)
        }
    }

    func removeRadioId(_ i: Int) {
        if stage({ $0.removeRadioIds.append(i) }, "Removed radio ID \(formatSlot(i))") {
            markRemoved(.radioIds, slot: i)
        }
    }

    // MARK: - Move

    /// Relocate a record to a free slot. The core rejects an occupied or
    /// out-of-range target, which surfaces as an error and leaves the work file
    /// untouched.

    func moveChannel(_ from: Int, to: Int) {
        if stage({ $0.moveChannels.append(MoveOp(from: from, to: to)) }, "Moved channel to #\(formatSlot(to))") {
            markMoved(.channels, from: from, to: to)
        }
    }

    func moveQytChannel(_ from: Int, to: Int) {
        if stageQyt({ $0.moveChannels.append(MoveOp(from: from, to: to)) },
                    "Moved channel to #\(formatSlot(to))") {
            markMoved(.qytChannels, from: from, to: to)
        }
    }

    func moveZone(_ from: Int, to: Int) {
        if stage({ $0.moveZones.append(MoveOp(from: from, to: to)) }, "Moved zone to #\(formatSlot(to))") {
            markMoved(.zones, from: from, to: to)
        }
    }

    func moveScanList(_ from: Int, to: Int) {
        if stage({ $0.moveScanLists.append(MoveOp(from: from, to: to)) }, "Moved scan list to #\(formatSlot(to))") {
            markMoved(.scanLists, from: from, to: to)
        }
    }

    func moveContact(_ from: Int, to: Int) {
        if stage({ $0.moveContacts.append(MoveOp(from: from, to: to)) }, "Moved contact to #\(formatSlot(to))") {
            markMoved(.contacts, from: from, to: to)
        }
    }

    func moveGroupList(_ from: Int, to: Int) {
        if stage({ $0.moveGroupLists.append(MoveOp(from: from, to: to)) }, "Moved group list to #\(formatSlot(to))") {
            markMoved(.groupLists, from: from, to: to)
        }
    }

    func moveRadioId(_ from: Int, to: Int) {
        if stage({ $0.moveRadioIds.append(MoveOp(from: from, to: to)) }, "Moved radio ID to #\(formatSlot(to))") {
            markMoved(.radioIds, from: from, to: to)
        }
    }

    // MARK: - Unsaved-change tracking

    /// True when `slot` in `section` holds a staged change the document doesn't
    /// have yet. Backs the per-row indicator in the tables.
    func isUnsaved(_ section: CodeplugSection, slot: Int) -> Bool {
        unsavedSlots[section]?.contains(slot) ?? false
    }

    /// True when `section` holds any staged change. Backs the sidebar dot.
    func hasUnsavedChanges(_ section: CodeplugSection) -> Bool {
        unsavedSections.contains(section)
    }

    private func markUnsaved(_ section: CodeplugSection, slots: [Int]) {
        unsavedSlots[section, default: []].formUnion(slots)
        unsavedSections.insert(section)
    }

    /// A removed record has no row left to mark, so only the section is flagged.
    private func markRemoved(_ section: CodeplugSection, slot: Int) {
        unsavedSlots[section]?.remove(slot)
        unsavedSections.insert(section)
    }

    /// The record's edited state travels with it: the vacated slot is clean (it
    /// holds nothing now) and the target slot carries the change.
    private func markMoved(_ section: CodeplugSection, from: Int, to: Int) {
        unsavedSlots[section]?.remove(from)
        markUnsaved(section, slots: [to])
    }

    private func clearUnsaved() {
        unsavedSlots = [:]
        unsavedSections = []
    }

    // MARK: - Undo / redo

    /// A full staged-state snapshot: the work-file bytes plus the unsaved-change
    /// bookkeeping that rides alongside them. Restoring one returns the editor to
    /// exactly the state it was in when the snapshot was taken.
    private struct EditSnapshot {
        let workData: Data
        let isDirty: Bool
        let unsavedSlots: [CodeplugSection: Set<Int>]
        let unsavedSections: Set<CodeplugSection>
    }

    /// Reverse the last staged edit, if any.
    func undo() {
        guard let url = fileURL, let previous = undoStack.popLast() else { return }
        if let current = snapshot() { redoStack.append(current) }
        restore(previous, url: url, verb: "Undo")
        refreshUndoRedo()
    }

    /// Re-apply the last undone edit, if any.
    func redo() {
        guard let url = fileURL, let next = redoStack.popLast() else { return }
        if let current = snapshot() { undoStack.append(current) }
        restore(next, url: url, verb: "Redo")
        refreshUndoRedo()
    }

    /// Snapshot the current staged state. Returns nil if the work file can't be
    /// read, in which case the edit simply isn't made undoable.
    private func snapshot() -> EditSnapshot? {
        guard let workData = try? Data(contentsOf: Recovery.workURL) else { return nil }
        return EditSnapshot(workData: workData, isDirty: isDirty,
                            unsavedSlots: unsavedSlots, unsavedSections: unsavedSections)
    }

    /// Write a snapshot's bytes back to the work file and reload from them,
    /// restoring the change bookkeeping and recovery marker to match.
    private func restore(_ snap: EditSnapshot, url: URL, verb: String) {
        do {
            try snap.workData.write(to: Recovery.workURL, options: .atomic)
            load(try RadioCore.dump(binPath: Recovery.workURL.path), url: url)
            isDirty = snap.isDirty
            unsavedSlots = snap.unsavedSlots
            unsavedSections = snap.unsavedSections
            if isDirty {
                try? Recovery.mark(original: url)
            } else {
                Recovery.clearManifest()
            }
            setStatus(verb)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    /// Drop all history. Called when a new document is opened or recovered, since
    /// a snapshot only makes sense against the document it was taken from.
    private func clearHistory() {
        undoStack.removeAll()
        redoStack.removeAll()
        refreshUndoRedo()
    }

    private func refreshUndoRedo() {
        canUndo = !undoStack.isEmpty
        canRedo = !redoStack.isEmpty
    }

    // MARK: - Internals

    /// Apply one operation to the work file and reload from it. The user's
    /// document is untouched; only `save()` writes there. Returns false when the
    /// core rejected the edit, so the caller doesn't record a change that didn't
    /// happen.
    @discardableResult
    private func stage(_ op: (inout EditSpec) -> Void, _ describe: String) -> Bool {
        var spec = EditSpec()
        op(&spec)
        return bulkStage(spec, describe)
    }

    /// Apply one QYT operation to the work file and reload, wrapped in the
    /// model-tagged envelope the core needs to tell a QYT batch from an AnyTone
    /// one. Everything downstream — undo, unsaved marks, recovery, save — is the
    /// same path the AnyTone edits take, because all of it works on the work
    /// file's bytes rather than on the spec.
    @discardableResult
    private func stageQyt(_ op: (inout QytEditSpec) -> Void, _ describe: String) -> Bool {
        var spec = QytEditSpec()
        op(&spec)
        return bulkStage(QytEditEnvelope(qyt: spec), describe)
    }

    /// Apply a pre-built edit spec to the work file and reload. Used directly by
    /// bulk operations, which build the full spec themselves. Generic over the
    /// spec type so each radio's edit shape goes through this one path.
    @discardableResult
    private func bulkStage<Spec: Encodable>(_ spec: Spec, _ describe: String) -> Bool {
        stageWorkFile(describe) {
            try RadioCore.applyEdits(input: Recovery.workURL.path, spec: spec,
                                     output: Recovery.workURL.path)
        }
    }

    /// Run `rewrite` against the work file in place, then reload and record the
    /// change as one undoable, unsaved edit. Every staged edit funnels through
    /// here, whatever produced the new bytes.
    private func stageWorkFile(_ describe: String, _ rewrite: () throws -> Void) -> Bool {
        guard let url = fileURL else { return false }
        // Capture the pre-edit state before touching the work file, so a
        // successful edit can be pushed onto the undo stack.
        let pre = snapshot()
        do {
            try rewrite()
            load(try RadioCore.dump(binPath: Recovery.workURL.path), url: url)
            isDirty = true
            try Recovery.mark(original: url)
            if let pre {
                undoStack.append(pre)
                redoStack.removeAll()
                refreshUndoRedo()
            }
            setStatus("\(describe) — unsaved")
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    /// Add picked repeaters to the open codeplug as one staged, undoable edit.
    /// The core writes the channels (plus, on the AnyTone, a zone and scan list)
    /// into the work file; nothing touches the file on disk until Save.
    @discardableResult
    func addRepeaters(_ repeaters: [Repeater], options: GeoBuildOptions) -> Bool {
        let before = recordSlots()
        let ok = stageWorkFile("Added \(repeaters.count) repeaters") {
            try RadioCore.geoBuild(binURL: Recovery.workURL, repeaters: repeaters,
                                   options: options, outURL: Recovery.workURL)
        }
        guard ok else { return false }
        let after = recordSlots()
        for (section, slots) in after {
            let old = before[section] ?? []
            let added = slots.subtracting(old)
            if !added.isEmpty { markUnsaved(section, slots: Array(added)) }
            // Clean mode removes records that have no row left to mark.
            if !old.subtracting(slots).isEmpty { unsavedSections.insert(section) }
        }
        let verb = options.mode == .clean ? "Replaced channels with" : "Added"
        setStatus("\(verb) \(repeaters.count) repeaters (unsaved)")
        return true
    }

    /// The occupied slots of every section a repeater import can change.
    private func recordSlots() -> [CodeplugSection: Set<Int>] {
        [.channels: Set(channels.map(\.id)),
         .zones: Set(zones.map(\.id)),
         .scanLists: Set(scanLists.map(\.id)),
         .contacts: Set(contacts.map(\.id)),
         .qytChannels: Set(qytChannels.map(\.id))]
    }

    /// Run `op`, then report which slot appeared in `list`. The core picks the
    /// slot (`first_free`) and the FFI doesn't report it back, so it's recovered
    /// by diffing the slot set. The status and the unsaved mark are set here
    /// rather than in `stage` because the slot isn't known until the reload has
    /// happened.
    private func addedSlot<T: Identifiable>(of list: KeyPath<CodeplugStore, [T]>,
                                            section: CodeplugSection, noun: String,
                                            _ op: (inout EditSpec) -> Void) -> Int? where T.ID == Int {
        let before = Set(self[keyPath: list].map(\.id))
        guard stage(op, "Added \(noun)") else { return nil }
        guard let slot = self[keyPath: list].map(\.id).first(where: { !before.contains($0) }) else {
            return nil
        }
        markUnsaved(section, slots: [slot])
        setStatus("Added \(noun) \(formatSlot(slot)) — unsaved")
        return slot
    }

    /// Show a status message, clearing it after a few seconds so stale text
    /// doesn't linger for the rest of the session.
    private func setStatus(_ message: String) {
        statusMessage = message
        statusClearTask?.cancel()
        statusClearTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 6_000_000_000)
            guard !Task.isCancelled else { return }
            self?.statusMessage = nil
        }
    }

    /// Copy `source` over `destination`, replacing whatever was there.
    private func replaceFile(at destination: URL, withContentsOf source: URL) throws {
        try? FileManager.default.removeItem(at: destination)
        try FileManager.default.copyItem(at: source, to: destination)
    }
}
