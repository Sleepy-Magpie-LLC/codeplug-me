package me.faulk.codeplugme.core

import java.io.IOException
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.codeplugme_core.ApiException

class CoreErrorsTest {

    /**
     * UniFFI renders a non-flat error's `message` from its field names, so the
     * raw `message` here is `"msg=..."`. Nothing user-facing may show that.
     */
    @Test
    fun a_core_failure_is_shown_without_uniffis_field_prefix() {
        val e = ApiException.Failed("radio stopped responding")
        assertEquals("msg=radio stopped responding", e.message)
        assertEquals("radio stopped responding", e.userMessage)
    }

    @Test
    fun an_ordinary_throwable_keeps_its_own_message() {
        assertEquals("disk full", IOException("disk full").userMessage)
    }

    @Test
    fun a_throwable_with_no_message_falls_back_to_its_type() {
        val e = IllegalStateException()
        assertEquals(e.toString(), e.userMessage)
    }
}
