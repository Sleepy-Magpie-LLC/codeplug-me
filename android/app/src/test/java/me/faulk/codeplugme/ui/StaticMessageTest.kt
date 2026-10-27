package me.faulk.codeplugme.ui

import me.faulk.codeplugme.ui.config.STATIC_MESSAGE_LIMIT
import me.faulk.codeplugme.ui.config.clampStaticMessage
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The LCD static message is 16 bytes of printable ASCII at `0x0FE0`, but the
 * radio only draws the first 13, so the settings screen filters as the user
 * types rather than failing at Apply.
 */
class StaticMessageTest {
    @Test
    fun `a short printable message passes through untouched`() {
        assertEquals("N7FYZ", clampStaticMessage("N7FYZ"))
        assertEquals("", clampStaticMessage(""))
    }

    @Test
    fun `a message longer than the LCD width is cut to what fits`() {
        assertEquals("0123456789ABC", clampStaticMessage("0123456789ABCDEFGHI"))
        assertEquals(13, STATIC_MESSAGE_LIMIT)
        assertEquals(STATIC_MESSAGE_LIMIT, clampStaticMessage("x".repeat(40)).length)
    }

    @Test
    fun `characters the radio cannot draw are dropped`() {
        assertEquals("caf ", clampStaticMessage("café 📻"))
        assertEquals("AB", clampStaticMessage("A\nB\t"))
    }
}
