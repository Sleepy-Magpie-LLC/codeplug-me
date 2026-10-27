package me.faulk.codeplugme.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * What a radio model can hold, as reported by the core. The app never keeps a
 * parallel table of this: Rust is the single source of truth, so adding a
 * model needs no app-side capability edit.
 *
 * `sections` is decoded as plain wire strings on purpose (rather than a typed
 * enum here): a future core listing a section this build has no screen for
 * must not fail the whole decode, and the UI layer already owns the mapping
 * from a section key to what it means (`me.faulk.codeplugme.state.CodeplugSection`),
 * dropping anything it doesn't recognize there. Keeping the raw strings here
 * means that leniency lives in exactly one place.
 */
@Serializable
data class Capabilities(
    /** Document section keys this model has, in sidebar order. */
    val sections: List<String>,
    @SerialName("channel_capacity") val channelCapacity: Int,
    @SerialName("channel_name_length") val channelNameLength: Int,
    @SerialName("supports_dmr") val supportsDmr: Boolean,
    /** How this model encodes CTCSS/DCS: a table index, or a frequency. */
    @SerialName("tone_encoding") val toneEncoding: String,
    @SerialName("power_options") val powerOptions: List<PickerOption>,
    @SerialName("bandwidth_options") val bandwidthOptions: List<PickerOption>,
)
