package me.faulk.codeplugme.ui

import me.faulk.codeplugme.state.CodeplugSection

/**
 * Where the app can be.
 *
 * The three top-level destinations are the three things this app is for, in
 * order: get a codeplug onto the radio, change the radio's configuration, edit
 * channels in bulk. Everything else the macOS app can do is reachable from
 * [More], which keeps the parity features without letting them crowd the three
 * that matter.
 */
sealed interface Destination {

    /** Priority 1: the connected radio and the codeplugs on this phone. */
    data object Radio : Destination

    /** Priority 2: the open codeplug's radio settings. */
    data object Config : Destination

    /** Priority 3: the open codeplug's channels, with bulk edit. */
    data object Channels : Destination

    /** The remaining record families, kept out of the way of the first three. */
    data object More : Destination

    /** One record family's list, reached from [More]. */
    data class Records(val section: CodeplugSection) : Destination

    /** One record's editor. */
    data class Editor(val section: CodeplugSection, val slot: Int) : Destination

    /** A record's membership list (a zone's channels, a group list's contacts). */
    data class Members(val section: CodeplugSection, val slot: Int) : Destination

    /** Picking records to add to a membership list. */
    data class MemberPicker(val section: CodeplugSection, val slot: Int) : Destination

    /** Building a codeplug from nearby repeaters, reached from [More]. */
    data object RepeaterImport : Destination

    /** Bulk-editing the current channel selection. */
    data class BulkChannels(val slots: Set<Int>) : Destination
}

/** The bottom-bar destinations, in priority order. */
val topLevelDestinations = listOf(
    Destination.Radio,
    Destination.Config,
    Destination.Channels,
    Destination.More,
)

/** The label under a bottom-bar icon. */
fun Destination.label(): String = when (this) {
    Destination.Radio -> "Radio"
    Destination.Config -> "Config"
    Destination.Channels -> "Channels"
    Destination.More -> "More"
    else -> ""
}
