package me.faulk.codeplugme.core

import kotlinx.coroutines.runBlocking
import me.faulk.codeplugme.model.BuildMode
import me.faulk.codeplugme.model.GeoBuildOptions
import me.faulk.codeplugme.model.Repeater
import me.faulk.codeplugme.model.RepeaterMode
import me.faulk.codeplugme.model.RepeaterSource
import me.faulk.codeplugme.model.Tone
import me.faulk.codeplugme.state.TestImages
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import uniffi.codeplugme_core.ApiException

/** The repeater build through the real core, over JNA. No network involved. */
class GeoCoreTest {

    private fun repeater(call: String, rxHz: Long, txHz: Long = rxHz + 5_000_000) = Repeater(
        source = RepeaterSource.HEARHAM,
        callsign = call,
        location = "Denver",
        rxHz = rxHz,
        txHz = txHz,
        mode = RepeaterMode.ANALOG,
        txTone = Tone.Ctcss(1000),
        rxTone = Tone.None,
        lat = 39.74,
        lon = -104.99,
        distanceKm = 1.0,
    )

    @Test
    fun capacity_is_reported_for_both_modes() = runBlocking {
        val clean = RadioCore.geoCapacity(TestImages.qyt(), BuildMode.CLEAN)
        val merge = RadioCore.geoCapacity(TestImages.qyt(), BuildMode.MERGE)
        assertTrue(clean.maxSelectable > 0)
        // Merge keeps the three channels already there, so fewer fit.
        assertTrue(merge.maxSelectable < clean.maxSelectable)
    }

    @Test
    fun a_build_returns_a_new_image_with_the_picks_and_leaves_the_base_alone() = runBlocking {
        val base = TestImages.qyt()
        val before = base.copyOf()
        val built = RadioCore.geoBuild(
            base,
            listOf(repeater("W0ABC", 447_100_000), repeater("W0DEF", 449_200_000)),
            GeoBuildOptions(BuildMode.CLEAN, "Nearby"),
        )
        assertArrayEquals(before, base)
        val channels = RadioCore.dump(built).qyt!!.channels
        assertEquals(2, channels.size)
    }

    @Test
    fun a_merge_refuses_a_repeater_the_base_already_has() = runBlocking {
        // FRS-01 in TestImages.qyt() is simplex on this frequency.
        val dup = repeater("FRS", 462_562_500, txHz = 462_562_500)
        try {
            RadioCore.geoBuild(TestImages.qyt(), listOf(dup), GeoBuildOptions(BuildMode.MERGE, "Nearby"))
            fail("a duplicate in merge mode should be refused")
        } catch (_: ApiException) {
        }
    }
}
