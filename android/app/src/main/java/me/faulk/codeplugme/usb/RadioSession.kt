package me.faulk.codeplugme.usb

import android.hardware.usb.UsbDevice
import android.hardware.usb.UsbDeviceConnection
import android.hardware.usb.UsbManager
import com.hoho.android.usbserial.driver.UsbSerialDriver
import com.hoho.android.usbserial.driver.UsbSerialPort
import com.hoho.android.usbserial.driver.UsbSerialProber
import me.faulk.codeplugme.core.userMessage
import uniffi.codeplugme_core.ApiException
import uniffi.codeplugme_core.baudFor
import uniffi.codeplugme_core.identify

/**
 * How the USB bridge is configured for one session.
 *
 * These are the settings the desktop never has to think about, because there
 * the OS driver owns the chip and the app just opens a `/dev` node. On Android
 * the app owns them, which means it can also get them wrong, which is why
 * [RadioSession.probe] exists.
 */
data class LinkConfig(
    /** Line rate. A property of the radio; see the core's `Model::baud`. */
    val baud: Int,
    /** Whether to drive DTR and RTS high. Neither radio reads them. */
    val assertControlLines: Boolean = true,
    /** Per-read timeout, matching codeplugme-core's desktop serial timeout. */
    val readTimeoutMs: Int = UsbRadioTransport.READ_TIMEOUT_MS,
) {
    /** Short label for a probe report line. */
    val label: String
        get() = "$baud, DTR/RTS ${if (assertControlLines) "on" else "off"}"
}

/**
 * Opens a radio's USB serial port for exactly one operation and closes it again.
 *
 * The tight lifecycle is not tidiness, it is a hardware requirement, and
 * getting it wrong broke every write on real hardware once already (see the
 * comment on `write_verified_image` in codeplugme-core's driver.rs):
 *
 * - The QYT KT-5000 accepts **exactly one handshake per open link**. An
 *   identify call followed by a read or write is two sessions, and the second
 *   one is refused.
 * - The KT-5000 has no exit command and **reboots when the port closes**, so
 *   there is no such thing as reopening to "check something first".
 *
 * So: one open, one complete operation, one close. Permission must already be
 * granted before [use] is called, because the permission dialog is asynchronous
 * and would otherwise land in the middle of a session.
 */
class RadioSession(private val usbManager: UsbManager) {

    /**
     * Bytes the last session received off the link, or -1 if none has run.
     *
     * Read by the caller to tell a link that never spoke from a transfer that
     * started fine and failed partway, which are different faults and want
     * different follow-up. See [RadioTransferService].
     */
    var bytesReceivedLastSession: Int = -1
        private set

    /**
     * Open [device], run [block] against a transport bound to it, and close.
     *
     * The port is closed on every path, including failure, so a crashed
     * transfer cannot leave the device claimed. Failures are re-thrown with
     * [describeLink] appended, because a bare timeout cannot be diagnosed from
     * a phone screen.
     */
    fun <T> use(device: RadioDevice, modelId: String, block: (UsbRadioTransport) -> T): T {
        val config = LinkConfig(baud = linkBaud(modelId))
        return open(device, config) { driver, handle, port, transport ->
            try {
                block(transport)
            } catch (e: Throwable) {
                throw ApiException.Failed(
                    "${e.userMessage}\n\n${describeLink(driver, handle, port, config, transport)}"
                )
            }
        }
    }

    /**
     * Handshake with the radio across every link configuration worth trying,
     * and report what each one got back.
     *
     * This exists because the app cannot see its own bridge. A read that fails
     * with "nothing came back" is consistent with a wrong line rate, a control
     * line the cable uses for something, and a bridge chip being driven with
     * the wrong register layout, and nothing in the failure distinguishes them.
     * Trying them settles it in one run instead of one round trip each.
     *
     * **This never writes to the radio.** Every attempt is `identify`, which is
     * a handshake and a read, so running it after a failed write cannot make
     * the radio's state any worse than the failure already did. Each attempt is
     * its own open and close, which is what the KT-5000's one-handshake-per-link
     * rule requires; it reboots between attempts, which is fine and expected.
     */
    fun probe(device: RadioDevice, modelId: String): String {
        val lines = probeConfigs(linkBaud(modelId)).map { config ->
            val outcome = try {
                open(device, config) { _, _, _, transport -> identify(transport, modelId) }
                    .ifBlank { "replied, but named nothing" }
            } catch (e: Throwable) {
                summarise(e)
            }
            "  ${config.label.padEnd(22)} $outcome"
        }
        return "Link probe, one fresh session each:\n" + lines.joinToString("\n")
    }

    /**
     * The rate the core says this radio's link runs at.
     *
     * Falls back to the AnyTone's nominal rate only if the core cannot name the
     * model, which means the transfer is about to fail on that anyway.
     */
    private fun linkBaud(modelId: String): Int =
        runCatching { baudFor(modelId).toInt() }.getOrDefault(UsbRadioTransport.BAUD)

