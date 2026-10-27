package me.faulk.codeplugme.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive

/**
 * One QYT channel update or add. `index` is omitted for adds, and a null
 * field is left out of the encoded JSON so only what changed is applied
 * (see [CodeplugJson]).
 *
 * `scanAdd` is the only flag field here. Its bit was confirmed against a
 * real KT-5000, so the core has a setter for it; power, bandwidth, BCL,
 * PTT-ID and the rest are still decoded for display only, and offering a
 * control for them would silently discard the user's change on the next
 * reload.
 *
 * [txFrequencyHz] is typed as a raw [JsonElement] rather than `Long?`,
 * because this field needs a third state beyond "set" and "absent": the core
 * reads an explicit JSON `null` here as "clear the TX frequency, make this
 * channel receive-only" (`present_even_if_null` in
 * `core/src/qyt/codeplug/edits.rs`), which is different from an absent key
 * meaning "leave it alone". A plain `Long?` can't express that third state
 * once `explicitNulls = false` is in effect, because encoding maps every
 * Kotlin `null` to an absent key regardless of the field's static type. A
 * [JsonElement] does: leaving this property `null` (the default) still
 * produces an absent key, but assigning it [JsonNull] produces a literal
 * `"tx_frequency_hz": null` because [JsonNull] is a non-null [JsonElement]
 * value that the JSON encoder writes as-is. Use [txFrequencyHzValue] or
 * [CLEAR_TX_FREQUENCY] to build one instead of constructing a [JsonElement]
 * by hand at each call site.
 */
@Serializable
data class QytChannelEdit(
    val index: Int? = null,
    val name: String? = null,
    @SerialName("rx_frequency_hz") val rxFrequencyHz: Long? = null,
    @SerialName("tx_frequency_hz") val txFrequencyHz: JsonElement? = null,
    @SerialName("rx_tone") val rxTone: Tone? = null,
    @SerialName("tx_tone") val txTone: Tone? = null,
    /** Include the channel in scanning; false skips it. */
    @SerialName("scan_add") val scanAdd: Boolean? = null,
) {
    companion object {
        /** The [txFrequencyHz] value that clears it, making the channel receive-only. */
        val CLEAR_TX_FREQUENCY: JsonElement = JsonNull

        /** The [txFrequencyHz] value that sets it to [hz]. */
        fun txFrequencyHzValue(hz: Long): JsonElement = JsonPrimitive(hz)
    }
}

/**
 * Settings-header fields to change, all optional so an untouched field is
 * left out of the JSON entirely and the core never writes it.
 *
 * The field list is exactly what `core/src/qyt/codeplug/settings.rs` models.
 * Everything else in the settings block is byte-preserved and deliberately
 * absent here; see that module's docs for why.
 */
@Serializable
data class QytSettingsEdit(
    val tmr: Int? = null,
    val squelch: Int? = null,
    val tot: Int? = null,
    val volume: Int? = null,
    @SerialName("backlight_timer") val backlightTimer: Int? = null,
    val beep: Boolean? = null,
    @SerialName("scan_rev") val scanRev: String? = null,
    @SerialName("remember_power") val rememberPower: Boolean? = null,
) {
    /** True when the edit would change nothing, so the caller can skip the whole round trip. */
    val isEmpty: Boolean get() = this == QytSettingsEdit()
}

/**
 * One VFO channel's changes, addressed by index 0 (A) or 1 (B).
 *
 * The repeater offset and shift direction are absent on purpose: the core
 * has no setters for them because their encoding is assumed rather than
 * confirmed against the radio.
 */
@Serializable
data class QytBandEdit(
    val index: Int,
    @SerialName("frequency_hz") val frequencyHz: Long? = null,
    @SerialName("rx_tone") val rxTone: Tone? = null,
    @SerialName("tx_tone") val txTone: Tone? = null,
    /** "Wide" | "Narrow", from `capabilities.bandwidth_options`. */
    val bandwidth: String? = null,
    /** "High" | "Low", from `capabilities.power_options`. */
    val power: String? = null,
    @SerialName("step_hz") val stepHz: Long? = null,
) {
    /** True when only the index is set, meaning this record needs no round trip. */
    val isEmpty: Boolean get() = this == QytBandEdit(index = index)
}

/** A batch of QYT edits, the `qyt` half of the edit envelope. */
@Serializable
data class QytEditSpec(
    val channels: List<QytChannelEdit> = emptyList(),
    @SerialName("add_channels") val addChannels: List<QytChannelEdit> = emptyList(),
    @SerialName("remove_channels") val removeChannels: List<Int> = emptyList(),
    @SerialName("move_channels") val moveChannels: List<MoveOp> = emptyList(),
    /**
     * Null when the batch changes no settings; the core takes a single
     * object here rather than an array, since there is one header.
     */
    val settings: QytSettingsEdit? = null,
    val bands: List<QytBandEdit> = emptyList(),
    /**
     * The line the radio shows on its LCD. Null leaves it alone; the core
     * space-pads it out to the field's 16 bytes at `0x0FE0`.
     */
    @SerialName("static_message") val staticMessage: String? = null,
) {
    /** True when there is nothing to apply. */
    val isEmpty: Boolean
        get() = channels.isEmpty() && addChannels.isEmpty() && removeChannels.isEmpty() &&
            moveChannels.isEmpty() && settings == null && bands.isEmpty() &&
            staticMessage == null
}

/**
 * The model-tagged envelope an edit batch travels in. The AnyTone side still
 * sends its spec bare, which the core accepts for backward compatibility,
 * but a QYT batch has to name itself or the core would read it as an
 * AnyTone spec.
 */
@Serializable
data class QytEditEnvelope(val qyt: QytEditSpec)
