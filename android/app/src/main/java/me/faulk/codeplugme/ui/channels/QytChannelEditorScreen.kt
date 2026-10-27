package me.faulk.codeplugme.ui.channels

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
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
import me.faulk.codeplugme.model.DumpQytChannel
import me.faulk.codeplugme.model.PickerOption
import me.faulk.codeplugme.ui.common.formatSlot
import me.faulk.codeplugme.ui.config.ToneField

/**
 * Editor for one QYT KT-5000 channel: name, frequencies, tones, and scan-skip,
 * plus a collapsed read-only disclosure for everything else the core decodes.
 *
 * Ported from `QytChannelEditorSheet` (macOS `QytEditorSheets.swift`). The
 * writable fields are exactly what the core can write back: name,
 * `rx_frequency_hz`, `tx_frequency_hz`, `rx_tone`, `tx_tone`, and `scan_add`.
 * Power, bandwidth, busy lockout, PTT-ID, scrambler, optional signalling and
 * speaker mute are decoded for DISPLAY ONLY, because only the scan-add bit was
 * confirmed against real hardware; offering a live control for the rest would
 * silently discard the user's change on the next reload.
 *
 * Clearing the TX frequency (the "Receive only" toggle) is a real third state,
 * distinct from leaving TX alone: it flows through `txFrequencyHz == null`.
 * Turning TX back on seeds it from RX, the simplex case and the sane default.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun QytChannelEditorScreen(
    channel: DumpQytChannel,
    nameLimit: Int,
    powerOptions: List<PickerOption>,
    bandwidthOptions: List<PickerOption>,
    onSave: (DumpQytChannel) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var draft by remember(channel) { mutableStateOf(channel) }
    var flagsExpanded by remember(channel) { mutableStateOf(false) }
    val receiveOnly = draft.txFrequencyHz == null

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
                // Truncated as the user types, not at Save: the radio stores
                // [nameLimit] characters, and silently losing the tail on save
                // would be a surprise.
                onValueChange = { draft = draft.copy(name = limitedName(it, nameLimit)) },
                label = { Text("Name") },
                supportingText = { Text("Up to $nameLimit characters.") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )

            EditorSection("Frequency") {
                FrequencyField(
                    "Receive (MHz)", draft.rxFrequencyHz,
                    { draft = draft.copy(rxFrequencyHz = it) },
                )
                FrequencyField(
                    "Transmit (MHz)", draft.txFrequencyHz ?: draft.rxFrequencyHz,
                    { draft = draft.copy(txFrequencyHz = it) },
                    enabled = !receiveOnly,
                )
                SwitchField(
                    "Receive only (no transmit)",
                    checked = receiveOnly,
                    onCheckedChange = { on ->
                        draft = draft.copy(txFrequencyHz = if (on) null else draft.rxFrequencyHz)
                    },
                )
            }

            EditorSection("Analog") {
                ToneField(
                    label = "RX tone",
                    tone = draft.rxTone,
                    onToneChange = { draft = draft.copy(rxTone = it) },
                    modifier = Modifier.fillMaxWidth(),
                )
                ToneField(
                    label = "TX tone",
                    tone = draft.txTone,
                    onToneChange = { draft = draft.copy(txTone = it) },
                    modifier = Modifier.fillMaxWidth(),
                )
            }

            EditorSection("Scan") {
                // Phrased as the radio's own menu does, so "on" is the
                // exceptional state the user went looking for. The stored bit
                // is the opposite sense (`scan_add`), inverted here rather
                // than in the model, which follows the core.
                SwitchField(
                    "Skip when scanning",
                    checked = !draft.scanAdd,
                    onCheckedChange = { draft = draft.copy(scanAdd = !it) },
                )
                Text(
                    "A skipped channel stays programmed but the scanner passes over it.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }

            EditorSection("Radio flags (read-only)") {
                Row(
                    Modifier
                        .fillMaxWidth()
                        .clickable { flagsExpanded = !flagsExpanded },
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text("Show details")
                    Icon(
                        if (flagsExpanded) Icons.Default.ExpandLess else Icons.Default.ExpandMore,
                        contentDescription = if (flagsExpanded) "Collapse" else "Expand",
                    )
                }
                if (flagsExpanded) {
                    ReadOnlyRow("Power", pickerLabel(draft.power, powerOptions))
                    ReadOnlyRow("Bandwidth", pickerLabel(draft.bandwidth, bandwidthOptions))
                    ReadOnlyRow("Busy channel lockout", yesNo(draft.busyLockout))
                    ReadOnlyRow("Scrambler", yesNo(draft.scramble))
                    ReadOnlyRow("PTT-ID", draft.pttId)
                    ReadOnlyRow("Optional signalling", draft.optionalSignaling)
                    ReadOnlyRow("Speaker mute", draft.speakerMute)
                    ReadOnlyRow("Signal code", draft.scode.toString())
                    ReadOnlyRow("Flag bytes", flagsRawHex(draft.flagsRaw))
                    Text(
                        "These come from the record's flag bytes, which round-trip " +
                            "untouched. They stay read-only until their bit assignments " +
                            "are confirmed against the radio, which scan already is.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}

/** One read-only label/value row in the flags disclosure. */
@Composable
private fun ReadOnlyRow(label: String, value: String) {
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(label, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(value)
    }
}
