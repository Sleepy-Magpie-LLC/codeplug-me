package me.faulk.codeplugme.ui.importer

import java.security.GeneralSecurityException
import java.util.Base64
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The token encryption format. A software AES key stands in for the Keystore
 * one, which the JVM test runner does not have.
 */
class TokenCipherTest {

    private fun newKey(): SecretKey = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()

    @Test
    fun roundTrips() {
        val key = newKey()
        val stored = TokenCipher.encrypt(key, "rbuapp_abc123")
        assertEquals("rbuapp_abc123", TokenCipher.decrypt(key, stored))
    }

    @Test
    fun storedValueHidesTheToken() {
        val stored = TokenCipher.encrypt(newKey(), "rbuapp_abc123")
        assertTrue(!String(Base64.getDecoder().decode(stored), Charsets.ISO_8859_1).contains("rbuapp"))
    }

    @Test
    fun usesAFreshIvEachTime() {
        val key = newKey()
        assertNotEquals(TokenCipher.encrypt(key, "same"), TokenCipher.encrypt(key, "same"))
    }

    @Test
    fun wrongKeyFails() {
        val stored = TokenCipher.encrypt(newKey(), "rbuapp_abc123")
        assertThrows(GeneralSecurityException::class.java) { TokenCipher.decrypt(newKey(), stored) }
    }

    @Test
    fun tamperedDataFails() {
        val key = newKey()
        val bytes = Base64.getDecoder().decode(TokenCipher.encrypt(key, "rbuapp_abc123"))
        bytes[bytes.size - 1] = (bytes[bytes.size - 1].toInt() xor 1).toByte()
        val tampered = Base64.getEncoder().encodeToString(bytes)
        assertThrows(GeneralSecurityException::class.java) { TokenCipher.decrypt(key, tampered) }
    }

    @Test
    fun malformedValueFails() {
        val key = newKey()
        assertThrows(IllegalArgumentException::class.java) { TokenCipher.decrypt(key, "not base64!") }
        assertThrows(IllegalArgumentException::class.java) { TokenCipher.decrypt(key, "AAAA") }
    }
}
