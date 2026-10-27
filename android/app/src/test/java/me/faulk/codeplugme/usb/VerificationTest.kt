package me.faulk.codeplugme.usb

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.codeplugme_core.supportsWriteVerify

/**
 * How a post-write read-back is reported.
 *
 * The KT-5000 acknowledges each write block on receipt and commits it
 * afterwards, so an acknowledgement is not proof the byte stuck. Reading the
 * radio back is the only proof, and these are the rules for saying what it
 * found.
 */
class VerificationTest {

    @Test
    fun a_run_reads_as_a_place_in_the_radio_not_as_an_offset() {
        assertEquals(
            "16 bytes at 0x1A50 in channel names",
            ByteRun(0x1A50, 16, "channel names").label,
        )
        // Singular, because "1 bytes" in a failure dialog reads as a bug.
        assertEquals("1 byte at 0x0E11 in settings header", ByteRun(0x0E11, 1, "settings header").label)
    }

    @Test
    fun verification_is_attempted_only_where_it_would_mean_something() {
        // The KT-5000 write sends the image verbatim, so a read-back is
        // directly comparable. The AnyTone write is a sector read-modify-write
        // that leaves unmodelled bytes as the radio had them, so a whole-image
        // comparison there would report differences that are not failures.
        assertTrue(supportsWriteVerify("qyt-kt5000"))
        assertTrue(!supportsWriteVerify("anytone-d878uv2"))
    }

    @Test
    fun a_write_state_carries_what_the_read_back_found() {
        val mismatch = TransferState.WriteComplete(
            "usb-1",
            Verification.Mismatched(listOf(ByteRun(0x1A50, 16, "channel names"))),
        )
        assertTrue(mismatch.verification is Verification.Mismatched)
        assertEquals(
            listOf("16 bytes at 0x1A50 in channel names"),
            (mismatch.verification as Verification.Mismatched).runs.map { it.label },
        )

        val clean = TransferState.WriteComplete("usb-1", Verification.Clean)
        assertEquals(Verification.Clean, clean.verification)
    }
}
