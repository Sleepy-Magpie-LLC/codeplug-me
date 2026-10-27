package me.faulk.codeplugme.ui.records

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
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import me.faulk.codeplugme.model.DumpContact
import me.faulk.codeplugme.model.DumpRadioId
import me.faulk.codeplugme.model.DumpScanList
import me.faulk.codeplugme.model.formatDmrId
import me.faulk.codeplugme.ui.common.NumberStepper

/**
 * Editors for the record families behind "More".
 *
 * Zones and group lists have no editor of their own: a zone is a name plus a
 * membership list, and membership is edited on its own screen because two
 * side-by-side transfer columns do not fit a phone. Their name is edited here
 * through [SimpleNameEditorScreen] and their members through the member
 * screens.
 *
 * Every editor keeps a local draft and commits on Save. A live-bound field would
 * stage a Rust round trip and an undo entry per keystroke.
 */

/** Chrome shared by every editor here: a back arrow, a title, and Save. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun EditorScaffold(
    title: String,
    onSave: () -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text(title) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = { Button(onClick = onSave) { Text("Save") } },
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
            content = { content() },
        )
    }
}

/**
 * Edits a record that is just a name plus a membership list.
 *
 * Used for zones and group lists. The membership button leads to the member
 * screens rather than embedding a picker, because the candidate list can run to
 * thousands of channels or contacts.
 */
@Composable
fun SimpleNameEditorScreen(
    title: String,
    name: String,
    memberLabel: String,
    memberCount: Int,
    onSave: (String) -> Unit,
    onEditMembers: () -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
    nameMaxLength: Int? = null,
) {
    var draft by remember(name) { mutableStateOf(name) }
    EditorScaffold(title, onSave = { onSave(draft) }, onBack = onBack, modifier = modifier) {
        OutlinedTextField(
            value = draft,
            onValueChange = { draft = if (nameMaxLength != null) it.take(nameMaxLength) else it },
            label = { Text("Name") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedButton(onClick = onEditMembers, modifier = Modifier.fillMaxWidth()) {
            Text("$memberLabel ($memberCount)")
        }
    }
}

/** Edits a DMR contact: name, ID, and whether it is a private, group, or all call. */
@Composable
fun ContactEditorScreen(
    contact: DumpContact,
    onSave: (DumpContact) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var draft by remember(contact) { mutableStateOf(contact) }
    var numberText by remember(contact) { mutableStateOf(formatDmrId(contact.number)) }
    var typeMenu by remember { mutableStateOf(false) }

    EditorScaffold(
        "Contact ${contact.index + 1}",
        onSave = { onSave(draft) },
        onBack = onBack,
        modifier = modifier,
    ) {
        OutlinedTextField(
            value = draft.name,
            onValueChange = { draft = draft.copy(name = it) },
            label = { Text("Name") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(
            value = numberText,
            onValueChange = { text ->
                numberText = text.filter { it.isDigit() }
                numberText.toLongOrNull()?.let { draft = draft.copy(number = it) }
            },
            label = { Text("DMR ID") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedButton(onClick = { typeMenu = true }, modifier = Modifier.fillMaxWidth()) {
            Text("Call type: ${draft.callType}")
        }
        DropdownMenu(expanded = typeMenu, onDismissRequest = { typeMenu = false }) {
            // Dump-valued tags; the edit layer converts them for the core.
            listOf("Private", "Group", "All").forEach { t ->
                DropdownMenuItem(
                    text = { Text(t) },
                    onClick = { draft = draft.copy(callType = t); typeMenu = false },
                )
            }
        }
    }
}

/** Edits a radio ID: the DMR identity this radio transmits under. */
@Composable
fun RadioIdEditorScreen(
    radioId: DumpRadioId,
    onSave: (DumpRadioId) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var draft by remember(radioId) { mutableStateOf(radioId) }
    var numberText by remember(radioId) { mutableStateOf(formatDmrId(radioId.number)) }

    EditorScaffold(
        "Radio ID ${radioId.index + 1}",
        onSave = { onSave(draft) },
        onBack = onBack,
        modifier = modifier,
    ) {
        OutlinedTextField(
            value = draft.name,
            onValueChange = { draft = draft.copy(name = it) },
            label = { Text("Name") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(
            value = numberText,
            onValueChange = { text ->
                numberText = text.filter { it.isDigit() }
                numberText.toLongOrNull()?.let { draft = draft.copy(number = it) }
            },
            label = { Text("DMR ID") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

/**
 * Edits a scan list.
 *
 * The priority, look-back, dwell and dropout values are raw radio units rather
 * than seconds, exactly as the macOS pane presents them: the core models the
 * bytes, and inventing a unit conversion the vendor CPS does not agree with
 * would be worse than showing the number the radio holds.
 */
@Composable
fun ScanListEditorScreen(
    scanList: DumpScanList,
    onSave: (DumpScanList) -> Unit,
    onEditMembers: () -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var draft by remember(scanList) { mutableStateOf(scanList) }

    EditorScaffold(
        "Scan list ${scanList.index + 1}",
        onSave = { onSave(draft) },
        onBack = onBack,
        modifier = modifier,
    ) {
        OutlinedTextField(
            value = draft.name,
            onValueChange = { draft = draft.copy(name = it) },
            label = { Text("Name") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedButton(onClick = onEditMembers, modifier = Modifier.fillMaxWidth()) {
            Text("Channels (${draft.members.size})")
        }

        Text("Priority", style = MaterialTheme.typography.titleSmall)
        NumberStepper(
            "Priority select",
            draft.priorityChannelSelect,
            { draft = draft.copy(priorityChannelSelect = it) },
            range = 0..3,
        )
        NumberStepper(
            "Priority channel 1",
            draft.priorityChannel1,
            { draft = draft.copy(priorityChannel1 = it) },
            range = 0..65535,
            supporting = "65535 means none",
        )
        NumberStepper(
            "Priority channel 2",
            draft.priorityChannel2,
            { draft = draft.copy(priorityChannel2 = it) },
            range = 0..65535,
            supporting = "65535 means none",
        )
        NumberStepper(
            "Revert channel",
            draft.revertChannel,
            { draft = draft.copy(revertChannel = it) },
            range = 0..65535,
        )

        Text("Timing", style = MaterialTheme.typography.titleSmall)
        NumberStepper("Look back A", draft.lookBackA, { draft = draft.copy(lookBackA = it) }, range = 0..255)
        NumberStepper("Look back B", draft.lookBackB, { draft = draft.copy(lookBackB = it) }, range = 0..255)
        NumberStepper("Dropout delay", draft.dropoutDelay, { draft = draft.copy(dropoutDelay = it) }, range = 0..255)
        NumberStepper("Dwell time", draft.dwellTime, { draft = draft.copy(dwellTime = it) }, range = 0..255)
    }
}
