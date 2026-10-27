package me.faulk.codeplugme.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * One channel update or add. `index` is omitted for adds. Every field is
 * nullable, and with [CodeplugJson]'s `encodeDefaults = false` /
 * `explicitNulls = false` configuration, a null field is left out of the
 * encoded JSON entirely, so only what actually changed is sent to the core.
 */
@Serializable
data class ChannelEdit(
    val index: Int? = null,
    val name: String? = null,
    @SerialName("rx_frequency_hz") val rxFrequencyHz: Long? = null,
    @SerialName("tx_frequency_hz") val txFrequencyHz: Long? = null,
    /** "analog" | "digital" | "mixed_analog" | "mixed_digital". */
    val mode: String? = null,
    /** "low" | "mid" | "high" | "turbo". */
    val power: String? = null,
    /** "narrow" | "wide". */
    val bandwidth: String? = null,
    @SerialName("color_code") val colorCode: Int? = null,
    @SerialName("time_slot") val timeSlot: Int? = null,
    @SerialName("contact_index") val contactIndex: Long? = null,
    @SerialName("radio_id_index") val radioIdIndex: Int? = null,
    @SerialName("group_list_index") val groupListIndex: Int? = null,
    /** RX signaling type: "none" | "ctcss" | "dcs". */
    @SerialName("rx_signaling_mode") val rxSignalingMode: String? = null,
    /** TX signaling type: "none" | "ctcss" | "dcs". */
    @SerialName("tx_signaling_mode") val txSignalingMode: String? = null,
    /** RX CTCSS tone index. */
    @SerialName("rx_ctcss") val rxCtcss: Int? = null,
    /** TX CTCSS tone index. */
    @SerialName("tx_ctcss") val txCtcss: Int? = null,
    /** RX DCS code word. */
    @SerialName("rx_dcs") val rxDcs: Int? = null,
    /** TX DCS code word. */
    @SerialName("tx_dcs") val txDcs: Int? = null,
    /** "carrier" | "tone". */
    @SerialName("squelch_mode") val squelchMode: String? = null,
    /** "off" | "dtmf" | "two_tone" | "five_tone". */
    @SerialName("optional_signaling") val optionalSignaling: String? = null,
    /** "always" | "channel_free" | "different_color_code" | "same_color_code". */
    val admit: String? = null,
    /** Scan-list index (`0xff` = none). */
    @SerialName("scan_list_index") val scanListIndex: Int? = null,
    @SerialName("dtmf_id_index") val dtmfIdIndex: Int? = null,
    @SerialName("two_tone_id_index") val twoToneIdIndex: Int? = null,
    @SerialName("five_tone_id_index") val fiveToneIdIndex: Int? = null,
    @SerialName("two_tone_decode_index") val twoToneDecodeIndex: Int? = null,
    /** RX-only / "PTT prohibit". */
    @SerialName("rx_only") val rxOnly: Boolean? = null,
    /** Talk-around / simplex. */
    @SerialName("talk_around") val talkAround: Boolean? = null,
    /** Call confirmation. */
    @SerialName("call_confirm") val callConfirm: Boolean? = null,
    /** Lone-worker / "work alone". */
    @SerialName("work_alone") val workAlone: Boolean? = null,
    /** Simplex TDMA. */
    @SerialName("simplex_tdma") val simplexTdma: Boolean? = null,
    /** Receive APRS. */
    @SerialName("rx_aprs") val rxAprs: Boolean? = null,
)

/** One zone update or add (`index` omitted for adds). */
@Serializable
data class ZoneEdit(
    val index: Int? = null,
    val name: String? = null,
    val members: List<Int>? = null,
)

/** One scan-list update or add (`index` omitted for adds). */
@Serializable
data class ScanListEdit(
    val index: Int? = null,
    val name: String? = null,
    val members: List<Int>? = null,
    @SerialName("priority_channel_select") val priorityChannelSelect: Int? = null,
    @SerialName("priority_channel_1") val priorityChannel1: Int? = null,
    @SerialName("priority_channel_2") val priorityChannel2: Int? = null,
    @SerialName("look_back_a") val lookBackA: Int? = null,
    @SerialName("look_back_b") val lookBackB: Int? = null,
    @SerialName("dropout_delay") val dropoutDelay: Int? = null,
    @SerialName("dwell_time") val dwellTime: Int? = null,
    @SerialName("revert_channel") val revertChannel: Int? = null,
)

