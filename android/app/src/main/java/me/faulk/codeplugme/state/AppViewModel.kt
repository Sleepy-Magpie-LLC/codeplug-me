package me.faulk.codeplugme.state

import android.app.Application
import android.content.Context
import android.net.Uri
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import me.faulk.codeplugme.core.RadioCore
import me.faulk.codeplugme.core.userMessage
import me.faulk.codeplugme.library.CodeplugLibrary
import me.faulk.codeplugme.library.CodeplugOrigin
import me.faulk.codeplugme.library.LibraryEntry
import me.faulk.codeplugme.model.SupportedModel
import me.faulk.codeplugme.usb.ManualModelSelection
import me.faulk.codeplugme.usb.RadioDevice
import me.faulk.codeplugme.usb.RadioDeviceRepository
import me.faulk.codeplugme.usb.RadioTransferService
import me.faulk.codeplugme.usb.RadioTransfers
import me.faulk.codeplugme.usb.TransferState
import me.faulk.codeplugme.usb.effectiveModelId
import me.faulk.codeplugme.usb.writeRefusalReason

/** What the Radio screen renders: devices, library, and what the app knows. */
data class RadioScreenState(
    val devices: List<RadioDevice> = emptyList(),
    val selectedDeviceKey: String? = null,
    val manual: ManualModelSelection? = null,
    val library: List<LibraryEntry> = emptyList(),
    val models: List<SupportedModel> = emptyList(),
    val busyMessage: String? = null,
    val error: String? = null,
    /** False when the platform does not advertise USB host support. */
    val reportsUsbHostSupport: Boolean = true,
) {
    /** The device the user is acting on, or the only plausible one. */
    val selectedDevice: RadioDevice?
        get() = devices.firstOrNull { it.key == selectedDeviceKey } ?: devices.firstOrNull()

    /** The model we believe is on the selected device, if any. */
    val selectedModelId: String?
        get() = selectedDevice?.let { effectiveModelId(it, manual) }

    /** True when the user must name the model before anything can happen. */
    val needsModelChoice: Boolean
        get() = selectedDevice != null && selectedModelId == null

    /** Display name for a model id. */
    fun displayName(modelId: String?): String =
        models.firstOrNull { it.id == modelId }?.displayName ?: modelId.orEmpty()
}

/**
 * Owns the app's long-lived state: the codeplug library, the editing store, and
 * the USB device list.
 *
 * Kept deliberately thin. The editing rules live in [CodeplugStore], which is a
 * plain class so it is testable without Android, and the transfer rules live in
 * the foreground service, which has to outlive this object anyway.
 */
class AppViewModel(app: Application) : AndroidViewModel(app) {

    private val context: Context get() = getApplication()

    val library = CodeplugLibrary(File(context.filesDir, "library"))
    private val recovery = Recovery(File(context.filesDir, "recovery"))
    val store = CodeplugStore(library, recovery)
    val devices = RadioDeviceRepository(context)

    private val _radio = MutableStateFlow(RadioScreenState())

    /** What the Radio screen renders. */
    val radio: StateFlow<RadioScreenState> = _radio

    /** The running transfer, or the last outcome. */
    val transfer: StateFlow<TransferState> = RadioTransfers.state

    /** Unsaved work from a previous run, surfaced once at startup. */
    private val _pendingRecovery = MutableStateFlow<RecoveryManifest?>(null)
    val pendingRecovery: StateFlow<RecoveryManifest?> = _pendingRecovery

    /** The write waiting on the user's confirmation, if any. */
    private val _pendingWrite = MutableStateFlow<WritePreflight?>(null)
    val pendingWrite: StateFlow<WritePreflight?> = _pendingWrite

    init {
        _radio.value = _radio.value.copy(reportsUsbHostSupport = devices.reportsUsbHostSupport)
        viewModelScope.launch {
            _radio.value = _radio.value.copy(models = runCatching { RadioCore.models() }.getOrDefault(emptyList()))
            refreshLibrary()
            refreshDevices()
            _pendingRecovery.value = store.pendingRecovery()
        }
        viewModelScope.launch {
            devices.attachEvents().collect { refreshDevices() }
        }
    }

    // -------------------------------------------------------------- devices

    /** Re-scan the USB bus and publish what is attached now. */
    fun refreshDevices() {
        devices.refresh()
        _radio.value = _radio.value.copy(devices = devices.devices.value)
    }

    /** Choose which attached device to act on. */
    fun selectDevice(key: String) {
        _radio.value = _radio.value.copy(selectedDeviceKey = key)
    }

    /**
     * Record the user naming the model behind an unattributed adapter.
     *
     * Bound to that device's key so it cannot leak to the next adapter plugged
     * in, which would be exactly the silent misattribution the core refuses to
     * make on its own.
     */
    fun chooseModel(deviceKey: String, modelId: String) {
        _radio.value = _radio.value.copy(manual = ManualModelSelection(deviceKey, modelId))
    }

