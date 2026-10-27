import SwiftUI
import AppKit

/// CodeplugMe: native macOS front end for the codeplugme-core Rust library.
/// Device tab talks to the radio over USB serial; Codeplug tab edits .bin
/// files offline. All protocol and codeplug logic lives in Rust.
@main
struct CodeplugMeApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(delegate.codeplug)
                .environmentObject(delegate.device)
                .frame(minWidth: 900, minHeight: 560)
        }
        .commands {
            CodeplugCommands(codeplug: delegate.codeplug)
        }

        Settings {
            ImportSettingsView()
        }
    }
}

/// File and Edit menu items for the codeplug editor.
///
/// This is a separate `Commands` struct, holding the store as `@ObservedObject`,
/// specifically so the menu items re-validate. Built inline in the App's
/// `.commands`, the `.disabled(...)` state is captured once at scene-build time
/// (when the store is empty and clean) and never updates — which leaves Save and
/// its ⌘S shortcut permanently disabled. A disabled menu item also swallows its
/// key equivalent, so the shortcut silently does nothing. Observing the store
/// here makes SwiftUI re-evaluate enablement whenever the store publishes.
struct CodeplugCommands: Commands {
    @ObservedObject var codeplug: CodeplugStore

    var body: some Commands {
        // There is no "new codeplug" concept: a codeplug only comes from a
        // radio backup or an existing file.
        CommandGroup(replacing: .newItem) {
            Button("Open Codeplug…") { codeplug.openWithPanel() }
                .keyboardShortcut("o")
        }
        // ⌘Z / ⇧⌘Z drive the codeplug editor's own history, not AppKit's text
        // undo. The stacks live in the store because the edits do.
        CommandGroup(replacing: .undoRedo) {
            Button("Undo") { codeplug.undo() }
                .keyboardShortcut("z")
                .disabled(!codeplug.canUndo)
            Button("Redo") { codeplug.redo() }
                .keyboardShortcut("z", modifiers: [.command, .shift])
                .disabled(!codeplug.canRedo)
        }
        CommandGroup(replacing: .saveItem) {
            Button("Save") { codeplug.save() }
                .keyboardShortcut("s")
                .disabled(!codeplug.isDirty)
            Button("Save As…") { codeplug.saveAs() }
                .keyboardShortcut("s", modifiers: [.command, .shift])
                .disabled(codeplug.fileURL == nil)
            Button("Revert to Saved") { codeplug.discardChanges() }
                .disabled(!codeplug.isDirty)
        }
        CommandGroup(after: .importExport) {
            Button("Add Repeaters from Database…") { codeplug.showingRepeaterImport = true }
                .disabled(codeplug.fileURL == nil)
        }
    }
}

/// Owns the stores. They live here rather than in a view because ⌘S in
/// `.commands` and the quit guard in `applicationShouldTerminate` both need to
/// reach them from outside the view hierarchy.
@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let codeplug = CodeplugStore()
    let device = DeviceStore()

    /// Block quit while the work file holds changes the document doesn't have.
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard codeplug.isDirty, let name = codeplug.fileURL?.lastPathComponent else {
            return .terminateNow
        }

        let alert = NSAlert()
        alert.messageText = "Save changes to \(name) before quitting?"
        alert.informativeText = "Your changes are staged but haven't been written to the file yet. "
            + "If you don't save them, they'll be offered back to you the next time you open the app."
        alert.addButton(withTitle: "Save")
        alert.addButton(withTitle: "Discard")
        alert.addButton(withTitle: "Cancel")

        switch alert.runModal() {
        case .alertFirstButtonReturn:
            codeplug.save()
            // A failed save leaves an error posted; don't quit over the top of it.
            return codeplug.isDirty ? .terminateCancel : .terminateNow
        case .alertSecondButtonReturn:
            Recovery.clear()
            return .terminateNow
        default:
            return .terminateCancel
        }
    }
}
