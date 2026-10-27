package me.faulk.codeplugme.model

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder

/**
 * Top-level payload of a codeplug dump: the model tag, its capabilities, and
 * exactly one per-model document payload.
 *
 * `model` stays a plain string (like [SupportedModel.id]) rather than
 * [RadioModel]: this type's own job is just to carry the right payload
 * through, and it does that by checking `model` against the payload that
 * showed up, not by resolving it to an enum first.
 */
@Serializable(with = CodeplugDumpSerializer::class)
data class CodeplugDump(
    val model: String,
    val displayName: String,
    val capabilities: Capabilities,
    val anytone: AnyToneDump?,
    val qyt: QytDump?,
)

/** The wire shape of [CodeplugDump], decoded first so it can be validated before becoming one. */
@Serializable
private data class CodeplugDumpDto(
    val model: String,
    @SerialName("display_name") val displayName: String,
    val capabilities: Capabilities,
    val anytone: AnyToneDump? = null,
    val qyt: QytDump? = null,
)

/**
 * Requires the payload `model` names to be present. A dump that says
 * "anytone" but carries no `anytone` object (or a `qyt` one) is a core bug,
 * and failing here surfaces it instead of opening a blank editor.
 */
object CodeplugDumpSerializer : KSerializer<CodeplugDump> {
    override val descriptor: SerialDescriptor = CodeplugDumpDto.serializer().descriptor

    override fun deserialize(decoder: Decoder): CodeplugDump {
        val dto = decoder.decodeSerializableValue(CodeplugDumpDto.serializer())
        if (dto.anytone == null && dto.qyt == null) {
            throw SerializationException(
                "dump names model \"${dto.model}\" but carries no payload"
            )
        }
        return CodeplugDump(dto.model, dto.displayName, dto.capabilities, dto.anytone, dto.qyt)
    }

    override fun serialize(encoder: Encoder, value: CodeplugDump) {
        encoder.encodeSerializableValue(
            CodeplugDumpDto.serializer(),
            CodeplugDumpDto(value.model, value.displayName, value.capabilities, value.anytone, value.qyt),
        )
    }
}
