package me.faulk.codeplugme.usb

import com.hoho.android.usbserial.driver.UsbSerialPort
import uniffi.codeplugme_core.ApiException
import uniffi.codeplugme_core.ByteTransport

/**
 * One USB bulk endpoint pair, reduced to the two calls this file needs.
 *
 * This exists so the buffering in [UsbRadioTransport] can be tested on a host
 * JVM against a fake that reproduces real bulk-endpoint packet behaviour. There
 * is no other implementation than [UsbSerialPortLink] in the app.
 */
interface SerialLink {

    /** Bytes the endpoint delivers in a single bulk packet, `wMaxPacketSize`. */
    val packetSize: Int

    /** Push the whole buffer out. */
    fun write(bytes: ByteArray, timeoutMs: Int)

    /**
     * Receive at most one bulk packet into [dest], returning its length, or 0
     * (or less) on timeout. [dest] is always at least [packetSize] long.
     */
    fun read(dest: ByteArray, timeoutMs: Int): Int
}

/** [SerialLink] over a usb-serial-for-android port. */
class UsbSerialPortLink(private val port: UsbSerialPort) : SerialLink {

    /**
     * The read endpoint's packet size, defaulting to the 64 bytes every
     * full-speed bulk endpoint uses when the endpoint cannot be inspected.
     */
    override val packetSize: Int =
        runCatching { port.readEndpoint?.maxPacketSize ?: 0 }.getOrDefault(0)
            .takeIf { it > 0 } ?: DEFAULT_PACKET_SIZE

    override fun write(bytes: ByteArray, timeoutMs: Int) = port.write(bytes, timeoutMs)

    override fun read(dest: ByteArray, timeoutMs: Int): Int = port.read(dest, timeoutMs)

    private companion object {
        const val DEFAULT_PACKET_SIZE = 64
    }
}

/**
 * The byte link to a radio, backed by a usb-serial-for-android port.
 *
 * This is the Android half of the seam that made the port possible: everything
 * above it in codeplugme-core talks only to a two-method transport, so the entire
 * protocol and codeplug stack runs unchanged here. The Rust `SerialTransport`
 * is its desktop twin, and this class deliberately reproduces its semantics
 * rather than inventing new ones.
 *
 * # Why the reads are buffered
 *
 * A host serial port hands out a byte stream; a USB bulk endpoint hands out
 * whole packets, and Android throws away whatever does not fit the buffer you
 * asked with. `UsbDeviceConnection.bulkTransfer` is given exactly the length
 * the caller passed, so asking for one byte of a 69-byte reply consumes the
 * radio's whole first packet and returns one byte of it. The other 63 are gone.
 *
 * That is fatal for the KT-5000, whose every block read begins by peeking a
 * single lead byte (the reply's leading `0x06` is optional on the wire, so the
 * protocol has to look before it can commit). On hardware it surfaced as the
 * first read failing with a timeout: the peek ate the reply and nothing was
 * left to satisfy the rest of it. The AnyTone survived only by accident, since
 * its transfer path happens to read every reply in one exactly-sized call.
 *
 * So this class never lets the protocol layer's read size reach USB. It always
 * asks the endpoint for a full packet, keeps what the caller did not want, and
 * serves the next read from that. The Rust side is unchanged and stays free to
 * read a byte at a time.
 *
 * Both radios run at 115200 8-N-1 with no flow control. That is nominal for the
 * AnyTone, whose CDC-ACM bridge ignores line coding, and real for the KT-5000,
 * whose cable is an actual PL2303 UART.
 *
 * Not thread-safe by design: exactly one transfer owns a port at a time. See
 * [RadioSession] for why the open/close lifecycle is as tight as it is.
 */
class UsbRadioTransport(
    private val link: SerialLink,
    private val readTimeoutMs: Int = READ_TIMEOUT_MS,
    private val writeTimeoutMs: Int = WRITE_TIMEOUT_MS,
) : ByteTransport {

    constructor(
        port: UsbSerialPort,
        readTimeoutMs: Int = READ_TIMEOUT_MS,
        writeTimeoutMs: Int = WRITE_TIMEOUT_MS,
    ) : this(UsbSerialPortLink(port), readTimeoutMs, writeTimeoutMs)

    /** Bytes received from the endpoint that no read has claimed yet. */
    private val pending = ByteArray(link.packetSize)

    /**
     * Bytes handed to the endpoint and bytes taken off it, for [describeTraffic].
     *
     * They exist to answer one question a timeout cannot answer on its own: did
     * the command actually reach the wire, or did the link never carry anything?
     * That is the difference between a radio that is not listening and a bridge
     * that is not transmitting, and it is not otherwise visible from the phone.
     */
    private var bytesSent = 0

    /** Bytes taken off the link this session. Read by [RadioSession]. */
    var bytesReceived = 0
        private set

    /** First unclaimed byte in [pending]. */
    private var pendingStart = 0

    /** One past the last valid byte in [pending]. */
    private var pendingEnd = 0

    /** Push the whole buffer to the radio. */
    override fun writeAll(bytes: ByteArray) {
        try {
            link.write(bytes, writeTimeoutMs)
            bytesSent += bytes.size
        } catch (e: Exception) {
            throw ApiException.Failed("serial write failed: ${e.message ?: e}")
        }
    }

    /**
     * Block until exactly [len] bytes have arrived.
     *
     * Bytes come from the leftovers of the last packet first, then from fresh
     * packets. A packet that never arrives is a timeout and fails rather than
     * spinning, which is what the Rust `SerialTransport` does and what every
     * protocol error path here expects: a short reply must surface as an error,
     * never as a partial success.
     */
    override fun readExact(len: UInt): ByteArray {
        val want = len.toInt()
        val out = ByteArray(want)
        var filled = 0
        while (filled < want) {
            if (pendingStart == pendingEnd) receivePacket(filled, want)
            val take = minOf(pendingEnd - pendingStart, want - filled)
            pending.copyInto(out, filled, pendingStart, pendingStart + take)
            pendingStart += take
            filled += take
        }
        return out
    }

    /**
     * Refill [pending] with one packet from the endpoint.
     *
     * [filled] and [want] only shape the timeout message, so the user is told
     * how far the reply got before the radio went quiet.
     */
    private fun receivePacket(filled: Int, want: Int) {
        val n = try {
            link.read(pending, readTimeoutMs)
        } catch (e: Exception) {
            throw ApiException.Failed("serial read failed: ${e.message ?: e}")
        }
        if (n <= 0) {
            throw ApiException.Failed(
                "radio stopped responding: got $filled of $want bytes before timing out"
            )
        }
        pendingStart = 0
        pendingEnd = n
        bytesReceived += n
    }

    /** How much has crossed the link so far, for a failure message. */
    fun describeTraffic(): String = "sent $bytesSent B, received $bytesReceived B"


    companion object {
        /** Per-read timeout, matching codeplugme-core's desktop serial timeout. */
        const val READ_TIMEOUT_MS = 2_000

        /** Per-write timeout. Writes are small and should never approach this. */
        const val WRITE_TIMEOUT_MS = 2_000

        /** The only baud both radios are known to work at. */
        const val BAUD = 115_200
    }
}
