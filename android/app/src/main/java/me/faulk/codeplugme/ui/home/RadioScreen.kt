package me.faulk.codeplugme.ui.home

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Upload
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import me.faulk.codeplugme.library.LibraryEntry
import me.faulk.codeplugme.ui.common.formatSize
import me.faulk.codeplugme.state.CodeplugState
import me.faulk.codeplugme.state.RadioScreenState
import me.faulk.codeplugme.usb.RadioDevice
import me.faulk.codeplugme.usb.writeRefusalReason

/**
 * The launch screen, and the whole of the primary use case.
 *
 * Everything needed to get a codeplug onto a radio is here and nowhere else:
 * what is connected, what codeplugs are on the phone, and the two buttons that
 * move bytes between them. Editing is a separate destination precisely so it
 * does not stand between the user and this.
 *
 * Codeplugs live in app storage rather than being picked from the file system
 * each time. The macOS app is single-document because a laptop makes that cheap;
 * on a phone it would put a file-picker round trip in front of every write.
 */
@Composable
fun RadioScreen(
    state: RadioScreenState,
    codeplug: CodeplugState,
    onSave: () -> Unit,
    onSaveAs: () -> Unit,
    onDiscard: () -> Unit,
    onSelectDevice: (String) -> Unit,
    onChooseModel: (deviceKey: String, modelId: String) -> Unit,
    onRefresh: () -> Unit,
    onRead: () -> Unit,
    onWrite: (entry: LibraryEntry) -> Unit,
    onOpen: (LibraryEntry) -> Unit,
    onImport: () -> Unit,
    onExport: (LibraryEntry) -> Unit,
    onRename: (LibraryEntry) -> Unit,
    onDuplicate: (LibraryEntry) -> Unit,
    onDelete: (LibraryEntry) -> Unit,
    modifier: Modifier = Modifier,
) {
    LazyColumn(modifier.fillMaxWidth(), contentPadding = androidx.compose.foundation.layout.PaddingValues(16.dp)) {
        item {
            ConnectedRadioCard(
                state = state,
                onSelectDevice = onSelectDevice,
                onChooseModel = onChooseModel,
                onRefresh = onRefresh,
                onRead = onRead,
            )
            Spacer(Modifier.height(24.dp))
        }

        if (codeplug.isOpen) {
            item {
                OpenCodeplugCard(
                    codeplug = codeplug,
                    onSave = onSave,
                    onSaveAs = onSaveAs,
                    onDiscard = onDiscard,
                )
                Spacer(Modifier.height(24.dp))
            }
        }

        item {
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("Codeplugs", style = MaterialTheme.typography.titleMedium)
                OutlinedButton(onClick = onImport) { Text("Import") }
            }
            Spacer(Modifier.height(8.dp))
        }

        if (state.library.isEmpty()) {
            item {
                Text(
                    "No codeplugs yet. Read one from your radio, or import a .bin file.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(vertical = 24.dp),
                )
            }
        }

        items(state.library, key = { it.id }) { entry ->
            LibraryRow(
                entry = entry,
                state = state,
                onWrite = { onWrite(entry) },
                onOpen = { onOpen(entry) },
                onExport = { onExport(entry) },
                onRename = { onRename(entry) },
                onDuplicate = { onDuplicate(entry) },
                onDelete = { onDelete(entry) },
            )
            HorizontalDivider()
        }
    }
}

/**
 * The codeplug currently open in the editor, and the three things you can do
 * with the edits staged against it.
 *
 * Saving is a first-class action here rather than only implied by writing to
 * the radio. Writing sends the staged bytes without saving them, which is the
 * right default for "tweak a channel, try it on the air", but it left no way at
 * all to keep a change without a radio attached.
 *
 * Save As is what makes an experiment safe: it files the staged bytes as a new
 * codeplug and leaves the one it started from untouched.
 */
@Composable
private fun OpenCodeplugCard(
    codeplug: CodeplugState,
    onSave: () -> Unit,
    onSaveAs: () -> Unit,
    onDiscard: () -> Unit,
) {
    Card(Modifier.fillMaxWidth(), elevation = CardDefaults.cardElevation(2.dp)) {
        Column(Modifier.padding(16.dp)) {
            Text("Editing", style = MaterialTheme.typography.titleMedium)
            Spacer(Modifier.height(8.dp))
            Text(
                codeplug.entryName,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = FontWeight.Medium,
            )
            Text(
                if (codeplug.isDirty) "Unsaved changes" else "No unsaved changes",
                style = MaterialTheme.typography.bodySmall,
                color = if (codeplug.isDirty) MaterialTheme.colorScheme.primary
                else MaterialTheme.colorScheme.onSurfaceVariant,
            )

            Spacer(Modifier.height(12.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = onSave, enabled = codeplug.isDirty) { Text("Save") }
                OutlinedButton(onClick = onSaveAs) { Text("Save As…") }
            }
            Spacer(Modifier.height(4.dp))
            TextButton(onClick = onDiscard, enabled = codeplug.isDirty) {
                Text("Discard changes")
            }
        }
    }
}

