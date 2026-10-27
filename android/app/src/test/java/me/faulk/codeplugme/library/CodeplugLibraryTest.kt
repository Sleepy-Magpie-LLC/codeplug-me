package me.faulk.codeplugme.library

import java.io.File
import kotlin.io.path.createTempDirectory
import me.faulk.codeplugme.state.TestImages
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/** The on-phone codeplug store, which is what makes writing to a radio fast. */
class CodeplugLibraryTest {

    private lateinit var root: File
    private lateinit var library: CodeplugLibrary

    @Before
    fun setUp() {
        root = createTempDirectory("library").toFile()
        library = CodeplugLibrary(root)
    }

    @After
    fun tearDown() {
        root.deleteRecursively()
    }

    @Test
    fun an_imported_codeplug_is_labelled_with_the_model_its_own_bytes_name() {
        val entry = library.add("Field day", TestImages.qyt(), sourceName = "field.bin")
        // Not from the file name and not from what the user believed: the model
        // is what decides which radio this may later be written to.
        assertEquals("qyt-kt5000", entry.modelId)
        assertEquals(TestImages.QYT_SIZE.toLong(), entry.sizeBytes)

        val anytone = library.add("Home", TestImages.anytone())
        assertEquals("anytone-d878uv2", anytone.modelId)
    }

    @Test
    fun a_file_that_is_not_a_codeplug_is_refused_at_import_not_at_write_time() {
        val e = runCatching { library.add("Junk", ByteArray(1234)) }.exceptionOrNull()
        assertNotNull("importing a non-codeplug must fail", e)
        assertTrue(e!!.message!!.contains("1234"))
        assertTrue(library.list().isEmpty())
    }

    @Test
    fun bytes_round_trip_through_the_store() {
        val image = TestImages.qyt()
        val entry = library.add("Field day", image)
        assertTrue(image.contentEquals(library.bytes(entry.id)))
    }

    @Test
    fun update_replaces_bytes_and_bumps_the_modified_time() {
        val entry = library.add("Field day", TestImages.qyt())
        val changed = TestImages.qyt().also { it[0] = 0x00 }
        val updated = library.update(entry.id, changed)
        assertTrue(changed.contentEquals(library.bytes(entry.id)))
        assertTrue(updated.modifiedAtEpochMs >= entry.modifiedAtEpochMs)
    }

    @Test
    fun rename_and_duplicate_leave_the_original_bytes_alone() {
        val entry = library.add("Field day", TestImages.qyt())
        library.rename(entry.id, "Renamed")
        assertEquals("Renamed", library.get(entry.id)!!.name)

        val copy = library.duplicate(entry.id)
        assertEquals("Renamed copy", copy.name)
        assertTrue(library.bytes(copy.id).contentEquals(library.bytes(entry.id)))
        assertEquals(2, library.list().size)
    }

    @Test
    fun delete_removes_the_entry_and_its_bytes() {
        val entry = library.add("Field day", TestImages.qyt())
        library.delete(entry.id)
        assertNull(library.get(entry.id))
        assertTrue(library.list().isEmpty())
        assertTrue(!library.fileFor(entry.id).exists())
    }

    @Test
    fun origin_is_recorded_because_a_write_sends_unmodelled_bytes_verbatim() {
        val read = library.add("Read from radio", TestImages.qyt(), origin = CodeplugOrigin.RADIO)
        val imported = library.add(
            "Someone else's",
            TestImages.qyt(),
            sourceName = "field.bin",
            origin = CodeplugOrigin.IMPORT,
        )
        assertEquals(CodeplugOrigin.RADIO, library.get(read.id)!!.origin)
        assertEquals(CodeplugOrigin.IMPORT, library.get(imported.id)!!.origin)
        // A copy is the same bytes, so it has the same provenance.
        assertEquals(CodeplugOrigin.RADIO, library.duplicate(read.id).origin)
    }

    @Test
    fun a_generated_codeplug_keeps_its_origin_through_the_index() {
        val built = library.add(
            "Nearby repeaters",
            TestImages.qyt(),
            sourceName = "Home",
            origin = CodeplugOrigin.GENERATED,
        )
        // Re-read through a fresh instance so the value really came off disk.
        val loaded = CodeplugLibrary(root).get(built.id)!!
        assertEquals(CodeplugOrigin.GENERATED, loaded.origin)
        assertEquals("Home", loaded.sourceName)
    }

    @Test
    fun an_index_written_before_origin_existed_still_loads() {
        val entry = library.add("Field day", TestImages.qyt())
        val index = File(root, "library.json")
        // Exactly what the previous build wrote: no origin field at all.
        index.writeText(index.readText().replace(Regex(",\\s*\"origin\":\\s*\"[A-Z]+\""), ""))
        val loaded = library.get(entry.id)
        assertNotNull("an older index must still load", loaded)
        assertEquals(CodeplugOrigin.UNKNOWN, loaded!!.origin)
    }

    @Test
    fun a_corrupt_index_does_not_make_the_whole_library_unreachable() {
        library.add("Field day", TestImages.qyt())
        File(root, "library.json").writeText("{ this is not json")
        // Healing to empty is the right failure: it loses the index, not the
        // codeplug files, so nothing the user cannot recover.
        assertTrue(library.list().isEmpty())
        assertEquals(1, root.listFiles()!!.count { it.name.endsWith(".bin") })
    }

    @Test
    fun an_entry_whose_file_vanished_is_not_listed() {
        val entry = library.add("Field day", TestImages.qyt())
        library.fileFor(entry.id).delete()
        assertTrue(library.list().isEmpty())
    }
}