    /**
     * Open the port, configure the bridge, hand everything to [block], close.
     *
     * The block receives the driver, device, and port alongside the transport
     * so a caller can describe the link it just failed on without this function
     * having to know what a failure looks like.
     */
    private fun <T> open(
        device: RadioDevice,
        config: LinkConfig,
        block: (UsbSerialDriver, UsbDevice, UsbSerialPort, UsbRadioTransport) -> T,
    ): T {
        val handle = device.device
            ?: throw ApiException.Failed("${device.label} is no longer attached.")
        if (!usbManager.hasPermission(handle)) {
            throw ApiException.Failed(
                "USB permission for ${device.label} was not granted. " +
                    "Grant it, then try again."
            )
        }

        val driver: UsbSerialDriver = UsbSerialProber.getDefaultProber()
            .probeDevice(handle)
            ?: throw ApiException.Failed(
                "No serial driver matches ${device.label}. A radio needs a " +
                    "CDC-ACM or a supported USB-serial bridge."
            )

        val connection: UsbDeviceConnection = usbManager.openDevice(handle)
            ?: throw ApiException.Failed("Could not open ${device.label}.")

        val port: UsbSerialPort = driver.ports.firstOrNull()
            ?: run {
                connection.close()
                throw ApiException.Failed("${device.label} exposes no serial port.")
            }

        bytesReceivedLastSession = -1
        var transport: UsbRadioTransport? = null
        try {
            port.open(connection)
            port.setParameters(
                config.baud,
                8,
                UsbSerialPort.STOPBITS_1,
                UsbSerialPort.PARITY_NONE,
            )
            // Both radios run without flow control and neither reads DTR or RTS,
            // so these are set only to match what a desktop OS driver does to a
            // port on open. [probe] tries them the other way round.
            runCatching { port.dtr = config.assertControlLines }
            runCatching { port.rts = config.assertControlLines }

            // Let the bridge finish configuring itself before the first command.
            //
            // On the desktop this moment does not exist: the OS driver set the
            // chip up at enumeration, long before the app opened the port. Here
            // the driver is this app, `setParameters` has just run a burst of
            // vendor control transfers, and the very next thing we do is put
            // bytes on the wire. Give the chip a beat to be ready for them.
            runCatching { Thread.sleep(SETTLE_MS) }

            val link = UsbSerialPortLink(port)
            // Anything the bridge buffered before we opened is not part of this
            // session's reply stream and would desynchronise the first read.
            runCatching { port.purgeHwBuffers(true, true) }
            drain(link)

            val t = UsbRadioTransport(link, readTimeoutMs = config.readTimeoutMs)
            transport = t
            return block(driver, handle, port, t)
        } finally {
            bytesReceivedLastSession = transport?.bytesReceived ?: 0
            runCatching { port.close() }
            runCatching { connection.close() }
        }
    }

    /**
     * Discard whatever is already sitting in the bridge's receive buffer.
     *
     * One packet at a time, because a bulk read only ever hands over one and a
     * larger buffer would just wait out the timeout on every packet boundary.
     */
    private fun drain(link: SerialLink) {
        val scratch = ByteArray(link.packetSize)
        while (true) {
            val n = runCatching { link.read(scratch, DRAIN_TIMEOUT_MS) }.getOrDefault(0)
            if (n <= 0) return
        }
    }

    /**
     * One line describing the link a failed transfer ran over.
     *
     * Appended to every failure because the interesting facts are all invisible
     * from a phone: which driver claimed the cable, what the bridge chip
     * actually is (radio programming cables are full of PL2303 variants and
     * clones, and they are not all driven the same way), whether the control
     * lines took, and whether the command reached the wire at all. Without this
     * a timeout says only "nothing came back", which is true of a dozen
     * different faults.
     */
    private fun describeLink(
        driver: UsbSerialDriver,
        device: UsbDevice,
        port: UsbSerialPort,
        config: LinkConfig,
        transport: UsbRadioTransport,
    ): String {
        val lines = runCatching { port.controlLines.joinToString("+").ifEmpty { "none" } }
            .getOrElse { "unreadable" }
        return buildString {
            append("Link: ")
            append(driver.javaClass.simpleName)
            append(String.format(", %04x:%04x", device.vendorId, device.productId))
            append(" rev ").append(runCatching { device.version }.getOrDefault("?"))
            // The Prolific driver picks its register layout from the USB class
            // and the revision, so both belong here: a clone that reports the
            // wrong one is driven with the wrong layout and goes quiet.
            append(" class ").append(device.deviceClass)
            append(", ").append(config.baud).append(" 8-N-1")
            append(", ").append(runCatching { port.readEndpoint?.maxPacketSize ?: 0 }.getOrDefault(0))
            append("-byte packets, control lines ").append(lines)
            append(", ").append(transport.describeTraffic())
        }
    }

    internal companion object {
        /** Short, because a quiet bridge is the normal case. */
        const val DRAIN_TIMEOUT_MS = 50

        /** Long enough for a bridge's vendor init to settle, short enough not to be felt. */
        const val SETTLE_MS = 150L

        /** Shorter than a transfer's: four attempts at two seconds is a long stare. */
        const val PROBE_TIMEOUT_MS = 1_200

        /** Every rate a radio here is known to use, for a probe to walk. */
        val KNOWN_BAUDS = listOf(115_200, 9_600)

        /**
         * What [probe] tries, in order, for a radio whose link runs at [baud].
         *
         * The radio's own rate first, then the others, and each with the control
         * lines both ways. This is the shape it is because of what it found on
         * 2026-09-04: the KT-5000 answered only at 9600 with DTR/RTS asserted,
         * after the desktop had read and written it for a month at a nominal
         * 115200. A probe that only tried what the app already believed would
         * have reported four failures and taught us nothing.
         */
        fun probeConfigs(baud: Int): List<LinkConfig> =
            (listOf(baud) + KNOWN_BAUDS.filter { it != baud }).flatMap { rate ->
                listOf(true, false).map { LinkConfig(rate, it, PROBE_TIMEOUT_MS) }
            }

        /** A probe line has room for an outcome, not for a diagnostic block. */
        fun summarise(e: Throwable): String {
            val first = e.userMessage.substringBefore('\n').trim()
            return when {
                first.contains("stopped responding") -> "no reply"
                first.length <= 60 -> first
                else -> first.take(57) + "..."
            }
        }
    }
}
