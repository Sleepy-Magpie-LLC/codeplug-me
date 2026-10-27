package me.faulk.codeplugme.ui.importer

import me.faulk.codeplugme.model.BuildMode
import me.faulk.codeplugme.model.GeoCapacity
import me.faulk.codeplugme.model.Repeater
import me.faulk.codeplugme.model.RepeaterMode
import me.faulk.codeplugme.model.RepeaterSource
import me.faulk.codeplugme.model.Tone
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import me.faulk.codeplugme.state.TestImages
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

/**
 * The import's selection rules.
 *
 * No base image is configured, so nothing here reaches the core; results and
 * capacity are handed in the way the fetch and capacity calls would.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ImportViewModelTest {

    private lateinit var vm: ImportViewModel

    /** A repeater with just the fields the rules look at varied. */
    private fun repeater(
        call: String,
        rxHz: Long = 146_940_000,
        mode: RepeaterMode = RepeaterMode.ANALOG,
        duplicate: Boolean = false,
    ) = Repeater(
        source = RepeaterSource.HEARHAM,
        callsign = call,
        location = "Denver",
        rxHz = rxHz,
        txHz = rxHz - 600_000,
        mode = mode,
        txTone = Tone.Ctcss(1000),
        rxTone = Tone.None,
        colorCode = if (mode == RepeaterMode.DMR) 1 else null,
        duplicate = duplicate,
    )

    private val results = listOf(
        repeater("W0AAA"),
        repeater("W0BBB", rxHz = 447_100_000),
        repeater("W0CCC", duplicate = true),
        repeater("W0DDD", rxHz = 446_500_000, mode = RepeaterMode.DMR),
    )

    private val s get() = vm.state.value

    @Before
    fun setUp() {
        vm = ImportViewModel()
        // These rules were written against clean mode; merge is the default and
        // has its own tests below. No base is set, so this asks the core nothing.
        vm.setMode(BuildMode.CLEAN)
        vm.onFetched(results)
        vm.onCapacity(GeoCapacity(available = 10, maxSelectable = 2))
    }

    @Test
    fun nothing_is_selected_until_the_user_picks_it() {
        assertEquals(0, s.selectedCount)
        assertFalse(s.canBuild)
    }

    @Test
    fun picking_counts_and_enables_build_while_it_fits() {
        vm.toggle(0)
        vm.toggle(1)
        assertEquals(2, s.selectedCount)
        assertFalse(s.isOverLimit)
        assertTrue(s.canBuild)

        vm.toggle(1)
        assertEquals(setOf(0), s.selected)
    }

    @Test
    fun going_over_the_limit_raises_the_alert_once_and_disables_build() {
        vm.toggle(0)
        vm.toggle(1)
        vm.toggle(2)
        assertTrue(s.isOverLimit)
        assertTrue(s.overLimitAlert)
        assertEquals(1, s.excess)
        assertFalse(s.canBuild)

        vm.dismissOverLimit()
        vm.toggle(3)
        // Still over, but the user already knows: no second dialog.
        assertFalse(s.overLimitAlert)
        assertFalse(s.canBuild)

        vm.toggle(3)
        vm.toggle(2)
        assertFalse(s.isOverLimit)
        assertTrue(s.canBuild)
    }

    @Test
    fun a_smaller_capacity_can_push_an_existing_selection_over() {
        vm.toggle(0)
        vm.toggle(1)
        vm.onCapacity(GeoCapacity(available = 1, maxSelectable = 1))
        assertTrue(s.overLimitAlert)
        assertFalse(s.canBuild)
    }

    @Test
    fun a_blocked_codeplug_cannot_build() {
        vm.toggle(0)
        vm.onCapacity(GeoCapacity(available = 0, maxSelectable = 5, blockedReason = "full"))
        assertFalse(s.canBuild)
    }

    @Test
    fun duplicates_are_unselectable_in_merge_and_dropped_on_switching() {
        vm.toggle(2)
        assertEquals(setOf(2), s.selected)

        vm.setMode(BuildMode.MERGE)
        assertTrue(s.selected.isEmpty())
        assertFalse(s.isSelectable(2))
        vm.toggle(2)
        assertTrue(s.selected.isEmpty())
    }

    @Test
    fun select_all_skips_duplicates_in_merge_and_select_none_clears() {
        vm.onCapacity(GeoCapacity(available = 10, maxSelectable = 10))
        vm.selectAll()
        assertEquals(setOf(0, 1, 2, 3), s.selected)

        vm.setMode(BuildMode.MERGE)
        vm.selectNone()
        vm.selectAll()
        assertEquals(setOf(0, 1, 3), s.selected)

        vm.selectNone()
        assertEquals(0, s.selectedCount)
    }

    @Test
    fun filters_narrow_the_list_and_select_all_to_what_is_visible() {
        vm.setBandFilter(BandFilter.TWO_METER)
        assertEquals(listOf(0, 2), s.visible.map { it.index })

        vm.setBandFilter(BandFilter.SEVENTY_CM)
        vm.setModeFilter(ModeFilter.DMR)
        assertEquals(listOf(3), s.visible.map { it.index })

        vm.selectAll()
        assertEquals(setOf(3), s.selected)

        // Hidden rows keep their state when the filter changes again.
        vm.setModeFilter(ModeFilter.ALL)
        assertEquals(listOf(1, 3), s.visible.map { it.index })
        assertEquals(setOf(3), s.selected)
    }

    @Test
    fun a_new_fetch_starts_with_nothing_picked() {
        vm.toggle(0)
        vm.onFetched(results)
        assertTrue(s.selected.isEmpty())
    }

    @Test
    fun radioid_and_unauthenticated_repeaterbook_are_never_sent() {
        vm.toggleSource(RepeaterSource.RADIOID)
        vm.toggleSource(RepeaterSource.REPEATERBOOK)
        assertEquals(listOf(RepeaterSource.HEARHAM), s.effectiveSources)

        vm.setRepeaterBookAuth(true)
        assertEquals(
            listOf(RepeaterSource.HEARHAM, RepeaterSource.REPEATERBOOK),
            s.effectiveSources,
        )
    }

    @Test
    fun the_row_shows_a_tone_for_analog_and_a_color_code_for_dmr() {
        assertEquals("100.0 Hz", signalling(results[0]))
        assertEquals("CC 1", signalling(results[3]))
        assertEquals("D023I", signalling(results[0].copy(txTone = Tone.Dcs(23, true))))
    }

    @Test
    fun coordinates_typed_as_text_are_recognised() {
        assertEquals(39.74 to -104.99, LocationLookup.parseCoordinates("39.74, -104.99"))
        assertEquals(39.74 to -104.99, LocationLookup.parseCoordinates("39.74 -104.99"))
        assertEquals(null, LocationLookup.parseCoordinates("80202"))
        assertEquals(null, LocationLookup.parseCoordinates("95, 10"))
    }

    // ----------------------------------------------------- add to codeplug

    @Test
    fun merge_is_the_default_mode() {
        assertEquals(BuildMode.MERGE, ImportViewModel().state.value.mode)
    }

    @Test
    fun a_merge_add_marks_the_picks_as_already_in_the_codeplug() = runBlocking {
        vm.setMode(BuildMode.MERGE)
        vm.toggle(0)
        var sent: List<Repeater> = emptyList()
        val ok = vm.addToCodeplug { picked, options ->
            sent = picked
            assertEquals(BuildMode.MERGE, options.mode)
            true
        }
        assertTrue(ok)
        assertEquals(listOf("W0AAA"), sent.map { it.callsign })
        assertEquals(listOf(true, false, true, false), s.results.map { it.duplicate })
        assertTrue(s.selected.isEmpty())
        assertFalse(s.isSelectable(0))
    }

    @Test
    fun a_clean_add_leaves_only_the_picks_as_duplicates() = runBlocking {
        vm.toggle(1)
        assertTrue(vm.addToCodeplug { _, _ -> true })
        assertEquals(listOf(false, true, false, false), s.results.map { it.duplicate })
    }

    @Test
    fun a_failed_add_keeps_the_selection() = runBlocking {
        vm.toggle(0)
        assertFalse(vm.addToCodeplug { _, _ -> false })
        assertEquals(setOf(0), s.selected)
        assertFalse(s.results[0].duplicate)
    }

    // ------------------------------------------------------- session state

    @Test
    fun results_survive_for_the_same_codeplug_and_drop_for_another() {
        Dispatchers.setMain(UnconfinedTestDispatcher())
        try {
            val image = TestImages.qyt()
            vm.configure("a", image, supportsDmr = false, hasRepeaterBookAuth = false)
            vm.onFetched(results)
            vm.setPlaceQuery("Denver, CO")
            vm.setRadius(60f)
            vm.toggle(0)

            // Leaving and coming back to the same codeplug changes nothing.
            vm.configure("a", image, supportsDmr = false, hasRepeaterBookAuth = false)
            assertEquals(4, s.results.size)
            assertEquals(setOf(0), s.selected)

            // Another codeplug: results go, the search form stays.
            vm.configure("b", image, supportsDmr = false, hasRepeaterBookAuth = false)
            assertTrue(s.results.isEmpty())
            assertTrue(s.selected.isEmpty())
            assertEquals("Denver, CO", s.placeQuery)
            assertEquals(60f, s.radiusKm)
        } finally {
            Dispatchers.resetMain()
        }
    }

    @Test
    fun bare_tokens_get_a_warning_and_prefixed_ones_do_not() {
        assertEquals(null, repeaterBookTokenWarning(""))
        assertEquals(null, repeaterBookTokenWarning("rbuapp_abc"))
        assertEquals(null, repeaterBookTokenWarning(" app_abc "))
        assertTrue(repeaterBookTokenWarning("ad56".repeat(16)) != null)
    }
}
