package me.faulk.codeplugme.ui.config

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import java.util.Locale
import kotlin.math.roundToInt
import me.faulk.codeplugme.model.Tone
import me.faulk.codeplugme.ui.common.NumberStepper

/**
 * Which of a [Tone]'s three shapes is selected, as a plain value so a
 * dropdown can offer it without carrying the associated data around. Port of
 * the macOS `ToneKind`.
 */
private enum class ToneKind(val label: String) {
    OFF("Off"),
    CTCSS("CTCSS"),
    DCS("DCS"),
}

/** Which kind a [Tone] is. */
private val Tone.kind: ToneKind
    get() = when (this) {
        is Tone.None -> ToneKind.OFF
        is Tone.Ctcss -> ToneKind.CTCSS
        is Tone.Dcs -> ToneKind.DCS
    }

/**
 * A representative tone of [this] kind, used when the user switches kinds.
 * 88.5 Hz and D023 are the first entries of their respective standard tables,
 * matching the macOS default.
 */
private fun ToneKind.defaultTone(): Tone = when (this) {
    ToneKind.OFF -> Tone.None
    ToneKind.CTCSS -> Tone.Ctcss(hzTenths = 885)
    ToneKind.DCS -> Tone.Dcs(code = 23, inverted = false)
}

/**
 * Editor for a QYT [Tone]: an Off / CTCSS / DCS selector, then whichever
 * fields that kind needs. Port of the macOS `QytToneField`; both the QYT
 * bands screen and the QYT channel editor use this.
 */
@Composable
fun ToneField(label: String, tone: Tone, onToneChange: (Tone) -> Unit, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        LabeledDropdown(
            label = label,
            options = ToneKind.entries,
            selected = tone.kind,
            optionLabel = { it.label },
            // Re-selecting the current kind must not throw away what the user
            // already typed for it.
            onSelect = { kind -> if (kind != tone.kind) onToneChange(kind.defaultTone()) },
        )
        when (tone) {
            is Tone.None -> Unit
            is Tone.Ctcss -> CtcssField(label, tone, onToneChange)
            is Tone.Dcs -> DcsFields(label, tone, onToneChange)
        }
    }
}

/** The CTCSS frequency, shown to the user in whole Hz though stored as tenths. */
@Composable
private fun CtcssField(label: String, tone: Tone.Ctcss, onToneChange: (Tone) -> Unit) {
    var text by remember(tone.hzTenths) { mutableStateOf(formatTenths(tone.hzTenths)) }
    OutlinedTextField(
        value = text,
        onValueChange = { input ->
            text = input
            val hz = input.trim().replace(',', '.').toDoubleOrNull() ?: return@OutlinedTextField
            val clamped = hz.coerceIn(0.0, 6_553.5)
            onToneChange(Tone.Ctcss(hzTenths = (clamped * 10).roundToInt()))
        },
        label = { Text("$label frequency (Hz)") },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
        supportingText = { Text("60.1 Hz and up; the radio stores tenths of a hertz.") },
        modifier = Modifier.fillMaxWidth(),
    )
}

/** The DCS code and its polarity. */
@Composable
private fun DcsFields(label: String, tone: Tone.Dcs, onToneChange: (Tone) -> Unit) {
    NumberStepper(
        label = "$label DCS code",
        value = tone.code,
        onValueChange = { onToneChange(Tone.Dcs(code = it, inverted = tone.inverted)) },
        range = 0..999,
    )
    SwitchRow(
        label = if (tone.inverted) "Inverted (reversed polarity)" else "Inverted",
        checked = tone.inverted,
        onCheckedChange = { onToneChange(Tone.Dcs(code = tone.code, inverted = it)) },
    )
}

/** "885" tenths of a hertz as "88.5". */
private fun formatTenths(hzTenths: Int): String =
    String.format(Locale.ROOT, "%.1f", hzTenths / 10.0)
