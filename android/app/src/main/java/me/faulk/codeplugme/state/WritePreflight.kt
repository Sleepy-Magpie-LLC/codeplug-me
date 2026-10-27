package me.faulk.codeplugme.state

import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import me.faulk.codeplugme.library.CodeplugOrigin
import me.faulk.codeplugme.library.LibraryEntry

/**
 * What the user is told before they confirm a write.
 *
 * The KT-5000's write sends the image verbatim, so every byte below the write
 * limit lands on the radio, including regions nothing in this app models. That
 * makes where the bytes came from a fact worth showing: an image read off the
 * radio being written carries that radio's own bytes there, an imported file
 * carries someone else's. Assembled before the dialog opens, because reading
 * the bytes and asking the core about them is IO.
 */
data class WritePreflight(
    val entry: LibraryEntry,
    val deviceLabel: String,
    /** Where the codeplug came from, as a sentence. */
    val originLine: String,
    /** True when the bytes about to go out are the editor's unsaved ones. */
    val hasUnsavedEdits: Boolean,
    /** Map regions this image would blank on the radio. Usually empty. */
    val erasedRegions: List<String>,
)

/**
 * Where a codeplug came from, as the sentence the write dialog shows.
 *
 * A plain function rather than a method so the wording is testable without an
 * Android context or a radio.
 */
fun originLine(entry: LibraryEntry): String = when (entry.origin) {
    CodeplugOrigin.RADIO ->
        "Read off a radio on ${dayOf(entry.importedAtEpochMs)}."

    CodeplugOrigin.IMPORT ->
        "Imported on ${dayOf(entry.importedAtEpochMs)}" +
            (entry.sourceName?.let { " from $it" } ?: "") + "."

    CodeplugOrigin.GENERATED ->
        "Built from repeater listings on ${dayOf(entry.importedAtEpochMs)}" +
            (entry.sourceName?.let { " on top of $it" } ?: "") + "."

    // Everything stored before the app recorded this. Saying so is better than
    // guessing, because the guess is the part that matters.
    CodeplugOrigin.UNKNOWN ->
        "This app does not know whether this came off a radio or from a file."
}

private fun dayOf(epochMs: Long): String =
    SimpleDateFormat("d MMM yyyy", Locale.getDefault()).format(Date(epochMs))
