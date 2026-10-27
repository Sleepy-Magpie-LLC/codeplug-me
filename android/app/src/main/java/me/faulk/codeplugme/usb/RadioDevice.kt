package me.faulk.codeplugme.usb

import android.hardware.usb.UsbDevice

/**
 * A USB device that might be a radio, and what we can say about it.
 *
 * [modelId] is the core's attribution from the USB descriptors and is null far
 * more often than you would expect: only the AnyTone's two bridge variants are
 * attributable. The KT-5000 ships with a stock Prolific PL2303 shared with
 * thousands of unrelated adapters, so it is always null there and the user must
 * name the model. See codeplugme-core's `usb_ids` module for the full reasoning.
 *
 * The identity fields are plain values and [device] is nullable so the write
 * targeting rules, which are the ones that decide whether a destructive write
 * may proceed, can be tested on a host JVM with no Android and no hardware.
 */
data class RadioDevice(
    /**
     * A stable key for this attachment. On Android this is the USB bus path
     * (e.g. `/dev/bus/usb/001/004`), which is what a manual model pick binds to
     * so it cannot leak to a different adapter.
     */
    val key: String,
    val vendorId: Int,
    val productId: Int,
    val productName: String?,
    /** The model the USB identity names, or null when it names none. */
    val modelId: String?,
    /** The live handle, absent in tests. */
    val device: UsbDevice? = null,
) {
    /** What to show the user when naming this device. */
    val label: String
        get() = productName?.takeIf { it.isNotBlank() }
            ?: String.format("USB %04x:%04x", vendorId, productId)
}
