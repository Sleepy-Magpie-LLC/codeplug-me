package me.faulk.codeplugme.model

/**
 * A radio model, matching the `id` strings the Rust core reports
 * (`core/src/model.rs`). The wire ids are a stable contract: saved documents
 * and preferences hold them, so never rename one.
 *
 * This is not `@Serializable` itself: the JSON fields that carry a model id
 * ([CodeplugDump.model], [SupportedModel.id]) are plain strings on the wire,
 * and each decides for itself how strict to be about an id this build doesn't
 * recognize. Use [fromWireId] to resolve one leniently.
 */
enum class RadioModel(val wireId: String, val displayName: String) {
    ANYTONE_D878_UV2("anytone-d878uv2", "AnyTone AT-D878UVII Plus"),
    QYT_KT5000("qyt-kt5000", "QYT KT-5000");

    companion object {
        /** Resolve a wire id to the [RadioModel] it names, or null if this build doesn't know it. */
        fun fromWireId(id: String): RadioModel? = entries.firstOrNull { it.wireId == id }
    }
}
