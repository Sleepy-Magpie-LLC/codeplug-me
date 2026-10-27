package me.faulk.codeplugme.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * One decoded QYT channel from the core's dump.
 *
 * Everything from `scode` down is read-only: the core decodes those bits
 * from the record's flag bytes but has no setters for them yet, because
 * their meanings come from CHIRP and have not been confirmed against this
 * radio one field at a time. `flagsRaw` carries bytes 12-15 verbatim so the
 * unmodelled bits are visible and provably round-trip.
 */
@Serializable
data class DumpQytChannel(
    val index: Int,
    val name: String,
    @SerialName("rx_frequency_hz") val rxFrequencyHz: Long,
    /** Null when the record stores no TX frequency, making the channel receive-only. */
    @SerialName("tx_frequency_hz") val txFrequencyHz: Long? = null,
    @SerialName("rx_tone") val rxTone: Tone,
    @SerialName("tx_tone") val txTone: Tone,
    /** PTT-ID signal code index (the low nibble of flag byte 12). */
    val scode: Int = 0,
    /** "Tone/DTCS" | "Tone/DTCS and Optsig" | "Tone/DTCS or Optsig". */
    @SerialName("speaker_mute") val speakerMute: String = "Tone/DTCS",
    /** "OFF" | "DTMF" | "2TONE" | "5TONE". */
    @SerialName("optional_signaling") val optionalSignaling: String = "OFF",
    val scramble: Boolean = false,
    /** "High" | "Low", matching `capabilities.power_options`. */
    val power: String = "High",
    /** "Wide" | "Narrow", matching `capabilities.bandwidth_options`. */
    val bandwidth: String = "Wide",
    @SerialName("busy_lockout") val busyLockout: Boolean = false,
    /** Included in scan (the core's `scan_add`; false means skipped). */
    @SerialName("scan_add") val scanAdd: Boolean = true,
    /** "OFF" | "BOT" | "EOT" | "BOTH". */
    @SerialName("ptt_id") val pttId: String = "OFF",
    /** Flag bytes 12-15 verbatim. */
    @SerialName("flags_raw") val flagsRaw: List<Int> = listOf(0, 0, 0, 0),
)

/**
 * The KT-5000 settings header the core models (`0x0E00`-`0x0E35`). Everything
 * else in the settings block is byte-preserved and not modelled.
 */
@Serializable
data class DumpQytSettings(
    /** Dual-band waiting (Menu 0 TMR): 0 = Off, 1 = On. */
    val tmr: Int,
    /** Squelch level, 0-9. */
    val squelch: Int,
    /** Transmit timeout as a step index. */
    val tot: Int,
    /** The same timeout in seconds, derived by the core. */
    @SerialName("tot_seconds") val totSeconds: Int,
    /** Power-on volume. */
    val volume: Int,
    /** Backlight timer: 0 is off, otherwise seconds. */
    @SerialName("backlight_timer") val backlightTimer: Int,
    /** Key beep on/off. */
    val beep: Boolean,
    /**
     * Scan resume method: `"TO"` | `"CO"` | `"SE"`. Null when the stored byte
     * is outside that set; the picker then shows an "Unknown" entry.
     */
    @SerialName("scan_rev") val scanRev: String? = null,
    /**
     * When on, applying power restores the last on/off state; when off,
     * applying power always leaves the radio off.
     */
    @SerialName("remember_power") val rememberPower: Boolean,
) {
    companion object {
        /**
         * A stand-in for "no document open", so a pane's draft is never
         * optional. The values are the header's own zeroes, not defaults
         * anyone would write: nothing is sent to the core unless the draft
         * differs from a real dump.
         */
        val EMPTY = DumpQytSettings(
            tmr = 0, squelch = 0, tot = 0, totSeconds = 15, volume = 0,
            backlightTimer = 0, beep = false, scanRev = "TO", rememberPower = false,
        )
    }
}

/**
 * One of the two VFO channels (A or B). The offset and shift are read-only in
 * the core: their encoding is assumed rather than confirmed.
 */
@Serializable
data class DumpQytBand(
    val index: Int,
    /** False when the record holds no decodable frequency (an unused VFO). */
    val programmed: Boolean,
    @SerialName("frequency_hz") val frequencyHz: Long? = null,
    @SerialName("offset_hz") val offsetHz: Long? = null,
    /** "Off" | "Plus" | "Minus". */
    val shift: String,
    @SerialName("rx_tone") val rxTone: Tone,
    @SerialName("tx_tone") val txTone: Tone,
    val scode: Int,
    @SerialName("speaker_mute") val speakerMute: String,
    @SerialName("optional_signaling") val optionalSignaling: String,
    val scramble: Boolean,
    val bandwidth: String,
    val power: String,
    @SerialName("step_hz") val stepHz: Long? = null,
)

/**
 * The QYT payload of the dump envelope, carried under its `"qyt"` key.
 * Non-optional for the same reason [AnyToneDump]'s required families are: a
 * core bug that dropped one should fail the decode loudly.
 */
@Serializable
data class QytDump(
    val channels: List<DumpQytChannel>,
    val settings: DumpQytSettings,
    val bands: List<DumpQytBand>,
    /** The radio's own ASCII tag at `0x0FE0`, read-only. */
    @SerialName("static_message") val staticMessage: String,
)
