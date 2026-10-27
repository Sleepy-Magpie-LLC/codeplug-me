package me.faulk.codeplugme.ui.common

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.automirrored.filled.Sort
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp

/** One way to order a record list, named for the sort menu. */
data class SortOption<T>(val label: String, val comparator: Comparator<T>)

/**
 * The list screen every record family uses.
 *
 * SwiftUI's `Table` backs eight panes on macOS, with sortable column headers,
 * multi-select and a context menu. None of that translates to a phone, so this
 * is a redesign rather than a port: a two-line row, sort as a menu instead of
 * tappable headers, search as a field, and long-press to enter a selection mode
 * whose action bar carries bulk edit and delete.
 *
 * Selection is a set of SLOT INDICES, never list positions, so sorting or
 * filtering the list cannot make an action land on the wrong record.
 */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalFoundationApi::class)
@Composable
fun <T> RecordListScreen(
    title: String,
    items: List<T>,
    slotOf: (T) -> Int,
    headline: (T) -> String,
    supporting: (T) -> String,
    isUnsaved: (T) -> Boolean,
    searchMatch: (T, String) -> Boolean,
    selection: Set<Int>,
    onSelectionChange: (Set<Int>) -> Unit,
    onOpen: (T) -> Unit,
    /** Null when this list IS a bottom-bar tab, which has nowhere to go back to. */
    onBack: (() -> Unit)?,
    modifier: Modifier = Modifier,
    actions: @Composable RowScope.() -> Unit = {},
    sorts: List<SortOption<T>> = emptyList(),
    onAdd: (() -> Unit)? = null,
    emptyMessage: String = "Nothing here yet.",
    filterBar: (@Composable () -> Unit)? = null,
    selectionActions: @Composable RowScope.(Set<Int>) -> Unit = {},
    /** A problem with the record (e.g. a duplicate channel): tints the row and adds a caption line. */
    warning: (T) -> String? = { null },
) {
    var query by remember { mutableStateOf("") }
    var sortIndex by remember { mutableStateOf(0) }
    var sortMenuOpen by remember { mutableStateOf(false) }
    val selecting = selection.isNotEmpty()

    val shown = remember(items, query, sortIndex, sorts) {
        val filtered = if (query.isBlank()) items else items.filter { searchMatch(it, query) }
        // Falls back to the first sort when the caller swaps in a shorter list
        // (e.g. a duplicates-only view that offers just a frequency sort).
        (sorts.getOrNull(sortIndex) ?: sorts.firstOrNull())?.let { filtered.sortedWith(it.comparator) } ?: filtered
    }

    Scaffold(
        modifier = modifier,
        topBar = {
            if (selecting) {
                TopAppBar(
                    title = { Text("${selection.size} selected") },
                    navigationIcon = {
                        IconButton(onClick = { onSelectionChange(emptySet()) }) {
                            Icon(Icons.Default.Close, contentDescription = "Clear selection")
                        }
                    },
                    actions = { selectionActions(selection) },
                )
            } else {
                TopAppBar(
                    title = { Text(title) },
                    // A tab root has an empty back stack, so a back arrow there
                    // would be a control that visibly does nothing.
                    navigationIcon = {
                        if (onBack != null) {
                            IconButton(onClick = onBack) {
                                Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                            }
                        }
                    },
                    actions = {
                        actions()
                        if (sorts.isNotEmpty()) {
                            IconButton(onClick = { sortMenuOpen = true }) {
                                Icon(Icons.AutoMirrored.Filled.Sort, contentDescription = "Sort")
                            }
                            DropdownMenu(
                                expanded = sortMenuOpen,
                                onDismissRequest = { sortMenuOpen = false },
                            ) {
                                sorts.forEachIndexed { i, s ->
                                    DropdownMenuItem(
                                        text = { Text(s.label) },
                                        onClick = { sortIndex = i; sortMenuOpen = false },
                                    )
                                }
                            }
                        }
                    },
                )
            }
        },
        floatingActionButton = {
            if (onAdd != null && !selecting) {
                FloatingActionButton(onClick = onAdd) {
                    Icon(Icons.Default.Add, contentDescription = "Add")
                }
            }
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
            filterBar?.invoke()

            if (shown.isEmpty()) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Text(
                        if (query.isBlank()) emptyMessage else "Nothing matches \"$query\".",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(32.dp),
                    )
                }
                return@Column
            }

            LazyColumn(Modifier.fillMaxSize()) {
                items(shown, key = slotOf) { item ->
                    val slot = slotOf(item)
                    RecordRow(
                        slotLabel = formatSlot(slot),
                        headline = headline(item),
                        supporting = supporting(item),
                        warning = warning(item),
                        selecting = selecting,
                        checked = slot in selection,
                        unsaved = isUnsaved(item),
                        onCheckedChange = { on ->
                            onSelectionChange(if (on) selection + slot else selection - slot)
                        },
                        modifier = Modifier.combinedClickable(
                            onClick = {
                                if (selecting) {
                                    onSelectionChange(
                                        if (slot in selection) selection - slot else selection + slot
                                    )
                                } else {
                                    onOpen(item)
                                }
                            },
                            // Long press is how a phone enters multi-select; it
                            // replaces the desktop's shift-click and drag.
                            onLongClick = { onSelectionChange(selection + slot) },
                        ),
                    )
                    HorizontalDivider()
                }
            }
        }
    }
}

/**
 * One record's row: a slot-number gutter on the left, then the record over two
 * lines beside it.
 *
 * Material's `ListItem` was the obvious fit and is deliberately not used. Its
 * overline puts the slot number on a line of its own above the name, which on a
 * channel list is three stacked lines where the first carries one short number.
 * A ruled gutter reads as a table instead: the eye scans numbers down the left
 * edge and the record's own text stays where a record's text belongs.
 */
@Composable
private fun RecordRow(
    slotLabel: String,
    headline: String,
    supporting: String,
    warning: String?,
    selecting: Boolean,
    checked: Boolean,
    unsaved: Boolean,
    onCheckedChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    val tint = if (warning != null) {
        Modifier.background(MaterialTheme.colorScheme.errorContainer.copy(alpha = 0.5f))
    } else {
        Modifier
    }
    Row(
        modifier.fillMaxWidth().then(tint).heightIn(min = 64.dp).padding(end = 16.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (selecting) {
            Checkbox(checked = checked, onCheckedChange = onCheckedChange)
        }
        // A minimum width rather than a fixed one: the AnyTone runs to four-digit
        // slots, and clipping the number would be worse than the gutter widening.
        Text(
            slotLabel,
            style = MaterialTheme.typography.titleMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.End,
            modifier = Modifier
                .widthIn(min = 40.dp)
                .padding(start = if (selecting) 0.dp else 16.dp, end = 12.dp),
        )
        VerticalDivider(Modifier.height(48.dp))
        Column(
            Modifier.weight(1f).padding(start = 12.dp, top = 10.dp, bottom = 10.dp),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            Text(headline, style = MaterialTheme.typography.bodyLarge)
            Text(
                supporting,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (warning != null) {
                Text(
                    warning,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
        }
        if (unsaved) UnsavedDot()
    }
}

/** A labelled row of filter chips, for the channel mode and slot filters. */
@Composable
fun FilterRow(content: @Composable RowScope.() -> Unit) {
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 16.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
        content = content,
    )
}
