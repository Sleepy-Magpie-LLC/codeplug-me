package me.faulk.codeplugme.ui.channels

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import me.faulk.codeplugme.state.BulkQytChannelUpdate

/**
 * Bulk editor for [count] selected QYT channels: scan-skip only.
 *
 * Ported from `BulkQytChannelEditorSheet` (macOS `BulkEditorSheets.swift`). It
 * is the only shared field the core can write across a QYT selection; see
 * [me.faulk.codeplugme.state.qytChannelEdit] for why the rest stay single-channel.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun BulkQytChannelEditorScreen(
    count: Int,
    onApply: (BulkQytChannelUpdate) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var changeScanSkip by remember { mutableStateOf(false) }
    // UI sense: on means skip when scanning. Mapped to scanAdd = !draftSkip on
    // apply, so the store/core keep their stored bit polarity.
    var draftSkip by remember { mutableStateOf(true) }

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text("Bulk Edit $count Channels") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Cancel")
                    }
                },
                actions = {
                    IconButton(onClick = {
                        if (changeScanSkip) {
                            onApply(BulkQytChannelUpdate(scanAdd = !draftSkip))
                        }
                        onBack()
                    }) {
                        Icon(Icons.Default.Check, contentDescription = "Apply")
                    }
                },
            )
        },
    ) { padding ->
        Column(Modifier.padding(padding).fillMaxSize().padding(16.dp)) {
            Row(
                Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Checkbox(checked = changeScanSkip, onCheckedChange = { changeScanSkip = it })
                Text("Skip when scanning", modifier = Modifier.width(140.dp))
                Switch(checked = draftSkip, onCheckedChange = { draftSkip = it }, enabled = changeScanSkip)
            }

            Text(
                "Only checked fields will be updated. Unchecked fields keep their " +
                    "current per-channel values.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(top = 8.dp),
            )
        }
    }
}
