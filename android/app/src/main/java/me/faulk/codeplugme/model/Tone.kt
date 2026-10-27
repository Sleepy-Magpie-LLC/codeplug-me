package me.faulk.codeplugme.model

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerializationException
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.descriptors.buildClassSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonEncoder
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/**
 * A sub-audible squelch tone, matching the core's tagged JSON
 * (`{"kind":"none"}`, `{"kind":"ctcss","hz_tenths":885}`,
 * `{"kind":"dcs","code":23,"inverted":false}`).
 *
 * `Tone` and `Tone.None` mean the same thing here whether decoded from an
 * AnyTone or a QYT dump; what differs between the two radios is how a
 * *reference* to a tone is stored (an AnyTone channel holds a table index,
 * a QYT channel holds one of these directly), which is why only the QYT
 * records carry this type.
 */
@Serializable(with = ToneSerializer::class)
sealed class Tone {
    /** No tone; the squelch is carrier-operated. */
    data object None : Tone()

    /** CTCSS, in tenths of a hertz (885 = 88.5 Hz). */
    data class Ctcss(val hzTenths: Int) : Tone()

    /** DCS / DTCS, by the three-digit code written on the radio. */
    data class Dcs(val code: Int, val inverted: Boolean) : Tone()
}

/**
 * Hand-written because [Tone] is a tagged union keyed on a `kind` string
 * rather than a field kotlinx.serialization's class discriminator machinery
 * would produce; this also lets decoding reject an unknown kind explicitly,
 * the same way the core would refuse to have produced one.
 */
object ToneSerializer : KSerializer<Tone> {
    override val descriptor: SerialDescriptor = buildClassSerialDescriptor("Tone")

    override fun serialize(encoder: Encoder, value: Tone) {
        val jsonEncoder = encoder as? JsonEncoder
            ?: throw SerializationException("Tone can only be serialized to JSON")
        val element: JsonObject = when (value) {
            is Tone.None -> buildJsonObject { put("kind", "none") }
            is Tone.Ctcss -> buildJsonObject {
                put("kind", "ctcss")
                put("hz_tenths", value.hzTenths)
            }
            is Tone.Dcs -> buildJsonObject {
                put("kind", "dcs")
                put("code", value.code)
                put("inverted", value.inverted)
            }
        }
        jsonEncoder.encodeJsonElement(element)
    }

    override fun deserialize(decoder: Decoder): Tone {
        val jsonDecoder = decoder as? JsonDecoder
            ?: throw SerializationException("Tone can only be deserialized from JSON")
        val obj = jsonDecoder.decodeJsonElement().jsonObject
        return when (val kind = obj.getValue("kind").jsonPrimitive.content) {
            "none" -> Tone.None
            "ctcss" -> Tone.Ctcss(hzTenths = obj.getValue("hz_tenths").jsonPrimitive.int)
            "dcs" -> Tone.Dcs(
                code = obj.getValue("code").jsonPrimitive.int,
                inverted = obj.getValue("inverted").jsonPrimitive.boolean,
            )
            else -> throw SerializationException("unknown tone kind \"$kind\"")
        }
    }
}
