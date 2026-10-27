package me.faulk.codeplugme.usb

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.hardware.usb.UsbManager
import android.os.Build
import android.os.IBinder
import kotlin.concurrent.thread
import me.faulk.codeplugme.R
import me.faulk.codeplugme.core.userMessage
import uniffi.codeplugme_core.ProgressListener
import uniffi.codeplugme_core.readImage
import uniffi.codeplugme_core.supportsWriteVerify
import uniffi.codeplugme_core.verifyImage
import uniffi.codeplugme_core.writeImage

/**
 * Runs one codeplug transfer in the foreground, so it survives backgrounding.
 *
 * This is not defensive polish. A full AnyTone transfer runs for minutes, and
 * Android will freeze or kill a backgrounded activity's work without warning.
 * Losing a read costs the user time; losing a write part-way leaves a radio
 * holding half a codeplug, which is the worst outcome this app can produce. So
 * the transfer belongs to a `connectedDevice` foreground service with an
 * ongoing notification, not to a screen.
 *
 * State is published to [RadioTransfers] rather than returned, because the
 * screen that started the transfer may be gone by the time it finishes.
 */
class RadioTransferService : Service() {

    /** Services have no client binding here; everything goes through the flow. */
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val request = pendingRequest
        pendingRequest = null
        if (request == null) {
            stopSelf(startId)
            return START_NOT_STICKY
        }

        startForeground(NOTIFICATION_ID, buildNotification(request.kind, null))