    // -------------------------------------------------------------- library

    /** Reload the stored codeplug list. */
    fun refreshLibrary() {
        _radio.value = _radio.value.copy(library = library.list())
    }

    /** Import a codeplug the user picked through the system file picker. */
    fun import(uri: Uri) = viewModelScope.launch {
        withBusy("Importing") {
            withContext(Dispatchers.IO) {
                val bytes = context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
                    ?: throw IllegalStateException("Could not read that file.")
                val name = displayNameOf(uri) ?: "Imported codeplug"
                library.add(
                    name.removeSuffix(".bin"),
                    bytes,
                    sourceName = name,
                    origin = CodeplugOrigin.IMPORT,
                )
            }
            refreshLibrary()
        }
    }

    /** Write a stored codeplug out through the system file picker. */
    fun export(entryId: String, uri: Uri) = viewModelScope.launch {
        withBusy("Exporting") {
            withContext(Dispatchers.IO) {
                val bytes = library.bytes(entryId)
                context.contentResolver.openOutputStream(uri)?.use { it.write(bytes) }
                    ?: throw IllegalStateException("Could not write that file.")
            }
        }
    }

    /**
     * Save the open codeplug's staged edits back over the entry it came from.
     *
     * The library index carries the byte count and modified time, so the list
     * on the Radio screen is refreshed after every save rather than left to go
     * stale until the next import.
     */
    fun saveOpen() = viewModelScope.launch {
        withContext(Dispatchers.IO) { store.save() }
        refreshLibrary()
    }

    /** Save the open codeplug's staged edits as a new entry, and edit that one. */
    fun saveOpenAs(name: String) = viewModelScope.launch {
        withContext(Dispatchers.IO) { store.saveAs(name) }
        refreshLibrary()
    }

    /** Throw away the open codeplug's staged edits. */
    fun discardOpenChanges() = viewModelScope.launch {
        store.discardChanges()
    }

    /**
     * File a codeplug built from repeater listings as a new entry, and open it.
     *
     * The base it was built on is named as the source, because its unmodelled
     * regions are what the new one carries.
     */
    fun saveGenerated(name: String, image: ByteArray) = viewModelScope.launch {
        val baseName = store.state.value.entryName.ifBlank { null }
        val entry = try {
            withContext(Dispatchers.IO) {
                library.add(
                    "$name ${timestamp()}",
                    image,
                    sourceName = baseName,
                    origin = CodeplugOrigin.GENERATED,
                )
            }
        } catch (e: Throwable) {
            return@launch showError(e)
        }
        refreshLibrary()
        store.open(entry.id)
    }

    /** Rename a stored codeplug. */
    fun rename(entryId: String, name: String) {
        runCatching { library.rename(entryId, name) }
            .onFailure { showError(it) }
        refreshLibrary()
    }

    /** Copy a stored codeplug, so an experiment does not risk the original. */
    fun duplicate(entryId: String) {
        runCatching { library.duplicate(entryId) }.onFailure { showError(it) }
        refreshLibrary()
    }

    /** Permanently delete a stored codeplug. */
    fun delete(entryId: String) {
        if (store.state.value.entryId == entryId) store.close()
        runCatching { library.delete(entryId) }.onFailure { showError(it) }
        refreshLibrary()
    }

    // ------------------------------------------------------------ transfers

    /**
     * Read the connected radio's codeplug into the library.
     *
     * Permission is requested here, before the session opens, because a session
     * is one uninterrupted open and a permission dialog cannot land inside it.
     */
    fun readFromRadio() = viewModelScope.launch {
        val s = _radio.value
        val device = s.selectedDevice ?: return@launch showError("No radio is connected.")
        val modelId = s.selectedModelId
            ?: return@launch showError("Choose which radio is on ${device.label} first.")
        if (!devices.requestPermission(device)) {
            return@launch showError("USB permission is needed to reach ${device.label}.")
        }
        if (!RadioTransferService.startRead(context, device, modelId)) {
            showError("The radio is already busy with another transfer.")
        }
    }

    /**
     * Work out what the write confirmation should say, and open it.
     *
     * The refusals that do not depend on the bytes are checked here, so a write
     * that cannot happen never reaches a confirmation dialog at all.
     */
    fun prepareWrite(entry: LibraryEntry) = viewModelScope.launch {
        val s = _radio.value
        val device = s.selectedDevice ?: return@launch showError("No radio is connected.")
        val modelId = entry.modelId
            ?: return@launch showError("That codeplug names no radio, so it cannot be written.")
        writeRefusalReason(device, modelId, s.manual, s.models)?.let {
            return@launch showError(it)
        }

        val staged = store.state.value.entryId == entry.id && store.state.value.isDirty
        val erased = try {
            RadioCore.erasedRegions(bytesToWrite(entry.id))
        } catch (e: Throwable) {
            return@launch showError(e)
        }

        _pendingWrite.value = WritePreflight(
            entry = entry,
            deviceLabel = device.label,
            originLine = originLine(entry),
            hasUnsavedEdits = staged,
            erasedRegions = erased,
        )
    }

