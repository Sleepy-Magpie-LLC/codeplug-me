package me.faulk.codeplugme.ui.importer

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import java.io.File
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import me.faulk.codeplugme.core.RadioCore
import me.faulk.codeplugme.core.userMessage
import me.faulk.codeplugme.model.BuildMode
import me.faulk.codeplugme.model.GeoBuildOptions
import me.faulk.codeplugme.model.GeoCapacity
import me.faulk.codeplugme.model.GeoQuery
import me.faulk.codeplugme.model.Repeater
import me.faulk.codeplugme.model.RepeaterBookAuth
import me.faulk.codeplugme.model.RepeaterMode
import me.faulk.codeplugme.model.RepeaterSource

/** Which band the results list shows. */
enum class BandFilter(val label: String) {
    ALL("All"),
    TWO_METER("2 m"),
    SEVENTY_CM("70 cm"),
}

/** Which mode the results list shows. */
enum class ModeFilter(val label: String) {
    ALL("All"),
    ANALOG("Analog"),
    DMR("DMR"),
}

/** Where to search around, with the names RadioID and RepeaterBook are queried by. */
data class ImportLocation(
    val lat: Double,
    val lon: Double,
    val country: String?,
    val state: String?,
    /** What the screen shows for this place. */
    val label: String,
)

/** Everything the import screen renders. */
data class ImportState(
    /** What is typed in the place search box. */
    val placeQuery: String = "",
    val location: ImportLocation? = null,
    val locating: Boolean = false,
    val radiusKm: Float = 40f,
    val sources: Set<RepeaterSource> = setOf(RepeaterSource.HEARHAM),
    val supportsDmr: Boolean = false,
    val hasRepeaterBookAuth: Boolean = false,
    /** Merge by default: the picks go into the open codeplug alongside what is there. */
    val mode: BuildMode = BuildMode.MERGE,
    val zoneName: String = "Nearby",
    val loading: Boolean = false,
    val building: Boolean = false,
    val error: String? = null,
    val results: List<Repeater> = emptyList(),
    /** Indices into [results]. */
    val selected: Set<Int> = emptySet(),
    val capacity: GeoCapacity? = null,
    val bandFilter: BandFilter = BandFilter.ALL,
    val modeFilter: ModeFilter = ModeFilter.ALL,
    /** True while the "too many selected" alert should be up. */
    val overLimitAlert: Boolean = false,
) {
    /** How many repeaters are picked. */
    val selectedCount: Int get() = selected.size

    /** True when more are picked than the codeplug can take. */
    val isOverLimit: Boolean
        get() = capacity != null && selectedCount > capacity.maxSelectable

    /** How many to deselect to fit, zero when it already fits. */
    val excess: Int get() = (selectedCount - (capacity?.maxSelectable ?: selectedCount)).coerceAtLeast(0)

    /**
     * The sources a fetch will actually ask.
     *
     * RadioID lists only DMR, so it is never sent for a radio without it, and
     * RepeaterBook is never sent without credentials it would refuse anyway.
     */
    val effectiveSources: List<RepeaterSource>
        get() = RepeaterSource.entries.filter { s ->
            s in sources && when (s) {
                RepeaterSource.RADIOID -> supportsDmr
                RepeaterSource.REPEATERBOOK -> hasRepeaterBookAuth
                RepeaterSource.HEARHAM -> true
            }
        }

    /** True when there is enough to search with. */
    val canFetch: Boolean
        get() = location != null && effectiveSources.isNotEmpty() && !loading && !building

    /** True when Add to codeplug and Save as new should be enabled. */
    val canBuild: Boolean
        get() = selectedCount > 0 && capacity != null && capacity.blockedReason == null &&
            !isOverLimit && zoneName.isNotBlank() && !loading && !building

    /** True when RepeaterBook's terms want its credit line shown. */
    val showsRepeaterBookCredit: Boolean
        get() = results.any { it.source == RepeaterSource.REPEATERBOOK }

    /**
     * Whether the row at [index] can be picked.
     *
     * In merge mode a repeater the base codeplug already has would be a second
     * copy of the same channel; in clean mode those channels are being replaced,
     * so it is a fresh pick like any other.
     */
    fun isSelectable(index: Int): Boolean =
        !(mode == BuildMode.MERGE && results[index].duplicate)

    /** The rows that pass both filters, with their indices into [results]. */
    val visible: List<IndexedValue<Repeater>>
        get() = results.withIndex().filter { (_, r) ->
            matchesBand(r) && when (modeFilter) {
                ModeFilter.ALL -> true
                ModeFilter.ANALOG -> r.mode == RepeaterMode.ANALOG
                ModeFilter.DMR -> r.mode == RepeaterMode.DMR
            }
        }

    /** Band by receive frequency, using each band's full radio range. */
    private fun matchesBand(r: Repeater): Boolean = when (bandFilter) {
        BandFilter.ALL -> true
        BandFilter.TWO_METER -> r.rxHz in 136_000_000L..174_000_000L
        BandFilter.SEVENTY_CM -> r.rxHz in 400_000_000L..480_000_000L
    }
}

