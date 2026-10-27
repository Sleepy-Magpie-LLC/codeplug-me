package me.faulk.codeplugme.ui

import me.faulk.codeplugme.model.DumpQytChannel
import me.faulk.codeplugme.model.Tone
import me.faulk.codeplugme.ui.channels.ScanFilter
import me.faulk.codeplugme.ui.channels.qytChannelHeadline
import me.faulk.codeplugme.ui.channels.qytChannelSummary
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The channel list's row text and its scan filter.
 *
 * Both are pure functions over a record, deliberately kept out of the
 * composables so the shape of a row is testable on the host JVM with no
 * Compose runtime and no device.
 */
class ChannelRowTest {

    private fun channel(
        index: Int = 0,
        name: String = "SIMPLEX",
        rx: Long = 400_000_000,
        tx: Long? = 410_000_000,
        scanAdd: Boolean = true,
    ) = DumpQytChannel(
        index = index,
        name = name,
        rxFrequencyHz = rx,
        txFrequencyHz = tx,
        rxTone = Tone.None,
        txTone = Tone.None,
        scanAdd = scanAdd,
    )

    @Test
    fun `the headline carries the name and both frequencies`() {
        assertEquals(
            "SIMPLEX (Rx 400.00000 MHz / Tx 410.00000 MHz)",
            qytChannelHeadline(channel()),
        )
    }

    @Test
    fun `a receive-only channel shows a dash for TX rather than a bare zero`() {
        assertTrue(qytChannelHeadline(channel(tx = null)).contains("Tx — MHz"))
    }

    @Test
    fun `the summary leads with scan membership in both directions`() {
        assertTrue(qytChannelSummary(channel(scanAdd = false)).startsWith("Scan: Skip"))
        assertTrue(qytChannelSummary(channel(scanAdd = true)).startsWith("Scan: Yes"))
    }

    @Test
    fun `the all filter keeps every channel`() {
        assertTrue(ScanFilter.All.matches(channel(scanAdd = true)))
        assertTrue(ScanFilter.All.matches(channel(scanAdd = false)))
    }

    @Test
    fun `the scanned filter keeps only channels the scanner stops on`() {
        assertTrue(ScanFilter.Scanned.matches(channel(scanAdd = true)))
        assertFalse(ScanFilter.Scanned.matches(channel(scanAdd = false)))
    }

    @Test
    fun `the skipped filter keeps only channels the scanner passes over`() {
        assertTrue(ScanFilter.Skipped.matches(channel(scanAdd = false)))
        assertFalse(ScanFilter.Skipped.matches(channel(scanAdd = true)))
    }
}
