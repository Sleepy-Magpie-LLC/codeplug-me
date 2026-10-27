package me.faulk.codeplugme.state

/**
 * Which entity family the editor is showing.
 *
 * The [key] values are the section names the core reports in a document's
 * `capabilities.sections`, so they are snake_case JSON keys, not display text.
 * The app renders its navigation from that list rather than from a hardcoded
 * table, which is why adding a radio to the core needs no change here.
 *
 * The QYT sections carry their own prefix because a QYT channel and an AnyTone
 * channel are different record shapes with different field meanings: on the
 * AnyTone a tone field is an index into a table, on the QYT it is tenths of a
 * hertz. Sharing one `channels` case would invite a screen to render the wrong
 * one.
 */
enum class CodeplugSection(val key: String, val title: String) {
    CHANNELS("channels", "Channels"),
    ZONES("zones", "Zones"),
    SCAN_LISTS("scan_lists", "Scan Lists"),
    CONTACTS("contacts", "Contacts"),
    GROUP_LISTS("group_lists", "Group Lists"),
    RADIO_IDS("radio_ids", "Radio IDs"),
    APRS("aprs", "APRS"),
    QYT_CHANNELS("qyt_channels", "Channels"),
    QYT_SETTINGS("qyt_settings", "Settings"),
    QYT_BANDS("qyt_bands", "VFO Channels");

    /** True for the sections the "Config" destination owns (priority 2). */
    val isConfig: Boolean
        get() = this == QYT_SETTINGS || this == QYT_BANDS || this == APRS

    /** True for the channel lists the "Channels" destination owns (priority 3). */
    val isChannels: Boolean
        get() = this == CHANNELS || this == QYT_CHANNELS

    companion object {
        /** The section for a core key, or null if this build does not know it. */
        fun fromKey(key: String): CodeplugSection? = entries.firstOrNull { it.key == key }
    }
}
