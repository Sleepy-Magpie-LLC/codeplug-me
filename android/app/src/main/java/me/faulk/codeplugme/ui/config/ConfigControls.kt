package me.faulk.codeplugme.ui.config

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import me.faulk.codeplugme.model.PickerOption

/**
 * Shared building blocks for the config screens.
 *
 * These port the small helper views the macOS QYT panes define locally
 * (`QytRawPicker`, `ApplyRevertFooter`, and the loose `LabeledContent` rows)
 * into composables the settings, bands and APRS screens all reuse.
 */

/**
 * A titled card grouping a handful of fields, standing in for a SwiftUI
 * `Form` `Section` (Compose has no direct equivalent).
 */
@Composable
fun ConfigSection(title: String, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Card(modifier.fillMaxWidth(), elevation = CardDefaults.cardElevation(1.dp)) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            content()
        }
    }
}

/** A read-only label/value row, for fields the pane only ever displays. */
@Composable
fun LabeledValue(label: String, value: String, modifier: Modifier = Modifier) {
    Row(modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
        Text(label, style = MaterialTheme.typography.bodyMedium)
        Text(
            value,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** A labelled on/off row, for a boolean field a SwiftUI `Toggle` would edit. */
@Composable
fun SwitchRow(label: String, checked: Boolean, onCheckedChange: (Boolean) -> Unit, modifier: Modifier = Modifier) {
    Row(modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(label, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
        Switch(checked = checked, onCheckedChange = onCheckedChange)
    }
}

/**
 * A button showing the current value that opens a dropdown of [options] on
 * tap, standing in for SwiftUI's `Picker`.
 *
 * This is the same `OutlinedButton` + `DropdownMenu` shape
 * `ContactEditorScreen`'s call-type picker uses in `ui/records`, rather than
 * Material 3's `ExposedDropdownMenuBox`: no experimental API, no anchor
 * plumbing, and it matches the rest of the app.
 */
@Composable
fun <T> LabeledDropdown(
    label: String,
    options: List<T>,
    selected: T,
    optionLabel: (T) -> String,
    onSelect: (T) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    var expanded by remember { mutableStateOf(false) }
    OutlinedButton(
        onClick = { expanded = true },
        enabled = enabled,
        modifier = modifier.fillMaxWidth(),
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

/**
 * A dropdown over a run of raw values, one label per index (port of the
 * macOS `QytRawPicker`).
 *
 * A stored [value] outside `labels.indices` gets its own "Unknown (n)" entry
 * rather than being clamped into range. An unprogrammed image reads `0xFF`
 * everywhere, and clamping would silently rewrite a byte the user never
 * touched.
 */
@Composable
fun RawPicker(
    title: String,
    value: Int,
    labels: List<String>,
    onValueChange: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    val options = if (value in labels.indices) labels.indices.toList() else labels.indices + value
    LabeledDropdown(
        label = title,
        options = options,
        selected = value,
        optionLabel = { if (it in labels.indices) labels[it] else "Unknown ($it)" },
        onSelect = onValueChange,
        modifier = modifier,
    )
}

/**
 * A dropdown over the core's tagged string options (power, bandwidth, and
 * the like), the tag-based counterpart to [RawPicker].
 *
 * A stored tag the [options] list doesn't recognise is offered as its own
 * "Unknown" entry rather than snapped to a known one, for the same reason
 * [RawPicker] does: silently rewriting the value would move the radio's
 * behaviour without the user asking for it.
 */
@Composable
fun TagPicker(
    title: String,
    tag: String,
    options: List<PickerOption>,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val known = options.map { it.tag }
    val tags = if (tag in known) known else known + tag
    LabeledDropdown(
        label = title,
        options = tags,
        selected = tag,
        optionLabel = { t -> options.firstOrNull { it.tag == t }?.label ?: "Unknown ($t)" },
        onSelect = onSelect,
        modifier = modifier,
        enabled = options.isNotEmpty(),
    )
}

/**
 * The Apply / Revert bar the QYT settings and bands screens share.
 *
 * These screens commit explicitly instead of live, because a live text field
 * would stage one core round trip and one undo entry per keystroke (see
 * `CodeplugOperations.updateQytSettings` / `updateQytBands`).
 */
@Composable
fun ApplyRevertFooter(
    hasChanges: Boolean,
    onRevert: () -> Unit,
    onApply: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Surface(modifier.fillMaxWidth(), tonalElevation = 3.dp) {
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (hasChanges) {
                Text(
                    "Unapplied changes",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.weight(1f),
                )
            } else {
                Spacer(Modifier.weight(1f))
            }
            TextButton(onClick = onRevert, enabled = hasChanges) { Text("Revert") }
            Spacer(Modifier.width(8.dp))
            Button(onClick = onApply, enabled = hasChanges) { Text("Apply") }
        }
    }
}
