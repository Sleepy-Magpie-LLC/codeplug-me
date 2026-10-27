package me.faulk.codeplugme.state

/**
 * Converts a dumped enum value to the tag the core's edit API parses.
 *
 * The two directions are not symmetric, which is a wart in the core's JSON and
 * not something this app can fix from here: a dump emits Rust's variant names
 * ("MixedAnalog", "ChannelFree"), while the edit spec parses lowercase or
 * snake_case ("mixed_analog", "channel_free"). Getting this wrong does not fail
 * loudly, it silently drops the field from the batch, so both helpers exist and
 * each field uses the one its core setter expects.
 */

/** Simple lowercasing, with the two compound mode names spelled out. */
fun editValue(dumped: String): String = when (dumped) {
    "MixedAnalog" -> "mixed_analog"
    "MixedDigital" -> "mixed_digital"
    else -> dumped.lowercase()
}

/**
 * Convert a CamelCase dump value ("ChannelFree", "TwoTone", "Dcs") to the
 * snake_case string the edit API parses ("channel_free", "two_tone", "dcs").
 * An underscore is inserted before each interior capital, then lowercased.
 */
fun snakeEditValue(dumped: String): String = buildString {
    dumped.forEachIndexed { i, c ->
        if (i > 0 && c.isUpperCase()) append('_')
        append(c.lowercaseChar())
    }
}
