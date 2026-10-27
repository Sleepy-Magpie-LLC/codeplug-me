package me.faulk.codeplugme.ui.channels

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import me.faulk.codeplugme.model.Capabilities
import me.faulk.codeplugme.model.DumpContact
import me.faulk.codeplugme.model.DumpGroupList
import me.faulk.codeplugme.model.DumpRadioId
import me.faulk.codeplugme.model.PickerOption
import me.faulk.codeplugme.model.formatDmrId
import me.faulk.codeplugme.state.BulkChannelUpdate
import me.faulk.codeplugme.ui.common.NumberStepper

/**
 * Bulk editor for [count] selected AnyTone channels: the eight fields that make
 * sense to apply across a selection, each opt-in via its own checkbox.
 *
 * Ported from `BulkChannelEditorSheet` (macOS `BulkEditorSheets.swift`). An
 * unchecked field is never sent, which is what leaves it untouched on every
 * selected channel; [onApply] receives a [BulkChannelUpdate] built from exactly
 * the checked fields.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun BulkChannelEditorScreen(
    count: Int,
    contacts: List<DumpContact>,
    groupLists: List<DumpGroupList>,
    radioIds: List<DumpRadioId>,
    capabilities: Capabilities?,
    onApply: (BulkChannelUpdate) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val powerOptions = capabilities?.powerOptions.orEmpty()
    val bandwidthOptions = capabilities?.bandwidthOptions.orEmpty()

    var changeMode by remember { mutableStateOf(false) }
    var changePower by remember { mutableStateOf(false) }
    var changeBandwidth by remember { mutableStateOf(false) }
    var changeColorCode by remember { mutableStateOf(false) }
    var changeTimeSlot by remember { mutableStateOf(false) }
    var changeContact by remember { mutableStateOf(false) }
    var changeGroupList by remember { mutableStateOf(false) }
    var changeRadioId by remember { mutableStateOf(false) }

    var draftMode by remember { mutableStateOf(channelModeOptions.first().tag) }
    var draftPower by remember(powerOptions) {
        mutableStateOf(powerOptions.firstOrNull()?.tag ?: "High")
    }
    var draftBandwidth by remember(bandwidthOptions) {
        mutableStateOf(bandwidthOptions.firstOrNull()?.tag ?: "Wide")
    }
    var draftColorCode by remember { mutableStateOf(1) }
    var draftTimeSlot by remember { mutableStateOf(1) }
    var draftContactIndex by remember(contacts) {
        mutableStateOf(contacts.firstOrNull()?.index?.toLong() ?: 0L)
    }
    var draftGroupListIndex by remember { mutableStateOf(LIST_REFERENCE_NONE) }
    var draftRadioIdIndex by remember(radioIds) {
        mutableStateOf(radioIds.firstOrNull()?.index ?: 0)
    }

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text("Bulk Edit $count Channels") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Cancel")
                    }
                },
                actions = {
                    IconButton(onClick = {
                        onApply(
                            BulkChannelUpdate(
                                mode = draftMode.takeIf { changeMode },
                                power = draftPower.takeIf { changePower },
                                bandwidth = draftBandwidth.takeIf { changeBandwidth },
                                colorCode = draftColorCode.takeIf { changeColorCode },
                                timeSlot = draftTimeSlot.takeIf { changeTimeSlot },
                                contactIndex = draftContactIndex.takeIf { changeContact },
                                groupListIndex = draftGroupListIndex.takeIf { changeGroupList },
                                radioIdIndex = draftRadioIdIndex.takeIf { changeRadioId },
                            )
                        )
                        onBack()
                    }) {
                        Icon(Icons.Default.Check, contentDescription = "Apply")
                    }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text(
                "Only checked fields will be updated. Unchecked fields keep their " +
                    "current per-channel values.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            BulkFieldRow("Mode", changeMode, { changeMode = it }) {
                DropdownField(
                    "Mode", channelModeOptions,
                    channelModeOptions.first { it.tag == draftMode }, { it.label },
                    { draftMode = it.tag }, enabled = changeMode,
                )
            }

            BulkFieldRow("Power", changePower, { changePower = it }) {
                DropdownField(
                    "Power", powerOptions,
                    powerOptions.firstOrNull { it.tag == draftPower }
                        ?: PickerOption(draftPower, draftPower),
                    { it.label }, { draftPower = it.tag }, enabled = changePower,
                )
            }

            BulkFieldRow("Bandwidth", changeBandwidth, { changeBandwidth = it }) {
                DropdownField(
                    "Bandwidth", bandwidthOptions,
                    bandwidthOptions.firstOrNull { it.tag == draftBandwidth }
                        ?: PickerOption(draftBandwidth, draftBandwidth),
                    { it.label }, { draftBandwidth = it.tag }, enabled = changeBandwidth,
                )
            }

            BulkFieldRow("Color code", changeColorCode, { changeColorCode = it }) {
                NumberStepper(
                    "Color code", draftColorCode, { draftColorCode = it },
                    range = 0..15, enabled = changeColorCode,
                )
            }

            BulkFieldRow("Time slot", changeTimeSlot, { changeTimeSlot = it }) {
                SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                    listOf(1, 2).forEachIndexed { i, slot ->
                        SegmentedButton(
                            selected = draftTimeSlot == slot,
                            onClick = { draftTimeSlot = slot },
                            shape = SegmentedButtonDefaults.itemShape(index = i, count = 2),
                            enabled = changeTimeSlot,
                        ) { Text("Slot $slot") }
                    }
                }
            }

            BulkFieldRow("Contact", changeContact, { changeContact = it }) {
                val choices = contacts.map { it.index.toLong() to "${it.name} (${formatDmrId(it.number)})" }
                if (choices.isNotEmpty()) {
                    DropdownField(
                        "Contact", choices,
                        choices.firstOrNull { it.first == draftContactIndex } ?: choices.first(),
                        { it.second }, { draftContactIndex = it.first },
                        enabled = changeContact,
                    )
                } else {
                    Text(
                        "No contacts to choose from.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }

            BulkFieldRow("RX group list", changeGroupList, { changeGroupList = it }) {
                val choices = listOf(LIST_REFERENCE_NONE to "None") +
                    groupLists.map { it.index to it.name }
                DropdownField(
                    "RX group list", choices,
                    choices.first { it.first == draftGroupListIndex }, { it.second },
                    { draftGroupListIndex = it.first }, enabled = changeGroupList,
                )
            }

            BulkFieldRow("Radio ID", changeRadioId, { changeRadioId = it }) {
                val choices = radioIds.map { it.index to "${it.name} (${formatDmrId(it.number)})" }
                if (choices.isNotEmpty()) {
                    DropdownField(
                        "Radio ID", choices,
                        choices.firstOrNull { it.first == draftRadioIdIndex } ?: choices.first(),
                        { it.second }, { draftRadioIdIndex = it.first },
                        enabled = changeRadioId,
                    )
                } else {
                    Text(
                        "No radio IDs to choose from.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}

/** One opt-in bulk field: a checkbox gating whether [control] takes effect. */
@Composable
private fun BulkFieldRow(
    label: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    control: @Composable () -> Unit,
) {
    Row(
        Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Checkbox(checked = checked, onCheckedChange = onCheckedChange)
        Text(label, modifier = Modifier.width(96.dp))
        Box(Modifier.weight(1f)) { control() }
    }
}
