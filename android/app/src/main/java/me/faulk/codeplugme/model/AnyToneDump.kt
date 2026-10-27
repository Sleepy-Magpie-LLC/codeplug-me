package me.faulk.codeplugme.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * One decoded channel from the core's dump. Field names/values match the
 * Rust `Channel` serialization (enum fields arrive capitalized, e.g.
 * "Analog").
 */
@Serializable
data class DumpChannel(
    val index: Int,
    val name: String,
    @SerialName("rx_frequency_hz") val rxFrequencyHz: Long,
    @SerialName("tx_frequency_hz") val txFrequencyHz: Long,
    val mode: String,
    val power: String,
    val bandwidth: String,
    @SerialName("color_code") val colorCode: Int,
    @SerialName("time_slot") val timeSlot: Int,
    @SerialName("contact_index") val contactIndex: Long,
    @SerialName("radio_id_index") val radioIdIndex: Int,
    @SerialName("group_list_index") val groupListIndex: Int,
    // --- Analog / signaling ---
    @SerialName("rx_signaling_mode") val rxSignalingMode: String,
    @SerialName("tx_signaling_mode") val txSignalingMode: String,
    @SerialName("rx_ctcss") val rxCtcss: Int,
    @SerialName("tx_ctcss") val txCtcss: Int,
    @SerialName("rx_dcs") val rxDcs: Int,
    @SerialName("tx_dcs") val txDcs: Int,
    @SerialName("squelch_mode") val squelchMode: String,
    @SerialName("optional_signaling") val optionalSignaling: String,
    val admit: String,
    @SerialName("scan_list_index") val scanListIndex: Int,
    @SerialName("dtmf_id_index") val dtmfIdIndex: Int,
    @SerialName("two_tone_id_index") val twoToneIdIndex: Int,
    @SerialName("five_tone_id_index") val fiveToneIdIndex: Int,
    @SerialName("two_tone_decode_index") val twoToneDecodeIndex: Int,
    // --- Flags ---
    @SerialName("rx_only") val rxOnly: Boolean,
    @SerialName("talk_around") val talkAround: Boolean,
    @SerialName("call_confirm") val callConfirm: Boolean,
    @SerialName("work_alone") val workAlone: Boolean,
    @SerialName("simplex_tdma") val simplexTdma: Boolean,
    @SerialName("rx_aprs") val rxAprs: Boolean,
    // --- Repeater shift. Read-only in this build: there is no ChannelEdit
    // field for either, because the core's edit spec has no setter for them
    // yet (see edits.rs's ChannelFields). ---
    @SerialName("repeater_mode") val repeaterMode: String,
    @SerialName("tx_offset_hz") val txOffsetHz: Long,
)

/** One decoded zone from the core's dump. */
@Serializable
data class DumpZone(
    val index: Int,
    val name: String,
    val channels: List<Int>,
    /** Channel index used when scanning A/B on this zone; sentinel `0xffff` (65535) means none. */
    @SerialName("a_channel") val aChannel: Int,
    @SerialName("b_channel") val bChannel: Int,
)

/** One decoded scan list from the core's dump. */
@Serializable
data class DumpScanList(
    val index: Int,
    val name: String,
    val members: List<Int>,
    @SerialName("priority_channel_select") val priorityChannelSelect: Int,
    @SerialName("priority_channel_1") val priorityChannel1: Int,
    @SerialName("priority_channel_2") val priorityChannel2: Int,
    @SerialName("look_back_a") val lookBackA: Int,
    @SerialName("look_back_b") val lookBackB: Int,
    @SerialName("dropout_delay") val dropoutDelay: Int,
    @SerialName("dwell_time") val dwellTime: Int,
    @SerialName("revert_channel") val revertChannel: Int,
)

/** One decoded DMR contact / talk group from the core's dump. */
@Serializable
data class DumpContact(
    val index: Int,
    val name: String,
    val number: Long,
    /** "Private" | "Group" | "All" (talk groups are Group calls). */
    @SerialName("call_type") val callType: String,
)

/** One decoded RX group list from the core's dump. */
@Serializable
data class DumpGroupList(
    val index: Int,
    val name: String,
    val members: List<Int>,
)

/** One decoded radio ID from the core's dump. */
@Serializable
data class DumpRadioId(
    val index: Int,
    val name: String,
    val number: Long,
)

/**
 * Read-only APRS settings from the core's dump (the core exposes a focused
 * subset; there is no edit path, this block is never written to the radio).
 */
@Serializable
data class DumpAprs(
    @SerialName("source_call") val sourceCall: String,
    @SerialName("source_ssid") val sourceSsid: Int,
    @SerialName("destination_call") val destinationCall: String,
    @SerialName("destination_ssid") val destinationSsid: Int,
    @SerialName("symbol_table") val symbolTable: Int,
    val symbol: Int,
    @SerialName("manual_tx_interval") val manualTxInterval: Int,
    @SerialName("auto_tx_interval") val autoTxInterval: Int,
    @SerialName("fm_power") val fmPower: Int,
    @SerialName("fm_tx_frequency_hz") val fmTxFrequencyHz: Long,
)

/**
 * The AnyTone payload of the dump envelope, carried under its `"anytone"`
 * key.
 *
 * Every entity family other than `scanLists`/`aprs` is required on purpose. A
 * core bug that dropped a whole family would then fail the decode loudly
 * instead of quietly rendering an empty pane. `scanLists` defaults to empty
 * and `aprs` to null for backward compatibility with a core build that
 * predates those sections.
 */
@Serializable
data class AnyToneDump(
    val channels: List<DumpChannel>,
    val zones: List<DumpZone>,
    @SerialName("scan_lists") val scanLists: List<DumpScanList> = emptyList(),
    val contacts: List<DumpContact>,
    @SerialName("group_lists") val groupLists: List<DumpGroupList>,
    @SerialName("radio_ids") val radioIds: List<DumpRadioId>,
    val aprs: DumpAprs? = null,
)
