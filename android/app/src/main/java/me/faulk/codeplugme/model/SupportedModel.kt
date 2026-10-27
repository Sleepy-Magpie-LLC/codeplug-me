package me.faulk.codeplugme.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * One entry of `codeplugme_models_json`: a radio this build can program.
 *
 * `id` stays a plain string (rather than [RadioModel]) so callers that just
 * need to compare or display it, like the write-target guard, never have to
 * unwrap a lookup that only matters when resolving the typed model.
 */
@Serializable
data class SupportedModel(
    val id: String,
    @SerialName("display_name") val displayName: String,
    @SerialName("image_size") val imageSize: Int,
    val capabilities: Capabilities,
) {
    /**
     * The [RadioModel] this id names, or null if this build doesn't recognize
     * it yet. A future core listing a new radio should not make the whole
     * models list fail to decode; it should just be a model nothing here can
     * fully key off of.
     */
    val model: RadioModel? get() = RadioModel.fromWireId(id)
}