        // A plain thread, not a coroutine: the core's transfer call is blocking
        // by design and owns this thread for its whole duration.
        thread(name = "radio-transfer", isDaemon = false) {
            runTransfer(request)
            stopSelf(startId)
        }
        return START_NOT_STICKY
    }

    /** Do the transfer and publish its outcome. */
    private fun runTransfer(request: TransferRequest) {
        val usbManager = getSystemService(Context.USB_SERVICE) as UsbManager
        val session = RadioSession(usbManager)
        val label = request.device.label

        // Tracks whether any byte could have reached the radio. Before the first
        // progress callback nothing has been written, which is what lets a
        // failed write say "nothing changed" honestly instead of hedging.
        var progressed = false
        val listener = object : ProgressListener {
            override fun onProgress(done: UInt, total: UInt) {
                progressed = true
                RadioTransfers.publish(
                    TransferState.Running(request.kind, label, done.toInt(), total.toInt())
                )
                notify(buildNotification(request.kind, done.toInt() to total.toInt()))
            }
        }

        try {
            when (request.kind) {
                TransferKind.READ -> {
                    val image = session.use(request.device, request.modelId) { transport ->
                        readImage(transport, request.modelId, listener)
                    }
                    RadioTransfers.publish(TransferState.ReadComplete(image, request.modelId))
                }

                TransferKind.WRITE -> {
                    val image = requireNotNull(request.image) { "a write needs an image" }
                    session.use(request.device, request.modelId) { transport ->
                        // force is true because the user already confirmed a
                        // destructive dialog; the core's gate exists so that
                        // confirmation cannot be skipped by a default argument.
                        writeImage(transport, image, true, label, listener)
                    }
                    RadioTransfers.publish(
                        TransferState.WriteComplete(label, verify(session, request, image))
                    )
                }

                TransferKind.VERIFY -> error("verification runs inside a write")
            }
        } catch (e: Throwable) {
            RadioTransfers.publish(
                TransferState.Failed(
                    kind = request.kind,
                    message = e.userMessage + probeReport(session, request, progressed),
                    mayHavePartiallyWritten = request.kind == TransferKind.WRITE && progressed,
                )
            )
        }
    }

    /**
     * Read the radio back and report what does not match what was just sent.
     *
     * This is not belt and braces. The KT-5000 acknowledges a write block on
     * receipt and commits it afterwards, so an ack is not proof the byte stuck;
     * a reset in between loses it silently. The app used to call a write done
     * on acks alone, which is how a write that erased 950 bytes of the radio's
     * own memory went unnoticed for days (2026-09-09). The core now refuses to
     * send that region at all, and this says so instead of assuming it.
     *
     * Its own session, necessarily: the radio reboots when the port closes, and
     * that reboot is also what commits the write. So the first attempt waits for
     * the radio to come back, and a failed attempt is retried, because a failure
     * here usually means it was still booting. Each attempt costs another reboot
     * on close, which is why there are only a few.
     */
    private fun verify(
        session: RadioSession,
        request: TransferRequest,
        sent: ByteArray,
    ): Verification {
        if (!runCatching { supportsWriteVerify(request.modelId) }.getOrDefault(false)) {
            return Verification.NotSupported
        }

        val label = request.device.label
        RadioTransfers.publish(TransferState.Running(TransferKind.VERIFY, label, 0, 0))
        notify(buildNotification(TransferKind.VERIFY, null))
        val listener = object : ProgressListener {
            override fun onProgress(done: UInt, total: UInt) {
                RadioTransfers.publish(
                    TransferState.Running(TransferKind.VERIFY, label, done.toInt(), total.toInt())
                )
                notify(buildNotification(TransferKind.VERIFY, done.toInt() to total.toInt()))
            }
        }

        var last: Throwable? = null
        repeat(VERIFY_ATTEMPTS) { attempt ->
            runCatching { Thread.sleep(if (attempt == 0) VERIFY_BOOT_MS else VERIFY_RETRY_MS) }
            try {
                val diffs = session.use(request.device, request.modelId) { transport ->
                    verifyImage(transport, sent, request.modelId, listener)
                }
                return if (diffs.isEmpty()) {
                    Verification.Clean
                } else {
                    Verification.Mismatched(
                        diffs.map { ByteRun(it.addr.toInt(), it.len.toInt(), it.region) }
                    )
                }
            } catch (e: Throwable) {
                last = e
            }
        }
        return Verification.Unavailable(last?.userMessage ?: "the radio did not answer")
    }

    /**
     * Probe the link and append the report, when a probe would tell us anything.
     *
     * Only worth running when the radio never said a word: a transfer that got
     * data and then failed has a different fault, and re-handshaking four times
     * would say nothing about it. The probe only ever handshakes and reads, so
     * it cannot make a failed write worse, but it still runs for several
     * seconds and that is not something to do on every failure.
     */
    private fun probeReport(
        session: RadioSession,
        request: TransferRequest,
        progressed: Boolean,
    ): String {
        if (progressed || session.bytesReceivedLastSession != 0) return ""
        return runCatching { "\n\n" + session.probe(request.device, request.modelId) }
            .getOrDefault("")
    }

    /** Build the ongoing notification, with progress when we have any. */
    private fun buildNotification(kind: TransferKind, progress: Pair<Int, Int>?): Notification {
        ensureChannel()
        val title = when (kind) {
            TransferKind.READ -> "Reading codeplug from radio"
            TransferKind.WRITE -> "Writing codeplug to radio"
            TransferKind.VERIFY -> "Checking what the radio kept"
        }
        val builder = Notification.Builder(this, CHANNEL_ID)
            .setContentTitle(title)
            .setContentText("Keep the cable connected until this finishes.")
            .setSmallIcon(R.drawable.ic_launcher_foreground)
            .setOngoing(true)

        if (progress != null && progress.second > 0) {
            builder.setProgress(progress.second, progress.first, false)
        } else {
            builder.setProgress(0, 0, true)
        }
        return builder.build()
    }

    private fun notify(n: Notification) {
        (getSystemService(NOTIFICATION_SERVICE) as NotificationManager)
            .notify(NOTIFICATION_ID, n)
    }

    private fun ensureChannel() {
        val mgr = getSystemService(NOTIFICATION_SERVICE) as NotificationManager
        if (mgr.getNotificationChannel(CHANNEL_ID) != null) return
        mgr.createNotificationChannel(
            NotificationChannel(
                CHANNEL_ID,
                getString(R.string.transfer_channel_name),
                NotificationManager.IMPORTANCE_LOW,
            )
        )
    }

    /** What one transfer needs to run. */
    private data class TransferRequest(
        val kind: TransferKind,
        val device: RadioDevice,
        val modelId: String,
        val image: ByteArray?,
    )

    companion object {
        private const val CHANNEL_ID = "radio-transfers"
        private const val NOTIFICATION_ID = 1

        /**
         * How long to let the radio boot before reading it back.
         *
         * The KT-5000 resets when the port closes and prints its boot banner
         * about a second later; this leaves room for a slower one.
         */
        private const val VERIFY_BOOT_MS = 3_000L

        /** Between attempts. A failure here is usually "still booting". */
        private const val VERIFY_RETRY_MS = 2_000L

        /** Each attempt costs the radio another reboot, so keep it small. */
        private const val VERIFY_ATTEMPTS = 3

        /**
         * Handed to the service in memory rather than through the Intent.
         *
         * A codeplug is up to 1.79 MB and a [android.hardware.usb.UsbDevice]
         * handle does not survive a round trip through an Intent extra usefully,
         * so the request is parked here for the service to pick up. Safe because
         * only one transfer may be in flight and [RadioTransfers.claim] enforces
         * that before this is ever set.
         */
        @Volatile
        private var pendingRequest: TransferRequest? = null

        /**
         * Start a read. Returns false if the radio is already busy.
         *
         * USB permission must already be granted: a session is one
         * uninterrupted open, and a permission dialog cannot land inside it.
         */
        fun startRead(context: Context, device: RadioDevice, modelId: String): Boolean {
            if (!RadioTransfers.claim(TransferKind.READ, device.label)) return false
            pendingRequest = TransferRequest(TransferKind.READ, device, modelId, null)
            launch(context)
            return true
        }

        /**
         * Start a write. Returns false if the radio is already busy.
         *
         * [modelId] is only a label here. The model that decides what protocol
         * is spoken comes from [image]'s own bytes inside the core, so a
         * mislabelled call still cannot write a QYT codeplug to an AnyTone.
         */
        fun startWrite(
            context: Context,
            device: RadioDevice,
            modelId: String,
            image: ByteArray,
        ): Boolean {
            if (!RadioTransfers.claim(TransferKind.WRITE, device.label)) return false
            pendingRequest = TransferRequest(TransferKind.WRITE, device, modelId, image)
            launch(context)
            return true
        }

        private fun launch(context: Context) {
            val intent = Intent(context, RadioTransferService::class.java)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }
    }
}
