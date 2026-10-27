package me.faulk.codeplugme.state

import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

/**
 * Points at unsaved edits left behind by a previous run.
 *
 * [libraryEntryId] names the codeplug the work file was staged from, so a
 * recovery offer can say which one it belongs to and can be refused cleanly
 * when that entry has since been deleted.
 */
@Serializable
data class RecoveryManifest(
    val libraryEntryId: String,
    val entryName: String,
    val modifiedAtEpochMs: Long,
)

/**
 * The staging area, and the reason a crash does not cost the user their edits.
 *
 * Every edit is applied to [workFile] by the core and never to the stored
 * codeplug, which only changes on an explicit save. That single rule is what
 * makes undo a byte snapshot, makes "write unsaved changes to the radio"
 * meaningful, and makes crash recovery a matter of noticing the work file is
 * still there.
 *
 * The directory is injectable so tests get their own, rather than sharing the
 * one real app state.
 */
class Recovery(private val directory: File) {

    private val manifestFile = File(directory, "manifest.json")
    private val json = Json { prettyPrint = true; ignoreUnknownKeys = true }

    /** The staged copy every edit is applied to. */
    val workFile: File = File(directory, "work.bin")

    init {
        directory.mkdirs()
    }

    /**
     * Unsaved work from a previous run, or null.
     *
     * A manifest with no work file beside it is stale and is cleared rather than
     * offered: there is nothing to restore.
     */
    fun pending(): RecoveryManifest? {
        if (!manifestFile.exists() || !workFile.exists()) {
            clearManifest()
            return null
        }
        return runCatching { json.decodeFromString<RecoveryManifest>(manifestFile.readText()) }
            .getOrNull()
    }

    /** Record that [workFile] holds unsaved edits to the named entry. */
    fun mark(entryId: String, entryName: String) {
        manifestFile.writeText(
            json.encodeToString(
                RecoveryManifest(entryId, entryName, System.currentTimeMillis())
            )
        )
    }

    /** Forget the manifest but keep the work file, e.g. after a save. */
    fun clearManifest() {
        manifestFile.delete()
    }

    /** Forget everything. */
    fun clear() {
        manifestFile.delete()
        workFile.delete()
    }
}
