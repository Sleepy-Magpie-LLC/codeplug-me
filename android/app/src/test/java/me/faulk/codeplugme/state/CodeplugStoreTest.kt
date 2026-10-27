package me.faulk.codeplugme.state

import java.io.File
import kotlin.io.path.createTempDirectory
import kotlinx.coroutines.runBlocking
import me.faulk.codeplugme.library.CodeplugLibrary
import me.faulk.codeplugme.library.LibraryEntry
import me.faulk.codeplugme.model.BuildMode
import me.faulk.codeplugme.model.GeoBuildOptions
import me.faulk.codeplugme.model.QytEditSpec
import me.faulk.codeplugme.model.Repeater
import me.faulk.codeplugme.model.RepeaterMode
import me.faulk.codeplugme.model.RepeaterSource
import me.faulk.codeplugme.model.Tone
import me.faulk.codeplugme.model.QytChannelEdit
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * The staging, undo and recovery lifecycle.
 *
 * Ported from the macOS `SaveModelTests`, because the rules being checked are
 * about the data model rather than about either platform. These run against the
 * real Rust core through JNA, so a change to the core that breaks the edit
 * contract fails here rather than on a radio.
 */
class CodeplugStoreTest {

    private lateinit var root: File
    private lateinit var library: CodeplugLibrary
    private lateinit var recovery: Recovery
    private lateinit var store: CodeplugStore
    private lateinit var entry: LibraryEntry

    @Before
    fun setUp() {
        root = createTempDirectory("store").toFile()
        library = CodeplugLibrary(File(root, "library"))
        recovery = Recovery(File(root, "recovery"))
        store = CodeplugStore(library, recovery)
        entry = library.add("Field day", TestImages.qyt())
    }

    @After
    fun tearDown() {
        root.deleteRecursively()
    }

    /** Rename channel 0, the smallest edit that definitely changes bytes. */
    private suspend fun renameChannel(name: String): Boolean = store.stageQyt(
        QytEditSpec(channels = listOf(QytChannelEdit(index = 0, name = name))),
        "Updated channel 1",
    )

    @Test
    fun opening_loads_the_document_and_starts_clean() = runBlocking {
        store.open(entry.id)
        val s = store.state.value
        assertEquals(entry.id, s.entryId)
        assertEquals("qyt-kt5000", s.modelId)
        assertFalse(s.isDirty)
        assertFalse(s.canUndo)
        assertTrue(s.sections.contains(CodeplugSection.QYT_CHANNELS))
    }

    @Test
    fun an_edit_stages_to_the_work_file_and_leaves_the_stored_codeplug_alone() = runBlocking {
        store.open(entry.id)
        val storedBefore = library.bytes(entry.id)

        assertTrue(renameChannel("EDITED"))

        assertTrue(store.state.value.isDirty)
        // The whole design rests on this: only save() touches the document.
        assertTrue(
            "an edit must not reach the stored codeplug",
            storedBefore.contentEquals(library.bytes(entry.id)),
        )
        assertFalse(
            "the work file must differ once an edit is staged",
            storedBefore.contentEquals(store.stagedBytes()),
        )
    }

    @Test
    fun saving_copies_the_staged_bytes_to_the_stored_codeplug_and_clears_dirty() = runBlocking {
        store.open(entry.id)
        renameChannel("EDITED")
        val staged = store.stagedBytes()

        store.save()

        assertTrue(staged.contentEquals(library.bytes(entry.id)))
        val s = store.state.value
        assertFalse(s.isDirty)
        assertTrue(s.unsavedSections.isEmpty())
        assertTrue(s.unsavedSlots.values.all { it.isEmpty() })
    }

    @Test
    fun saving_as_files_the_staged_bytes_under_a_new_entry_and_leaves_the_original() = runBlocking {
        store.open(entry.id)
        val original = library.bytes(entry.id)
        renameChannel("EDITED")
        val staged = store.stagedBytes()

        val newId = store.saveAs("Field day rev B")

        assertNotNull(newId)
        assertTrue("the new entry holds the staged bytes", staged.contentEquals(library.bytes(newId!!)))
        assertTrue("the codeplug saved from must not change", original.contentEquals(library.bytes(entry.id)))
    }

