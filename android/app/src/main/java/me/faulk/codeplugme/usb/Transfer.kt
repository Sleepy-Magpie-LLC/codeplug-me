package me.faulk.codeplugme.usb

/** What a transfer is doing to the radio. */
enum class TransferKind {
    /** Reading the radio's codeplug off it. Non-destructive. */
    READ,

    /** Writing a codeplug onto the radio. Destructive and not undoable. */
    WRITE,

    /**
     * Reading the radio back after a write to check what actually landed.
     *
     * Its own session, on its own open, because the KT-5000 reboots when the
     * port closes and only then will it answer another handshake.
     */
    VERIFY,
}

/** One run of bytes that came back different from what was written. */
data class ByteRun(val addr: Int, val len: Int, val region: String) {
    /** One line a person can act on: "16 bytes at 0x1A50 in channel names". */
    val label: String
        get() = "$len ${if (len == 1) "byte" else "bytes"} at 0x%04X in %s".format(addr, region)
}

/**
 * What reading the radio back after a write found.
 *
 * A write is not confirmed by its acknowledgements. The KT-5000 acks each block
 * on receipt and commits it afterwards, so a reset arriving in between loses a
 * block the host was told it had (measured 2026-09-09). Only a read-back says
 * what the radio is really holding.
 */
sealed interface Verification {
    /** The radio holds exactly what was sent. */
    data object Clean : Verification

    /** Not checked, because this radio's write cannot be verified this way. */
    data object NotSupported : Verification

    /** The check itself could not run, e.g. the radio never came back up. */
    data class Unavailable(val reason: String) : Verification

    /** The radio holds something else. Every [runs] entry is real damage. */
    data class Mismatched(val runs: List<ByteRun>) : Verification
}

/**
 * Where a transfer has got to.
 *
 * Modelled as a closed set of states rather than a bag of booleans so the UI
 * cannot render "in progress" and "failed" at once, and so a completion is
 * impossible to miss.
 */
sealed interface TransferState {
    /** Nothing is happening; the radio is free. */
    data object Idle : TransferState

    /** A transfer is running. [done] of [total] blocks have moved. */
    data class Running(
        val kind: TransferKind,
        val deviceLabel: String,
        val done: Int,
        val total: Int,
    ) : TransferState {
        /** 0.0 to 1.0, or null before the first progress report. */
        val fraction: Float? get() = if (total > 0) done.toFloat() / total else null
    }

    /** A read finished and produced [image]. */
    data class ReadComplete(val image: ByteArray, val modelId: String) : TransferState {
        override fun equals(other: Any?): Boolean =
            other is ReadComplete && modelId == other.modelId && image.contentEquals(other.image)

        override fun hashCode(): Int = 31 * modelId.hashCode() + image.contentHashCode()
    }

    /** A write finished, with what the read-back afterwards found. */
    data class WriteComplete(
        val deviceLabel: String,
        val verification: Verification,
    ) : TransferState

    /**
     * A transfer failed. [mayHavePartiallyWritten] is the difference between
     * "nothing happened" and "the radio may now hold half a codeplug", and the
     * UI must say so plainly when it is true.
     */
    data class Failed(
        val kind: TransferKind,
        val message: String,
        val mayHavePartiallyWritten: Boolean,
    ) : TransferState
}
