package me.faulk.codeplugme.core

import uniffi.codeplugme_core.ApiException

/**
 * What to show the user when something the core touched failed.
 *
 * `ApiException.Failed` carries the core's own message in [ApiException.Failed.msg],
 * but UniFFI generates its `message` as the field rendering `"msg=..."`, which
 * is not something to put in front of a user. Every place the app displays a
 * caught throwable goes through here so the prefix is stripped in exactly one
 * file rather than leaking into screens one at a time.
 */
val Throwable.userMessage: String
    get() = when (this) {
        is ApiException.Failed -> msg
        else -> message ?: toString()
    }
