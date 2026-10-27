package me.faulk.codeplugme.usb

import me.faulk.codeplugme.model.SupportedModel

/**
 * The user saying "the radio on this device is that model", for a device whose
 * USB identity names no radio.
 *
 * It is tied to a device key rather than kept as a bare model because it must
 * not leak: picking "QYT KT-5000" for one adapter says nothing about the next
 * one plugged in, and carrying it over would be exactly the silent
 * misattribution the core refuses to make on its own.
 */
data class ManualModelSelection(val deviceKey: String, val modelId: String)

/**
 * Which radio we believe is on [device]: its USB attribution when the core
 * could make one, otherwise the user's explicit pick for that same device.
 * Null means nobody has said, and nothing may be read or written until
 * somebody does.
 */
fun effectiveModelId(device: RadioDevice, manual: ManualModelSelection?): String? {
    device.modelId?.let { return it }
    if (manual == null || manual.deviceKey != device.key) return null
    return manual.modelId
}

/**
 * Why [device] must not receive a codeplug belonging to [documentModelId],
 * phrased for the user, or null when the write may go ahead.
 *
 * This is the layer in front of the core's guard, not a replacement for it.
 * `writeImage` still handshakes the radio and refuses on a mismatch before
 * emitting a byte; this exists so the common cases surface as a sentence the
 * user can act on instead of a failed transfer.
 */
fun writeRefusalReason(
    device: RadioDevice,
    documentModelId: String?,
    manual: ManualModelSelection?,
    models: List<SupportedModel>,
): String? {
    fun name(id: String) = models.firstOrNull { it.id == id }?.displayName ?: id

    if (documentModelId == null) {
        return "No codeplug is selected, so there is nothing to write."
    }
    val target = effectiveModelId(device, manual)
        ?: return "The adapter on ${device.label} doesn't identify which radio is behind it. " +
            "Choose ${name(documentModelId)} for it first, then try again."
    if (target != documentModelId) {
        return "This codeplug is for a ${name(documentModelId)}, but ${device.label} is " +
            "a ${name(target)}. Writing it would overwrite the wrong radio."
    }
    return null
}

/**
 * The devices a codeplug belonging to [documentModelId] may be written to.
 *
 * Everything else is withheld rather than offered and then rejected: a
 * destructive action the app already knows will fail should not be one tap from
 * a confirmation dialog.
 */
fun writableTargets(
    devices: List<RadioDevice>,
    documentModelId: String?,
    manual: ManualModelSelection?,
    models: List<SupportedModel>,
): List<RadioDevice> = devices.filter {
    writeRefusalReason(it, documentModelId, manual, models) == null
}
