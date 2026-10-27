package me.faulk.codeplugme.model

import kotlinx.serialization.json.Json

/**
 * The single [Json] configuration the rest of the app decodes dumps and
 * encodes edits with.
 *
 * `ignoreUnknownKeys` means a future core field never breaks decoding here.
 * `encodeDefaults = false` and `explicitNulls = false` together are what
 * make an edit spec minimal: an untouched, nullable field is left out of the
 * encoded JSON rather than sent as `null`, so the core only applies what
 * actually changed. [QytChannelEdit.txFrequencyHz] is the one field that
 * needs an explicit JSON `null` anyway; see its doc comment for how it gets
 * one despite this setting.
 */
object CodeplugJson {
    val instance: Json = Json {
        ignoreUnknownKeys = true
        encodeDefaults = false
        explicitNulls = false
    }
}
