package me.faulk.codeplugme.ui.channels

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Card
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import me.faulk.codeplugme.model.DumpChannel
import me.faulk.codeplugme.model.DumpQytChannel
import me.faulk.codeplugme.model.PickerOption
import me.faulk.codeplugme.model.Tone
import me.faulk.codeplugme.model.formatMhz
import me.faulk.codeplugme.ui.common.formatSlot
import me.faulk.codeplugme.ui.common.parseMhz

/**
 * Display and semantics shared by the AnyTone and QYT channel screens.
 *
 * Ported from `Models.swift` / `QytModels.swift`. The picker option lists below
 * carry dump-valued tags (e.g. "Analog", "ChannelFree") that bind directly to a
 * [DumpChannel] field, matching the source of truth on macOS; there is no core
 * capability for these because they are fixed by the channel record's own enum,
 * unlike power/bandwidth which vary per radio model.
 */

/** Picker options for channel mode. */
val channelModeOptions: List<PickerOption> = listOf(
    PickerOption("Analog", "Analog"),
    PickerOption("Digital", "Digital"),
    PickerOption("MixedAnalog", "Mixed (A)"),
    PickerOption("MixedDigital", "Mixed (D)"),
)

/** Picker options for the TX-permit / admit criterion. */
val admitOptions: List<PickerOption> = listOf(
    PickerOption("Always", "Always"),
    PickerOption("ChannelFree", "Channel Free"),
    PickerOption("DifferentColorCode", "Different CC"),
    PickerOption("SameColorCode", "Same CC"),
)

/** Picker options for a channel's RX/TX signaling type. */
val signalingModeOptions: List<PickerOption> = listOf(
    PickerOption("None", "None"),
    PickerOption("Ctcss", "CTCSS"),
    PickerOption("Dcs", "DCS"),
)

/** Picker options for squelch mode. */
val squelchModeOptions: List<PickerOption> = listOf(
    PickerOption("Carrier", "Carrier"),
    PickerOption("Tone", "Tone (CTCSS/DCS)"),
)

/** Picker options for optional signaling. */
val optionalSignalingOptions: List<PickerOption> = listOf(
    PickerOption("Off", "Off"),
    PickerOption("Dtmf", "DTMF"),
    PickerOption("TwoTone", "2-Tone"),
    PickerOption("FiveTone", "5-Tone"),
)

/** `0xff` in the group-list / scan-list index means "no list". */
const val LIST_REFERENCE_NONE = 255

/** Whether a channel mode carries a digital (DMR) component; only pure "Analog" doesn't. */
fun modeHasDigital(mode: String): Boolean = mode != "Analog"

/** Whether a channel mode carries analog signaling; pure "Digital" doesn't. */
fun modeHasAnalog(mode: String): Boolean = mode != "Digital"

/** Whether an AnyTone channel passes the Channels-screen filters. Null means "no constraint". */
fun channelMatchesFilter(
    channel: DumpChannel,
    mode: String?,
    colorCode: Int?,
    slot: Int?,
): Boolean =
    (mode == null || channel.mode == mode) &&
        (colorCode == null || channel.colorCode == colorCode) &&
        (slot == null || channel.timeSlot == slot)

/** The human label for a tag in a set of core-supplied picker options, or the tag itself. */
fun pickerLabel(tag: String, options: List<PickerOption>): String =
    options.firstOrNull { it.tag == tag }?.label ?: tag

/** How a tone reads in a row or field: "Off", "88.5 Hz", "D023N". */
fun toneLabel(tone: Tone): String = when (tone) {
    is Tone.None -> "Off"
    is Tone.Ctcss -> "${tone.hzTenths / 10}.${tone.hzTenths % 10} Hz"
    is Tone.Dcs -> "D%03d%s".format(tone.code, if (tone.inverted) "I" else "N")
}

/** Format an optional TX frequency, or an em dash for a receive-only channel. */
fun formatOptionalMhz(hz: Long?): String = hz?.let(::formatMhz) ?: "—"

/** Flag bytes 12-15 as hex, for the QYT editor's read-only disclosure. */
fun flagsRawHex(flagsRaw: List<Int>): String =
    flagsRaw.joinToString(" ") { "%02X".format(it) }

