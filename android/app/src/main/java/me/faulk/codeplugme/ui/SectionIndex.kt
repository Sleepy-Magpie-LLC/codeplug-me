package me.faulk.codeplugme.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import me.faulk.codeplugme.state.CodeplugSection
import me.faulk.codeplugme.ui.common.UnsavedDot

/**
 * A tappable list of codeplug sections.
 *
 * Both the Config tab and the More tab are the same thing: a list of sections
 * the open radio actually has, taken from the core's `capabilities.sections`
 * rather than from a table here. They differ only in which sections they filter
 * to and what each row's second line says.
 */
@Composable
fun SectionIndex(
    sections: List<CodeplugSection>,
    subtitle: (CodeplugSection) -> String,
    hasUnsaved: (CodeplugSection) -> Boolean,
    onOpen: (CodeplugSection) -> Unit,
    modifier: Modifier = Modifier,
) {
    LazyColumn(modifier.fillMaxSize()) {
        items(sections, key = { it.key }) { section ->
            ListItem(
                headlineContent = { Text(section.title) },
                supportingContent = {
                    val sub = subtitle(section)
                    if (sub.isNotEmpty()) Text(sub)
                },
                trailingContent = {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        if (hasUnsaved(section)) UnsavedDot(Modifier.padding(end = 8.dp))
                        Icon(
                            Icons.AutoMirrored.Filled.KeyboardArrowRight,
                            contentDescription = null,
                        )
                    }
                },
                modifier = Modifier.clickable { onOpen(section) },
            )
            HorizontalDivider()
        }
    }
}
