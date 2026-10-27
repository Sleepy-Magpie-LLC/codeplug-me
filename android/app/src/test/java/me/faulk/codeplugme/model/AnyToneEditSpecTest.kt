package me.faulk.codeplugme.model

import kotlinx.serialization.encodeToString
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Edit specs must encode only the fields a caller actually set. */
class AnyToneEditSpecTest {
    @Test
    fun `an untouched channel edit field is left out of the JSON entirely`() {
        val edit = ChannelEdit(index = 2, name = "EDITED", rxFrequencyHz = 146_520_000)

        val json = CodeplugJson.instance.encodeToString(edit)

        assertEquals("""{"index":2,"name":"EDITED","rx_frequency_hz":146520000}""", json)
    }

    @Test
    fun `an empty edit spec encodes with no arrays present`() {
        val spec = EditSpec()

        assertTrue(spec.isEmpty)
        assertEquals("{}", CodeplugJson.instance.encodeToString(spec))
    }

    @Test
    fun `a move op and a channel add encode only their own arrays`() {
        val spec = EditSpec(
            addChannels = listOf(ChannelEdit(name = "ADDED", rxFrequencyHz = 446_000_000)),
            moveChannels = listOf(MoveOp(from = 2, to = 100)),
        )

        assertFalse(spec.isEmpty)
        val json = CodeplugJson.instance.encodeToString(spec)
        assertEquals(
            """{"add_channels":[{"name":"ADDED","rx_frequency_hz":446000000}],""" +
                """"move_channels":[{"from":2,"to":100}]}""",
            json,
        )
    }
}
