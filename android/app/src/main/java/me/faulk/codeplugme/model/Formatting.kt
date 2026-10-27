package me.faulk.codeplugme.model

import java.util.Locale

/**
 * Format a frequency in Hz as a MHz string with fixed 5 decimals (the
 * codeplug's 10 Hz BCD resolution), no grouping, dot decimal separator.
 *
 * Pinned to [Locale.ROOT] because this is a hardware value rather than
 * prose: a device whose locale uses a comma decimal separator or a
 * non-Arabic digit set must still produce "145.50000", not "145,50000", or
 * the field would fail to round-trip back through [hzFromMhz].
 */
fun formatMhz(hz: Long): String = String.format(Locale.ROOT, "%.5f", hz / 1_000_000.0)

/**
 * Parse a MHz string typed into a field back to the Hz a record stores,
 * rounded to the codeplug's 10 Hz BCD resolution and clamped to what the
 * field can hold.
 */
fun hzFromMhz(mhz: Double): Long {
    val clamped = mhz.coerceIn(0.0, 999.999_99)
    return Math.round(clamped * 100_000) * 10
}

/**
 * Format a DMR ID as plain digits, pinned to [Locale.ROOT] so it can never
 * pick up a grouping separator: a DMR ID is an identifier, not a quantity,
 * and `3113043` must never render as `3,113,043`.
 */
fun formatDmrId(id: Long): String = String.format(Locale.ROOT, "%d", id)
