package me.faulk.codeplugme.state

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.serialization.encodeToString
import me.faulk.codeplugme.core.RadioCore
import me.faulk.codeplugme.core.userMessage
import me.faulk.codeplugme.library.CodeplugLibrary
import me.faulk.codeplugme.model.BuildMode
import me.faulk.codeplugme.model.CodeplugDump
import me.faulk.codeplugme.model.CodeplugJson
import me.faulk.codeplugme.model.EditSpec
import me.faulk.codeplugme.model.GeoBuildOptions
import me.faulk.codeplugme.model.QytEditEnvelope
import me.faulk.codeplugme.model.QytEditSpec
import me.faulk.codeplugme.model.Repeater

/**
 * Holds the codeplug being edited and every way it can change.
 *
 * Ported from the macOS app's `CodeplugStore.swift`, whose architecture is
 * platform-neutral and worth keeping exactly. Four invariants carry the design,
 * and breaking any of them reintroduces a class of bug:
 *
 * 1. **The state is always a fresh parse of the work file.** Every edit goes to
 *    the core, which rewrites the file and re-parses its own output to verify
 *    it. Nothing here patches a record in memory, so an edit can never leave the
 *    UI showing something the bytes do not say.
 * 2. **The stored codeplug is only written by [save].** Everything else touches
 *    the work file, which is what makes discarding changes free and makes
 *    "write unsaved edits straight to the radio" a coherent thing to offer.
 * 3. **Undo is a whole-file byte snapshot**, not an inverse operation. Add,
 *    remove, move, and bulk edits are then all undoable through one path with
 *    no per-operation inverse to get wrong.
 * 4. **Records are addressed by slot index, never by list position.** A sorted
 *    or filtered list must not be able to commit an edit to the wrong record.
 *
 * Deliberately not a ViewModel: it is a plain class so the whole editing model
 * is testable on the host JVM with no Android and no radio.
 */
