package com.lincdkeyinput.data

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * 生物识别解锁门控（L4-BIO）。
 *
 * 设计：用 Android Keystore 生成一个绑定生物识别的 AES-GCM 密钥
 * （[KeyGenParameterSpec.Builder.setUserAuthenticationRequired]=true、
 * [setInvalidatedByBiometricEnrollment]=true），用它把"主密码"加密后落盘。
 * 解锁时通过 [BiometricPrompt] + [BiometricPrompt.CryptoObject] 让用户做一次生物识别，
 * 成功后才解开密钥、解密出主密码，再走正常的 [VaultManager.unlock] 流程。
 *
 * 密钥本身存在硬件支持的 Keystore 中，APP 进程拿不到原始密钥；落盘文件只是密文 + IV。
 * 注册新的指纹/人脸会让密钥永久失效（[KeyPermanentlyInvalidatedException]），此时自动降级为主密码解锁。
 */
class BiometricGate(context: Context) {

    private val appContext = context.applicationContext
    private val bioFile = File(appContext.filesDir, BIO_FILE)

    /** 设备是否具备可用的强生物识别（已录入且硬件就绪）。 */
    fun isHardwareAvailable(): Boolean =
        BiometricManager.from(appContext)
            .canAuthenticate(BiometricManager.Authenticators.BIOMETRIC_STRONG) ==
            BiometricManager.BIOMETRIC_SUCCESS

    /** 用户是否已为本保险库启用了生物识别解锁。 */
    fun isEnabled(): Boolean = bioFile.exists()

    /** 关闭生物识别解锁：删除密文并销毁 Keystore 密钥。 */
    fun disable() {
        runCatching { bioFile.delete() }
        runCatching { keyStore().deleteEntry(KEY_ALIAS) }
    }

    /**
     * 启用生物识别：在一次生物识别认证后，用 Keystore 密钥加密 [masterPassword] 并落盘。
     * 无论成功失败都会把传入的 [masterPassword] 清零。
     */
    fun enable(
        activity: FragmentActivity,
        masterPassword: ByteArray,
        onResult: (success: Boolean, error: String?) -> Unit,
    ) {
        val cipher = try {
            encryptCipher()
        } catch (e: Exception) {
            masterPassword.fill(0)
            onResult(false, "无法初始化密钥：${e.message}")
            return
        }
        prompt(
            activity = activity,
            title = "启用生物识别解锁",
            cipher = cipher,
            onSuccess = { c ->
                try {
                    val ciphertext = c.doFinal(masterPassword)
                    bioFile.writeBytes(c.iv + ciphertext)
                    onResult(true, null)
                } catch (e: Exception) {
                    onResult(false, "加密失败：${e.message}")
                } finally {
                    masterPassword.fill(0)
                }
            },
            onError = { msg ->
                masterPassword.fill(0)
                onResult(false, msg)
            },
        )
    }

    /**
     * 用生物识别解锁：认证成功后解密出主密码字节，交给 [onSuccess]。
     * 调用方负责在用完后把回调里的字节清零。
     */
    fun unlock(
        activity: FragmentActivity,
        onSuccess: (masterPassword: ByteArray) -> Unit,
        onError: (String) -> Unit,
    ) {
        val data = try {
            bioFile.readBytes()
        } catch (e: Exception) {
            onError("没有生物识别数据，请用主密码解锁")
            return
        }
        if (data.size <= IV_LEN) {
            onError("生物识别数据损坏，请用主密码解锁")
            return
        }
        val iv = data.copyOfRange(0, IV_LEN)
        val ciphertext = data.copyOfRange(IV_LEN, data.size)
        val cipher = try {
            decryptCipher(iv)
        } catch (e: KeyPermanentlyInvalidatedException) {
            disable()
            onError("生物识别已变更，请用主密码解锁")
            return
        } catch (e: Exception) {
            onError("无法初始化密钥：${e.message}")
            return
        }
        prompt(
            activity = activity,
            title = "用生物识别解锁",
            cipher = cipher,
            onSuccess = { c ->
                try {
                    onSuccess(c.doFinal(ciphertext))
                } catch (e: Exception) {
                    onError("解密失败：${e.message}")
                }
            },
            onError = onError,
        )
    }

    private fun prompt(
        activity: FragmentActivity,
        title: String,
        cipher: Cipher,
        onSuccess: (Cipher) -> Unit,
        onError: (String) -> Unit,
    ) {
        val executor = ContextCompat.getMainExecutor(activity)
        val biometricPrompt = BiometricPrompt(
            activity,
            executor,
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    val c = result.cryptoObject?.cipher
                    if (c != null) onSuccess(c) else onError("认证结果缺少密钥")
                }

                override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                    onError(errString.toString())
                }
            },
        )
        val info = BiometricPrompt.PromptInfo.Builder()
            .setTitle(title)
            .setNegativeButtonText("用主密码")
            .setAllowedAuthenticators(BiometricManager.Authenticators.BIOMETRIC_STRONG)
            .build()
        biometricPrompt.authenticate(info, BiometricPrompt.CryptoObject(cipher))
    }

    private fun keyStore(): KeyStore =
        KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }

    private fun existingKey(): SecretKey {
        val entry = keyStore().getEntry(KEY_ALIAS, null) as? KeyStore.SecretKeyEntry
            ?: throw IllegalStateException("生物识别密钥不存在")
        return entry.secretKey
    }

    private fun createKey(): SecretKey {
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .setUserAuthenticationRequired(true)
                .setInvalidatedByBiometricEnrollment(true)
                .build(),
        )
        return generator.generateKey()
    }

    private fun encryptCipher(): Cipher {
        // 每次启用都换一把新密钥，旧密文随之作废。
        runCatching { keyStore().deleteEntry(KEY_ALIAS) }
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, createKey())
        return cipher
    }

    private fun decryptCipher(iv: ByteArray): Cipher {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, existingKey(), GCMParameterSpec(GCM_TAG_BITS, iv))
        return cipher
    }

    private companion object {
        const val ANDROID_KEYSTORE = "AndroidKeyStore"
        const val KEY_ALIAS = "vault_bio_key"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val BIO_FILE = "bio.bin"
        const val IV_LEN = 12
        const val GCM_TAG_BITS = 128
    }
}