/**
 * The repeater import's state and rules.
 *
 * One instance lives for the activity, so leaving the screen and coming back
 * brings back the same search and results. Results belong to the codeplug they
 * were fetched against: their "already in codeplug" flags only hold for it, so
 * opening the screen on another codeplug drops them but keeps the form.
 *
 * Selection lives here rather than in the screen so the rules that matter
 * (nothing picked until the user picks it, duplicates unpickable in merge,
 * never more than fit) are unit-testable without a device. The Android-only
 * parts (location, geocoding, preferences) stay in the screen and hand their
 * answers in.
 */
class ImportViewModel : ViewModel() {

    private val _state = MutableStateFlow(ImportState())

    /** What the screen renders. */
    val state: StateFlow<ImportState> = _state

    /** The codeplug the build goes on top of, as staged when the screen opened. */
    private var base: ByteArray? = null

    /** The library entry the current results were fetched against. */
    private var resultsEntryId: String? = null

    /**
     * Point the import at the open codeplug.
     *
     * Called each time the screen opens; [baseImage] is the staged image, so
     * unsaved edits, including repeaters added on an earlier visit, are part of
     * what gets built on.
     */
    fun configure(entryId: String?, baseImage: ByteArray, supportsDmr: Boolean, hasRepeaterBookAuth: Boolean) {
        base = baseImage
        if (entryId != resultsEntryId) {
            resultsEntryId = entryId
            _state.update { it.copy(results = emptyList(), selected = emptySet(), overLimitAlert = false) }
        }
        _state.update { it.copy(supportsDmr = supportsDmr, hasRepeaterBookAuth = hasRepeaterBookAuth) }
        refreshCapacity()
    }

    /** Change what is typed in the place search box. */
    fun setPlaceQuery(query: String) = _state.update { it.copy(placeQuery = query) }

    /** Record whether RepeaterBook credentials are saved now. */
    fun setRepeaterBookAuth(present: Boolean) = _state.update { it.copy(hasRepeaterBookAuth = present) }

    /** Show or clear the "finding your location" spinner. */
    fun setLocating(locating: Boolean) = _state.update { it.copy(locating = locating) }

    /** Use [location] as the search center. */
    fun setLocation(location: ImportLocation?) =
        _state.update { it.copy(location = location, locating = false) }

    /** Report a location or geocoding problem inline. */
    fun setError(message: String?) = _state.update { it.copy(error = message, locating = false) }

    /** Change the search radius. */
    fun setRadius(km: Float) = _state.update { it.copy(radiusKm = km) }

    /** Turn one source on or off. */
    fun toggleSource(source: RepeaterSource) = _state.update {
        it.copy(sources = if (source in it.sources) it.sources - source else it.sources + source)
    }

    /** Change the zone the new channels go in. */
    fun setZoneName(name: String) = _state.update { it.copy(zoneName = name) }

    /** Change the band filter. Selection is kept for rows it hides. */
    fun setBandFilter(filter: BandFilter) = _state.update { it.copy(bandFilter = filter) }

    /** Change the mode filter. Selection is kept for rows it hides. */
    fun setModeFilter(filter: ModeFilter) = _state.update { it.copy(modeFilter = filter) }

    /**
     * Switch between clean and merge.
     *
     * Merge makes duplicates unpickable, so any already picked are dropped, and
     * the capacity is different in each mode so it is fetched again.
     */
    fun setMode(mode: BuildMode) {
        val s = _state.value
        if (s.mode == mode) return
        val switched = s.copy(mode = mode)
        val kept = s.selected.filterTo(mutableSetOf()) { switched.isSelectable(it) }
        _state.value = select(switched, kept)
        refreshCapacity()
    }

    /** Pick or unpick one row. Unpickable rows ignore the tap. */
    fun toggle(index: Int) {
        val s = _state.value
        if (index !in s.results.indices || !s.isSelectable(index)) return
        _state.value = select(s, if (index in s.selected) s.selected - index else s.selected + index)
    }

    /** Pick every visible, pickable row. Rows hidden by a filter keep their state. */
    fun selectAll() {
        val s = _state.value
        val visible = s.visible.map { it.index }.filter { s.isSelectable(it) }
        _state.value = select(s, s.selected + visible)
    }

