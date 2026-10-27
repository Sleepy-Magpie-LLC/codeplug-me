package me.faulk.codeplugme.ui.importer

import android.annotation.SuppressLint
import android.content.Context
import android.location.Address
import android.location.Geocoder
import android.location.Location
import android.location.LocationManager
import android.os.Build
import android.os.CancellationSignal
import java.util.Locale
import kotlin.coroutines.resume
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/**
 * The phone's location and place names, without Google Play Services.
 *
 * Geocoder is asked in [Locale.US] on purpose: the core matches RadioID and
 * RepeaterBook by the English country and state names ("United States",
 * "Colorado"), and a German phone would otherwise hand it "Vereinigte Staaten".
 */
object LocationLookup {

    /**
     * A coarse fix, or null when none is available.
     *
     * The caller must already hold ACCESS_COARSE_LOCATION. A fresh fix is tried
     * on API 30+, with the newest last-known fix as the fallback, because a
     * phone indoors can take a long time to produce a fresh one.
     */
    @SuppressLint("MissingPermission")
    suspend fun current(context: Context): Location? {
        val lm = context.getSystemService(LocationManager::class.java) ?: return null
        val providers = lm.getProviders(true)
        val lastKnown = providers.mapNotNull { runCatching { lm.getLastKnownLocation(it) }.getOrNull() }
            .maxByOrNull { it.time }
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return lastKnown

        // The passive provider only relays other apps' fixes and may never answer.
        val provider = providers.firstOrNull { it == LocationManager.NETWORK_PROVIDER }
            ?: providers.firstOrNull { it != LocationManager.PASSIVE_PROVIDER }
            ?: return lastKnown
        val fresh = withTimeoutOrNull(FIX_TIMEOUT_MS) {
            suspendCancellableCoroutine<Location?> { cont ->
                val signal = CancellationSignal()
                cont.invokeOnCancellation { signal.cancel() }
                lm.getCurrentLocation(provider, signal, context.mainExecutor) { cont.resume(it) }
            }
        }
        return fresh ?: lastKnown
    }

    /** The place at a coordinate, as an [ImportLocation]; the bare coordinate if unnamed. */
    @Suppress("DEPRECATION")
    suspend fun reverse(context: Context, lat: Double, lon: Double): ImportLocation {
        val address = geocode(context) { it.getFromLocation(lat, lon, 1) }
        return toLocation(lat, lon, address)
    }

    /**
     * Resolve what the user typed: "lat, lon" as is, anything else (a city, a
     * ZIP) through Geocoder. Null when nothing matches.
     */
    @Suppress("DEPRECATION")
    suspend fun forward(context: Context, query: String): ImportLocation? {
        parseCoordinates(query)?.let { (lat, lon) -> return reverse(context, lat, lon) }
        val address = geocode(context) { it.getFromLocationName(query, 1) } ?: return null
        return toLocation(address.latitude, address.longitude, address)
    }

    /** "39.74, -104.99" as a coordinate pair, or null if it is not one. */
    fun parseCoordinates(text: String): Pair<Double, Double>? {
        val parts = text.split(',', ' ').filter { it.isNotBlank() }
        if (parts.size != 2) return null
        val lat = parts[0].toDoubleOrNull() ?: return null
        val lon = parts[1].toDoubleOrNull() ?: return null
        if (lat !in -90.0..90.0 || lon !in -180.0..180.0) return null
        return lat to lon
    }

    /**
     * Run one blocking Geocoder lookup off the main thread.
     *
     * The blocking overloads are deprecated on API 33 in favour of listener
     * ones, but they still work and minSdk is 29, so one code path is simpler
     * (hence the suppressions on the callers). Geocoder throws on a network
     * failure, which is treated as "no answer".
     */
    private suspend fun geocode(context: Context, lookup: (Geocoder) -> List<Address>?): Address? =
        withContext(Dispatchers.IO) {
            if (!Geocoder.isPresent()) return@withContext null
            runCatching { lookup(Geocoder(context, Locale.US))?.firstOrNull() }.getOrNull()
        }

    /** Build the import's location from a coordinate and whatever Geocoder knew. */
    private fun toLocation(lat: Double, lon: Double, address: Address?): ImportLocation {
        val name = listOfNotNull(address?.locality ?: address?.subAdminArea, address?.adminArea)
            .joinToString(", ")
        return ImportLocation(
            lat = lat,
            lon = lon,
            country = address?.countryName,
            state = address?.adminArea,
            label = name.ifEmpty { "%.4f, %.4f".format(Locale.ROOT, lat, lon) },
        )
    }

    private const val FIX_TIMEOUT_MS = 15_000L
}
