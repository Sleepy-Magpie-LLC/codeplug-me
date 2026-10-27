package me.faulk.codeplugme.ui.members

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ArrowDownward
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import me.faulk.codeplugme.ui.common.formatSlot

/**
 * Edits a membership list (a zone's channels, a group list's contacts).
 *
 * This is the one place the macOS design could not be ported. There, membership
 * is a two-column transfer: everything available on the left, everything already
 * a member on the right, and buttons between them. Those columns are 760 points
 * wide and simply do not exist on a phone.
 *
 * So it is two screens instead. This one owns the members and their order, which
 * matters because a zone's channel order is the order the radio steps through
 * with the up and down keys. [MemberPickerScreen] is where things are added,
 * with search and multi-select, because the candidate side can run to thousands
 * of channels.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MemberListScreen(
    title: String,
    candidates: List<MemberCandidate>,
    members: List<Int>,
    onMembersChange: (List<Int>) -> Unit,
    onAdd: () -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val items = remember(candidates, members) { MemberTransfer.memberItems(candidates, members) }

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
            )
        },
        floatingActionButton = {
            FloatingActionButton(onClick = onAdd) {
                Icon(Icons.Default.Add, contentDescription = "Add members")
            }
        },
    ) { padding ->
        if (items.isEmpty()) {
            Column(Modifier.padding(padding).fillMaxSize().padding(32.dp)) {
                Text(
                    "Nothing in this list yet.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            return@Scaffold
        }

        LazyColumn(Modifier.padding(padding).fillMaxSize()) {
            items(items, key = { it.index }) { item ->
                val position = members.indexOf(item.index)
                ListItem(
                    overlineContent = { Text(formatSlot(item.index)) },
                    headlineContent = { Text(item.label) },
                    trailingContent = {
                        androidx.compose.foundation.layout.Row {
                            IconButton(
                                onClick = {
                                    onMembersChange(
                                        MemberTransfer.reordering(members, position, position - 1)
                                    )
                                },
                                enabled = position > 0,
                            ) { Icon(Icons.Default.ArrowUpward, contentDescription = "Move up") }
                            IconButton(
                                onClick = {
                                    onMembersChange(
                                        MemberTransfer.reordering(members, position, position + 1)
                                    )
                                },
                                enabled = position < members.lastIndex,
                            ) { Icon(Icons.Default.ArrowDownward, contentDescription = "Move down") }
                            IconButton(onClick = {
                                onMembersChange(
                                    MemberTransfer.removing(setOf(item.index), members)
                                )
                            }) { Icon(Icons.Default.Delete, contentDescription = "Remove") }
                        }
                    },
                )
                HorizontalDivider()
            }
        }
    }
}

/**
 * Picks records to add to a membership list.
 *
 * Already-members are filtered out rather than shown checked, which is what
 * stops a member being added twice. Search is not optional here: the AnyTone
 * holds up to 4000 channels and 10000 contacts.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MemberPickerScreen(
    title: String,
    candidates: List<MemberCandidate>,
    members: List<Int>,
    onConfirm: (List<Int>) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var query by remember { mutableStateOf("") }
    var selection by remember { mutableStateOf(emptySet<Int>()) }

    val available = remember(candidates, members) { MemberTransfer.available(candidates, members) }
    val shown = remember(available, query) {
        if (query.isBlank()) available
        else available.filter { it.label.contains(query, ignoreCase = true) }
    }

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text(title) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Cancel")
                    }
                },
                actions = {
                    TextButton(
                        onClick = {
                            onConfirm(MemberTransfer.adding(selection, candidates, members))
                        },
                        enabled = selection.isNotEmpty(),
                    ) { Text("Add ${selection.size}") }
                },
            )
        },
    ) { padding ->
        Column(Modifier.padding(padding).fillMaxSize()) {
            OutlinedTextField(
                value = query,
                onValueChange = { query = it },
                label = { Text("Search") },
                singleLine = true,
                leadingIcon = { Icon(Icons.Default.Search, contentDescription = null) },
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
            )

            if (shown.isEmpty()) {
                Text(
                    if (available.isEmpty()) "Nothing left to add."
                    else "Nothing matches \"$query\".",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(32.dp),
                )
                return@Column
            }

            LazyColumn(Modifier.fillMaxSize()) {
                items(shown, key = { it.index }) { item ->
                    val checked = item.index in selection
                    ListItem(
                        leadingContent = {
                            Checkbox(
                                checked = checked,
                                onCheckedChange = { on ->
                                    selection =
                                        if (on) selection + item.index else selection - item.index
                                },
                            )
                        },
                        overlineContent = { Text(formatSlot(item.index)) },
                        headlineContent = { Text(item.label) },
                        modifier = Modifier.clickable {
                            selection =
                                if (checked) selection - item.index else selection + item.index
                        },
                    )
                    HorizontalDivider()
                }
            }
        }
    }
}
