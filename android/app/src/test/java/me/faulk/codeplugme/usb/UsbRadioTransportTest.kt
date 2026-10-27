package me.faulk.codeplugme.usb

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import me.faulk.codeplugme.core.userMessage
import uniffi.codeplugme_core.ApiException

/**
 * A [SerialLink] that behaves the way an Android bulk endpoint really does.
 *
 * The point of the fake is the packet discipline, not the bytes: a read hands
 * back at most one packet, and anything of that packet which does not fit the
 * caller's buffer is **discarded**, exactly as `bulkTransfer` discards it. A
 * transport that asks USB for one byte at a time therefore loses data here for
 * the same reason it lost data on the radio.
 */
private class FakeLink(
    override val packetSize: Int = 64,
    replies: List<ByteArray> = emptyList(),
) : SerialLink {

    /** Packets still queued for delivery, in order. */
    private val packets = ArrayDeque(replies.flatMap { it.toPackets(packetSize) })

    /** Everything the transport has written, concatenated. */
    val written = mutableListOf<Byte>()

    /** How many times the endpoint was read. */
    var reads = 0
        private set

    override fun write(bytes: ByteArray, timeoutMs: Int) {
        written.addAll(bytes.toList())
    }

    override fun read(dest: ByteArray, timeoutMs: Int): Int {
        reads++
        val packet = packets.removeFirstOrNull() ?: return 0
        val n = minOf(packet.size, dest.size)
        packet.copyInto(dest, 0, 0, n)
        return n
    }
}

/** Split a reply into the packets a `packetSize` endpoint would deliver. */
private fun ByteArray.toPackets(packetSize: Int): List<ByteArray> =
    if (isEmpty()) emptyList() else toList().chunked(packetSize).map { it.toByteArray() }

class UsbRadioTransportTest {

    /**
     * The KT-5000's block-read reply: `06 58 AH AL LL` then 64 data bytes, 69
     * in all, and the protocol peeks the first byte before committing to the
     * rest. This is the exact sequence that failed on hardware.
     */
    @Test
    fun a_one_byte_peek_does_not_lose_the_rest_of_the_packet() {
        val data = ByteArray(0x40) { (it and 0xFF).toByte() }
        val reply = byteArrayOf(0x06, 0x58, 0x00, 0x40, 0x40) + data
        val link = FakeLink(replies = listOf(reply))
        val transport = UsbRadioTransport(link)

        assertArrayEquals(byteArrayOf(0x06), transport.readExact(1u))
        assertArrayEquals(byteArrayOf(0x58), transport.readExact(1u))
        assertArrayEquals(byteArrayOf(0x00, 0x40, 0x40), transport.readExact(3u))
        assertArrayEquals(data, transport.readExact(0x40u))
    }

    /** A reply spanning two packets is reassembled across the boundary. */
    @Test
    fun a_read_spanning_a_packet_boundary_is_reassembled() {
        val reply = ByteArray(69) { (it and 0xFF).toByte() }
        val link = FakeLink(replies = listOf(reply))

        assertArrayEquals(reply, UsbRadioTransport(link).readExact(69u))
        assertEquals("one read per packet", 2, link.reads)
    }

    /** The QYT identity body: a peek, then the remaining 50 bytes. */
    @Test
    fun the_qyt_identity_body_survives_its_lead_byte_peek() {
        val body = ByteArray(51) { if (it == 0) 0x06 else (it and 0xFF).toByte() }
        val transport = UsbRadioTransport(FakeLink(replies = listOf(body)))

        assertEquals(0x06.toByte(), transport.readExact(1u)[0])
        assertArrayEquals(body.copyOfRange(1, 51), transport.readExact(50u))
    }

    /** Leftovers must not survive into a read that is issued after them. */
    @Test
    fun buffered_bytes_are_consumed_before_a_new_packet_is_fetched() {
        val link = FakeLink(replies = listOf(byteArrayOf(1, 2, 3), byteArrayOf(4, 5)))
        val transport = UsbRadioTransport(link)

        assertArrayEquals(byteArrayOf(1), transport.readExact(1u))
        assertArrayEquals(byteArrayOf(2, 3, 4), transport.readExact(3u))
        assertArrayEquals(byteArrayOf(5), transport.readExact(1u))
        assertEquals(2, link.reads)
    }

    /** A radio that goes quiet must fail, never return a short buffer. */
    @Test
    fun a_silent_radio_fails_with_how_far_the_reply_got() {
        val link = FakeLink(replies = listOf(byteArrayOf(0x06, 0x58)))
        val transport = UsbRadioTransport(link)

        val e = try {
            transport.readExact(5u)
            null
        } catch (e: ApiException.Failed) {
            e
        }
        assertTrue("expected a timeout failure", e != null)
        // Read through userMessage, not `message`: UniFFI renders the latter as
        // "msg=...". This is the text the user is shown.
        assertTrue(e!!.userMessage, e.userMessage.contains("got 2 of 5 bytes"))
    }

    /**
     * The traffic counters are what a failure message uses to say whether the
     * command reached the wire, so they have to count the wire, not the caller.
     */
    @Test
    fun traffic_is_counted_as_the_link_saw_it() {
        val link = FakeLink(replies = listOf(ByteArray(69)))
        val transport = UsbRadioTransport(link)

        assertEquals("sent 0 B, received 0 B", transport.describeTraffic())

        transport.writeAll(ByteArray(8))
        assertEquals("sent 8 B, received 0 B", transport.describeTraffic())

        // One byte asked for, a whole 64-byte packet received: the counter
        // reports the packet, because that is what crossed the link.
        transport.readExact(1u)
        assertEquals("sent 8 B, received 64 B", transport.describeTraffic())
    }

    /** A handshake that goes out and is never answered is the reported case. */
    @Test
    fun an_unanswered_command_is_reported_as_sent_but_nothing_back() {
        val link = FakeLink()
        val transport = UsbRadioTransport(link)
        transport.writeAll(ByteArray(8))

        try {
            transport.readExact(1u)
        } catch (_: ApiException.Failed) {
        }
        assertEquals("sent 8 B, received 0 B", transport.describeTraffic())
    }

    /** Writes go out whole and untouched. */
    @Test
    fun a_write_reaches_the_link_verbatim() {
        val link = FakeLink()
        UsbRadioTransport(link).writeAll(byteArrayOf(0x55, 0x20, 0x21))
        assertEquals(listOf<Byte>(0x55, 0x20, 0x21), link.written)
    }
}
