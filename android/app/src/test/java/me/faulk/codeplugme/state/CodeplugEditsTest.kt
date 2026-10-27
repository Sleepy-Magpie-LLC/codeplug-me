package me.faulk.codeplugme.state

import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.jsonObject
import me.faulk.codeplugme.model.CodeplugJson
import me.faulk.codeplugme.model.DumpQytChannel
import me.faulk.codeplugme.model.Tone
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The diff-to-minimal-edit layer.
 *
 * Minimality is not an optimisation. The core writes only the fields a batch
 * names, and a codeplug is full of bytes this app does not model, so sending a
 * whole record back would re-encode fields nobody touched.
 */
class CodeplugEditsTest {

    private fun qytChannel(
        index: Int = 0,
        name: String = "FRS-01",
        rx: Long = 462_562_500,
        tx: Long? = 462_562_500,
        scanAdd: Boolean = true,
    ) = DumpQytChannel(
        index = index,
        name = name,
        rxFrequencyHz = rx,
        txFrequencyHz = tx,
        rxTone = Tone.None,
        txTone = Tone.None,
        power = "High",
        bandwidth = "Wide",
        scanAdd = scanAdd,
        busyLockout = false,
        scramble = false,
        pttId = "OFF",
        optionalSignaling = "OFF",
        speakerMute = "Tone/DTCS",
        scode = 0,
        flagsRaw = listOf(0, 0, 0, 36),
    )

    @Test
    fun an_unchanged_record_produces_no_edit_at_all() {
        val ch = qytChannel()
        assertNull("no change must mean no round trip and no undo step", qytChannelEdit(ch, ch))
    }

    @Test
    fun only_the_changed_field_appears_in_the_encoded_batch() {
        val edit = qytChannelEdit(qytChannel(), qytChannel(name = "EDITED"))!!
        val obj = CodeplugJson.instance.encodeToString(edit).let {
            Json.parseToJsonElement(it).jsonObject
        }
        assertEquals(setOf("index", "name"), obj.keys)
    }

    @Test
    fun clearing_the_tx_frequency_sends_an_explicit_null_not_an_absent_key() {
        // These are different instructions to the core: absent means "leave it
        // alone", explicit null means "clear it, make this receive-only".
        val edit = qytChannelEdit(qytChannel(), qytChannel(tx = null))!!
        val obj = Json.parseToJsonElement(CodeplugJson.instance.encodeToString(edit)).jsonObject
        assertTrue("tx_frequency_hz must be present", obj.containsKey("tx_frequency_hz"))
        assertEquals(JsonNull, obj["tx_frequency_hz"])
    }

    @Test
    fun leaving_the_tx_frequency_alone_omits_the_key_entirely() {
        val edit = qytChannelEdit(qytChannel(), qytChannel(name = "EDITED"))!!
        val obj = Json.parseToJsonElement(CodeplugJson.instance.encodeToString(edit)).jsonObject
        assertFalse(obj.containsKey("tx_frequency_hz"))
    }

    @Test
    fun setting_a_tx_frequency_sends_the_number() {
        val edit = qytChannelEdit(qytChannel(tx = null), qytChannel(tx = 462_000_000))!!
        val obj = Json.parseToJsonElement(CodeplugJson.instance.encodeToString(edit)).jsonObject
        assertEquals("462000000", obj["tx_frequency_hz"].toString())
    }

    @Test
    fun compound_enum_values_are_converted_to_the_tags_the_core_parses() {
        // A dump emits Rust variant names; the edit API parses lowercase or
        // snake_case. Getting this wrong drops the field silently.
        assertEquals("mixed_analog", editValue("MixedAnalog"))
        assertEquals("mixed_digital", editValue("MixedDigital"))
        assertEquals("analog", editValue("Analog"))
        assertEquals("high", editValue("High"))
    }

    @Test
    fun camel_case_values_become_snake_case_for_the_edit_api() {
        assertEquals("channel_free", snakeEditValue("ChannelFree"))
        assertEquals("two_tone", snakeEditValue("TwoTone"))
        assertEquals("dcs", snakeEditValue("Dcs"))
        assertEquals("none", snakeEditValue("None"))
    }

    @Test
    fun a_bulk_update_with_nothing_opted_in_is_empty() {
        assertTrue(BulkChannelUpdate().isEmpty)
        assertTrue(BulkQytChannelUpdate().isEmpty)
        assertFalse(BulkChannelUpdate(power = "High").isEmpty)
    }

    @Test
    fun a_bulk_update_names_every_selected_slot_and_only_the_opted_in_fields() {
        val spec = BulkChannelUpdate(power = "High").toSpec(listOf(3, 7, 11))
        assertEquals(listOf(3, 7, 11), spec.channels.map { it.index })
        assertTrue(spec.channels.all { it.power == "high" })
        // An unchecked field must be left alone on every selected channel.
        assertTrue(spec.channels.all { it.name == null && it.colorCode == null })
    }

    @Test
    fun a_bulk_update_encodes_without_the_fields_that_were_not_opted_in() {
        val spec = BulkQytChannelUpdate(scanAdd = false).toSpec(listOf(2))
        val json = CodeplugJson.instance.encodeToString(spec)
        assertTrue(json.contains("scan_add"))
        assertFalse(json.contains("\"name\""))
        assertFalse(json.contains("rx_frequency_hz"))
    }

    @Test
    fun move_and_remove_specs_target_the_right_family() {
        val move = moveSpec(CodeplugSection.ZONES, from = 2, to = 9)
        assertEquals(1, move.moveZones.size)
        assertTrue(move.moveChannels.isEmpty())

        val remove = removeSpec(CodeplugSection.CONTACTS, listOf(1, 2))
        assertEquals(listOf(1, 2), remove.removeContacts)
        assertTrue(remove.removeChannels.isEmpty())
    }
}
