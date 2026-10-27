package me.faulk.codeplugme.library

import java.io.File
import java.util.UUID
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import uniffi.codeplugme_core.ApiException
import uniffi.codeplugme_core.detectModel

/** Where a stored codeplug's bytes originally came from. */
@Serializable
enum class CodeplugOrigin {
    /** Read off a radio by this app. Every region holds what that radio held. */
    RADIO,

    /** Imported from a file. Its unmodelled regions are whatever the file has. */
    IMPORT,

    /**
     * Built by this app from repeater listings on top of another codeplug. Its
     * unmodelled regions are whatever that base codeplug had.
     */
    GENERATED,

    /** Stored before origin was recorded, so it could be either. */
    UNKNOWN,
}

/**
 * One codeplug held on the phone.
 *
 * [modelId] comes from the image's own bytes via the core, never from the file
 * name or from what the user believed they were importing, because it is what
 * decides which radio the codeplug may be written to.
 *
 * [origin] is here because the KT-5000's write sends the image verbatim: every
 * byte below the write limit lands on the radio, including regions nothing in
 * this app models. An image read off the radio being written carries that
 * radio's own bytes there; anything else carries someone else's. The user is
 * told which before they confirm a write. Defaulted so an index written by an
 * earlier build still loads.
 */
@Serializable
data class LibraryEntry(
    val id: String,
    val name: String,
    val modelId: String?,
    val sizeBytes: Long,
    val sourceName: String?,
    val importedAtEpochMs: Long,
    val modifiedAtEpochMs: Long,
    val origin: CodeplugOrigin = CodeplugOrigin.UNKNOWN,
)

/** On-disk shape of the index. Versioned so it can be migrated later. */
@Serializable
private data class LibraryIndex(val version: Int = 1, val entries: List<LibraryEntry> = emptyList())

/**
 * The codeplugs stored on the phone, and the reason the primary flow is fast.
 *
 * The macOS app is single-document: you open a `.bin` from the file system every
 * time. On a phone that would put a Storage Access Framework round trip in front
 * of every single write, which is exactly the wrong cost on the thing this app
 * exists to do. So codeplugs live in app-private storage and SAF is used only at
 * the edges, to import a file in and to export one out.
 *
 * Not a cache: this is the user's data, and deleting an entry is destructive.
 */
class CodeplugLibrary(private val root: File) {

    private val indexFile = File(root, INDEX_FILE)
    private val json = Json { prettyPrint = true; ignoreUnknownKeys = true }

    init {
        root.mkdirs()
    }

    /** Every stored codeplug, newest first. */
    fun list(): List<LibraryEntry> =
        readIndex().entries.sortedByDescending { it.modifiedAtEpochMs }

    /** One entry by id, or null if it is gone. */
    fun get(id: String): LibraryEntry? = readIndex().entries.firstOrNull { it.id == id }

    /** The backing file for an entry. Callers read and write bytes through it. */
    fun fileFor(id: String): File = File(root, "$id.bin")

    /** The bytes of a stored codeplug. */
    fun bytes(id: String): ByteArray {
        val f = fileFor(id)
        if (!f.exists()) throw ApiException.Failed("That codeplug is no longer on this phone.")
        return f.readBytes()
    }

    /**
     * Store [image] as a new entry named [name].
     *
     * A file whose bytes match no supported radio is refused here rather than at
     * write time. Importing junk and only discovering it with a radio plugged in
     * is a worse place to find out.
     */
    fun add(
        name: String,
        image: ByteArray,
        sourceName: String? = null,
        origin: CodeplugOrigin = CodeplugOrigin.UNKNOWN,
    ): LibraryEntry {
        val modelId = detectModel(image)
            ?: throw ApiException.Failed(
                "This file is ${image.size} bytes, which matches no supported radio " +
                    "codeplug. It may be for a different radio, or not a codeplug at all."
            )
        val now = System.currentTimeMillis()
        val entry = LibraryEntry(
            id = UUID.randomUUID().toString(),
            name = name.ifBlank { "Untitled" },
            modelId = modelId,
            sizeBytes = image.size.toLong(),
            sourceName = sourceName,
            importedAtEpochMs = now,
            modifiedAtEpochMs = now,
            origin = origin,
        )
        fileFor(entry.id).writeBytes(image)
        writeIndex(readIndex().let { it.copy(entries = it.entries + entry) })
        return entry
    }

    /** Replace an entry's bytes, e.g. when saving edits back to it. */
    fun update(id: String, image: ByteArray): LibraryEntry {
        val existing = get(id) ?: throw ApiException.Failed("That codeplug is no longer on this phone.")
        fileFor(id).writeBytes(image)
        val updated = existing.copy(
            sizeBytes = image.size.toLong(),
            modifiedAtEpochMs = System.currentTimeMillis(),
        )
        replace(updated)
        return updated
    }

    /** Rename an entry. The bytes are untouched. */
    fun rename(id: String, name: String): LibraryEntry {
        val existing = get(id) ?: throw ApiException.Failed("That codeplug is no longer on this phone.")
        val updated = existing.copy(name = name.ifBlank { existing.name })
        replace(updated)
        return updated
    }

    /** Copy an entry, so an experiment does not risk the original. */
    fun duplicate(id: String): LibraryEntry {
        val existing = get(id) ?: throw ApiException.Failed("That codeplug is no longer on this phone.")
        return add("${existing.name} copy", bytes(id), existing.sourceName, existing.origin)
    }

    /** Permanently remove an entry and its bytes. */
    fun delete(id: String) {
        fileFor(id).delete()
        writeIndex(readIndex().let { idx -> idx.copy(entries = idx.entries.filterNot { it.id == id }) })
    }

    /** Swap one entry in the index for an updated copy of itself. */
    private fun replace(entry: LibraryEntry) {
        writeIndex(
            readIndex().let { idx ->
                idx.copy(entries = idx.entries.map { if (it.id == entry.id) entry else it })
            }
        )
    }

    /**
     * Read the index, healing it rather than failing.
     *
     * A corrupt or truncated index must not make every stored codeplug
     * unreachable, so a parse failure falls back to an empty index and the
     * orphan sweep below leaves the `.bin` files alone.
     */
    private fun readIndex(): LibraryIndex {
        if (!indexFile.exists()) return LibraryIndex()
        return runCatching { json.decodeFromString<LibraryIndex>(indexFile.readText()) }
            .getOrElse { LibraryIndex() }
            // An entry whose file vanished is not showable; drop it from the view
            // without rewriting the index, so a transient failure is not permanent.
            .let { idx -> idx.copy(entries = idx.entries.filter { fileFor(it.id).exists() }) }
    }

    private fun writeIndex(index: LibraryIndex) {
        // Write-then-rename so an interrupted save cannot truncate the index.
        val tmp = File(root, "$INDEX_FILE.tmp")
        tmp.writeText(json.encodeToString(index))
        tmp.renameTo(indexFile)
    }

    private companion object {
        const val INDEX_FILE = "library.json"
    }
}
