package me.faulk.codeplugme.ui.config

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import java.util.Locale
import me.faulk.codeplugme.model.Capabilities
import me.faulk.codeplugme.model.DumpQytBand
import me.faulk.codeplugme.model.formatMhz
import me.faulk.codeplugme.ui.common.EmptyState
import me.faulk.codeplugme.ui.common.parseMhz

/**
 * The QYT KT-5000's two VFO channels (A and B). Port of the macOS
 * `QytBandsPane`.
 *
 * Only frequency, RX/TX tone, bandwidth, power and tuning step are editable,
 * because those are the only VFO fields the core has setters for. The
 * repeater offset and shift direction are shown read-only: their byte
 * encoding is assumed from another model and has not been confirmed against
 * this radio, so writing them could put the transmit frequency somewhere
 * unintended.
 *
 * @param bands the radio's VFO records (normally two: A and B).
 * @param capabilities the model's picker option lists, or null before a
 *   codeplug is loaded.
 * @param onApply called with the edited draft (same size and indices as
 *   [bands]) when the user taps Apply.
 * @param onBack called when the user navigates back, or null when this screen
 *   is a bottom-bar tab's own content and has nowhere to go back to.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun QytBandsScreen(
    bands: List<DumpQytBand>,
    capabilities: Capabilities?,
    onApply: (List<DumpQytBand>) -> Unit,
    onBack: (() -> Unit)?,
    modifier: Modifier = Modifier,
) {
    var draft by remember { mutableStateOf(bands) }

    // Re-seed from the document when it changes underneath the screen, so an
    // undo of an applied change is reflected here.
    LaunchedEffect(bands) { draft = bands }

    val hasChanges = draft != bands

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text("VFO Channels") },
                // A tab root has an empty back stack, so a back arrow there
                // would be a control that visibly does nothing.
                navigationIcon = {
                    if (onBack != null) {
                        IconButton(onClick = onBack) {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                        }
                    }
                },
            )
        },
        bottomBar = {
            if (bands.isNotEmpty()) {
                ApplyRevertFooter(
                    hasChanges = hasChanges,
                    onRevert = { draft = bands },
                    onApply = { onApply(draft) },
                )
            }
        },
    ) { padding ->
        if (bands.isEmpty()) {
            EmptyState(
                title = "No VFO channels",
                detail = "This codeplug has no QYT VFO records.",
                modifier = Modifier.padding(padding).fillMaxSize(),
            )
            return@Scaffold
        }

        Column(
            Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            draft.forEachIndexed { i, vfo ->
                ConfigSection(bandHeader(vfo)) {
                    BandFields(
                        vfo = vfo,
                        capabilities = capabilities,
                        onChange = { updated -> draft = draft.toMutableList().also { it[i] = updated } },
                    )
                }
            }

            Text(
                "The repeater offset and shift direction are read-only: their byte encoding " +
                    "is assumed from another model and has not been confirmed against this " +
                    "radio, so writing them could put the transmit frequency somewhere " +
                    "unintended.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/** The editable and read-only fields for one VFO. */
@Composable
private fun BandFields(vfo: DumpQytBand, capabilities: Capabilities?, onChange: (DumpQytBand) -> Unit) {
    FrequencyField(
        label = "Frequency (MHz)",
        hz = vfo.frequencyHz,
        onValueChange = { onChange(vfo.copy(frequencyHz = it)) },
    )
    ToneField(
        label = "RX tone",
        tone = vfo.rxTone,
        onToneChange = { onChange(vfo.copy(rxTone = it)) },
    )
    ToneField(
        label = "TX tone",
        tone = vfo.txTone,
        onToneChange = { onChange(vfo.copy(txTone = it)) },
    )
    TagPicker(
        title = "Bandwidth",
        tag = vfo.bandwidth,
        options = capabilities?.bandwidthOptions.orEmpty(),
        onSelect = { onChange(vfo.copy(bandwidth = it)) },
    )
    TagPicker(
        title = "Power",
        tag = vfo.power,
        options = capabilities?.powerOptions.orEmpty(),
        onSelect = { onChange(vfo.copy(power = it)) },
    )
    TuningStepPicker(
        value = vfo.stepHz,
        onSelect = { onChange(vfo.copy(stepHz = it)) },
    )
    LabeledValue("Repeater offset", offsetLabel(vfo))
    LabeledValue("Shift", vfo.shift)
}

/** The VFO frequency in MHz. The core owns the digit-per-byte codec on this
 * radio; this field only ever moves the decoded hertz value across the FFI. */
@Composable
private fun FrequencyField(label: String, hz: Long?, onValueChange: (Long) -> Unit) {
    var text by remember(hz) { mutableStateOf(formatMhz(hz ?: 0L)) }
    OutlinedTextField(
        value = text,
        onValueChange = { input ->
            text = input
            parseMhz(input)?.let(onValueChange)
        },
        label = { Text(label) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
        modifier = Modifier.fillMaxWidth(),
    )
}

/**
 * The tuning steps this radio offers, in hertz. The core validates against
 * its own list and rejects anything else with a readable error, so a stale
 * entry here fails loudly instead of writing a bad step byte.
 */
private val tuningStepsHz = listOf(2_500L, 5_000L, 6_250L, 10_000L, 12_500L, 25_000L)

/**
 * The tuning-step picker. A step outside [tuningStepsHz] (or a missing one)
 * gets its own "Unknown" entry rather than being clamped, so opening the
 * screen never rewrites a step byte the core could not decode.
 */
@Composable
private fun TuningStepPicker(value: Long?, onSelect: (Long?) -> Unit) {
    val options: List<Long?> =
        if (value == null || value in tuningStepsHz) tuningStepsHz else listOf(value) + tuningStepsHz
    LabeledDropdown(
        label = "Tuning step",
        options = options,
        selected = value,
        optionLabel = { it?.let(::stepLabel) ?: "Unknown" },
        onSelect = onSelect,
    )
}

private fun stepLabel(hz: Long): String {
    val khz = hz / 1000.0
    return if (khz == Math.floor(khz)) "${khz.toLong()} kHz" else String.format(Locale.ROOT, "%.2f kHz", khz)
}

/** "VFO A", with an unprogrammed slot saying so rather than a bare 0 MHz. */
private fun bandHeader(vfo: DumpQytBand): String {
    val letters = listOf("A", "B")
    val name = "VFO " + (letters.getOrNull(vfo.index) ?: vfo.index.toString())
    return if (vfo.programmed) name else "$name (unprogrammed)"
}

private fun offsetLabel(vfo: DumpQytBand): String {
    val hz = vfo.offsetHz ?: return "—"
    return "${formatMhz(hz)} MHz"
}
