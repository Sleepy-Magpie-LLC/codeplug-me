package me.faulk.codeplugme.model

import kotlinx.serialization.SerializationException
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.decodeFromJsonElement
import org.junit.Assert.assertEquals
import org.junit.Assert.fail
import org.junit.Test

/** [Tone] must round-trip through the core's tagged JSON for all three kinds. */
class ToneTest {
    @Test
    fun `decodes and re-encodes all three tone kinds identically`() {
        val variants = CodeplugJson.instance.parseToJsonElement(TestFixtures.TONE_VARIANTS) as JsonArray

        val expected = listOf(
            Tone.None,
            Tone.Ctcss(hzTenths = 885),
            Tone.Dcs(code = 23, inverted = false),
        )

        variants.forEachIndexed { i, element ->
            val decoded = CodeplugJson.instance.decodeFromJsonElement<Tone>(element)
            assertEquals(expected[i], decoded)

            // Round-trip: re-encoding must reproduce the same JSON shape.
            val reEncoded = CodeplugJson.instance.parseToJsonElement(
                CodeplugJson.instance.encodeToString(decoded),
            )
            assertEquals(element, reEncoded)
        }
    }

    @Test
    fun `an unknown tone kind throws rather than defaulting to none`() {
        try {
            CodeplugJson.instance.decodeFromString<Tone>("""{"kind":"warble"}""")
            fail("expected an unknown tone kind to throw")
        } catch (e: SerializationException) {
            assertEquals(true, e.message.orEmpty().contains("warble"))
        }
    }
}
