package me.faulk.codeplugme.ui.common

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Remove
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.text.KeyboardOptions

/**
 * A labelled integer field with decrement and increment buttons.
 *
 * SwiftUI's `Stepper` has no Material equivalent and the macOS editors use it
 * about fifteen times, so this exists once rather than being open-coded per
 * field. The text field stays editable because stepping from 0 to 200 one tap
 * at a time is not a real interaction.
 *
 * Out-of-range typing is clamped on commit rather than rejected as you type,
 * so a partially typed number is never eaten mid-keystroke.
 */
@Composable
fun NumberStepper(
    label: String,
    value: Int,
    onValueChange: (Int) -> Unit,
    modifier: Modifier = Modifier,
    range: IntRange = 0..Int.MAX_VALUE,
    enabled: Boolean = true,
    supporting: String? = null,
) {
    var text by remember(value) { mutableStateOf(value.toString()) }

    fun commit(raw: String) {
        val parsed = raw.trim().toIntOrNull() ?: return
        onValueChange(parsed.coerceIn(range.first, range.last))
    }

    Row(modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        OutlinedTextField(
            value = text,
            onValueChange = { text = it; commit(it) },
            label = { Text(label) },
            supportingText = supporting?.let { { Text(it) } },
            singleLine = true,
            enabled = enabled,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
            modifier = Modifier.weight(1f),
        )
        IconButton(
            onClick = { onValueChange((value - 1).coerceIn(range.first, range.last)) },
            enabled = enabled && value > range.first,
        ) { Icon(Icons.Default.Remove, contentDescription = "Decrease $label") }
        IconButton(
            onClick = { onValueChange((value + 1).coerceIn(range.first, range.last)) },
            enabled = enabled && value < range.last,
        ) { Icon(Icons.Default.Add, contentDescription = "Increase $label") }
    }
}

/**
 * A small filled dot marking a record or section with staged, unsaved changes.
 *
 * Mirrors the macOS row and sidebar indicators, and is the only cue that the
 * bytes on the phone differ from the stored codeplug.
 */
@Composable
fun UnsavedDot(modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.size(8.dp),
        shape = CircleShape,
        color = MaterialTheme.colorScheme.primary,
    ) {}
}

/** Centred placeholder for a list or screen with nothing in it yet. */
@Composable
fun EmptyState(
    title: String,
    detail: String? = null,
    modifier: Modifier = Modifier,
    action: (@Composable () -> Unit)? = null,
) {
    Column(
        modifier = modifier.fillMaxSize().padding(32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Text(title, style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
        if (detail != null) {
            Spacer(Modifier.width(8.dp))
            Text(
                detail,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(top = 8.dp),
            )
        }
        if (action != null) {
            Spacer(Modifier.width(16.dp))
            Column(Modifier.padding(top = 16.dp)) { action() }
        }
    }
}
