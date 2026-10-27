package me.faulk.codeplugme.model

import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.JsonArray
import org.junit.Assert.assertEquals
import org.junit.Test

/** Picked repeaters go back to the core as they came, so nothing may be lost. */
class RepeaterTest {

    private val coreJson = """
        [
          {"source":"repeaterbook","callsign":"W0ABC","location":"Denver","rx_hz":146940000,
           "tx_hz":146340000,"mode":"analog","tx_tone":{"kind":"ctcss","hz_tenths":1000},
           "rx_tone":{"kind":"dcs","code":23,"inverted":true},"color_code":null,
           "lat":39.74,"lon":-104.99,"distance_km":3.2,"duplicate":true},
          {"source":"radioid","callsign":"K0DMR","location":"Boulder, Colorado","rx_hz":446500000,
           "tx_hz":441500000,"mode":"dmr","tx_tone":{"kind":"none"},"rx_tone":{"kind":"none"},
           "color_code":1,"lat":null,"lon":null,"distance_km":null,"duplicate":false}
        ]
    """.trimIndent()

    @Test
    fun every_field_survives_a_round_trip() {
        val decoded = CodeplugJson.instance.decodeFromString<List<Repeater>>(coreJson)
        assertEquals(RepeaterSource.REPEATERBOOK, decoded[0].source)
        assertEquals(Tone.Dcs(23, true), decoded[0].rxTone)
        assertEquals(1, decoded[1].colorCode)

        val reEncoded = CodeplugJson.instance.encodeToString(decoded)
        assertEquals(decoded, CodeplugJson.instance.decodeFromString<List<Repeater>>(reEncoded))

        // Same JSON once the core's explicit nulls and default false are taken
        // out, since the core reads a missing one back as exactly that.
        val stripped = (CodeplugJson.instance.parseToJsonElement(coreJson) as JsonArray).map { el ->
            kotlinx.serialization.json.JsonObject(
                (el as kotlinx.serialization.json.JsonObject).filterValues {
                    it != kotlinx.serialization.json.JsonNull &&
                        it.toString() != "false"
                }
            )
        }
        assertEquals(JsonArray(stripped), CodeplugJson.instance.parseToJsonElement(reEncoded))
    }
}