    @Test
    fun saving_as_rebinds_editing_to_the_new_entry_and_clears_dirty() = runBlocking {
        store.open(entry.id)
        renameChannel("EDITED")

        val newId = store.saveAs("Field day rev B")

        val s = store.state.value
        assertEquals(newId, s.entryId)
        assertEquals("Field day rev B", s.entryName)
        assertFalse(s.isDirty)
        assertTrue(s.unsavedSections.isEmpty())
    }

    @Test
    fun discarding_changes_reloads_the_stored_codeplug() = runBlocking {
        store.open(entry.id)
        val original = library.bytes(entry.id)
        renameChannel("EDITED")

        store.discardChanges()

        assertFalse(store.state.value.isDirty)
        assertTrue(original.contentEquals(store.stagedBytes()))
    }

    @Test
    fun undo_returns_the_exact_bytes_from_before_the_edit() = runBlocking {
        store.open(entry.id)
        val before = store.stagedBytes()
        renameChannel("EDITED")
        assertTrue(store.state.value.canUndo)

        store.undo()

        // A whole-file snapshot, not an inverse operation, so this must be
        // byte-exact rather than merely equivalent.
        assertTrue(before.contentEquals(store.stagedBytes()))
        assertTrue(store.state.value.canRedo)
    }

    @Test
    fun redo_reapplies_what_undo_reversed() = runBlocking {
        store.open(entry.id)
        renameChannel("EDITED")
        val edited = store.stagedBytes()

        store.undo()
        store.redo()

        assertTrue(edited.contentEquals(store.stagedBytes()))
    }

    @Test
    fun a_new_edit_clears_the_redo_stack() = runBlocking {
        store.open(entry.id)
        renameChannel("ONE")
        store.undo()
        assertTrue(store.state.value.canRedo)

        renameChannel("TWO")

        assertFalse(
            "an edit after an undo must not leave a stale redo branch",
            store.state.value.canRedo,
        )
    }

    @Test
    fun undo_and_redo_are_no_ops_when_there_is_no_history() = runBlocking {
        store.open(entry.id)
        val before = store.stagedBytes()
        store.undo()
        store.redo()
        assertTrue(before.contentEquals(store.stagedBytes()))
    }

    @Test
    fun opening_a_document_clears_history_from_the_previous_one() = runBlocking {
        store.open(entry.id)
        renameChannel("EDITED")
        assertTrue(store.state.value.canUndo)

        val other = library.add("Other", TestImages.qyt())
        store.open(other.id)

        // A snapshot only makes sense against the document it came from.
        assertFalse(store.state.value.canUndo)
        assertFalse(store.state.value.canRedo)
    }

    @Test
    fun an_edit_marks_its_slot_and_section_unsaved_and_saving_clears_them() = runBlocking {
        store.open(entry.id)
        renameChannel("EDITED")
        store.markUnsaved(CodeplugSection.QYT_CHANNELS, listOf(0))

        assertTrue(store.state.value.isUnsaved(CodeplugSection.QYT_CHANNELS, 0))
        assertTrue(store.state.value.hasUnsavedChanges(CodeplugSection.QYT_CHANNELS))

        store.save()

        assertFalse(store.state.value.isUnsaved(CodeplugSection.QYT_CHANNELS, 0))
    }

    @Test
    fun an_edit_records_a_recovery_manifest_and_saving_clears_it() = runBlocking {
        store.open(entry.id)
        assertNull(recovery.pending())

        renameChannel("EDITED")
        val pending = recovery.pending()
        assertNotNull("unsaved work must be recoverable after a crash", pending)
        assertEquals(entry.id, pending!!.libraryEntryId)

        store.save()
        assertNull(recovery.pending())
    }