    /** Unpick everything. */
    fun selectNone() {
        _state.value = select(_state.value, emptySet())
    }

    /** Close the over-limit alert. The counter stays red until it fits. */
    fun dismissOverLimit() = _state.update { it.copy(overLimitAlert = false) }

    /** Search the chosen sources around the chosen place. */
    fun fetch(auth: RepeaterBookAuth?, cacheDir: File) {
        val s = _state.value
        val image = base ?: return
        val location = s.location ?: return
        if (!s.canFetch) return
        val query = GeoQuery(
            lat = location.lat,
            lon = location.lon,
            radiusKm = s.radiusKm.toDouble(),
            country = location.country,
            state = location.state,
            sources = s.effectiveSources,
            repeaterbook = auth.takeIf { RepeaterSource.REPEATERBOOK in s.effectiveSources },
        )
        _state.update { it.copy(loading = true, error = null) }
        viewModelScope.launch {
            try {
                onFetched(RadioCore.geoFetch(query, image, cacheDir))
            } catch (e: Throwable) {
                _state.update { it.copy(loading = false, error = e.userMessage) }
            }
        }
    }

    /**
     * Add the picks to the open codeplug through [add], the default action.
     *
     * On success the picks are in the codeplug now, so they are flagged as
     * already there and unpicked, ready for the next visit.
     */
    suspend fun addToCodeplug(add: suspend (List<Repeater>, GeoBuildOptions) -> Boolean): Boolean {
        val s = _state.value
        if (!s.canBuild) return false
        val picked = s.selected.sorted().map { s.results[it] }
        _state.update { it.copy(building = true, error = null) }
        return try {
            val ok = add(picked, GeoBuildOptions(s.mode, s.zoneName.trim()))
            if (ok) markSelectionAdded()
            ok
        } finally {
            _state.update { it.copy(building = false) }
        }
    }

    /**
     * Flag the picked rows as already in the codeplug and unpick them. A clean
     * build removed every channel that was there before, so then only the rows
     * just added count as duplicates.
     */
    internal fun markSelectionAdded() = _state.update { s ->
        val replacedAll = s.mode == BuildMode.CLEAN
        s.copy(
            results = s.results.mapIndexed { i, r ->
                val added = i in s.selected
                r.copy(duplicate = if (replacedAll) added else r.duplicate || added)
            },
            selected = emptySet(),
            overLimitAlert = false,
        )
    }

    /**
     * Build a new image from the picked repeaters, or null on failure. The
     * alternative to [addToCodeplug]: the open codeplug is not changed.
     *
     * The core checks the limit again, so a stale capacity cannot get an
     * oversized selection past it.
     */
    suspend fun build(): ByteArray? {
        val s = _state.value
        val image = base ?: return null
        if (!s.canBuild) return null
        val picked = s.selected.sorted().map { s.results[it] }
        _state.update { it.copy(building = true, error = null) }
        return try {
            RadioCore.geoBuild(image, picked, GeoBuildOptions(s.mode, s.zoneName.trim()))
        } catch (e: Throwable) {
            _state.update { it.copy(error = e.userMessage) }
            null
        } finally {
            _state.update { it.copy(building = false) }
        }
    }

    /** New results replace the old ones, and start with nothing picked. */
    internal fun onFetched(results: List<Repeater>) {
        _state.update {
            it.copy(results = results, selected = emptySet(), loading = false, overLimitAlert = false)
        }
    }

    /** A capacity answer arrived; it may push the current selection over. */
    internal fun onCapacity(capacity: GeoCapacity) {
        val s = _state.value
        _state.value = withAlert(s, s.copy(capacity = capacity))
    }

    /** Ask the core how many fit in the current mode. */
    private fun refreshCapacity() {
        val image = base ?: return
        val mode = _state.value.mode
        viewModelScope.launch {
            try {
                val capacity = RadioCore.geoCapacity(image, mode)
                // A slow answer for a mode the user has already left is stale.
                if (_state.value.mode == mode) onCapacity(capacity)
            } catch (e: Throwable) {
                _state.update { it.copy(error = e.userMessage) }
            }
        }
    }

    /** [s] with [selected] picked. */
    private fun select(s: ImportState, selected: Set<Int>): ImportState =
        withAlert(s, s.copy(selected = selected))

    /**
     * [next] with the over-limit alert raised on the step that goes over.
     *
     * Only on the crossing, not on every further tap: once the counter is red
     * the user knows, and a dialog per tap would be noise.
     */
    private fun withAlert(before: ImportState, next: ImportState): ImportState {
        val alert = when {
            !next.isOverLimit -> false
            !before.isOverLimit -> true
            else -> before.overLimitAlert
        }
        return next.copy(overLimitAlert = alert)
    }
}
