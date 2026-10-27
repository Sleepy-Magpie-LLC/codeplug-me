package me.faulk.codeplugme.ui.config

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import me.faulk.codeplugme.model.DumpQytSettings
import me.faulk.codeplugme.ui.common.EmptyState
import me.faulk.codeplugme.ui.common.NumberStepper

/**
 * The QYT KT-5000's radio settings screen: the fifty-four-byte header at
 * `0x0E00` the core models, and nothing else. Port of the macOS
 * `QytSettingsPane`.
 *
 * # Why this screen is short
 *
 * CHIRP parses a larger settings struct here, borrowed from a different QYT
 * model, then disables it for this radio because several of its fields
 * decode out of range against a real capture. Offering a control for one of
 * those fields would write a byte to the radio that means something else.
 * Only the fields confirmed against this radio's own capture are here; the
 * rest of the settings block round-trips untouched.
 *
 * The display message is the exception to that narrowness, and it is not from
 * CHIRP's suspect struct: it is the 16 ASCII bytes at `0x0FE0`, which ship
 * reading `Model:KT-5000` and are the line the radio shows on its own screen.
 *
 * @param settings the current settings, or null when the codeplug has none.
 * @param staticMessage the LCD line at `0x0FE0`.
 * @param onApply called with the edited draft and message when the user taps
 *   Apply.
 * @param onBack called when the user navigates back, or null when this screen
 *   is a bottom-bar tab's own content and has nowhere to go back to.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun QytSettingsScreen(
    settings: DumpQytSettings?,
    staticMessage: String,
    onApply: (DumpQytSettings, String) -> Unit,
    onBack: (() -> Unit)?,
    modifier: Modifier = Modifier,
) {
    var draft by remember { mutableStateOf(settings ?: DumpQytSettings.EMPTY) }
    var messageDraft by remember { mutableStateOf(staticMessage) }

    // Re-seed from the document whenever it changes underneath the screen, so
    // an undo of an applied change shows up here instead of leaving the draft
    // holding a value the codeplug no longer has.
    LaunchedEffect(settings) { draft = settings ?: DumpQytSettings.EMPTY }
    LaunchedEffect(staticMessage) { messageDraft = staticMessage }

    val hasChanges = settings != null &&
        (draft != settings || messageDraft != staticMessage)

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text("Radio Settings") },
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
        bottomBar = {
            if (settings != null) {
                ApplyRevertFooter(
                    hasChanges = hasChanges,
                    onRevert = {
                        draft = settings
                        messageDraft = staticMessage
                    },
                    onApply = { onApply(draft, messageDraft) },
                )
            }
        },
    ) { padding ->
        if (settings == null) {
            EmptyState(
                title = "No settings",
                detail = "This codeplug has no QYT settings header.",
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
            ConfigSection("Display") {
                OutlinedTextField(
                    value = messageDraft,
                    onValueChange = { messageDraft = clampStaticMessage(it) },
                    label = { Text("Display message") },
                    singleLine = true,
                    supportingText = {
                        Text("${STATIC_MESSAGE_LIMIT - messageDraft.length} characters left")
                    },
                    modifier = Modifier.fillMaxWidth(),
                )
                LabeledValue(
                    "Shown on the radio",
                    "The line the radio draws on its own screen. It ships reading " +
                        "\"Model:KT-5000\", which is why it looks like a fixed label.",
                )
            }

            ConfigSection("Settings") {
                RawPicker(
                    title = "Squelch",
                    value = draft.squelch,
                    labels = squelchLevels,
                    onValueChange = { draft = draft.copy(squelch = it) },
                )
                RawPicker(
                    title = "Transmit timeout",
                    value = draft.tot,
                    labels = timeoutSteps,
                    onValueChange = { draft = draft.copy(tot = it) },
                )
                NumberStepper(
                    label = "Power-on volume",
                    value = draft.volume,
                    onValueChange = { draft = draft.copy(volume = it) },
                    range = 0..255,
                )
                RawPicker(
                    title = "Backlight timer",
                    value = draft.backlightTimer,
                    labels = backlightTimes,
                    onValueChange = { draft = draft.copy(backlightTimer = it) },
                )
                RawPicker(
                    title = "Dual watch",
                    value = draft.tmr,
                    labels = watchModes,
                    onValueChange = { draft = draft.copy(tmr = it) },
                )
                SwitchRow(
                    label = "Key beep",
                    checked = draft.beep,
                    onCheckedChange = { draft = draft.copy(beep = it) },
                )
                ScanResumePicker(
                    selection = draft.scanRev,
                    onSelect = { draft = draft.copy(scanRev = it) },
                )
                SwitchRow(
                    label = "Remember power state",
                    checked = draft.rememberPower,
                    onCheckedChange = { draft = draft.copy(rememberPower = it) },
                )
            }

            Text(
                "These are the settings bytes confirmed against a capture of this radio. " +
                    "The rest of the settings block round-trips untouched, and more controls " +
                    "appear here as each field is confirmed against the radio itself rather " +
                    "than inherited from another model's memory map.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/**
 * Picker for scan resume (`TO` / `CO` / `SE`). Port of the macOS
 * `QytScanRevPicker`.
 *
 * [selection] is nullable because the core decodes any byte outside those
 * three to `null` rather than a fourth string: there is no raw value left to
 * show, so a null selection just displays as "Unknown" until the user picks
 * one of the three known labels.
 */
@Composable
private fun ScanResumePicker(selection: String?, onSelect: (String?) -> Unit, modifier: Modifier = Modifier) {
    val known = listOf("TO" to "TO (timeout)", "CO" to "CO (carrier)", "SE" to "SE (search)")
    LabeledDropdown(
        label = "Scan resume",
        options = known.map { it.first },
        selected = selection ?: "",
        optionLabel = { tag -> known.firstOrNull { it.first == tag }?.second ?: "Unknown" },
        onSelect = onSelect,
        modifier = modifier,
    )
}

/** Squelch levels 0-9; 0 is an open squelch. */
private val squelchLevels = (0..9).map { if (it == 0) "0 (open)" else it.toString() }

/** Transmit timeout, a step index counting 15-second steps from 15 s. */
private val timeoutSteps = (0..38).map { "${(it + 1) * 15} s" }

/** Backlight timer: 0 is off, otherwise seconds (CHIRP's `LIST_OFF1TO50`). */
private val backlightTimes = (0..50).map { if (it == 0) "Off" else "$it s" }

/** Dual-band waiting (Menu 0 TMR). This radio is Off/On only. */
private val watchModes = listOf("Off", "On")

/**
 * The static message field is 16 bytes at `0x0FE0`, but the LCD only draws the
 * first 13, the width of the factory `Model:KT-5000`.
 */
const val STATIC_MESSAGE_LIMIT = 13

/**
 * Keep the display message to what the radio can store: printable ASCII, at
 * most [STATIC_MESSAGE_LIMIT] characters. Filtering as the user types means an
 * unrenderable keystroke never reaches the draft, so Apply cannot fail on it.
 */
fun clampStaticMessage(text: String): String =
    text.filter { it in ' '..'~' }.take(STATIC_MESSAGE_LIMIT)
