package me.faulk.codeplugme.ui.config

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import me.faulk.codeplugme.model.DumpAprs
import me.faulk.codeplugme.model.formatMhz
import me.faulk.codeplugme.ui.common.EmptyState

/**
 * Read-only view of the radio's APRS identity settings. Port of the macOS
 * `APRSPane`.
 *
 * This is read-only for a specific, confirmed reason, not a placeholder for
 * an editor that hasn't been written yet: this block lives in the AnyTone
 * global-settings region, and writing that region once locked a radio behind
 * an unknown power-on password with a Chinese menu. There is no edit path
 * here on purpose.
 *
 * @param aprs the decoded APRS block, or null when the codeplug has none.
 * @param onBack called when the user navigates back, or null when this screen
 *   is a bottom-bar tab's own content and has nowhere to go back to.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AprsScreen(aprs: DumpAprs?, onBack: (() -> Unit)?, modifier: Modifier = Modifier) {
    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text("APRS") },
                // A tab root has an empty back stack, so a back arrow there
                // would be a control that visibly does nothing.
                navigationIcon = {
                    if (onBack != null) {
                        IconButton(onClick = onBack) {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                        }
                    }
                },
            )
        },
    ) { padding ->
        if (aprs == null) {
            EmptyState(
                title = "No APRS settings",
                detail = "This codeplug has no APRS block.",
                modifier = Modifier.padding(padding).fillMaxSize(),
            )
            return@Scaffold
        }

        Column(
            Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text(
                "AnyTone global settings are not writable in this app. That memory region " +
                    "is read-only because writing it once locked a radio behind an unknown " +
                    "power-on password with Chinese menus. These values are shown for " +
                    "reference; edit them in the vendor CPS.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            ConfigSection("Identity") {
                LabeledValue("My Call Sign", aprs.sourceCall.ifEmpty { "—" })
                LabeledValue("My SSID", aprs.sourceSsid.toString())
                LabeledValue("Destination", aprs.destinationCall.ifEmpty { "—" })
                LabeledValue("Destination SSID", aprs.destinationSsid.toString())
                LabeledValue("Symbol Table", symbolString(aprs.symbolTable))
                LabeledValue("Map Icon", symbolString(aprs.symbol))
            }

            ConfigSection("Analog TX") {
                LabeledValue(
                    "TX Frequency",
                    if (aprs.fmTxFrequencyHz == 0L) "—" else "${formatMhz(aprs.fmTxFrequencyHz)} MHz",
                )
                LabeledValue("TX Power (raw)", aprs.fmPower.toString())
            }

            ConfigSection("Timing (raw values)") {
                LabeledValue("Manual TX Interval", aprs.manualTxInterval.toString())
                LabeledValue("Auto TX Interval", aprs.autoTxInterval.toString())
            }
        }
    }
}

/** Render a symbol-selector byte as its ASCII character when printable. */
private fun symbolString(v: Int): String {
    val printable = v in 0x20 until 0x7f
    return if (printable) "${v.toChar()} ($v)" else v.toString()
}
