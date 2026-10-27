package me.faulk.codeplugme.model

import kotlinx.serialization.Serializable

/**
 * One `(tag, label)` choice for a picker. `tag` is the value the core emits
 * in a dump and accepts in an edit; `label` is what the user reads.
 */
@Serializable
data class PickerOption(val tag: String, val label: String)
