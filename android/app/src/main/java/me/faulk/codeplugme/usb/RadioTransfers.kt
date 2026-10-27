package me.faulk.codeplugme.usb

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

/**
 * The one place a transfer's state lives.
 *
 * A transfer outlives the screen that started it: it runs in a foreground
 * service so backgrounding the app cannot kill a write half-finished, and a
 * ViewModel that is recreated meanwhile still has to find it. A process-wide
 * holder is the simplest thing that survives both.
 *
 * Only one transfer may run at a time. Two concurrent sessions on one radio is
 * not a case worth supporting; it is a case worth refusing.
 */
object RadioTransfers {

    private val _state = MutableStateFlow<TransferState>(TransferState.Idle)

    /** The running transfer, or the last one's outcome. */
    val state: StateFlow<TransferState> = _state

    /** True while a transfer holds the radio. */
    val isBusy: Boolean get() = _state.value is TransferState.Running

    /** Publish a new state. Called by [RadioTransferService] only. */
    internal fun publish(next: TransferState) {
        _state.value = next
    }

    /**
     * Claim the radio, or fail if it is already claimed.
     *
     * Returns false when a transfer is already running, so the caller can say
     * so instead of starting a second session that the hardware would reject.
     */
    internal fun claim(kind: TransferKind, deviceLabel: String): Boolean {
        if (isBusy) return false
        _state.value = TransferState.Running(kind, deviceLabel, done = 0, total = 0)
        return true
    }

    /**
     * Acknowledge a finished or failed transfer, returning to idle.
     *
     * The UI calls this after it has shown the outcome, so a completion cannot
     * be lost to a recomposition before the user has seen it.
     */
    fun acknowledge() {
        if (_state.value !is TransferState.Running) _state.value = TransferState.Idle
    }
}
