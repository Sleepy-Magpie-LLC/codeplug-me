package me.faulk.codeplugme.core

import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import me.faulk.codeplugme.model.BuildMode
import me.faulk.codeplugme.model.CodeplugDump
import me.faulk.codeplugme.model.CodeplugJson
import me.faulk.codeplugme.model.GeoBuildOptions
import me.faulk.codeplugme.model.GeoCapacity
import me.faulk.codeplugme.model.GeoQuery
import me.faulk.codeplugme.model.Repeater
import me.faulk.codeplugme.model.SupportedModel
import uniffi.codeplugme_core.ApiException

/**
 * The app's single door into codeplugme-core.
 *
 * Nothing else in the app calls `uniffi.codeplugme_core` directly, for the same
 * reason the macOS app funnels everything through one `RadioCore.swift`: the
 * decode contract with the core lives in exactly one file, so a change to the
 * JSON envelope has one place to break.
 *
 * Every call here is blocking in the core and is dispatched off the main thread.
 * The transfer calls are the exception and live in the foreground service
 * instead, because they run for minutes and must outlive the caller.
 *
 * The core functions are called by their fully qualified names rather than
 * imported. An import would be shadowed by a member of the same name here, and
 * the call would silently become infinite self-recursion: each level dispatches
 * to `Dispatchers.IO` and waits, so the pool fills with threads waiting on
 * deeper levels and the whole thing deadlocks with almost no CPU used, which
 * looks nothing like a stack overflow and takes a thread dump to find.
 */
object RadioCore {

    /** Every radio the core supports, with its capabilities. */
    suspend fun models(): List<SupportedModel> = withContext(Dispatchers.IO) {
        CodeplugJson.instance.decodeFromString<List<SupportedModel>>(
            uniffi.codeplugme_core.modelsJson()
        )
    }

    /** Parse a codeplug image into the typed, model-tagged document. */
    suspend fun dump(image: ByteArray): CodeplugDump = withContext(Dispatchers.IO) {
        CodeplugJson.instance.decodeFromString<CodeplugDump>(
            uniffi.codeplugme_core.dumpJson(image)
        )
    }

    /**
     * Apply an edit batch and return the new image.
     *
     * The core re-parses its own output before returning it, so a spec that
     * would corrupt the codeplug fails here rather than on the radio.
     */
    @Throws(ApiException::class)
    suspend fun applyEdits(image: ByteArray, editsJson: String): ByteArray =
        withContext(Dispatchers.IO) { uniffi.codeplugme_core.applyEdits(image, editsJson) }

    /** The model an image's bytes name, or null if they match no radio. */
    suspend fun detect(image: ByteArray): String? = withContext(Dispatchers.IO) {
        uniffi.codeplugme_core.detectModel(image)
    }

    /** The exact image size a model's codeplug must be. */
    suspend fun imageSize(modelId: String): Long = withContext(Dispatchers.IO) {
        uniffi.codeplugme_core.imageSizeFor(modelId).toLong()
    }

    /**
     * Map regions this image would blank if it were written to a radio.
     *
     * Empty for an image read off a radio, and for any model whose write leaves
     * unmodelled bytes alone. A non-empty answer belongs in front of the user
     * before the write, not in a diff afterwards.
     */
    suspend fun erasedRegions(image: ByteArray): List<String> = withContext(Dispatchers.IO) {
        uniffi.codeplugme_core.erasedRegions(image)
    }

    /**
     * Repeaters near [query] for the radio [image] is for, nearest first.
     *
     * Goes to the network. The free sources' downloads are cached in
     * [cacheDir] for a day so refining a search does not refetch them.
     */
    @Throws(ApiException::class)
    suspend fun geoFetch(query: GeoQuery, image: ByteArray, cacheDir: File): List<Repeater> =
        withContext(Dispatchers.IO) {
            cacheDir.mkdirs()
            CodeplugJson.instance.decodeFromString<List<Repeater>>(
                uniffi.codeplugme_core.geoFetch(
                    CodeplugJson.instance.encodeToString(query), image, cacheDir.path
                )
            )
        }

    /** How many repeaters can be picked for [image] in [mode]. */
    @Throws(ApiException::class)
    suspend fun geoCapacity(image: ByteArray, mode: BuildMode): GeoCapacity =
        withContext(Dispatchers.IO) {
            CodeplugJson.instance.decodeFromString<GeoCapacity>(
                uniffi.codeplugme_core.geoCapacity(image, mode.key)
            )
        }

    /**
     * A new image built from [image] plus exactly [picked].
     *
     * [image] itself is untouched. A selection that does not fit is an error
     * from the core, never silently truncated.
     */
    @Throws(ApiException::class)
    suspend fun geoBuild(image: ByteArray, picked: List<Repeater>, options: GeoBuildOptions): ByteArray =
        withContext(Dispatchers.IO) {
            uniffi.codeplugme_core.geoBuild(
                image,
                CodeplugJson.instance.encodeToString(picked),
                CodeplugJson.instance.encodeToString(options),
            )
        }
}
