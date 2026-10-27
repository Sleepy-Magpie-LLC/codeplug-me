package me.faulk.codeplugme.model

import kotlinx.serialization.SerializationException
import kotlinx.serialization.decodeFromString
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/** Decodes both radios' real dump envelopes, and the strictness/leniency rules around them. */
class CodeplugDumpTest {
    @Test
    fun `decodes a full AnyTone envelope`() {
        val dump = CodeplugJson.instance.decodeFromString<CodeplugDump>(TestFixtures.ANYTONE_ENVELOPE)

        assertEquals("anytone-d878uv2", dump.model)
        assertEquals("AnyTone AT-D878UVII Plus", dump.displayName)
        assertNull(dump.qyt)
        val anytone = dump.anytone ?: error("expected an anytone payload")

        assertEquals(1, anytone.channels.size)
        val channel = anytone.channels.single()
        assertEquals("CALLING", channel.name)
        assertEquals(146_520_000L, channel.rxFrequencyHz)
        assertEquals("Simplex", channel.repeaterMode)
        assertEquals(0L, channel.txOffsetHz)

        assertEquals(1, anytone.zones.size)
        assertEquals(0, anytone.zones.single().aChannel)
        assertEquals(1, anytone.contacts.size)
        assertEquals(1, anytone.groupLists.size)
        assertEquals(1, anytone.radioIds.size)
        assertEquals(3_113_043L, anytone.radioIds.single().number)
        assertEquals(1, anytone.scanLists.size)
        assertEquals(65535, anytone.scanLists.single().priorityChannel1)
        assertEquals("", anytone.aprs?.sourceCall)

        assertEquals(4000, dump.capabilities.channelCapacity)
        assertTrue(dump.capabilities.supportsDmr)
        assertEquals(
            listOf("channels", "zones", "scan_lists", "contacts", "group_lists", "radio_ids", "aprs"),
            dump.capabilities.sections,
        )
    }

    @Test
    fun `decodes a full QYT envelope`() {
        val dump = CodeplugJson.instance.decodeFromString<CodeplugDump>(TestFixtures.QYT_ENVELOPE)

        assertEquals("qyt-kt5000", dump.model)
        assertNull(dump.anytone)
        val qyt = dump.qyt ?: error("expected a qyt payload")

        assertEquals("Model:KT-5000", qyt.staticMessage)
        assertEquals(2, qyt.channels.size)
        val ch0 = qyt.channels[0]
        assertEquals("FRS-01", ch0.name)
        assertEquals(462_562_500L, ch0.rxFrequencyHz)
        assertEquals(462_562_500L, ch0.txFrequencyHz)
        assertEquals(Tone.None, ch0.rxTone)
        assertEquals(listOf(0, 0, 0, 36), ch0.flagsRaw)

        assertEquals(2, qyt.bands.size)
        assertEquals(462_562_500L, qyt.bands[0].frequencyHz)
        assertEquals(2_500L, qyt.bands[0].stepHz)

        assertEquals(6, qyt.settings.squelch)
        assertEquals("CO", qyt.settings.scanRev)
        assertTrue(qyt.settings.rememberPower)
        assertEquals(60, qyt.settings.totSeconds)

        assertFalse(dump.capabilities.supportsDmr)
        assertEquals(
            listOf("qyt_channels", "qyt_settings", "qyt_bands"),
            dump.capabilities.sections,
        )
    }

    @Test
    fun `an unrecognized section name does not fail the Capabilities decode`() {
        val json = """
            {"sections":["channels","some_future_section","aprs"],
             "channel_capacity":1,"channel_name_length":1,"supports_dmr":false,
             "tone_encoding":"table_index","power_options":[],"bandwidth_options":[]}
        """.trimIndent()

        val capabilities = CodeplugJson.instance.decodeFromString<Capabilities>(json)

        // sections stays raw strings here; dropping what a screen doesn't
        // recognize is the UI layer's job (me.faulk.codeplugme.state.CodeplugSection).
        assertEquals(listOf("channels", "some_future_section", "aprs"), capabilities.sections)
    }

    @Test
    fun `a model id that is not recognised decodes to a null model rather than failing`() {
        val json = """{"id":"future-radio-9000","display_name":"Future Radio 9000","image_size":1,
            "capabilities":{"sections":[],"channel_capacity":1,"channel_name_length":1,
            "supports_dmr":false,"tone_encoding":"table_index","power_options":[],"bandwidth_options":[]}}"""

        val supported = CodeplugJson.instance.decodeFromString<SupportedModel>(json)

        assertEquals("future-radio-9000", supported.id)
        assertNull(supported.model)
    }

    @Test
    fun `a recognised model id resolves`() {
        val json = """{"id":"qyt-kt5000","display_name":"QYT KT-5000","image_size":1,
            "capabilities":{"sections":[],"channel_capacity":1,"channel_name_length":1,
            "supports_dmr":false,"tone_encoding":"hz_tenths","power_options":[],"bandwidth_options":[]}}"""

        val supported = CodeplugJson.instance.decodeFromString<SupportedModel>(json)

        assertEquals(RadioModel.QYT_KT5000, supported.model)
    }

    @Test
    fun `a dump naming a model but carrying no payload throws`() {
        val json = """
            {"model":"anytone-d878uv2","display_name":"AnyTone AT-D878UVII Plus",
             "capabilities":{"sections":[],"channel_capacity":1,"channel_name_length":1,
             "supports_dmr":true,"tone_encoding":"table_index","power_options":[],"bandwidth_options":[]}}
        """.trimIndent()

        try {
            CodeplugJson.instance.decodeFromString<CodeplugDump>(json)
            fail("expected a payload-less dump to throw")
        } catch (e: SerializationException) {
            assertTrue(e.message.orEmpty().contains("no payload"))
        }
    }
}
