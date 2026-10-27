package me.faulk.codeplugme.model

import org.junit.Assert.assertEquals
import org.junit.Test
import java.util.Locale

/**
 * Pins two shipped bugs: a DMR ID must never pick up a grouping separator,
 * and a frequency must always show all 5 decimals. Both are exercised under
 * a locale that would expose either bug if the formatter used it instead of
 * [Locale.ROOT].
 */
class FormattingTest {
    @Test
    fun `a DMR ID renders as plain digits, never grouped`() {
        assertEquals("3113043", formatDmrId(3_113_043))
    }

    @Test
    fun `a DMR ID stays ungrouped under a locale that groups digits`() {
        val original = Locale.getDefault()
        try {
            Locale.setDefault(Locale.GERMANY)
            assertEquals("3113043", formatDmrId(3_113_043))
        } finally {
            Locale.setDefault(original)
        }
    }

    @Test
    fun `a frequency renders with all 5 decimals`() {
        assertEquals("145.50000", formatMhz(145_500_000))
    }

    @Test
    fun `a frequency uses a dot decimal separator under a comma locale`() {
        val original = Locale.getDefault()
        try {
            Locale.setDefault(Locale.GERMANY)
            assertEquals("145.50000", formatMhz(145_500_000))
        } finally {
            Locale.setDefault(original)
        }
    }

    @Test
    fun `hzFromMhz round-trips formatMhz`() {
        assertEquals(146_520_000L, hzFromMhz(146.52))
    }
}
