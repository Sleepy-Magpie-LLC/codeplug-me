package me.faulk.codeplugme.ui.common

import java.util.Locale
import me.faulk.codeplugme.model.hzFromMhz

/**
 * Presentation-only formatting.
 *
 * The number formatters that matter for correctness (frequencies and DMR IDs)
 * live in `me.faulk.codeplugme.model.Formatting` beside the records they format,
 * because getting them wrong corrupts a value rather than just looking odd.
 * What is left here is the text that only ever reaches a human eye.
 */

/** A one-based slot number for display. Slots are zero-based everywhere else. */
fun formatSlot(index: Int): String = (index + 1).toString()

/** A byte count as a short human string, for the library rows. */
fun formatSize(bytes: Long): String = when {
    bytes >= 1_000_000 -> String.format(Locale.ROOT, "%.1f MB", bytes / 1_000_000.0)
    bytes >= 1_000 -> String.format(Locale.ROOT, "%.0f kB", bytes / 1_000.0)
    else -> "$bytes B"
}

/**
 * Parse a user-typed MHz string to hertz, or null when it is not a number.
 *
 * A comma is accepted as a decimal separator because a phone keyboard in a
 * comma-decimal locale offers one, and rejecting it would make the field
 * unusable there. The value itself is still stored through the locale-pinned
 * [hzFromMhz].
 */
fun parseMhz(text: String): Long? {
    val mhz = text.trim().replace(',', '.').toDoubleOrNull() ?: return null
    return hzFromMhz(mhz)
}
