package me.faulk.codeplugme.usb

import me.faulk.codeplugme.model.Capabilities
import me.faulk.codeplugme.model.SupportedModel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The rules deciding whether a codeplug may be written to a connected radio.
 *
 * This is the layer in front of the core's own guard, not a replacement for it:
 * `writeImage` still handshakes the radio and refuses on a mismatch before
 * emitting a byte. What is tested here is that the common cases surface as a
 * sentence the user can act on, and that a write the app knows will fail is
 * never offered in the first place.
 */
class WriteTargetingTest {

    private val caps = Capabilities(
        sections = emptyList(),
        channelCapacity = 0,
        channelNameLength = 0,
        supportsDmr = false,
        toneEncoding = "table_index",
        powerOptions = emptyList(),
        bandwidthOptions = emptyList(),
    )

    private val models = listOf(
        SupportedModel("anytone-d878uv2", "AnyTone AT-D878UVII Plus", 1_789_328, caps),
        SupportedModel("qyt-kt5000", "QYT KT-5000", 12_800, caps),
    )

    /** An AnyTone, attributable from its GD32 bridge's USB identity. */
    private val anytone = RadioDevice(
        key = "/dev/bus/usb/001/004",
        vendorId = 0x28e9,
        productId = 0x018a,
        productName = "AnyTone",
        modelId = "anytone-d878uv2",
    )

    /**
     * The KT-5000's cable: a stock Prolific PL2303 that names no radio. This is
     * the normal case for that radio, not an edge case.
     */
    private val unattributed = RadioDevice(
        key = "/dev/bus/usb/001/005",
        vendorId = 0x067b,
        productId = 0x2303,
        productName = "USB-Serial Controller",
        modelId = null,
    )

    @Test
    fun an_attributed_device_needs_no_manual_pick() {
        assertEquals("anytone-d878uv2", effectiveModelId(anytone, manual = null))
    }

    @Test
    fun an_unattributed_device_has_no_model_until_the_user_names_one() {
        assertNull(effectiveModelId(unattributed, manual = null))
        val manual = ManualModelSelection(unattributed.key, "qyt-kt5000")
        assertEquals("qyt-kt5000", effectiveModelId(unattributed, manual))
    }

    @Test
    fun a_manual_pick_does_not_leak_to_a_different_adapter() {
        // Picking "KT-5000" for one cable says nothing about the next one
        // plugged in. Carrying it over would be exactly the silent
        // misattribution the core refuses to make on its own.
        val manual = ManualModelSelection(unattributed.key, "qyt-kt5000")
        val other = unattributed.copy(key = "/dev/bus/usb/001/009")
        assertNull(effectiveModelId(other, manual))
    }

    @Test
    fun an_attributed_device_ignores_a_manual_pick_that_contradicts_it() {
        val manual = ManualModelSelection(anytone.key, "qyt-kt5000")
        assertEquals("anytone-d878uv2", effectiveModelId(anytone, manual))
    }

    @Test
    fun a_matching_model_may_be_written() {
        assertNull(writeRefusalReason(anytone, "anytone-d878uv2", null, models))
    }

    @Test
    fun a_mismatched_model_is_refused_by_name() {
        val reason = writeRefusalReason(anytone, "qyt-kt5000", null, models)
        assertNotNull(reason)
        assertTrue(reason!!.contains("QYT KT-5000"))
        assertTrue(reason.contains("AnyTone AT-D878UVII Plus"))
    }

    @Test
    fun an_unidentified_adapter_is_refused_with_an_instruction_not_just_a_complaint() {
        val reason = writeRefusalReason(unattributed, "qyt-kt5000", null, models)
        assertNotNull(reason)
        assertTrue(reason!!.contains("QYT KT-5000"))
    }

    @Test
    fun no_open_codeplug_is_refused() {
        assertNotNull(writeRefusalReason(anytone, null, null, models))
    }

    @Test
    fun only_eligible_devices_are_offered_as_write_targets() {
        val devices = listOf(anytone, unattributed)
        // Nothing may receive a QYT codeplug until the PL2303 is named.
        assertTrue(writableTargets(devices, "qyt-kt5000", null, models).isEmpty())

        val manual = ManualModelSelection(unattributed.key, "qyt-kt5000")
        val targets = writableTargets(devices, "qyt-kt5000", manual, models)
        assertEquals(listOf(unattributed), targets)
    }

    @Test
    fun an_unknown_model_id_still_produces_a_readable_refusal() {
        // A codeplug for a radio this build has no name for must not render a
        // sentence with a blank where the radio should be.
        val reason = writeRefusalReason(anytone, "some-future-radio", null, models)
        assertNotNull(reason)
        assertTrue(reason!!.contains("some-future-radio"))
    }
}
