package me.faulk.codeplugme.ui

import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreHoriz
import androidx.compose.material.icons.filled.Radio
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.automirrored.filled.List
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.launch
import me.faulk.codeplugme.library.LibraryEntry
import me.faulk.codeplugme.state.AppViewModel
import me.faulk.codeplugme.state.CodeplugSection
import me.faulk.codeplugme.ui.home.RadioScreen

/**
 * The app shell: the current destination, the transfer overlay, and every
 * app-level dialog.
 *
 * Navigation is a plain back stack of [Destination] values rather than
 * `NavHost` routes. Half of what this app navigates to is addressed by a record
 * slot and a section, which would mean encoding and re-parsing them as route
 * strings for no gain, and the whole app is one activity with no deep links.
 */
@Composable
fun CodeplugApp(vm: AppViewModel) {
    val radio by vm.radio.collectAsStateWithLifecycle()
    val codeplug by vm.store.state.collectAsStateWithLifecycle()
    val transfer by vm.transfer.collectAsStateWithLifecycle()
    val recovery by vm.pendingRecovery.collectAsStateWithLifecycle()
    val pendingWrite by vm.pendingWrite.collectAsStateWithLifecycle()
    val snackbar = remember { SnackbarHostState() }
    val scope = rememberCoroutineScope()

    var tab by remember { mutableStateOf<Destination>(Destination.Radio) }
    val stack = remember { mutableStateListOf<Destination>() }
    val current = stack.lastOrNull() ?: tab

    var selection by remember { mutableStateOf(emptySet<Int>()) }
    var renaming by remember { mutableStateOf<LibraryEntry?>(null) }
    var deleting by remember { mutableStateOf<LibraryEntry?>(null) }
    var exporting by remember { mutableStateOf<LibraryEntry?>(null) }
    var savingAs by remember { mutableStateOf(false) }
    var discarding by remember { mutableStateOf(false) }

    fun push(d: Destination) { stack.add(d) }
    fun pop() { if (stack.isNotEmpty()) stack.removeAt(stack.lastIndex) }

    BackHandler(enabled = stack.isNotEmpty()) { pop() }

    val importPicker = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument()
    ) { uri -> uri?.let(vm::import) }

    val exportPicker = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument("application/octet-stream")
    ) { uri ->
        val entry = exporting
        exporting = null
        if (uri != null && entry != null) vm.export(entry.id, uri)
    }

    // Errors are informational and should not sit on top of the flow the user
    // was in, so they are a snackbar rather than a dialog.
    LaunchedEffect(radio.error) {
        radio.error?.let { snackbar.showSnackbar(it); vm.clearError() }
    }
    LaunchedEffect(codeplug.error) {
        codeplug.error?.let { snackbar.showSnackbar(it); vm.store.clearError() }
    }
    LaunchedEffect(codeplug.status) {
        codeplug.status?.let { snackbar.showSnackbar(it); vm.store.clearStatus() }
    }

    Scaffold(
        snackbarHost = { SnackbarHost(snackbar) },
        bottomBar = {
            // The bottom bar only makes sense at the top of the stack; a record
            // editor is a modal step, not a place to switch tabs from.
            if (stack.isEmpty()) {
                NavigationBar {
                    topLevelDestinations.forEach { d ->
                        NavigationBarItem(
                            selected = tab == d,
                            onClick = { tab = d; selection = emptySet() },
                            icon = { Icon(iconFor(d), contentDescription = null) },
                            label = { Text(d.label()) },
                        )
                    }
                }
            }
        },
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding)) {
            AppContent(
                vm = vm,
                current = current,
                radio = radio,
                codeplug = codeplug,
                selection = selection,
                onSelectionChange = { selection = it },
                push = ::push,
                pop = ::pop,
                onWriteRequest = { vm.prepareWrite(it) },
                onRenameRequest = { renaming = it },
                onDeleteRequest = { deleting = it },
                onExportRequest = { exporting = it; exportPicker.launch("${it.name}.bin") },
                onImportRequest = { importPicker.launch(arrayOf("*/*")) },
                onSaveAsRequest = { savingAs = true },
                onDiscardChangesRequest = { discarding = true },
                scope = scope,
            )

            TransferOverlay(
                state = transfer,
                onAcknowledge = vm::acknowledgeTransfer,
                onAcceptRead = { vm.acceptRead(it) },
            )
        }
    }

    // Writing replaces the radio's entire configuration and cannot be undone
    // from here, so it always costs a deliberate confirmation even though it is
    // the app's primary action.
    pendingWrite?.let { pending ->
        AlertDialog(
            onDismissRequest = vm::cancelWrite,
            title = { Text("Write ${pending.entry.name} to the radio?") },
            text = {
                Column(
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                    modifier = Modifier.verticalScroll(rememberScrollState()),
                ) {
                    Text(
                        "This replaces everything on ${pending.deviceLabel}: all channels, " +
                            "zones and settings. It cannot be undone. Read a backup off the " +
                            "radio first if you do not have one."
                    )
                    // Where the bytes came from decides what happens to the
                    // regions this app does not model: they are sent as the
                    // image has them, whatever that is.
                    Text(pending.originLine, style = MaterialTheme.typography.bodySmall)
                    if (pending.hasUnsavedEdits) {
                        Text(
                            "Your unsaved edits are included.",
                            style = MaterialTheme.typography.bodySmall,
                        )
                    }
                    if (pending.erasedRegions.isNotEmpty()) {
                        Text(
                            "This codeplug holds nothing in " +
                                pending.erasedRegions.joinToString(" and ") +
                                ", so writing it erases that on the radio.",
                            color = MaterialTheme.colorScheme.error,
                        )
                    }
                }
            },
            confirmButton = {
                // Only the entry id crosses into the write. Reading up to
                // 1.79 MB and deciding between the staged and stored bytes both
                // happen off the main thread inside the view model.
                TextButton(onClick = vm::confirmWrite) { Text("Write") }
            },
            dismissButton = { TextButton(onClick = vm::cancelWrite) { Text("Cancel") } },
        )
    }

    // Save As names a new library entry from the staged bytes, leaving the
    // codeplug it was opened from exactly as it is on disk.
    if (savingAs) {
        NameDialog(
            title = "Save as new codeplug",
            confirmLabel = "Save",
            initial = "${codeplug.entryName} copy",
            onConfirm = { vm.saveOpenAs(it); savingAs = false },
            onDismiss = { savingAs = false },
        )
    }

    // Discarding throws away work with no undo left behind it, so it is
    // confirmed even though the button that reaches it already says so.
    if (discarding) {
        AlertDialog(
            onDismissRequest = { discarding = false },
            title = { Text("Discard changes to ${codeplug.entryName}?") },
            text = { Text("Every edit you have not saved is lost. This cannot be undone.") },
            confirmButton = {
                TextButton(onClick = { vm.discardOpenChanges(); discarding = false }) {
                    Text("Discard")
                }
            },
            dismissButton = { TextButton(onClick = { discarding = false }) { Text("Cancel") } },
        )
    }

    renaming?.let { entry ->
        NameDialog(
            title = "Rename codeplug",
            confirmLabel = "Rename",
            initial = entry.name,
            onConfirm = { vm.rename(entry.id, it); renaming = null },
            onDismiss = { renaming = null },
        )
    }

    deleting?.let { entry ->
        AlertDialog(
            onDismissRequest = { deleting = null },
            title = { Text("Delete ${entry.name}?") },
            text = { Text("This removes it from this phone. The radio is not affected.") },
            confirmButton = {
                TextButton(onClick = { vm.delete(entry.id); deleting = null }) { Text("Delete") }
            },
            dismissButton = { TextButton(onClick = { deleting = null }) { Text("Cancel") } },
        )
    }

    recovery?.let { manifest ->
        AlertDialog(
            onDismissRequest = { vm.discardRecovery() },
            title = { Text("Unsaved changes found") },
            text = {
                Text(
                    "The app closed with unsaved edits to \"${manifest.entryName}\". Restore them?"
                )
            },
            confirmButton = {
                TextButton(onClick = { vm.restoreRecovery(manifest) }) { Text("Restore") }
            },
            dismissButton = { TextButton(onClick = { vm.discardRecovery() }) { Text("Discard") } },
        )
    }
}

/** The bottom-bar icon for a top-level destination. */
private fun iconFor(d: Destination) = when (d) {
    Destination.Radio -> Icons.Default.Radio
    Destination.Config -> Icons.Default.Settings
    Destination.Channels -> Icons.AutoMirrored.Filled.List
    else -> Icons.Default.MoreHoriz
}

/** Prompt for a library entry name, for both renaming and Save As. */
@Composable
private fun NameDialog(
    title: String,
    confirmLabel: String,
    initial: String,
    onConfirm: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var text by remember { mutableStateOf(initial) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            OutlinedTextField(
                value = text,
                onValueChange = { text = it },
                singleLine = true,
                label = { Text("Name") },
            )
        },
        confirmButton = {
            TextButton(onClick = { onConfirm(text) }, enabled = text.isNotBlank()) {
                Text(confirmLabel)
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
