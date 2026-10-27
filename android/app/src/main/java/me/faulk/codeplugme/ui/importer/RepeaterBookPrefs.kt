package me.faulk.codeplugme.ui.importer

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Log
import java.security.GeneralSecurityException
import java.security.KeyStore
import java.util.Base64
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import me.faulk.codeplugme.model.RepeaterBookAuth

/**
 * The user's RepeaterBook token and contact email, in private preferences.
 *
 * The token is encrypted with an AES key held in the Android Keystore, the
 * counterpart of the Keychain on macOS. The email is not secret and is
 * stored as is.
 *
 * RepeaterBook binds each app token to a User-Agent naming a contact address,
 * so both are the user's own and nothing here has a default.
 */
object RepeaterBookPrefs {

    /**
     * The saved credentials, or null unless both are set.
     *
     * A token saved in plain text by an older version is encrypted on first
     * read. A token that no longer decrypts (the Keystore key was wiped) is
     * cleared, so the settings ask for it again.
     */
    fun load(context: Context): RepeaterBookAuth? {
        val p = prefs(context)
        val contact = p.getString(KEY_CONTACT, null).orEmpty()
        val token = p.getString(KEY_TOKEN_ENC, null)?.let { stored ->
            try {
                TokenCipher.decrypt(keystoreKey(), stored)
            } catch (e: GeneralSecurityException) {
                Log.w(TAG, "RepeaterBook token could not be decrypted; clearing it", e)
                p.edit().remove(KEY_TOKEN_ENC).apply()
                null
            } catch (e: IllegalArgumentException) {
                Log.w(TAG, "RepeaterBook token is malformed; clearing it", e)
                p.edit().remove(KEY_TOKEN_ENC).apply()
                null
            }
        } ?: p.getString(KEY_LEGACY_TOKEN, null)?.also { legacy ->
            save(context, legacy, contact)
        }
        return if (token.isNullOrBlank() || contact.isBlank()) null else RepeaterBookAuth(token, contact)
    }

    /** Save both fields; blank ones clear the credentials. */
    fun save(context: Context, token: String, contact: String) {
        val trimmed = token.trim()
        val edit = prefs(context).edit()
            .remove(KEY_LEGACY_TOKEN)
            .putString(KEY_CONTACT, contact.trim())
        if (trimmed.isEmpty()) {
            edit.remove(KEY_TOKEN_ENC)
        } else {
            edit.putString(KEY_TOKEN_ENC, TokenCipher.encrypt(keystoreKey(), trimmed))
        }
        edit.apply()
    }

    /** The app's Keystore key for the token, created on first use. */
    private fun keystoreKey(): SecretKey {
        val store = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        (store.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }
        val spec = KeyGenParameterSpec.Builder(
            KEY_ALIAS,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .build()
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
            .apply { init(spec) }
            .generateKey()
    }

    private fun prefs(context: Context) =
        context.getSharedPreferences("repeaterbook", Context.MODE_PRIVATE)

    private const val TAG = "RepeaterBookPrefs"
    private const val KEYSTORE = "AndroidKeyStore"
    private const val KEY_ALIAS = "repeaterbook_token"
    /** Where versions before encryption kept the token in plain text. */
    private const val KEY_LEGACY_TOKEN = "token"
    private const val KEY_TOKEN_ENC = "token_enc"
    private const val KEY_CONTACT = "contact"
}

/**
 * AES-GCM for one short string, stored as base64 of the IV followed by the
 * ciphertext. Separate from the Keystore so it can be tested with a plain key.
 */
internal object TokenCipher {

    /** Encrypt [plain] under [key]; the cipher picks a fresh random IV. */
    fun encrypt(key: SecretKey, plain: String): String {
        val cipher = Cipher.getInstance(TRANSFORMATION).apply { init(Cipher.ENCRYPT_MODE, key) }
        val sealed = cipher.doFinal(plain.toByteArray(Charsets.UTF_8))
        return Base64.getEncoder().encodeToString(cipher.iv + sealed)
    }

    /**
     * Decrypt a value from [encrypt]. Throws [GeneralSecurityException] when
     * the key is wrong or the data was altered, and [IllegalArgumentException]
     * when it is not valid base64 or too short.
     */
    fun decrypt(key: SecretKey, stored: String): String {
        val bytes = Base64.getDecoder().decode(stored)
        require(bytes.size > IV_BYTES) { "stored token is too short" }
        val cipher = Cipher.getInstance(TRANSFORMATION).apply {
            init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(TAG_BITS, bytes, 0, IV_BYTES))
        }
        return String(cipher.doFinal(bytes, IV_BYTES, bytes.size - IV_BYTES), Charsets.UTF_8)
    }

    private const val TRANSFORMATION = "AES/GCM/NoPadding"
    private const val IV_BYTES = 12
    private const val TAG_BITS = 128
}