/** One contact update or add (`index` omitted for adds). */
@Serializable
data class ContactEdit(
    val index: Int? = null,
    val name: String? = null,
    val number: Long? = null,
    @SerialName("call_type") val callType: String? = null,
)

/** One group-list update or add (`index` omitted for adds). */
@Serializable
data class GroupListEdit(
    val index: Int? = null,
    val name: String? = null,
    val members: List<Int>? = null,
)

/** One radio-ID update or add (`index` omitted for adds). */
@Serializable
data class RadioIdEdit(
    val index: Int? = null,
    val name: String? = null,
    val number: Long? = null,
)

/**
 * Relocate the record at `from` to the free slot `to` (backs the editable
 * "#" column). Encoded into the `move_*` arrays of [EditSpec].
 */
@Serializable
data class MoveOp(val from: Int, val to: Int)

/**
 * Full batch passed to the core's apply-edits call. Empty arrays are fine;
 * the core applies updates, then removals, then additions.
 */
@Serializable
data class EditSpec(
    val channels: List<ChannelEdit> = emptyList(),
    @SerialName("add_channels") val addChannels: List<ChannelEdit> = emptyList(),
    @SerialName("remove_channels") val removeChannels: List<Int> = emptyList(),
    @SerialName("move_channels") val moveChannels: List<MoveOp> = emptyList(),
    val zones: List<ZoneEdit> = emptyList(),
    @SerialName("add_zones") val addZones: List<ZoneEdit> = emptyList(),
    @SerialName("remove_zones") val removeZones: List<Int> = emptyList(),
    @SerialName("move_zones") val moveZones: List<MoveOp> = emptyList(),
    @SerialName("scan_lists") val scanLists: List<ScanListEdit> = emptyList(),
    @SerialName("add_scan_lists") val addScanLists: List<ScanListEdit> = emptyList(),
    @SerialName("remove_scan_lists") val removeScanLists: List<Int> = emptyList(),
    @SerialName("move_scan_lists") val moveScanLists: List<MoveOp> = emptyList(),
    val contacts: List<ContactEdit> = emptyList(),
    @SerialName("add_contacts") val addContacts: List<ContactEdit> = emptyList(),
    @SerialName("remove_contacts") val removeContacts: List<Int> = emptyList(),
    @SerialName("move_contacts") val moveContacts: List<MoveOp> = emptyList(),
    @SerialName("group_lists") val groupLists: List<GroupListEdit> = emptyList(),
    @SerialName("add_group_lists") val addGroupLists: List<GroupListEdit> = emptyList(),
    @SerialName("remove_group_lists") val removeGroupLists: List<Int> = emptyList(),
    @SerialName("move_group_lists") val moveGroupLists: List<MoveOp> = emptyList(),
    @SerialName("radio_ids") val radioIds: List<RadioIdEdit> = emptyList(),
    @SerialName("add_radio_ids") val addRadioIds: List<RadioIdEdit> = emptyList(),
    @SerialName("remove_radio_ids") val removeRadioIds: List<Int> = emptyList(),
    @SerialName("move_radio_ids") val moveRadioIds: List<MoveOp> = emptyList(),
) {
    /** True when there is nothing to apply. */
    val isEmpty: Boolean
        get() = channels.isEmpty() && addChannels.isEmpty() && removeChannels.isEmpty() && moveChannels.isEmpty() &&
            zones.isEmpty() && addZones.isEmpty() && removeZones.isEmpty() && moveZones.isEmpty() &&
            scanLists.isEmpty() && addScanLists.isEmpty() && removeScanLists.isEmpty() && moveScanLists.isEmpty() &&
            contacts.isEmpty() && addContacts.isEmpty() && removeContacts.isEmpty() && moveContacts.isEmpty() &&
            groupLists.isEmpty() && addGroupLists.isEmpty() && removeGroupLists.isEmpty() && moveGroupLists.isEmpty() &&
            radioIds.isEmpty() && addRadioIds.isEmpty() && removeRadioIds.isEmpty() && moveRadioIds.isEmpty()
}
