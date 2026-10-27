package me.faulk.codeplugme.ui

import me.faulk.codeplugme.model.DumpChannel
import me.faulk.codeplugme.model.DumpQytChannel
import me.faulk.codeplugme.model.Tone
import me.faulk.codeplugme.ui.channels.channelFrequencyOrder
import me.faulk.codeplugme.ui.channels.duplicateChannelSlots
import me.faulk.codeplugme.ui.channels.duplicateLabel
import me.faulk.codeplugme.ui.channels.duplicateQytChannelSlots
import me.faulk.codeplugme.ui.channels.qytChannelFrequencyOrder
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Duplicate-channel detection behind the channel lists' highlight. Names are
 * ignored; frequencies must match, plus color code and slot for DMR.
 */
class ChannelDuplicateTest {

    private fun channel(
        index: Int,
        name: String = "REPEATER",
        rx: Long = 442_000_000,
        tx: Long = 447_000_000,
        mode: String = "Digital",
        colorCode: Int = 1,
        timeSlot: Int = 1,
    ) = DumpChannel(
        index = index, name = name, rxFrequencyHz = rx, txFrequencyHz = tx, mode = mode,
        power = "High", bandwidth = "Narrow", colorCode = colorCode, timeSlot = timeSlot,
        contactIndex = 0, radioIdIndex = 0, groupListIndex = 255,
        rxSignalingMode = "None", txSignalingMode = "None", rxCtcss = 0, txCtcss = 0,
        rxDcs = 0, txDcs = 0, squelchMode = "Carrier", optionalSignaling = "Off",
        admit = "Always", scanListIndex = 255, dtmfIdIndex = 0, twoToneIdIndex = 0,
        fiveToneIdIndex = 0, twoToneDecodeIndex = 0, rxOnly = false, talkAround = false,
        callConfirm = false, workAlone = false, simplexTdma = false, rxAprs = false,
        repeaterMode = "Off", txOffsetHz = 0,
    )

    private fun qyt(index: Int, name: String = "FRS-01", tx: Long? = 462_562_500) = DumpQytChannel(
        index = index, name = name, rxFrequencyHz = 462_562_500, txFrequencyHz = tx,
        rxTone = Tone.None, txTone = Tone.None,
    )

    @Test
    fun `digital twins under different names flag each other`() {
        val dupes = duplicateChannelSlots(
            listOf(channel(0, name = "A"), channel(5, name = "B"), channel(9, rx = 146_520_000)),
        )
        assertEquals(mapOf(0 to listOf(5), 5 to listOf(0)), dupes)
    }

    @Test
    fun `a different color code or time slot is a different DMR channel`() {
        assertTrue(duplicateChannelSlots(listOf(channel(0), channel(1, colorCode = 2))).isEmpty())
        assertTrue(duplicateChannelSlots(listOf(channel(0), channel(1, timeSlot = 2))).isEmpty())
    }

    @Test
    fun `analog channels ignore color code and slot`() {
        val dupes = duplicateChannelSlots(
            listOf(channel(0, mode = "Analog"), channel(1, mode = "Analog", colorCode = 7, timeSlot = 2)),
        )
        assertEquals(mapOf(0 to listOf(1), 1 to listOf(0)), dupes)
    }

    @Test
    fun `analog and digital on the same pair are not duplicates`() {
        assertTrue(duplicateChannelSlots(listOf(channel(0, mode = "Analog"), channel(1))).isEmpty())
    }

    @Test
    fun `a mixed mode does not match the pure mode on the same pair`() {
        assertTrue(duplicateChannelSlots(listOf(channel(0), channel(1, mode = "MixedDigital"))).isEmpty())
        assertTrue(duplicateChannelSlots(listOf(channel(0, mode = "Analog"), channel(1, mode = "MixedAnalog"))).isEmpty())
        assertEquals(
            mapOf(0 to listOf(1), 1 to listOf(0)),
            duplicateChannelSlots(listOf(channel(0, mode = "MixedDigital"), channel(1, mode = "MixedDigital"))),
        )
    }

    @Test
    fun `a different TX is not a duplicate`() {
        assertTrue(duplicateChannelSlots(listOf(channel(0), channel(1, tx = 442_000_000))).isEmpty())
    }

    @Test
    fun `a three-way group lists the others in slot order`() {
        val dupes = duplicateChannelSlots(listOf(channel(8), channel(2), channel(4)))
        assertEquals(listOf(4, 8), dupes[2])
        assertEquals("Duplicate of channel 5, 9", duplicateLabel(dupes.getValue(2)))
    }

    @Test
    fun `QYT matches on RX and TX, receive-only included`() {
        val dupes = duplicateQytChannelSlots(
            listOf(qyt(0), qyt(3, name = "FAMILY"), qyt(4, tx = 467_562_500), qyt(5, tx = null), qyt(6, tx = null)),
        )
        assertEquals(mapOf(0 to listOf(3), 3 to listOf(0), 5 to listOf(6), 6 to listOf(5)), dupes)
    }

    @Test
    fun `a different tone splits analog twins but not digital ones`() {
        val toned = channel(0, mode = "Analog").copy(txSignalingMode = "Ctcss", txCtcss = 12)
        assertTrue(duplicateChannelSlots(listOf(toned, toned.copy(index = 1, txCtcss = 13))).isEmpty())
        assertTrue(duplicateChannelSlots(listOf(toned, channel(1, mode = "Analog"))).isEmpty())

        val dcs = channel(0, mode = "Analog").copy(rxSignalingMode = "Dcs", rxDcs = 23, rxCtcss = 4)
        assertEquals(
            "the CTCSS index is ignored under DCS",
            mapOf(0 to listOf(1), 1 to listOf(0)),
            duplicateChannelSlots(listOf(dcs, dcs.copy(index = 1, rxCtcss = 9))),
        )

        val digital = channel(0).copy(txSignalingMode = "Ctcss", txCtcss = 12)
        assertEquals(
            mapOf(0 to listOf(1), 1 to listOf(0)),
            duplicateChannelSlots(listOf(digital, channel(1).copy(txSignalingMode = "Dcs", txDcs = 23))),
        )
    }

    @Test
    fun `QYT tones split otherwise identical channels`() {
        val toned = qyt(0).copy(txTone = Tone.Ctcss(1000))
        val dupes = duplicateQytChannelSlots(
            listOf(toned, toned.copy(index = 1), toned.copy(index = 2, txTone = Tone.Ctcss(1035)),
                qyt(3).copy(rxTone = Tone.Dcs(23, false))),
        )
        assertEquals(mapOf(0 to listOf(1), 1 to listOf(0)), dupes)
    }

    @Test
    fun `duplicates-only sort groups twins by RX, then TX, then slot`() {
        val sorted = listOf(channel(9, rx = 146_520_000, tx = 146_520_000), channel(7), channel(1, tx = 446_000_000), channel(3))
            .sortedWith(channelFrequencyOrder)
        assertEquals(listOf(9, 1, 3, 7), sorted.map { it.index })
    }

    @Test
    fun `QYT duplicates-only sort puts receive-only first within an RX`() {
        val sorted = listOf(qyt(4), qyt(2, tx = null), qyt(1)).sortedWith(qytChannelFrequencyOrder)
        assertEquals(listOf(2, 1, 4), sorted.map { it.index })
    }
}
