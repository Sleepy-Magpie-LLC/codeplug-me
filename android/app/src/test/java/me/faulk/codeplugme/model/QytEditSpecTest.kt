package me.faulk.codeplugme.model

import kotlinx.serialization.encodeToString
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The QYT edit spec's minimal encoding, and specifically the explicit-null
 * case for [QytChannelEdit.txFrequencyHz]: an absent key must mean "leave it
 * alone" and an explicit JSON `null` must mean "clear it", even though the
 * rest of the model relies on absent-for-null encoding everywhere else.
 */
class QytEditSpecTest {
    @Test
    fun `leaving tx frequency untouched omits the key entirely`() {
        val edit = QytChannelEdit(index = 1, name = "KEEP")

        val json = CodeplugJson.instance.encodeToString(edit)

        assertEquals("""{"index":1,"name":"KEEP"}""", json)
    }

    @Test
    fun `clearing tx frequency encodes an explicit JSON null`() {
        val edit = QytChannelEdit(index = 2, txFrequencyHz = QytChannelEdit.CLEAR_TX_FREQUENCY)

        val json = CodeplugJson.instance.encodeToString(edit)

        assertEquals("""{"index":2,"tx_frequency_hz":null}""", json)
    }

    @Test
    fun `setting tx frequency encodes the value`() {
        val edit = QytChannelEdit(
            index = 0,
            txFrequencyHz = QytChannelEdit.txFrequencyHzValue(462_562_500),
        )

        val json = CodeplugJson.instance.encodeToString(edit)

        assertEquals("""{"index":0,"tx_frequency_hz":462562500}""", json)
    }

    @Test
    fun `a settings edit only carries the fields that were changed`() {
        val edit = QytSettingsEdit(squelch = 9, tot = 0, beep = true, scanRev = "SE", rememberPower = true)

        val json = CodeplugJson.instance.encodeToString(edit)

        assertEquals(
            """{"squelch":9,"tot":0,"beep":true,"scan_rev":"SE","remember_power":true}""",
            json,
        )
    }

    @Test
    fun `a static message edit encodes as a bare string under the core's key`() {
        val spec = QytEditSpec(staticMessage = "N7FYZ MOBILE")

        assertEquals("""{"static_message":"N7FYZ MOBILE"}""", CodeplugJson.instance.encodeToString(spec))
        assertEquals(false, spec.isEmpty)
    }

    @Test
    fun `an empty qyt edit spec encodes as an empty object`() {
        val spec = QytEditSpec()

        assertEquals("{}", CodeplugJson.instance.encodeToString(spec))
        assertEquals("""{"qyt":{}}""", CodeplugJson.instance.encodeToString(QytEditEnvelope(spec)))
    }
}