/** "Yes" / "No" for a read-only boolean row. */
fun yesNo(value: Boolean): String = if (value) "Yes" else "No"

/**
 * Truncate a channel name to what the radio can store. The limit comes from
 * `capabilities.channel_name_length` rather than a constant here, so it follows
 * the core.
 */
fun limitedName(name: String, limit: Int): String =
    if (name.length <= limit) name else name.take(limit)

/**
 * Whether a QYT channel passes the channels-screen search: name or either
 * frequency. There is deliberately no mode / color-code / slot filter, since
 * this radio is analog-only and has none of those fields.
 */
fun qytChannelMatchesFilter(channel: DumpQytChannel, query: String): Boolean {
    if (query.isBlank()) return true
    return channel.name.contains(query, ignoreCase = true) ||
        formatMhz(channel.rxFrequencyHz).contains(query) ||
        formatOptionalMhz(channel.txFrequencyHz).contains(query)
}

/**
 * What makes two channels "the same channel" for duplicate highlighting.
 * The mode is part of it, so channels of different types (analog, digital,
 * or either mixed mode) on the same pair never match each other. Color code
 * and time slot are null for an analog channel. Mode is null for a radio with
 * only one channel type. The tones are null when the channel has no tone, or
 * no analog side for one to apply to. There is no offset field: both radios
 * derive TX from RX plus the offset, so matching TX already matches it.
 */
data class ChannelFrequencyKey(
    val rxFrequencyHz: Long,
    val txFrequencyHz: Long?,
    val mode: String? = null,
    val rxTone: String? = null,
    val txTone: String? = null,
    val colorCode: Int? = null,
    val timeSlot: Int? = null,
)

/**
 * Map each slot that shares its key with at least one other record to the
 * other slots in its group (ascending). Slots with no twin are left out.
 */
fun <T> duplicateSlots(items: List<T>, slotOf: (T) -> Int, keyOf: (T) -> ChannelFrequencyKey): Map<Int, List<Int>> =
    items.groupBy(keyOf, slotOf).values
        .filter { it.size > 1 }
        .flatMap { group -> group.sorted().let { slots -> slots.map { s -> s to slots.filter { it != s } } } }
        .toMap()

/**
 * AnyTone duplicates: same RX, TX and mode, for any mode with an analog side
 * also the same RX / TX tones, and for any mode with a DMR component also the
 * same color code and time slot. Names are ignored on purpose.
 */
fun duplicateChannelSlots(channels: List<DumpChannel>): Map<Int, List<Int>> =
    duplicateSlots(channels, DumpChannel::index) { c ->
        val digital = modeHasDigital(c.mode)
        val analog = modeHasAnalog(c.mode)
        ChannelFrequencyKey(
            rxFrequencyHz = c.rxFrequencyHz,
            txFrequencyHz = c.txFrequencyHz,
            mode = c.mode,
            rxTone = if (analog) anytoneToneKey(c.rxSignalingMode, c.rxCtcss, c.rxDcs) else null,
            txTone = if (analog) anytoneToneKey(c.txSignalingMode, c.txCtcss, c.txDcs) else null,
            colorCode = if (digital) c.colorCode else null,
            timeSlot = if (digital) c.timeSlot else null,
        )
    }

/**
 * One side's tone as a duplicate-key value: null for no tone, otherwise the
 * signaling type with the one value that type uses. The CTCSS index is
 * ignored under DCS and the reverse, since the radio ignores it too.
 */
fun anytoneToneKey(signalingMode: String, ctcss: Int, dcs: Int): String? = when (signalingMode) {
    "Ctcss" -> "ctcss $ctcss"
    "Dcs" -> "dcs $dcs"
    else -> null
}

/**
 * QYT duplicates: same RX, TX and tones (two receive-only channels on one RX
 * count). The radio is analog-only, so there is no color code or slot to match.
 */
