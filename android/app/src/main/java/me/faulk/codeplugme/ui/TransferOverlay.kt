package me.faulk.codeplugme.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import me.faulk.codeplugme.usb.TransferKind
import me.faulk.codeplugme.usb.TransferState
import me.faulk.codeplugme.usb.Verification

/**
 * A full-screen scrim while a transfer runs, and a dialog when one ends.
 *
 * The scrim is deliberately blocking. There is nothing useful to do while the
 * radio is being read or written, and letting the user wander into the editor
 * mid-write invites them to unplug the cable.
 */
@Composable
fun TransferOverlay(
    state: TransferState,
    onAcknowledge: () -> Unit,
    onAcceptRead: (TransferState.ReadComplete) -> Unit,
) {
    when (state) {
        is TransferState.Idle -> Unit

        is TransferState.Running -> Box(
            Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.6f)),
            contentAlignment = Alignment.Center,
        ) {
            Column(
                Modifier.padding(32.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                val verb = when (state.kind) {
                    TransferKind.READ -> "Reading from"
                    TransferKind.WRITE -> "Writing to"
                    TransferKind.VERIFY -> "Checking what was kept on"
                }
                Text(
                    "$verb ${state.deviceLabel}",
                    style = MaterialTheme.typography.titleMedium,
                    color = Color.White,
                )
                val fraction = state.fraction
                if (fraction == null) {
                    CircularProgressIndicator(color = Color.White)
                } else {
                    LinearProgressIndicator(
                        progress = { fraction },
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Text(
                        "${state.done} of ${state.total} blocks",
                        style = MaterialTheme.typography.bodySmall,
                        color = Color.White,
                    )
                }
                Text(
                    "Keep the cable connected.",
                    style = MaterialTheme.typography.bodySmall,
                    color = Color.White,
                )
            }
        }

        is TransferState.ReadComplete -> AlertDialog(
            onDismissRequest = { onAcceptRead(state) },
            title = { Text("Read complete") },
            text = { Text("The codeplug was read from the radio and saved to your codeplugs.") },
            confirmButton = { TextButton(onClick = { onAcceptRead(state) }) { Text("OK") } },
        )

        // The title comes from the read-back, not from the write: acks alone
        // cannot tell these three outcomes apart, and the difference between
        // them is the whole reason the read-back happens.
        is TransferState.WriteComplete -> AlertDialog(
            onDismissRequest = onAcknowledge,
            title = {
                Text(
                    when (state.verification) {
                        is Verification.Mismatched -> "The radio did not keep everything"
                        else -> "Write complete"
                    }
                )
            },
            text = {
                Column(
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                    modifier = Modifier.verticalScroll(rememberScrollState()),
                ) {
                    Text("The codeplug was written to ${state.deviceLabel}.")
                    when (val v = state.verification) {
                        is Verification.Clean -> Text(
                            "Read back afterwards and checked: the radio holds exactly " +
                                "what was sent."
                        )

                        is Verification.NotSupported -> Text(
                            "Power-cycle the radio to be sure it has committed the change."
                        )

                        is Verification.Unavailable -> Text(
                            "It could not be read back to check, so what the radio kept is " +
                                "unconfirmed: ${v.reason}"
                        )

                        is Verification.Mismatched -> {
                            Text(
                                "Reading it back found bytes the radio did not keep. Write " +
                                    "it again, and check the radio before using it on the air.",
                                color = MaterialTheme.colorScheme.error,
                            )
                            SelectionContainer {
                                Text(
                                    v.runs.joinToString("\n") { "• ${it.label}" },
                                    style = MaterialTheme.typography.bodySmall,
                                )
                            }
                        }
                    }
                }
            },
            confirmButton = { TextButton(onClick = onAcknowledge) { Text("OK") } },
        )

        is TransferState.Failed -> AlertDialog(
            onDismissRequest = onAcknowledge,
            title = {
                Text(if (state.kind == TransferKind.READ) "Read failed" else "Write failed")
            },
            text = {
                // Scrollable and selectable: a link failure carries a
                // diagnostic block that is taller than a phone dialog and is
                // only useful if it can be copied out.
                Column(
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                    modifier = Modifier.verticalScroll(rememberScrollState()),
                ) {
                    SelectionContainer { Text(state.message) }
                    // The difference between "nothing happened" and "your radio
                    // may be holding half a codeplug" is the single most
                    // important thing this dialog can say.
                    if (state.mayHavePartiallyWritten) {
                        Text(
                            "Some blocks had already been sent when this failed, so the " +
                                "radio may hold a partly written codeplug. Write a known " +
                                "good codeplug to it before using it on the air.",
                            color = MaterialTheme.colorScheme.error,
                        )
                    }
                }
            },
            confirmButton = { TextButton(onClick = onAcknowledge) { Text("OK") } },
        )
    }
}
