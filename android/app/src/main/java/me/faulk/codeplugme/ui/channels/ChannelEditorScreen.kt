package me.faulk.codeplugme.ui.channels

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import me.faulk.codeplugme.model.Capabilities
import me.faulk.codeplugme.model.DumpChannel
import me.faulk.codeplugme.model.DumpContact
import me.faulk.codeplugme.model.DumpGroupList
import me.faulk.codeplugme.model.DumpRadioId
import me.faulk.codeplugme.model.DumpScanList
import me.faulk.codeplugme.model.PickerOption
import me.faulk.codeplugme.model.formatDmrId
import me.faulk.codeplugme.ui.common.NumberStepper
import me.faulk.codeplugme.ui.common.formatSlot

/**
 * Editor for one AnyTone channel: 32 fields grouped as Frequency, Channel,
 * Digital (DMR), Analog, and Flags.
 *
 * Ported from `ChannelEditorSheet` (macOS `EditorSheets.swift`). The Digital
 * group is disabled for a pure-"Analog" channel and the Analog group for a
 * pure-"Digital" one, exactly as macOS does it; each field inside a disabled
 * group also gets `enabled = false` directly, since Compose has no equivalent
 * of SwiftUI's cascading `.disabled()`.
 *
 * The caller owns [channel]; nothing here writes to the codeplug directly.
 * [onSave] is invoked with the edited draft only when the user taps the check
 * mark, and [onBack] alone discards it, matching the sheet's Cancel / Done.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChannelEditorScreen(
    channel: DumpChannel,
    contacts: List<DumpContact>,
    groupLists: List<DumpGroupList>,
    radioIds: List<DumpRadioId>,
    scanLists: List<DumpScanList>,
    capabilities: Capabilities?,
    onSave: (DumpChannel) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var draft by remember(channel) { mutableStateOf(channel) }
    val isDigital = modeHasDigital(draft.mode)
    val hasAnalog = modeHasAnalog(draft.mode)
    val powerOptions = capabilities?.powerOptions.orEmpty()
    val bandwidthOptions = capabilities?.bandwidthOptions.orEmpty()

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text("Channel ${formatSlot(draft.index)}") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Cancel")
                    }
                },
                actions = {
                    IconButton(onClick = { onSave(draft); onBack() }) {
                        Icon(Icons.Default.Check, contentDescription = "Save")
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
            OutlinedTextField(
                value = draft.name,
                onValueChange = { draft = draft.copy(name = it) },
                label = { Text("Name") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )

            EditorSection("Frequency") {
                FrequencyField(
                    "Receive (MHz)", draft.rxFrequencyHz,
                    { draft = draft.copy(rxFrequencyHz = it) },
                )
                FrequencyField(
                    "Transmit (MHz)", draft.txFrequencyHz,
                    { draft = draft.copy(txFrequencyHz = it) },
                )
            }

            EditorSection("Channel") {
                DropdownField(
                    "Mode", channelModeOptions, taggedOrRaw(channelModeOptions, draft.mode),
                    { it.label }, { draft = draft.copy(mode = it.tag) },
                )
                DropdownField(
                    "Power", powerOptions, taggedOrRaw(powerOptions, draft.power),
                    { it.label }, { draft = draft.copy(power = it.tag) },
                )
                DropdownField(
                    "Bandwidth", bandwidthOptions, taggedOrRaw(bandwidthOptions, draft.bandwidth),
                    { it.label }, { draft = draft.copy(bandwidth = it.tag) },
                )

                // "None" plus a synthetic entry when the stored index no longer
                // names a real scan list, so the dropdown never silently rewrites
                // a dangling reference to whatever happens to be first.
                val scanChoices = buildList {
                    add(LIST_REFERENCE_NONE to "None")
                    addAll(scanLists.map { it.index to it.name })
                    if (draft.scanListIndex != LIST_REFERENCE_NONE &&
                        scanLists.none { it.index == draft.scanListIndex }
                    ) {
                        add(draft.scanListIndex to "#${formatSlot(draft.scanListIndex)} (missing)")
                    }
                }
                DropdownField(
                    "Scan list", scanChoices,
                    scanChoices.first { it.first == draft.scanListIndex }, { it.second },
                    { draft = draft.copy(scanListIndex = it.first) },
                )

                DropdownField(
                    "TX permit", admitOptions, taggedOrRaw(admitOptions, draft.admit),
                    { it.label }, { draft = draft.copy(admit = it.tag) },
                )
            }

            EditorSection("Digital (DMR)", enabled = isDigital) {
                NumberStepper(
                    "Color code", draft.colorCode, { draft = draft.copy(colorCode = it) },
                    range = 0..15, enabled = isDigital,
                )

                SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                    listOf(1, 2).forEachIndexed { i, slot ->
                        SegmentedButton(
                            selected = draft.timeSlot == slot,
                            onClick = { draft = draft.copy(timeSlot = slot) },
                            shape = SegmentedButtonDefaults.itemShape(index = i, count = 2),
                            enabled = isDigital,
                        ) { Text("Slot $slot") }
                    }
                }

                // No "None" option: a channel always references some contact,
                // with a "(missing)" fallback shown when that slot is gone.
                val contactChoices = buildList {
                    addAll(contacts.map { it.index.toLong() to "${it.name} (${formatDmrId(it.number)})" })
                    if (contacts.none { it.index.toLong() == draft.contactIndex }) {
                        add(draft.contactIndex to "#${formatSlot(draft.contactIndex.toInt())} (missing)")
                    }
                }
                DropdownField(
                    "Contact", contactChoices,
                    contactChoices.first { it.first == draft.contactIndex }, { it.second },
                    { draft = draft.copy(contactIndex = it.first) },
                    enabled = isDigital && contacts.isNotEmpty(),
                )

                val groupChoices = buildList {
                    add(LIST_REFERENCE_NONE to "None")
                    addAll(groupLists.map { it.index to it.name })
                    if (draft.groupListIndex != LIST_REFERENCE_NONE &&
                        groupLists.none { it.index == draft.groupListIndex }
                    ) {
                        add(draft.groupListIndex to "#${formatSlot(draft.groupListIndex)} (missing)")
                    }
                }
                DropdownField(
                    "RX group list", groupChoices,
                    groupChoices.first { it.first == draft.groupListIndex }, { it.second },
                    { draft = draft.copy(groupListIndex = it.first) },
                    enabled = isDigital,
                )

                val radioChoices = buildList {
                    addAll(radioIds.map { it.index to "${it.name} (${formatDmrId(it.number)})" })
                    if (radioIds.none { it.index == draft.radioIdIndex }) {
                        add(draft.radioIdIndex to "#${formatSlot(draft.radioIdIndex)} (missing)")
                    }
                }
                DropdownField(
                    "Radio ID", radioChoices,
                    radioChoices.first { it.first == draft.radioIdIndex }, { it.second },
                    { draft = draft.copy(radioIdIndex = it.first) },
                    enabled = isDigital && radioIds.isNotEmpty(),
                )
            }

            EditorSection("Analog", enabled = hasAnalog) {
                DropdownField(
                    "RX signaling", signalingModeOptions,
                    taggedOrRaw(signalingModeOptions, draft.rxSignalingMode), { it.label },
                    { draft = draft.copy(rxSignalingMode = it.tag) }, enabled = hasAnalog,
                )
                DropdownField(
                    "TX signaling", signalingModeOptions,
                    taggedOrRaw(signalingModeOptions, draft.txSignalingMode), { it.label },
                    { draft = draft.copy(txSignalingMode = it.tag) }, enabled = hasAnalog,
                )
                NumberStepper(
                    "RX CTCSS index", draft.rxCtcss, { draft = draft.copy(rxCtcss = it) },
                    range = 0..255, enabled = hasAnalog,
                )
                NumberStepper(
                    "TX CTCSS index", draft.txCtcss, { draft = draft.copy(txCtcss = it) },
                    range = 0..255, enabled = hasAnalog,
                )
                NumberStepper(
                    "RX DCS code", draft.rxDcs, { draft = draft.copy(rxDcs = it) },
                    range = 0..65535, enabled = hasAnalog,
                )
                NumberStepper(
                    "TX DCS code", draft.txDcs, { draft = draft.copy(txDcs = it) },
                    range = 0..65535, enabled = hasAnalog,
                )
                DropdownField(
                    "Squelch mode", squelchModeOptions,
                    taggedOrRaw(squelchModeOptions, draft.squelchMode), { it.label },
                    { draft = draft.copy(squelchMode = it.tag) }, enabled = hasAnalog,
                )
                DropdownField(
                    "Optional signal", optionalSignalingOptions,
                    taggedOrRaw(optionalSignalingOptions, draft.optionalSignaling), { it.label },
                    { draft = draft.copy(optionalSignaling = it.tag) }, enabled = hasAnalog,
                )
                NumberStepper(
                    "DTMF ID index", draft.dtmfIdIndex, { draft = draft.copy(dtmfIdIndex = it) },
                    range = 0..255, enabled = hasAnalog,
                )
                NumberStepper(
                    "2-Tone ID index", draft.twoToneIdIndex,
                    { draft = draft.copy(twoToneIdIndex = it) }, range = 0..255, enabled = hasAnalog,
                )
                NumberStepper(
                    "5-Tone ID index", draft.fiveToneIdIndex,
                    { draft = draft.copy(fiveToneIdIndex = it) }, range = 0..255, enabled = hasAnalog,
                )
            }

            EditorSection("Flags") {
                SwitchField(
                    "PTT prohibit (RX only)", draft.rxOnly, { draft = draft.copy(rxOnly = it) },
                )
                SwitchField(
                    "Talk around (simplex)", draft.talkAround, { draft = draft.copy(talkAround = it) },
                )
                SwitchField("Work alone", draft.workAlone, { draft = draft.copy(workAlone = it) })
                SwitchField(
                    "Call confirmation", draft.callConfirm, { draft = draft.copy(callConfirm = it) },
                )
                SwitchField("APRS RX", draft.rxAprs, { draft = draft.copy(rxAprs = it) })
                SwitchField(
                    "Simplex TDMA", draft.simplexTdma, { draft = draft.copy(simplexTdma = it) },
                )
            }
        }
    }
}

/**
 * The option matching [tag] in [options], or a stand-in built from the tag
 * itself when the core lists no such option (e.g. capabilities not loaded
 * yet). Keeps every dropdown from crashing on a value it doesn't recognize.
 */
private fun taggedOrRaw(options: List<PickerOption>, tag: String): PickerOption =
    options.firstOrNull { it.tag == tag } ?: PickerOption(tag, tag)