    @Test
    fun recovery_restores_the_unsaved_bytes_not_the_stored_ones() = runBlocking {
        store.open(entry.id)
        renameChannel("EDITED")
        val staged = store.stagedBytes()

        // Simulate a fresh process: a new store over the same directories.
        val restarted = CodeplugStore(library, Recovery(File(root, "recovery")))
        val manifest = restarted.pendingRecovery()
        assertNotNull(manifest)
        restarted.restoreRecovery(manifest!!)

        assertTrue(staged.contentEquals(restarted.stagedBytes()))
        assertTrue(restarted.state.value.isDirty)
    }

    @Test
    fun recovery_naming_a_deleted_codeplug_is_refused_rather_than_opened() = runBlocking {
        store.open(entry.id)
        renameChannel("EDITED")
        val manifest = recovery.pending()!!
        library.delete(entry.id)

        val restarted = CodeplugStore(library, Recovery(File(root, "recovery")))
        restarted.restoreRecovery(manifest)

        // There is nowhere to save it back to, so opening it would strand the work.
        assertNull(restarted.state.value.entryId)
        assertNotNull(restarted.state.value.error)
    }

    @Test
    fun a_rejected_edit_leaves_the_work_file_untouched_and_records_no_undo_step() = runBlocking {
        store.open(entry.id)
        val before = store.stagedBytes()

        // Slot 9999 does not exist on a 200-channel radio.
        val ok = store.stageQyt(
            QytEditSpec(channels = listOf(QytChannelEdit(index = 9999, name = "NOPE"))),
            "Updated channel 10000",
        )

        assertFalse("the core must reject an out-of-range slot", ok)
        assertTrue(before.contentEquals(store.stagedBytes()))
        assertFalse("a rejected edit must not become an undo step", store.state.value.canUndo)
        assertNotNull(store.state.value.error)
    }

    @Test
    fun closing_discards_staged_work_and_empties_the_state() = runBlocking {
        store.open(entry.id)
        renameChannel("EDITED")

        store.close()

        assertNull(store.state.value.entryId)
        assertFalse(store.state.value.isOpen)
        assertNull(recovery.pending())
    }

    @Test
    fun an_anytone_codeplug_loads_its_own_sections_and_not_the_qyt_ones() = runBlocking {
        val at = library.add("Home", TestImages.anytone())
        store.open(at.id)

        val s = store.state.value
        assertEquals("anytone-d878uv2", s.modelId)
        assertTrue(s.sections.contains(CodeplugSection.CHANNELS))
        assertFalse(s.sections.contains(CodeplugSection.QYT_CHANNELS))
        // Switching radios must not leave the previous one's records behind a
        // pane that no longer exists.
        assertTrue(s.qytChannels.isEmpty())
        assertNull(s.qytSettings)
    }

    // ----------------------------------------------------- repeater import

    /** A 2 m analog repeater with a CTCSS access tone. */
    private fun analog(call: String, rxHz: Long) = Repeater(
        source = RepeaterSource.HEARHAM,
        callsign = call,
        location = "Denver",
        rxHz = rxHz,
        txHz = rxHz - 600_000,
        mode = RepeaterMode.ANALOG,
        txTone = Tone.Ctcss(1000),
        rxTone = Tone.None,
    )

    @Test
    fun adding_repeaters_stages_into_the_open_codeplug_and_undoes_as_one() = runBlocking {
        store.open(entry.id)
        val storedBefore = library.bytes(entry.id)
        val channelsBefore = store.state.value.qytChannels.size

        val ok = store.addRepeaters(
            listOf(analog("W0AAA", 146_940_000), analog("W0BBB", 147_000_000)),
            GeoBuildOptions(BuildMode.MERGE, "Nearby"),
        )

        assertTrue(store.state.value.error ?: "", ok)
        val s = store.state.value
        assertEquals(channelsBefore + 2, s.qytChannels.size)
        assertTrue(s.isDirty)
        assertTrue(s.canUndo)
        assertTrue(s.hasUnsavedChanges(CodeplugSection.QYT_CHANNELS))
        assertEquals(entry.id, s.entryId)
        assertTrue("the stored codeplug is untouched until Save",
            storedBefore.contentEquals(library.bytes(entry.id)))

        store.undo()
        assertEquals(channelsBefore, store.state.value.qytChannels.size)
    }
}