/** What is plugged in, what it is, and the read action. */
@Composable
private fun ConnectedRadioCard(
    state: RadioScreenState,
    onSelectDevice: (String) -> Unit,
    onChooseModel: (String, String) -> Unit,
    onRefresh: () -> Unit,
    onRead: () -> Unit,
) {
    Card(Modifier.fillMaxWidth(), elevation = CardDefaults.cardElevation(2.dp)) {
        Column(Modifier.padding(16.dp)) {
            Row(
                Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("Radio", style = MaterialTheme.typography.titleMedium)
                IconButton(onClick = onRefresh) {
                    Icon(Icons.Default.Refresh, contentDescription = "Rescan USB")
                }
            }

            val device = state.selectedDevice
            if (device == null) {
                Text(
                    "No radio connected.",
                    style = MaterialTheme.typography.bodyLarge,
                    fontWeight = FontWeight.Medium,
                )
                Text(
                    "Connect the programming cable through a USB OTG adapter.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(top = 4.dp),
                )
                // Only worth saying when the list is empty, and worth hedging:
                // some phones run OTG fine without advertising the feature, so
                // this explains a likely cause rather than declaring defeat.
                if (!state.reportsUsbHostSupport) {
                    Text(
                        "This phone does not report USB host support. It may still " +
                            "work, but if nothing ever appears here, that is probably why.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error,
                        modifier = Modifier.padding(top = 8.dp),
                    )
                }
                return@Column
            }

            // More than one candidate is unusual, so the picker only appears
            // when there is actually a choice to make.
            if (state.devices.size > 1) {
                Spacer(Modifier.height(8.dp))
                DevicePicker(state.devices, device, onSelectDevice)
            }

            Spacer(Modifier.height(8.dp))
            Text(device.label, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.Medium)
            Text(
                String.format("USB %04x:%04x", device.vendorId, device.productId),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            if (state.needsModelChoice) {
                ModelChooser(state, device, onChooseModel)
            } else {
                Spacer(Modifier.height(8.dp))
                AssistChip(onClick = {}, label = { Text(state.displayName(state.selectedModelId)) })
            }

            Spacer(Modifier.height(16.dp))
            OutlinedButton(
                onClick = onRead,
                enabled = state.selectedModelId != null,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Icon(Icons.Default.Download, contentDescription = null)
                Spacer(Modifier.height(0.dp))
                Text("  Read from Radio")
            }
        }
    }
}

/**
 * Asks which radio is behind an adapter the USB descriptors cannot identify.
 *
 * This is not an edge case for the KT-5000, it is the normal path: its cable is
 * a stock Prolific PL2303 shared with thousands of unrelated adapters, so it can
 * never be attributed automatically and the user always names it.
 */
@Composable
private fun ModelChooser(
    state: RadioScreenState,
    device: RadioDevice,
    onChooseModel: (String, String) -> Unit,
) {
    Spacer(Modifier.height(12.dp))
    Text(
        "This adapter doesn't say which radio is behind it. Choose one:",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    Spacer(Modifier.height(8.dp))
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        state.models.forEach { m ->
            OutlinedButton(onClick = { onChooseModel(device.key, m.id) }) { Text(m.displayName) }
        }
    }
}

/** Choose between several attached candidates. */
@Composable
private fun DevicePicker(
    devices: List<RadioDevice>,
    selected: RadioDevice,
    onSelect: (String) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    Row(verticalAlignment = Alignment.CenterVertically) {
        OutlinedButton(onClick = { open = true }) { Text(selected.label) }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            devices.forEach { d ->
                DropdownMenuItem(
                    text = { Text(d.label) },
                    onClick = { onSelect(d.key); open = false },
                )
            }
        }
    }
}

/**
 * One stored codeplug, with Write as its primary action.
 *
 * A codeplug that cannot go to the connected radio shows a disabled button and
 * the reason, rather than being offered and then rejected: a destructive action
 * the app already knows will fail should not be one tap from a confirmation.
 */
@Composable
private fun LibraryRow(
    entry: LibraryEntry,
    state: RadioScreenState,
    onWrite: () -> Unit,
    onOpen: () -> Unit,
    onExport: () -> Unit,
    onRename: () -> Unit,
    onDuplicate: () -> Unit,
    onDelete: () -> Unit,
) {
    var menuOpen by remember { mutableStateOf(false) }
    val device = state.selectedDevice
    // Written out rather than with an elvis: `device?.let { refusal } ?: msg`
    // reads naturally but is wrong, because a device that CAN be written to
    // makes the let return null and falls through to the no-radio message,
    // leaving the Write button disabled forever.
    val refusal: String? =
        if (device == null) "No radio connected."
        else writeRefusalReason(device, entry.modelId, state.manual, state.models)

    ListItem(
        headlineContent = { Text(entry.name) },
        supportingContent = {
            Column {
                Text(
                    "${state.displayName(entry.modelId)} - ${formatSize(entry.sizeBytes)}",
                    style = MaterialTheme.typography.bodySmall,
                )
                if (refusal != null) {
                    Text(
                        refusal,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        },
        trailingContent = {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Button(
                    onClick = onWrite,
                    enabled = refusal == null,
                    colors = ButtonDefaults.buttonColors(
                        containerColor = MaterialTheme.colorScheme.error,
                        contentColor = MaterialTheme.colorScheme.onError,
                    ),
                ) {
                    Icon(Icons.Default.Upload, contentDescription = null)
                    Text(" Write")
                }
                IconButton(onClick = { menuOpen = true }) {
                    Icon(Icons.Default.MoreVert, contentDescription = "More actions")
                }
                DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                    DropdownMenuItem(text = { Text("Edit") }, onClick = { menuOpen = false; onOpen() })
                    DropdownMenuItem(text = { Text("Rename") }, onClick = { menuOpen = false; onRename() })
                    DropdownMenuItem(text = { Text("Duplicate") }, onClick = { menuOpen = false; onDuplicate() })
                    DropdownMenuItem(text = { Text("Export") }, onClick = { menuOpen = false; onExport() })
                    DropdownMenuItem(text = { Text("Delete") }, onClick = { menuOpen = false; onDelete() })
                }
            }
        },
        modifier = Modifier.clickable(onClick = onOpen),
    )
}
