package me.faulk.codeplugme.usb

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.hardware.usb.UsbDevice
import android.hardware.usb.UsbManager
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlin.coroutines.resume
import uniffi.codeplugme_core.modelForUsbId

/**
 * Finds attached radios, attributes them, and gets USB permission for them.
 *
 * Attribution goes through the Rust core's [modelForUsbId] rather than a Kotlin
 * table, so both apps share one source of truth for which USB identities are
 * radios. A null answer is routine: only the AnyTone's two bridge variants are
 * attributable, and the KT-5000's stock PL2303 cable never will be.
 */
class RadioDeviceRepository(private val context: Context) {

    private val usbManager: UsbManager =
        context.getSystemService(Context.USB_SERVICE) as UsbManager

    private val _devices = MutableStateFlow(currentDevices())

    /** Attached devices that a serial driver could plausibly claim. */
    val devices: StateFlow<List<RadioDevice>> = _devices

    /**
     * Whether the platform reports USB host support.
     *
     * Informational, never a gate. Some phones run OTG perfectly well without
     * declaring `android.hardware.usb.host`, so refusing to work on a false
     * here would lock out working hardware. It is used only to explain an
     * empty device list, which is otherwise indistinguishable from a bad cable.
     */
    val reportsUsbHostSupport: Boolean =
        context.packageManager.hasSystemFeature(PackageManager.FEATURE_USB_HOST)

    /** Re-scan the USB bus. Cheap; safe to call on every screen resume. */
    fun refresh() {
        _devices.value = currentDevices()
    }

    /** Whether [device] may be opened without prompting. */
    fun hasPermission(device: RadioDevice): Boolean {
        val handle = device.device ?: return false
        return usbManager.hasPermission(handle)
    }

    /**
     * Ask for USB permission and suspend until the user answers.
     *
     * Permission is requested up front, never mid-transfer, because a session
     * must be a single uninterrupted open (see [RadioSession]).
     */
    suspend fun requestPermission(device: RadioDevice): Boolean {
        if (hasPermission(device)) return true
        val handle = device.device ?: return false
        return suspendCancellableCoroutine { cont ->
            val receiver = object : BroadcastReceiver() {
                override fun onReceive(ctx: Context, intent: Intent) {
                    if (intent.action != ACTION_USB_PERMISSION) return
                    context.unregisterReceiver(this)
                    val granted =
                        intent.getBooleanExtra(UsbManager.EXTRA_PERMISSION_GRANTED, false)
                    if (cont.isActive) cont.resume(granted)
                }
            }
            ContextCompat_registerReceiver(context, receiver, IntentFilter(ACTION_USB_PERMISSION))
            cont.invokeOnCancellation { runCatching { context.unregisterReceiver(receiver) } }

            val flags = PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE
            val pi = PendingIntent.getBroadcast(
                context,
                0,
                Intent(ACTION_USB_PERMISSION).setPackage(context.packageName),
                flags,
            )
            usbManager.requestPermission(handle, pi)
        }
    }

    /**
     * Emits whenever a device is attached or detached.
     *
     * Detach matters beyond refreshing a list: pulling the cable mid-write
     * leaves the radio in an unknown state, and the transfer needs to fail
     * loudly rather than hang.
     */
    fun attachEvents(): Flow<Unit> = callbackFlow {
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(ctx: Context, intent: Intent) {
                refresh()
                trySend(Unit)
            }
        }
        val filter = IntentFilter().apply {
            addAction(UsbManager.ACTION_USB_DEVICE_ATTACHED)
            addAction(UsbManager.ACTION_USB_DEVICE_DETACHED)
        }
        ContextCompat_registerReceiver(context, receiver, filter)
        awaitClose { runCatching { context.unregisterReceiver(receiver) } }
    }

    /** Snapshot the bus, attributing each device through the core. */
    private fun currentDevices(): List<RadioDevice> =
        usbManager.deviceList.values.map { it.toRadioDevice() }
            // Attributed radios first: the common case should be the top row.
            .sortedWith(compareByDescending<RadioDevice> { it.modelId != null }.thenBy { it.label })

    private fun UsbDevice.toRadioDevice(): RadioDevice = RadioDevice(
        key = deviceName,
        vendorId = vendorId,
        productId = productId,
        productName = productName,
        modelId = modelForUsbId(vendorId.toUShort(), productId.toUShort()),
        device = this,
    )

    companion object {
        /** Private to this app; the package suffix keeps it from colliding. */
        const val ACTION_USB_PERMISSION = "me.faulk.codeplugme.USB_PERMISSION"
    }
}

/**
 * Register a receiver with the exported flag Android 13+ requires.
 *
 * These are all app-internal or system broadcasts, so NOT_EXPORTED is correct.
 */
private fun ContextCompat_registerReceiver(
    context: Context,
    receiver: BroadcastReceiver,
    filter: IntentFilter,
) {
    if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
        context.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
    } else {
        @Suppress("UnspecifiedRegisterReceiverFlag")
        context.registerReceiver(receiver, filter)
    }
}
