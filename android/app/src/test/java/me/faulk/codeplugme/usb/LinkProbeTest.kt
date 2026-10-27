package me.faulk.codeplugme.usb

import java.io.IOException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.codeplugme_core.ApiException
import uniffi.codeplugme_core.baudFor

/**
 * The link rate is the one setting on this path that fails silently: get it
 * wrong and the port opens, commands go out, and the radio simply never
 * answers. It cost a full debugging round to find, so it is pinned here as well
 * as in the core, and the probe that found it is pinned with it.
 */
class LinkProbeTest {

    @Test
    fun the_app_takes_each_radios_link_rate_from_the_core() {
        assertEquals(9_600u, baudFor("qyt-kt5000"))
        assertEquals(115_200u, baudFor("anytone-d878uv2"))
    }

    @Test
    fun the_probe_tries_the_radios_own_rate_first_and_both_control_line_states() {
        val configs = RadioSession.probeConfigs(9_600)

        assertEquals(9_600, configs.first().baud)
        assertTrue("the known-good state comes first", configs.first().assertControlLines)
        assertEquals(
            "the radio's own rate is exhausted before any other is tried",
            listOf(9_600, 9_600),
            configs.take(2).map { it.baud },
        )
    }

    @Test
    fun the_probe_covers_every_known_rate_both_ways_and_repeats_none() {
        for (baud in RadioSession.KNOWN_BAUDS) {
            val configs = RadioSession.probeConfigs(baud)
            assertEquals("no duplicate attempts", configs.size, configs.distinct().size)
            assertEquals(RadioSession.KNOWN_BAUDS.toSet(), configs.map { it.baud }.toSet())
            for (rate in RadioSession.KNOWN_BAUDS) {
                assertEquals(
                    "both control line states at $rate",
                    setOf(true, false),
                    configs.filter { it.baud == rate }.map { it.assertControlLines }.toSet(),
                )
            }
        }
    }

    /** An unknown rate must still be tried, not silently replaced. */
    @Test
    fun a_rate_outside_the_known_list_is_still_tried_first() {
        val configs = RadioSession.probeConfigs(4_800)
        assertEquals(4_800, configs.first().baud)
        assertEquals(6, configs.size)
    }

    /** Several attempts at the full transfer timeout is too long to stare at. */
    @Test
    fun probe_attempts_time_out_faster_than_a_transfer_does() {
        for (config in RadioSession.probeConfigs(9_600)) {
            assertTrue(
                "${config.label} should not use the full transfer timeout",
                config.readTimeoutMs < UsbRadioTransport.READ_TIMEOUT_MS,
            )
        }
    }

    @Test
    fun a_config_labels_itself_for_the_report() {
        assertEquals("115200, DTR/RTS on", LinkConfig(115_200, true).label)
        assertEquals("9600, DTR/RTS off", LinkConfig(9_600, false).label)
    }

    /** A report line has room for an outcome, not a diagnostic block. */
    @Test
    fun a_timeout_is_summarised_as_no_reply() {
        val e = ApiException.Failed(
            "transport I/O error: radio stopped responding: got 0 of 1 bytes " +
                "before timing out\n\nLink: ProlificSerialDriver, 067b:2303"
        )
        assertEquals("no reply", RadioSession.summarise(e))
    }

    @Test
    fun another_failure_keeps_its_own_first_line_but_stays_short() {
        assertEquals("no serial driver", RadioSession.summarise(IOException("no serial driver")))

        val long = RadioSession.summarise(IOException("x".repeat(200)))
        assertTrue(long.length <= 60)
        assertTrue(long.endsWith("..."))
    }
}
