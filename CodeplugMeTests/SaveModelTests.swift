import XCTest
import CryptoKit
@testable import CodeplugMe

/// End-to-end tests of the save model, driving the real Rust core against real
/// files. These exist because the central promise of the editor — "nothing
/// touches your codeplug until you press Save" — is a claim about bytes on
/// disk, and only a real file can prove it.
///
/// `CODEPLUGME_RECOVERY_DIR` points the recovery lifecycle at a temp directory so
/// a test run can't disturb a real unsaved session.
@MainActor
final class SaveModelTests: XCTestCase {

    private var tempDir: URL!
    private var codeplug: URL!

    /// An all-zero image of the exact codeplug size parses as a valid, empty
    /// codeplug — the same synthetic fixture the Rust tests use, so no real
    /// radio data is involved.
    ///
    /// This must equal `device::codeplug_size()`, which is the sum of the
    /// modeled regions: adding a region to the memory map changes it, and the
    /// core then rejects this fixture by size with every test in this file
    /// failing at `open`. `RadioCore.expectedCodeplugSize` now exposes the real
    /// value across the FFI; `testExpectedCodeplugSizeMatchesTheFixture` asserts
    /// this hand-tracked constant still agrees with it.
    private static let codeplugSize = 1_789_328

    override func setUpWithError() throws {
        try super.setUpWithError()
        tempDir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("anytone-save-tests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)

        setenv("CODEPLUGME_RECOVERY_DIR", tempDir.appendingPathComponent("recovery").path, 1)
        Recovery.clear()

        codeplug = tempDir.appendingPathComponent("codeplug.bin")
        try Data(count: Self.codeplugSize).write(to: codeplug)
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

    // MARK: - The core promise

    /// Adding a zone must not touch the user's file. This is the regression the
    /// whole work-file design exists to prevent: the previous build wrote the
    /// .bin in place the instant you clicked Add, with no backup and no undo.
    func testAddingAZoneDoesNotWriteTheDocument() throws {
        let before = try sha(codeplug)
        let store = CodeplugStore()
        store.open(url: codeplug)
        XCTAssertNil(store.errorMessage)

        let slot = store.addZone()

        XCTAssertNotNil(slot, "add should report the slot the core chose")
        XCTAssertTrue(store.isDirty)
        XCTAssertEqual(store.zones.count, 1, "the staged zone is visible in the UI")
        XCTAssertEqual(try sha(codeplug), before, "the document must be byte-identical until Save")
    }

    /// Save is what commits, and it must land the staged change.
    func testSaveWritesStagedChangesToTheDocument() throws {
        let before = try sha(codeplug)
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addZone()
        store.save()

        XCTAssertNil(store.errorMessage)
        XCTAssertFalse(store.isDirty)
        XCTAssertNotEqual(try sha(codeplug), before, "Save must write the document")

        // Reopening from disk proves the change persisted, not just that bytes moved.
        let reopened = CodeplugStore()
        reopened.open(url: codeplug)
        XCTAssertEqual(reopened.zones.count, 1)
        XCTAssertFalse(reopened.isDirty)
    }

    /// Editing a record stages through the core and survives a save.
    func testEditingAZoneNameStagesThenPersists() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addZone()
        let before = try sha(codeplug)

        var zone = try XCTUnwrap(store.zones.first)
        zone.name = "Local Simplex"
        store.update(zone)

        XCTAssertEqual(store.zones.first?.name, "Local Simplex")
        XCTAssertEqual(try sha(codeplug), before, "a field edit must not write the document either")

        store.save()
        let reopened = CodeplugStore()
        reopened.open(url: codeplug)
        XCTAssertEqual(reopened.zones.first?.name, "Local Simplex")
    }

    /// Discard throws the work file away and returns to what's on disk.
    func testDiscardRevertsToTheSavedDocument() throws {
        let before = try sha(codeplug)
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addZone()
        XCTAssertTrue(store.isDirty)

        store.discardChanges()

        XCTAssertFalse(store.isDirty)
        XCTAssertEqual(store.zones.count, 0, "the staged zone is gone")
        XCTAssertEqual(try sha(codeplug), before)
    }

    /// Saving with nothing staged must not rewrite the file.
    func testSaveWithNoChangesIsANoOp() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        let before = try sha(codeplug)

        store.save()

        XCTAssertEqual(try sha(codeplug), before)
    }