    /**
     * The bytes a write would send for this codeplug.
     *
     * The work file when this is the codeplug open in the editor, so unsaved
     * edits reach the radio; the stored entry otherwise. One function because
     * the confirmation must inspect exactly what the write will send.
     *
     * On the IO dispatcher: an AnyTone codeplug is 1.79 MB and reading it on
     * the main thread is enough to drop frames or trip an ANR on a slow phone.
     */
    private suspend fun bytesToWrite(entryId: String): ByteArray = withContext(Dispatchers.IO) {
        if (store.state.value.entryId == entryId) store.stagedBytes()
        else library.bytes(entryId)
    }

    /** Dismiss the write confirmation without writing. */
    fun cancelWrite() {
        _pendingWrite.value = null
    }

    /** The user confirmed the write; send it. */
    fun confirmWrite() {
        val pending = _pendingWrite.value ?: return
        _pendingWrite.value = null
        writeToRadio(pending.entry.id)
    }

    /**
     * Write a stored codeplug to the connected radio.
     *
     * The bytes come from the WORK FILE when this codeplug is the one open in
     * the editor, so unsaved edits reach the radio. That matches the macOS
     * behaviour and is what makes "tweak a channel, write it, try it" a single
     * loop. See [bytesToWrite].
     *
     * Reachable without [prepareWrite] having run, so it repeats the refusals
     * rather than trusting that a confirmation happened.
     */
    fun writeToRadio(entryId: String) = viewModelScope.launch {
        val s = _radio.value
        val entry = library.get(entryId) ?: return@launch showError("That codeplug is gone.")
        val modelId = entry.modelId
            ?: return@launch showError("That codeplug names no radio, so it cannot be written.")
        val device = s.selectedDevice ?: return@launch showError("No radio is connected.")
        writeRefusalReason(device, modelId, s.manual, s.models)?.let {
            return@launch showError(it)
        }
        if (!devices.requestPermission(device)) {
            return@launch showError("USB permission is needed to reach ${device.label}.")
        }

        val bytes = try {
            bytesToWrite(entryId)
        } catch (e: Throwable) {
            return@launch showError(e)
        }

        if (!RadioTransferService.startWrite(context, device, modelId, bytes)) {
            showError("The radio is already busy with another transfer.")
        }
    }

    /**
     * File a completed read into the library and clear the transfer.
     *
     * Done here rather than in the service so the read is stored exactly once,
     * even if the screen that started it was recreated meanwhile.
     */
    fun acceptRead(state: TransferState.ReadComplete) = viewModelScope.launch {
        val name = "Read from radio ${timestamp()}"
        // Off the main thread: this writes up to 1.79 MB and runs the model
        // detection through the core.
        withContext(Dispatchers.IO) {
            runCatching { library.add(name, state.image, origin = CodeplugOrigin.RADIO) }
        }
            .onFailure { showError(it) }
        refreshLibrary()
        RadioTransfers.acknowledge()
    }

    /** Dismiss a finished or failed transfer. */
    fun acknowledgeTransfer() = RadioTransfers.acknowledge()

    // ------------------------------------------------------------- recovery

    /** Reopen a previous run's unsaved edits. */
    fun restoreRecovery(manifest: RecoveryManifest) = viewModelScope.launch {
        store.restoreRecovery(manifest)
        _pendingRecovery.value = null
    }

    /** Throw away a previous run's unsaved edits. */
    fun discardRecovery() {
        store.discardRecovery()
        _pendingRecovery.value = null
    }

    // ------------------------------------------------------------ messages

    /** Dismiss the Radio screen's error. */
    fun clearError() {
        _radio.value = _radio.value.copy(error = null)
    }

    private fun showError(message: String) {
        _radio.value = _radio.value.copy(error = message)
    }

    private fun showError(t: Throwable) = showError(t.userMessage)

    /** Run [block] with a busy label, reporting any failure to the user. */
    private suspend fun withBusy(label: String, block: suspend () -> Unit) {
        _radio.value = _radio.value.copy(busyMessage = label, error = null)
        try {
            block()
        } catch (e: Throwable) {
            showError(e)
        } finally {
            _radio.value = _radio.value.copy(busyMessage = null)
        }
    }

    /** The human name behind a content URI, if the provider offers one. */
    private fun displayNameOf(uri: Uri): String? =
        context.contentResolver.query(uri, null, null, null, null)?.use { c ->
            val i = c.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
            if (i >= 0 && c.moveToFirst()) c.getString(i) else null
        }

    private fun timestamp(): String =
        SimpleDateFormat("yyyy-MM-dd HH:mm", Locale.ROOT).format(Date())

}
