package me.faulk.codeplugme.state

import me.faulk.codeplugme.library.CodeplugOrigin
import me.faulk.codeplugme.library.LibraryEntry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.codeplugme_core.erasedRegions

/**
 * What the write confirmation tells the user about the bytes it is about to
 * send.
 *
 * This exists because of 2026-09-09: a codeplug written from this app blanked
 * 950 bytes of the radio's own memory, and nothing in the flow had said where
 * those bytes came from or what the write would do to the regions the app does
 * not model. The core no longer sends that particular region at all, and this
 * layer says the rest out loud.
 */
class WritePreflightTest {

    private fun entry(origin: CodeplugOrigin, source: String? = null) = LibraryEntry(
        id = "1",
        name = "Field day",
        modelId = "qyt-kt5000",
        sizeBytes = TestImages.QYT_SIZE.toLong(),
        sourceName = source,
        importedAtEpochMs = 1_757_000_000_000,
        modifiedAtEpochMs = 1_757_000_000_000,
        origin = origin,
    )

    @Test
    fun the_origin_line_says_which_of_the_three_cases_this_is() {
        assertTrue(originLine(entry(CodeplugOrigin.RADIO)).startsWith("Read off a radio on"))

        val imported = originLine(entry(CodeplugOrigin.IMPORT, source = "field.bin"))
        assertTrue(imported, imported.startsWith("Imported on"))
        assertTrue(imported, imported.contains("field.bin"))

        // An import with no file name must still read as a sentence.
        assertFalse(originLine(entry(CodeplugOrigin.IMPORT)).contains("from ."))

        val generated = originLine(entry(CodeplugOrigin.GENERATED, source = "Home"))
        assertTrue(generated, generated.startsWith("Built from repeater listings on"))
        assertTrue(generated, generated.contains("on top of Home"))
        assertFalse(originLine(entry(CodeplugOrigin.GENERATED)).contains("on top of"))

        // The honest answer for anything stored by an older build.
        assertTrue(originLine(entry(CodeplugOrigin.UNKNOWN)).contains("does not know"))
    }

    @Test
    fun an_image_read_off_a_radio_raises_no_erasure_warning() {
        // A read carries the radio's own bytes everywhere, including the
        // regions this app never models, so there is nothing to warn about.
        val populated = TestImages.qyt().also { image ->
            for (i in 0x2AF0 until 0x2B00) image[i] = 0x11
            for (i in 0x3000 until 0x3100) image[i] = 0x22
        }
        assertTrue(erasedRegions(populated).isEmpty())
    }

    @Test
    fun an_image_missing_a_region_the_radio_normally_holds_is_named_before_the_write() {
        val stale = TestImages.qyt().also { image ->
            for (i in 0x2AF0 until 0x2B00) image[i] = 0x11
            for (i in 0x3000 until 0x3100) image[i] = 0xFF.toByte()
        }
        assertTrue(erasedRegions(stale).contains("DTMF / autodial"))
    }
}
