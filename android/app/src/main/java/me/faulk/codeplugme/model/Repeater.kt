package me.faulk.codeplugme.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** A repeater database the core can query. */
@Serializable
enum class RepeaterSource {
    /** hearham.com: free, worldwide, has coordinates. */
    @SerialName("hearham") HEARHAM,

    /** radioid.net's DMR list: free, city and state only. */
    @SerialName("radioid") RADIOID,

    /** RepeaterBook: needs the user's own app token. */
    @SerialName("repeaterbook") REPEATERBOOK,
}

/** How a repeater is worked. */
@Serializable
enum class RepeaterMode {
    @SerialName("analog") ANALOG,
    @SerialName("dmr") DMR,
}

/**
 * One repeater from `geoFetch`, matching the core's `geo::Repeater`.
 *
 * The selected ones go back to `geoBuild` exactly as they came, so every field
 * the core sends is modelled here; dropping one would change what gets built.
 * Frequencies are from the radio's side: [rxHz] is the repeater's output.
 */
@Serializable
data class Repeater(
    val source: RepeaterSource,
    val callsign: String,
    val location: String,
    @SerialName("rx_hz") val rxHz: Long,
    @SerialName("tx_hz") val txHz: Long,
    val mode: RepeaterMode,
    @SerialName("tx_tone") val txTone: Tone,
    @SerialName("rx_tone") val rxTone: Tone,
    @SerialName("color_code") val colorCode: Int? = null,
    val lat: Double? = null,
    val lon: Double? = null,
    @SerialName("distance_km") val distanceKm: Double? = null,
    /** The base codeplug already has a channel on these frequencies. */
    val duplicate: Boolean = false,
)

/** RepeaterBook credentials, sent only when that source is selected. */
@Serializable
data class RepeaterBookAuth(val token: String, val contact: String)

/** The `geoFetch` query. Country and state are full names, as Geocoder gives them. */
@Serializable
data class GeoQuery(
    val lat: Double,
    val lon: Double,
    @SerialName("radius_km") val radiusKm: Double,
    val country: String? = null,
    val state: String? = null,
    val sources: List<RepeaterSource>,
    val repeaterbook: RepeaterBookAuth? = null,
)

/** Clean replaces every channel, zone and scan list; merge adds to them. */
@Serializable
enum class BuildMode(val key: String) {
    @SerialName("clean") CLEAN("clean"),
    @SerialName("merge") MERGE("merge"),
}

/** The `geoBuild` options. */
@Serializable
data class GeoBuildOptions(val mode: BuildMode, @SerialName("zone_name") val zoneName: String)

/** How many repeaters fit in the base codeplug, from `geoCapacity`. */
@Serializable
data class GeoCapacity(
    val available: Int,
    @SerialName("max_selectable") val maxSelectable: Int,
    @SerialName("zone_limit") val zoneLimit: Int? = null,
    @SerialName("scan_list_limit") val scanListLimit: Int? = null,
    @SerialName("blocked_reason") val blockedReason: String? = null,
)
