package me.faulk.codeplugme.ui.records

import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import me.faulk.codeplugme.model.DumpContact
import me.faulk.codeplugme.model.DumpGroupList
import me.faulk.codeplugme.model.DumpRadioId
import me.faulk.codeplugme.model.DumpScanList
import me.faulk.codeplugme.model.DumpZone
import me.faulk.codeplugme.model.formatDmrId
import me.faulk.codeplugme.state.CodeplugSection
import me.faulk.codeplugme.state.CodeplugState
import me.faulk.codeplugme.ui.common.RecordListScreen
import me.faulk.codeplugme.ui.common.SortOption

/**
 * The list screens for the record families behind "More": zones, scan lists,
 * contacts, group lists, and radio IDs.
 *
 * All five are the same shape, so they share [RecordListScreen] and differ only
 * in what each row says and how it sorts. Keeping them here rather than in five
 * files makes it obvious that they are five instances of one pattern, which is
 * also why adding a sixth family is nearly free.
 */

/** A Delete action for the selection bar, shared by every list here. */
@Composable
private fun DeleteAction(selection: Set<Int>, onDelete: (Set<Int>) -> Unit) {
    IconButton(onClick = { onDelete(selection) }) {
        Icon(Icons.Default.Delete, contentDescription = "Delete selected")
    }
}

/** Zones: named groups of channels, in the order the radio steps through them. */
@Composable
fun ZonesScreen(
    state: CodeplugState,
    selection: Set<Int>,
    onSelectionChange: (Set<Int>) -> Unit,
    onOpen: (DumpZone) -> Unit,
    onAdd: () -> Unit,
    onDelete: (Set<Int>) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) = RecordListScreen(
    title = "Zones",
    items = state.zones,
    slotOf = { it.index },
    headline = { it.name },
    supporting = { "${it.channels.size} channels" },
    isUnsaved = { state.isUnsaved(CodeplugSection.ZONES, it.index) },
    searchMatch = { z, q -> z.name.contains(q, ignoreCase = true) },
    sorts = listOf(
        SortOption("Slot") { a, b -> a.index - b.index },
        SortOption("Name") { a, b -> a.name.compareTo(b.name, ignoreCase = true) },
    ),
    selection = selection,
    onSelectionChange = onSelectionChange,
    onOpen = onOpen,
    onAdd = onAdd,
    onBack = onBack,
    emptyMessage = "No zones yet.",
    selectionActions = { DeleteAction(it, onDelete) },
    modifier = modifier,
)

/** Scan lists: which channels the radio sweeps, and how it prioritises them. */
@Composable
fun ScanListsScreen(
    state: CodeplugState,
    selection: Set<Int>,
    onSelectionChange: (Set<Int>) -> Unit,
    onOpen: (DumpScanList) -> Unit,
    onAdd: () -> Unit,
    onDelete: (Set<Int>) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) = RecordListScreen(
    title = "Scan Lists",
    items = state.scanLists,
    slotOf = { it.index },
    headline = { it.name },
    supporting = { "${it.members.size} channels" },
    isUnsaved = { state.isUnsaved(CodeplugSection.SCAN_LISTS, it.index) },
    searchMatch = { s, q -> s.name.contains(q, ignoreCase = true) },
    sorts = listOf(
        SortOption("Slot") { a, b -> a.index - b.index },
        SortOption("Name") { a, b -> a.name.compareTo(b.name, ignoreCase = true) },
    ),
    selection = selection,
    onSelectionChange = onSelectionChange,
    onOpen = onOpen,
    onAdd = onAdd,
    onBack = onBack,
    emptyMessage = "No scan lists yet.",
    selectionActions = { DeleteAction(it, onDelete) },
    modifier = modifier,
)

/**
 * Contacts, which on a DMR radio are talk groups as often as they are people.
 *
 * Search matches the DMR ID as well as the name, because a talk group is far
 * more often remembered by its number.
 */
@Composable
fun ContactsScreen(
    state: CodeplugState,
    selection: Set<Int>,
    onSelectionChange: (Set<Int>) -> Unit,
    onOpen: (DumpContact) -> Unit,
    onAdd: () -> Unit,
    onDelete: (Set<Int>) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) = RecordListScreen(
    title = "Contacts",
    items = state.contacts,
    slotOf = { it.index },
    headline = { it.name },
    supporting = { "${it.callType} - ${formatDmrId(it.number)}" },
    isUnsaved = { state.isUnsaved(CodeplugSection.CONTACTS, it.index) },
    searchMatch = { c, q ->
        c.name.contains(q, ignoreCase = true) || formatDmrId(c.number).contains(q)
    },
    sorts = listOf(
        SortOption("Slot") { a, b -> a.index - b.index },
        SortOption("Name") { a, b -> a.name.compareTo(b.name, ignoreCase = true) },
        SortOption("DMR ID") { a, b -> a.number.compareTo(b.number) },
    ),
    selection = selection,
    onSelectionChange = onSelectionChange,
    onOpen = onOpen,
    onAdd = onAdd,
    onBack = onBack,
    emptyMessage = "No contacts yet.",
    selectionActions = { DeleteAction(it, onDelete) },
    modifier = modifier,
)

/** RX group lists: which talk groups a channel will receive. */
@Composable
fun GroupListsScreen(
    state: CodeplugState,
    selection: Set<Int>,
    onSelectionChange: (Set<Int>) -> Unit,
    onOpen: (DumpGroupList) -> Unit,
    onAdd: () -> Unit,
    onDelete: (Set<Int>) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) = RecordListScreen(
    title = "Group Lists",
    items = state.groupLists,
    slotOf = { it.index },
    headline = { it.name },
    supporting = { "${it.members.size} contacts" },
    isUnsaved = { state.isUnsaved(CodeplugSection.GROUP_LISTS, it.index) },
    searchMatch = { g, q -> g.name.contains(q, ignoreCase = true) },
    sorts = listOf(
        SortOption("Slot") { a, b -> a.index - b.index },
        SortOption("Name") { a, b -> a.name.compareTo(b.name, ignoreCase = true) },
    ),
    selection = selection,
    onSelectionChange = onSelectionChange,
    onOpen = onOpen,
    onAdd = onAdd,
    onBack = onBack,
    emptyMessage = "No group lists yet.",
    selectionActions = { DeleteAction(it, onDelete) },
    modifier = modifier,
)

/** Radio IDs: the DMR identities this radio transmits under. */
@Composable
fun RadioIdsScreen(
    state: CodeplugState,
    selection: Set<Int>,
    onSelectionChange: (Set<Int>) -> Unit,
    onOpen: (DumpRadioId) -> Unit,
    onAdd: () -> Unit,
    onDelete: (Set<Int>) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) = RecordListScreen(
    title = "Radio IDs",
    items = state.radioIds,
    slotOf = { it.index },
    headline = { it.name },
    supporting = { formatDmrId(it.number) },
    isUnsaved = { state.isUnsaved(CodeplugSection.RADIO_IDS, it.index) },
    searchMatch = { r, q ->
        r.name.contains(q, ignoreCase = true) || formatDmrId(r.number).contains(q)
    },
    sorts = listOf(SortOption("Slot") { a, b -> a.index - b.index }),
    selection = selection,
    onSelectionChange = onSelectionChange,
    onOpen = onOpen,
    onAdd = onAdd,
    onBack = onBack,
    emptyMessage = "No radio IDs yet.",
    selectionActions = { DeleteAction(it, onDelete) },
    modifier = modifier,
)