    // MARK: - Crash recovery

    /// After a change, the recovery manifest describes the session. This is what
    /// a fresh launch finds after a crash — no clean shutdown runs in between.
    func testUnsavedChangesLeaveARecoverableSession() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        var zone = try XCTUnwrap({ _ = store.addZone(); return store.zones.first }())
        zone.name = "Repeaters"
        store.update(zone)

        let pending = try XCTUnwrap(Recovery.pending(), "a crash must leave a recoverable session")
        XCTAssertEqual(pending.originalPath, codeplug.path)

        // A brand-new store, as if the app had just relaunched.
        let relaunched = CodeplugStore()
        relaunched.checkForRecovery()
        let manifest = try XCTUnwrap(relaunched.pendingRecovery)
        relaunched.restoreRecovery(manifest)

        XCTAssertNil(relaunched.errorMessage)
        XCTAssertEqual(relaunched.zones.first?.name, "Repeaters", "the edit survived the crash")
        XCTAssertTrue(relaunched.isDirty, "restored work is still unsaved")
        XCTAssertEqual(relaunched.fileURL, codeplug)
    }

    /// A clean save leaves nothing to recover — otherwise every launch would
    /// prompt about work the user already committed.
    func testSavingClearsTheRecoverableSession() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addZone()
        XCTAssertNotNil(Recovery.pending())

        store.save()

        XCTAssertNil(Recovery.pending())
    }

    func testDiscardingRecoveryClearsIt() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addZone()

        let relaunched = CodeplugStore()
        relaunched.checkForRecovery()
        XCTAssertNotNil(relaunched.pendingRecovery)
        relaunched.discardRecovery()

        XCTAssertNil(relaunched.pendingRecovery)
        XCTAssertNil(Recovery.pending())
    }

    /// A manifest pointing at a file that's since been moved or deleted is
    /// stale: Save would have nowhere to write, so it must not be offered.
    func testRecoveryIsDroppedWhenTheOriginalIsGone() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addZone()
        try FileManager.default.removeItem(at: codeplug)

        XCTAssertNil(Recovery.pending())
    }

    // MARK: - Rename stability

    /// The app's user-facing name changed to Codeplug, but the on-disk recovery
    /// location must not move: an unsaved session from before the rename has to
    /// still be found after upgrading. This asserts the directory itself, not
    /// just its presence, so a future rename of the path would fail this test.
    ///
    /// `CODEPLUGME_RECOVERY_DIR` is unset here to reach the real (unoverridden)
    /// path-building code, then restored to `setUpWithError`'s temp override
    /// before returning — `tearDownWithError` calls `Recovery.clear()` and must
    /// not run it against the real directory.
    func testRecoveryDirectoryIsUnchangedByTheRename() {
        let override = ProcessInfo.processInfo.environment["CODEPLUGME_RECOVERY_DIR"]
        unsetenv("CODEPLUGME_RECOVERY_DIR")
        defer { if let override { setenv("CODEPLUGME_RECOVERY_DIR", override, 1) } }

        XCTAssertTrue(Recovery.directory.path.hasSuffix("Application Support/CodeplugMe/Recovery"))
    }

    // MARK: - Version compatibility

    /// The FFI-reported expected size is the single source of truth the open
    /// path checks against, and it must match the fixture these tests use.
    func testExpectedCodeplugSizeMatchesTheFixture() {
        XCTAssertEqual(RadioCore.expectedCodeplugSize, Self.codeplugSize)
    }

    /// A file that isn't the right size is from a different model or an older
    /// firmware, and opening it must be refused with a message instead of
    /// seeding the work file with something the core can't parse.
    func testOpeningAWrongSizeFileIsRejectedAsIncompatible() throws {
        let wrong = tempDir.appendingPathComponent("old.bin")
        try Data(count: Self.codeplugSize - 16).write(to: wrong)
        let store = CodeplugStore()

        store.open(url: wrong)

        XCTAssertNil(store.fileURL, "an incompatible file must not become the open document")
        XCTAssertEqual(store.zones.count, 0)
        let message = try XCTUnwrap(store.errorMessage)
        XCTAssertTrue(message.contains("incompatible") || message.contains("different radio"),
                      "the error should explain the file is the wrong version")
        XCTAssertNil(Recovery.pending(), "a rejected open leaves no staged session")
    }

    /// A correctly sized file still opens — the gate rejects only mismatches.
    func testOpeningACompatibleFileSucceeds() {
        let store = CodeplugStore()
        store.open(url: codeplug)
        XCTAssertNil(store.errorMessage)
        XCTAssertEqual(store.fileURL, codeplug)
    }

    // MARK: - Closing

    /// Close returns the editor to the empty state and clears the staging work
    /// file, so nothing from the closed document lingers to be half-applied.
    func testCloseResetsToTheEmptyStateAndClearsTheWorkFile() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addZone()
        XCTAssertTrue(FileManager.default.fileExists(atPath: Recovery.workURL.path))

        store.close()

        XCTAssertNil(store.fileURL)
        XCTAssertEqual(store.zones.count, 0)
        XCTAssertFalse(store.isDirty)
        XCTAssertFalse(store.canUndo)
        XCTAssertFalse(FileManager.default.fileExists(atPath: Recovery.workURL.path),
                       "closing clears the work file so no orphan is left behind")
        XCTAssertNil(Recovery.pending())
    }

    // MARK: - Editing continues after a save

    /// `replaceItem` consumes the work file, so the store re-seeds it. Without
    /// that, the first edit after a save would fail.
    func testEditingWorksAfterASave() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addZone()
        store.save()

        let slot = store.addZone()

        XCTAssertNotNil(slot, "a second add after saving must still work")
        XCTAssertNil(store.errorMessage)
        XCTAssertEqual(store.zones.count, 2)
        XCTAssertTrue(store.isDirty)
    }

    /// Removing is staged like everything else.
    func testRemoveIsStagedNotWritten() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addZone()
        store.save()
        let saved = try sha(codeplug)

        store.removeZone(try XCTUnwrap(store.zones.first).index)

        XCTAssertEqual(store.zones.count, 0)
        XCTAssertEqual(try sha(codeplug), saved, "the removal must not reach the document")
        store.save()
        XCTAssertNotEqual(try sha(codeplug), saved)
    }

    /// A rejected move surfaces an error and leaves the staged state alone,
    /// rather than silently corrupting the work file.
    func testMoveToAnOccupiedSlotIsRejected() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addZone()
        _ = store.addZone()
        XCTAssertEqual(store.zones.count, 2)

        store.moveZone(0, to: 1)

        XCTAssertNotNil(store.errorMessage, "moving onto an occupied slot must report an error")
        XCTAssertEqual(store.zones.count, 2)
    }

    // MARK: - Unsaved-change tracking

    /// The dot next to a row and the dot next to the sidebar section both come
    /// from here, so an edit has to light up exactly its own slot and section.
    func testEditingARecordMarksItsSlotAndSectionUnsaved() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        let slot = try XCTUnwrap(store.addZone())
        store.save()
        XCTAssertFalse(store.hasUnsavedChanges(.zones), "a saved file has nothing to flag")

        var zone = try XCTUnwrap(store.zones.first)
        zone.name = "Repeaters"
        store.update(zone)

        XCTAssertTrue(store.isUnsaved(.zones, slot: slot))
        XCTAssertTrue(store.hasUnsavedChanges(.zones))
        XCTAssertFalse(store.hasUnsavedChanges(.channels), "an untouched section stays clean")
    }

    /// Saving is what clears the marks; otherwise the dots would outlive the
    /// changes they describe.
    func testSavingClearsTheUnsavedMarks() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        let slot = try XCTUnwrap(store.addZone())
        XCTAssertTrue(store.isUnsaved(.zones, slot: slot))

        store.save()

        XCTAssertFalse(store.isUnsaved(.zones, slot: slot))
        XCTAssertFalse(store.hasUnsavedChanges(.zones))
    }

    /// A removal is a real unsaved change but leaves no row to mark, so the
    /// section dot must survive on its own.
    func testRemovingMarksTheSectionButNotTheVacatedSlot() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        let slot = try XCTUnwrap(store.addZone())
        store.save()

        store.removeZone(slot)

        XCTAssertTrue(store.hasUnsavedChanges(.zones), "the removal is still unsaved work")
        XCTAssertFalse(store.isUnsaved(.zones, slot: slot), "the slot holds nothing to mark now")
    }

    /// A rejected edit must not leave a mark claiming a change that never landed.
    func testARejectedEditLeavesNoUnsavedMark() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addZone()
        _ = store.addZone()
        store.save()

        store.moveZone(0, to: 1) // slot 1 is occupied

        XCTAssertNotNil(store.errorMessage)
        XCTAssertFalse(store.hasUnsavedChanges(.zones), "a rejected move changed nothing")
    }

    /// Discarding returns to the saved file, marks included.
    func testDiscardClearsTheUnsavedMarks() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addZone()
        XCTAssertTrue(store.hasUnsavedChanges(.zones))

        store.discardChanges()

        XCTAssertFalse(store.hasUnsavedChanges(.zones))
    }

    /// Bulk edits mark every channel they touched, not just the first.
    func testBulkUpdateMarksEveryTouchedChannel() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        let a = try XCTUnwrap(store.addChannel())
        let b = try XCTUnwrap(store.addChannel())
        store.save()

        store.bulkUpdateChannels(BulkChannelUpdate(indices: [a, b], power: "Low"))

        XCTAssertTrue(store.isUnsaved(.channels, slot: a))
        XCTAssertTrue(store.isUnsaved(.channels, slot: b))
    }

    // MARK: - Save As

    /// Save As writes a new document and continues editing it, leaving the
    /// original exactly as it was.
    func testSaveAsWritesANewFileAndLeavesTheOriginalAlone() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addZone()
        let original = try sha(codeplug)
        let copy = tempDir.appendingPathComponent("copy.bin")

        store.write(to: copy)

        XCTAssertNil(store.errorMessage)
        XCTAssertEqual(try sha(codeplug), original, "Save As must not touch the old document")
        XCTAssertEqual(store.fileURL, copy, "editing continues in the new document")
        XCTAssertFalse(store.isDirty)

        let reopened = CodeplugStore()
        reopened.open(url: copy)
        XCTAssertEqual(reopened.zones.count, 1, "the staged zone landed in the new file")
    }

    /// After Save As, further edits go to the new document.
    func testEditingContinuesInTheNewDocumentAfterSaveAs() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addZone()
        let copy = tempDir.appendingPathComponent("copy.bin")
        store.write(to: copy)
        let originalAfterSaveAs = try sha(codeplug)

        _ = store.addZone()
        store.save()

        XCTAssertEqual(try sha(codeplug), originalAfterSaveAs, "the old document stays untouched")
        let reopened = CodeplugStore()
        reopened.open(url: copy)
        XCTAssertEqual(reopened.zones.count, 2)
    }

    /// Bulk editing channels stages through the core and survives a save.
    func testBulkChannelUpdateStagesThenPersists() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)

        // Add two channels to bulk edit
        let slot1 = try XCTUnwrap(store.addChannel())
        let slot2 = try XCTUnwrap(store.addChannel())
        XCTAssertEqual(store.channels.count, 2)

        let before = try sha(codeplug)

        let update = BulkChannelUpdate(
            indices: [slot1, slot2],
            mode: "Digital",
            power: "High",
            bandwidth: "Wide",
            colorCode: 12,
            timeSlot: 2,
            contactIndex: 1,
            groupListIndex: 255,
            radioIdIndex: 0
        )

        store.bulkUpdateChannels(update)

        XCTAssertEqual(store.channels.first(where: { $0.index == slot1 })?.mode, "Digital")
        XCTAssertEqual(store.channels.first(where: { $0.index == slot1 })?.power, "High")
        XCTAssertEqual(store.channels.first(where: { $0.index == slot1 })?.bandwidth, "Wide")
        XCTAssertEqual(store.channels.first(where: { $0.index == slot1 })?.colorCode, 12)
        XCTAssertEqual(store.channels.first(where: { $0.index == slot1 })?.timeSlot, 2)
        XCTAssertEqual(store.channels.first(where: { $0.index == slot1 })?.contactIndex, 1)
        XCTAssertEqual(store.channels.first(where: { $0.index == slot1 })?.groupListIndex, 255)
        XCTAssertEqual(store.channels.first(where: { $0.index == slot1 })?.radioIdIndex, 0)

        XCTAssertEqual(store.channels.first(where: { $0.index == slot2 })?.mode, "Digital")
        XCTAssertEqual(store.channels.first(where: { $0.index == slot2 })?.power, "High")
        XCTAssertEqual(store.channels.first(where: { $0.index == slot2 })?.bandwidth, "Wide")
        XCTAssertEqual(store.channels.first(where: { $0.index == slot2 })?.colorCode, 12)
        XCTAssertEqual(store.channels.first(where: { $0.index == slot2 })?.timeSlot, 2)
        XCTAssertEqual(store.channels.first(where: { $0.index == slot2 })?.contactIndex, 1)
        XCTAssertEqual(store.channels.first(where: { $0.index == slot2 })?.groupListIndex, 255)
        XCTAssertEqual(store.channels.first(where: { $0.index == slot2 })?.radioIdIndex, 0)

        XCTAssertEqual(try sha(codeplug), before, "bulk channel edit must not write the document until Save")

        store.save()
        let reopened = CodeplugStore()
        reopened.open(url: codeplug)
        XCTAssertEqual(reopened.channels.first(where: { $0.index == slot1 })?.mode, "Digital")
        XCTAssertEqual(reopened.channels.first(where: { $0.index == slot1 })?.power, "High")
        XCTAssertEqual(reopened.channels.first(where: { $0.index == slot1 })?.colorCode, 12)
        XCTAssertEqual(reopened.channels.first(where: { $0.index == slot1 })?.timeSlot, 2)
    }

    // MARK: - New-channel defaults

    /// A freshly added channel points at RX group list "None" (0xff), not the
    /// core's raw default of slot 0, which would silently attach group list #1.
    func testAddedChannelDefaultsToNoRXGroup() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)

        let slot = try XCTUnwrap(store.addChannel())

        XCTAssertEqual(store.channels.first(where: { $0.index == slot })?.groupListIndex, 255)
    }

    // MARK: - Undo / redo

    /// Undo reverses the last staged edit and re-derives the records from the
    /// restored work file.
    func testUndoReversesTheLastEdit() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        XCTAssertFalse(store.canUndo)

        _ = store.addZone()
        XCTAssertEqual(store.zones.count, 1)
        XCTAssertTrue(store.canUndo)

        store.undo()

        XCTAssertEqual(store.zones.count, 0, "undo removes the staged zone")
        XCTAssertFalse(store.canUndo, "nothing left to undo")
        XCTAssertTrue(store.canRedo)
    }

    /// Undoing back to the opened state clears the dirty flag and the recovery
    /// marker, since there is no longer anything staged.
    func testUndoToOpenedStateIsClean() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addZone()
        XCTAssertTrue(store.isDirty)

        store.undo()

        XCTAssertFalse(store.isDirty, "back at the opened state, nothing is staged")
        XCTAssertNil(Recovery.pending(), "and there is nothing to recover")
    }

    /// Redo re-applies an undone edit.
    func testRedoReappliesAnUndoneEdit() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addZone()
        store.undo()
        XCTAssertEqual(store.zones.count, 0)

        store.redo()

        XCTAssertEqual(store.zones.count, 1, "redo brings the zone back")
        XCTAssertTrue(store.isDirty)
        XCTAssertFalse(store.canRedo)
    }

    /// A new edit after an undo drops the redo stack, so redo can't resurrect a
    /// branch the user has moved past.
    func testANewEditClearsTheRedoStack() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addZone()
        store.undo()
        XCTAssertTrue(store.canRedo)

        _ = store.addChannel()

        XCTAssertFalse(store.canRedo, "a fresh edit invalidates the redo branch")
    }

    /// Undo restores the per-slot unsaved marks it captured, not a guess.
    func testUndoRestoresUnsavedMarks() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        let a = try XCTUnwrap(store.addZone())
        store.save()
        XCTAssertFalse(store.hasUnsavedChanges(.zones))

        let b = try XCTUnwrap(store.addZone())
        XCTAssertTrue(store.isUnsaved(.zones, slot: b))

        store.undo()

        XCTAssertFalse(store.isUnsaved(.zones, slot: b), "the undone zone's mark is gone")
        XCTAssertFalse(store.hasUnsavedChanges(.zones), "back to the saved state's marks")
        XCTAssertEqual(store.zones.map(\.index), [a])
    }

    /// Opening a document wipes any history from a previous one.
    func testOpeningClearsHistory() throws {
        let store = CodeplugStore()
        store.open(url: codeplug)
        _ = store.addZone()
        XCTAssertTrue(store.canUndo)

        store.open(url: codeplug)

        XCTAssertFalse(store.canUndo, "a freshly opened document has no history")
        XCTAssertFalse(store.canRedo)
    }
}