fun duplicateQytChannelSlots(channels: List<DumpQytChannel>): Map<Int, List<Int>> =
    duplicateSlots(channels, DumpQytChannel::index) {
        ChannelFrequencyKey(
            rxFrequencyHz = it.rxFrequencyHz,
            txFrequencyHz = it.txFrequencyHz,
            rxTone = if (it.rxTone == Tone.None) null else toneLabel(it.rxTone),
            txTone = if (it.txTone == Tone.None) null else toneLabel(it.txTone),
        )
    }

/**
 * Duplicates-only sort: RX, then TX, then slot, so each group of twins sits
 * together in slot order.
 */
val channelFrequencyOrder: Comparator<DumpChannel> =
    compareBy({ it.rxFrequencyHz }, { it.txFrequencyHz }, { it.index })

/** QYT duplicates-only sort: RX, then TX (receive-only first), then slot. */
val qytChannelFrequencyOrder: Comparator<DumpQytChannel> =
    compareBy({ it.rxFrequencyHz }, { it.txFrequencyHz ?: -1L }, { it.index })

/** Row caption for a duplicated channel: "Duplicate of channel 12, 40". */
fun duplicateLabel(others: List<Int>): String =
    "Duplicate of channel " + others.joinToString(", ") { formatSlot(it) }

// ------------------------------------------------------------------- widgets

/**
 * A titled card grouping related fields, standing in for SwiftUI's `Form`
 * `Section`. When [enabled] is false the title dims to signal the whole group
 * is inert; each field inside still needs its own `enabled` passed through; a
 * child composable does not inherit it automatically the way a SwiftUI
 * `.disabled()` modifier cascades.
 */
@Composable
fun EditorSection(
    title: String,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    content: @Composable ColumnScope.() -> Unit,
) {
    Card(modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(
                title,
                style = MaterialTheme.typography.titleMedium,
                color = if (enabled) MaterialTheme.colorScheme.onSurface
                else MaterialTheme.colorScheme.onSurfaceVariant,
            )
            content()
        }
    }
}

/** A labelled on/off row, standing in for SwiftUI's `Toggle`. */
@Composable
fun SwitchField(
    label: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    Row(
        modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, color = if (enabled) MaterialTheme.colorScheme.onSurface
        else MaterialTheme.colorScheme.onSurfaceVariant)
        Switch(checked = checked, onCheckedChange = onCheckedChange, enabled = enabled)
    }
}

/**
 * A dropdown selection field, standing in for SwiftUI's `Picker`. Generic over
 * whatever the caller's option list holds, so it serves both a fixed
 * [PickerOption] list and a reference list built from records (contacts, group
 * lists) with their own display strings.
 *
 * A plain `OutlinedButton` + `DropdownMenu` rather than an
 * `ExposedDropdownMenuBox`: the latter is a text-field-shaped picker with its
 * own experimental anchor API, and this app's other editors (see
 * `ui/records/RecordEditors.kt`'s `ContactEditorScreen`) already settled on the
 * simpler button-and-menu shape for a field that is never typed into.
 */
@Composable
fun <T> DropdownField(
    label: String,
    options: List<T>,
    selected: T,
    optionLabel: (T) -> String,
    onSelect: (T) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    var expanded by remember { mutableStateOf(false) }
    Column(modifier) {
        OutlinedButton(
            onClick = { expanded = true },
            enabled = enabled,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("$label: ${optionLabel(selected)}")
        }
        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            options.forEach { option ->
                DropdownMenuItem(
                    text = { Text(optionLabel(option)) },
                    onClick = { onSelect(option); expanded = false },
                )
            }
        }
    }
}

/**
 * An editable frequency field in MHz, backing an Hz-valued field. Always reads
 * through [formatMhz] and writes through [parseMhz] (rule: never `toString()`
 * on a frequency), so a comma-decimal or grouping-separator locale can never
 * corrupt the stored value.
 */
@Composable
fun FrequencyField(
    label: String,
    hz: Long,
    onHzChange: (Long) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    var text by remember(hz) { mutableStateOf(formatMhz(hz)) }
    OutlinedTextField(
        value = text,
        onValueChange = { text = it; parseMhz(it)?.let(onHzChange) },
        label = { Text(label) },
        enabled = enabled,
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
        modifier = modifier.fillMaxWidth(),
    )
}
