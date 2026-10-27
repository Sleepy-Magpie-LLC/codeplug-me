package me.faulk.codeplugme.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import me.faulk.codeplugme.state.CodeplugSection
import me.faulk.codeplugme.state.CodeplugState
import me.faulk.codeplugme.ui.common.EmptyState

/**
 * The record families that are not one of the app's three priorities.
 *
 * Which entries appear comes from the core's `capabilities.sections` for the
 * open codeplug, so a radio with no zones simply does not show zones and adding
 * a radio to the core needs no change in this file.
 */
@Composable
fun MoreScreen(
    state: CodeplugState,
    onOpenSection: (CodeplugSection) -> Unit,
    onImportRepeaters: () -> Unit,
    modifier: Modifier = Modifier,
) {
    if (!state.isOpen) {
        EmptyState(
            "No codeplug open",
            "Open one from the Radio tab to edit it.",
            modifier = modifier,
        )
        return
    }

    // Channels and the config sections have their own destinations already.
    val sections = state.sections.filterNot { it.isChannels || it.isConfig }
    Column(modifier.fillMaxSize()) {
        // Every radio has channels, so this row is here even when nothing else is.
        ListItem(
            headlineContent = { Text("Add nearby repeaters") },
            supportingContent = { Text("Add repeater listings around you to this codeplug") },
            trailingContent = {
                Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null)
            },
            modifier = Modifier.clickable(onClick = onImportRepeaters),
        )
        HorizontalDivider()

        if (sections.isEmpty()) {
            EmptyState(
                "Nothing else",
                "This radio's channels and settings are all it has.",
                modifier = Modifier.weight(1f),
            )
        } else {
            SectionIndex(
                sections = sections,
                subtitle = { countLabel(state, it) },
                hasUnsaved = { state.hasUnsavedChanges(it) },
                onOpen = onOpenSection,
                modifier = Modifier.weight(1f),
            )
        }
    }
}

/** How many records a section holds, for the row's second line. */
private fun countLabel(state: CodeplugState, section: CodeplugSection): String = when (section) {
    CodeplugSection.ZONES -> "${state.zones.size} zones"
    CodeplugSection.SCAN_LISTS -> "${state.scanLists.size} scan lists"
    // The AnyTone always reports its full 10000 contact slots, so counting the
    // named ones is the number that means something to a user.
    CodeplugSection.CONTACTS -> "${state.contacts.count { it.name.isNotBlank() }} contacts"
    CodeplugSection.GROUP_LISTS -> "${state.groupLists.size} group lists"
    CodeplugSection.RADIO_IDS -> "${state.radioIds.size} radio IDs"
    else -> ""
}