class CodeplugStore(
    private val library: CodeplugLibrary,
    private val recovery: Recovery,
) {
    private val _state = MutableStateFlow(CodeplugState())

    /** What the editor screens render. */
    val state: StateFlow<CodeplugState> = _state

    /** Pre-edit snapshots, most recent last. */
    private val undoStack = ArrayDeque<EditSnapshot>()
    private val redoStack = ArrayDeque<EditSnapshot>()

    /**
     * A full staged-state snapshot: the work-file bytes plus the unsaved-change
     * bookkeeping that rides alongside them. Restoring one returns the editor to
     * exactly the state it was in when the snapshot was taken.
     */
    private data class EditSnapshot(
        val workBytes: ByteArray,
        val isDirty: Boolean,
        val unsavedSlots: Map<CodeplugSection, Set<Int>>,
        val unsavedSections: Set<CodeplugSection>,
    ) {
        override fun equals(other: Any?) =
            other is EditSnapshot && workBytes.contentEquals(other.workBytes)

        override fun hashCode() = workBytes.contentHashCode()
    }

    // ---------------------------------------------------------------- opening

    /** Unsaved work from a previous run, or null. */
    fun pendingRecovery(): RecoveryManifest? = recovery.pending()

    /**
     * Reopen the codeplug a previous run was editing, keeping its unsaved edits.
     *
     * A manifest naming an entry that has since been deleted is stale: there is
     * nothing to save back to, so the work is discarded rather than opened into
     * a document with no home.
     */
    suspend fun restoreRecovery(manifest: RecoveryManifest) {
        val entry = library.get(manifest.libraryEntryId)
        if (entry == null) {
            recovery.clear()
            setError("The codeplug those unsaved edits belonged to is no longer on this phone.")
            return
        }
        runCatching { reloadFromWorkFile(entry.id, entry.name, dirty = true) }
            .onFailure { setError(it.userMessage) }
            .onSuccess {
                clearHistory()
                setStatus("Restored unsaved changes to ${entry.name}")
            }
    }

    /** Throw away a previous run's unsaved edits. */
    fun discardRecovery() = recovery.clear()

    /**
     * Open a stored codeplug for editing.
     *
     * The stored bytes are copied to the work file first; from that moment the
     * stored entry is not touched again until [save].
     */
    suspend fun open(entryId: String) {
        val entry = library.get(entryId)
        if (entry == null) {
            setError("That codeplug is no longer on this phone.")
            return
        }
        try {
            recovery.workFile.writeBytes(library.bytes(entryId))
            recovery.clearManifest()
            reloadFromWorkFile(entry.id, entry.name, dirty = false)
            clearHistory()
            setStatus("Opened ${entry.name}")
        } catch (e: Throwable) {
            setError(e.userMessage)
        }
    }

    /** Close the open codeplug, discarding any staged edits. */
    fun close() {
        recovery.clear()
        undoStack.clear()
        redoStack.clear()
        _state.value = CodeplugState()
    }

    // ---------------------------------------------------------------- saving

    /** Copy the staged work back to the stored codeplug. */
    suspend fun save() {
        val s = _state.value
        val id = s.entryId ?: return
        try {
            library.update(id, recovery.workFile.readBytes())
            recovery.clearManifest()
            _state.value = s.copy(
                isDirty = false,
                unsavedSlots = emptyMap(),
                unsavedSections = emptySet(),
                status = "Saved ${s.entryName}",
            )
        } catch (e: Throwable) {
            setError(e.userMessage)
        }
    }

    /**
     * Save the staged work as a NEW stored codeplug, and keep editing that one.
     *
     * The original entry is left exactly as it was on disk, which is the point:
     * this is how an experiment becomes its own codeplug instead of overwriting
     * the one it started from. Editing rebinds to the new entry, because the
     * alternative (staying bound to the original with the changes now also
     * living elsewhere) leaves two homes for one set of edits.
     *
     * Undo history survives: the snapshots are byte images of the same document,
     * and where that document is stored does not change what undoing means.
     */
    suspend fun saveAs(name: String): String? {
        val s = _state.value
        if (s.entryId == null) return null
        return try {
            val entry = library.add(name.ifBlank { s.entryName }, recovery.workFile.readBytes())
            recovery.clearManifest()
            _state.value = s.copy(
                entryId = entry.id,
                entryName = entry.name,
                isDirty = false,
                unsavedSlots = emptyMap(),
                unsavedSections = emptySet(),
                status = "Saved ${entry.name}",
            )
            entry.id
        } catch (e: Throwable) {
            setError(e.userMessage)
            null
        }
    }

    /** Throw away staged edits and reload from the stored codeplug. */
    suspend fun discardChanges() {
        val id = _state.value.entryId ?: return
        open(id)
    }

    /**
     * The bytes that would go to the radio right now.
     *
     * Deliberately the WORK file, not the stored one, so writing to the radio
     * includes unsaved edits. This matches the macOS behaviour and is what
     * makes "tweak a channel, write it, try it" a single loop.
     */
    fun stagedBytes(): ByteArray = recovery.workFile.readBytes()

    // ------------------------------------------------------------ undo / redo

    /** Reverse the last staged edit, if any. */
    suspend fun undo() {
        val previous = undoStack.removeLastOrNull() ?: return
        snapshot()?.let { redoStack.addLast(it) }
        restore(previous, "Undo")
    }

    /** Reapply the last undone edit, if any. */
    suspend fun redo() {
        val next = redoStack.removeLastOrNull() ?: return
        snapshot()?.let { undoStack.addLast(it) }
        restore(next, "Redo")
    }

    // -------------------------------------------------------------- staging

    /**
     * Apply an AnyTone edit batch to the work file and reload from it.
     *
     * Returns false when the core rejected the batch, so a caller does not
     * record a change that did not happen. The work file is untouched on
     * rejection: the core writes its output only after re-parsing it.
     */
    suspend fun stage(spec: EditSpec, describe: String): Boolean =
        stageEncoded(CodeplugJson.instance.encodeToString(spec), describe)

    /**
     * Apply a QYT edit batch, wrapped in the model-tagged envelope the core
     * needs to tell a QYT batch from an AnyTone one.
     *
     * Everything downstream (undo, unsaved marks, recovery, save) is the same
     * path the AnyTone edits take, because all of it works on the work file's
     * bytes rather than on the spec.
     */
    suspend fun stageQyt(spec: QytEditSpec, describe: String): Boolean =
        stageEncoded(CodeplugJson.instance.encodeToString(QytEditEnvelope(spec)), describe)

    /** Apply an encoded edit batch through [stageBytes]. */
    private suspend fun stageEncoded(editsJson: String, describe: String): Boolean =
        stageBytes(describe) { RadioCore.applyEdits(it, editsJson) }

    /**
     * Add picked repeaters to the open codeplug as one staged, undoable edit.
     *
     * The core writes the channels (plus, on the AnyTone, a zone and scan list)
     * into the work file; the stored codeplug is untouched until Save. New
     * records are marked unsaved, and a clean build that removed records flags
     * their sections.
     */
    suspend fun addRepeaters(picked: List<Repeater>, options: GeoBuildOptions): Boolean {
        val before = recordSlots(_state.value)
        val ok = stageBytes("Added ${picked.size} repeaters") {
            RadioCore.geoBuild(it, picked, options)
        }
        if (!ok) return false
        val after = recordSlots(_state.value)
        for ((section, slots) in after) {
            val old = before[section].orEmpty()
            val added = slots - old
            if (added.isNotEmpty()) markUnsaved(section, added.toList())
            if ((old - slots).isNotEmpty()) {
                _state.value = _state.value.copy(unsavedSections = _state.value.unsavedSections + section)
            }
        }
        val verb = if (options.mode == BuildMode.CLEAN) "Replaced channels with" else "Added"
        setStatus("$verb ${picked.size} repeaters, unsaved")
        return true
    }

    /** The occupied slots of every section a repeater import can change. */
    private fun recordSlots(s: CodeplugState): Map<CodeplugSection, Set<Int>> = mapOf(
        CodeplugSection.CHANNELS to s.channels.map { it.index }.toSet(),
        CodeplugSection.ZONES to s.zones.map { it.index }.toSet(),
        CodeplugSection.SCAN_LISTS to s.scanLists.map { it.index }.toSet(),
        CodeplugSection.CONTACTS to s.contacts.map { it.index }.toSet(),
        CodeplugSection.QYT_CHANNELS to s.qytChannels.map { it.index }.toSet(),
    )

    /**
     * The one path every edit takes, whichever radio it is for and whatever
     * produced the new bytes: [rewrite] turns the work file's bytes into the
     * next ones, then the state reloads and the change becomes undoable.
     */
    private suspend fun stageBytes(describe: String, rewrite: suspend (ByteArray) -> ByteArray): Boolean {
        val s = _state.value
        val entryId = s.entryId ?: return false
        // Capture the pre-edit state before touching the work file, so a
        // successful edit can be pushed onto the undo stack.
        val pre = snapshot()
        return try {
            val out = rewrite(recovery.workFile.readBytes())
            recovery.workFile.writeBytes(out)
            reloadFromWorkFile(entryId, s.entryName, dirty = true)
            recovery.mark(entryId, s.entryName)
            if (pre != null) {
                undoStack.addLast(pre)
                redoStack.clear()
            }
            _state.value = _state.value.copy(
                canUndo = undoStack.isNotEmpty(),
                canRedo = redoStack.isNotEmpty(),
                status = "$describe, unsaved",
            )
            true
        } catch (e: Throwable) {
            setError(e.userMessage)
            false
        }
    }

    /**
     * Run [block], then report which slot appeared in the list [pick] returns.
     *
     * The core picks the slot (first free) and does not report it back, so it is
     * recovered by diffing the slot set across the reload. The unsaved mark is
     * set here rather than in [stageEncoded] because the slot is not known until
     * the reload has happened.
     */
    suspend fun stageAdd(
        section: CodeplugSection,
        noun: String,
        pick: (CodeplugState) -> List<Int>,
        block: suspend () -> Boolean,
    ): Int? {
        val before = pick(_state.value).toSet()
        if (!block()) return null
        val slot = pick(_state.value).firstOrNull { it !in before } ?: return null
        markUnsaved(section, listOf(slot))
        _state.value = _state.value.copy(status = "Added $noun ${slot + 1}, unsaved")
        return slot
    }

    // ------------------------------------------------------ unsaved tracking

    /** Flag [slots] in [section] as holding staged changes. */
    fun markUnsaved(section: CodeplugSection, slots: List<Int>) {
        val s = _state.value
        val next = s.unsavedSlots.toMutableMap()
        next[section] = (next[section].orEmpty() + slots).toSet()
        _state.value = s.copy(unsavedSlots = next, unsavedSections = s.unsavedSections + section)
    }

    /** A removed record has no row left to mark, so only the section is flagged. */
    fun markRemoved(section: CodeplugSection, slot: Int) {
        val s = _state.value
        val next = s.unsavedSlots.toMutableMap()
        next[section] = next[section].orEmpty() - slot
        _state.value = s.copy(unsavedSlots = next, unsavedSections = s.unsavedSections + section)
    }

    /**
     * The record's edited state travels with it: the vacated slot is clean (it
     * holds nothing now) and the target slot carries the change.
     */
    fun markMoved(section: CodeplugSection, from: Int, to: Int) {
        markRemoved(section, from)
        markUnsaved(section, listOf(to))
    }

    // ------------------------------------------------------------- messages

    /** Show a transient status line. */
    fun setStatus(message: String) {
        _state.value = _state.value.copy(status = message)
    }

    /** Surface a failure to the user. */
    fun setError(message: String) {
        _state.value = _state.value.copy(error = message)
    }

    /** Dismiss the current error, after the UI has shown it. */
    fun clearError() {
        _state.value = _state.value.copy(error = null)
    }

    /** Dismiss the current status line. */
    fun clearStatus() {
        _state.value = _state.value.copy(status = null)
    }

    // ------------------------------------------------------------- internals

    /** Parse the work file and replace every record in the state from it. */
    private suspend fun reloadFromWorkFile(entryId: String, entryName: String, dirty: Boolean) {
        val dump = RadioCore.dump(recovery.workFile.readBytes())
        _state.value = merge(_state.value, dump, entryId, entryName, dirty)
    }

    /**
     * Fold a parsed document into the state.
     *
     * Both payload families are written on every load, with the absent one
     * cleared, so switching between an AnyTone and a QYT codeplug cannot leave
     * the previous radio's records showing behind a pane that no longer exists.
     */
    private fun merge(
        prev: CodeplugState,
        dump: CodeplugDump,
        entryId: String,
        entryName: String,
        dirty: Boolean,
    ): CodeplugState {
        val a = dump.anytone
        val q = dump.qyt
        return prev.copy(
            entryId = entryId,
            entryName = entryName,
            modelId = dump.model,
            displayName = dump.displayName,
            capabilities = dump.capabilities,
            channels = a?.channels.orEmpty(),
            zones = a?.zones.orEmpty(),
            scanLists = a?.scanLists.orEmpty(),
            contacts = a?.contacts.orEmpty(),
            groupLists = a?.groupLists.orEmpty(),
            radioIds = a?.radioIds.orEmpty(),
            aprs = a?.aprs,
            qytChannels = q?.channels.orEmpty(),
            qytSettings = q?.settings,
            qytBands = q?.bands.orEmpty(),
            qytStaticMessage = q?.staticMessage.orEmpty(),
            isDirty = dirty,
            error = null,
        )
    }

    /** Capture the current staged state, or null when nothing is open. */
    private fun snapshot(): EditSnapshot? {
        val s = _state.value
        if (s.entryId == null || !recovery.workFile.exists()) return null
        return EditSnapshot(
            workBytes = recovery.workFile.readBytes(),
            isDirty = s.isDirty,
            unsavedSlots = s.unsavedSlots,
            unsavedSections = s.unsavedSections,
        )
    }

    /** Put a snapshot back and reload the state from it. */
    private suspend fun restore(snap: EditSnapshot, verb: String) {
        val s = _state.value
        val entryId = s.entryId ?: return
        try {
            recovery.workFile.writeBytes(snap.workBytes)
            reloadFromWorkFile(entryId, s.entryName, snap.isDirty)
            if (snap.isDirty) recovery.mark(entryId, s.entryName) else recovery.clearManifest()
            _state.value = _state.value.copy(
                unsavedSlots = snap.unsavedSlots,
                unsavedSections = snap.unsavedSections,
                canUndo = undoStack.isNotEmpty(),
                canRedo = redoStack.isNotEmpty(),
                status = verb,
            )
        } catch (e: Throwable) {
            setError(e.userMessage)
        }
    }

    /**
     * Drop all history. Called when a document is opened or recovered, since a
     * snapshot only makes sense against the document it was taken from.
     */
    private fun clearHistory() {
        undoStack.clear()
        redoStack.clear()
        _state.value = _state.value.copy(canUndo = false, canRedo = false)
    }
}
