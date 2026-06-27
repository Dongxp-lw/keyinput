package com.lincdkeyinput.data

import android.net.Uri
import uniffi.vault_core.TotpAlgorithm

/** 从 otpauth:// 链接解析出的 TOTP 参数。 */
data class OtpAuthData(
    val secret: String, // Base32 原文（与 URI 中一致）
    val issuer: String,
    val accountName: String,
    val algorithm: TotpAlgorithm,
    val digits: UInt,
    val periodSeconds: UInt,
)

/**
 * otpauth:// Key URI 解析（事实标准，非 RFC 6238 的一部分）。
 *
 * 仅做**文本粘贴**解析，不引入相机/二维码扫描（保持离线优先与最小权限；扫码留作后续单独决策）。
 * 形如 `otpauth://totp/Issuer:account?secret=BASE32&issuer=...&algorithm=SHA1&digits=6&period=30`。
 * 非 otpauth/totp 或缺 secret 时返回 null。未知/非法的 algorithm/digits/period 回退到默认值。
 */
object OtpAuth {

    fun parse(input: String): OtpAuthData? {
        val trimmed = input.trim()
        if (!trimmed.startsWith("otpauth://", ignoreCase = true)) return null
        val uri = try {
            Uri.parse(trimmed)
        } catch (e: Exception) {
            return null
        }
        if (!"totp".equals(uri.host, ignoreCase = true)) return null
        val secret = uri.getQueryParameter("secret")?.takeIf { it.isNotBlank() } ?: return null

        // label = path 去掉前导 '/'，形如 "Issuer:account" 或 "account"（已被 Uri 解码）。
        val label = uri.path?.trimStart('/').orEmpty()
        val labelIssuer: String
        val account: String
        if (label.contains(':')) {
            labelIssuer = label.substringBefore(':').trim()
            account = label.substringAfter(':').trim()
        } else {
            labelIssuer = ""
            account = label
        }
        val issuer = uri.getQueryParameter("issuer")?.takeIf { it.isNotBlank() } ?: labelIssuer

        val algorithm = when (uri.getQueryParameter("algorithm")?.uppercase()) {
            "SHA256" -> TotpAlgorithm.SHA256
            "SHA512" -> TotpAlgorithm.SHA512
            else -> TotpAlgorithm.SHA1
        }
        val digits = uri.getQueryParameter("digits")?.toUIntOrNull()?.takeIf { it == 6u || it == 8u } ?: 6u
        val period = uri.getQueryParameter("period")?.toUIntOrNull()?.takeIf { it > 0u } ?: 30u

        return OtpAuthData(
            secret = secret,
            issuer = issuer,
            accountName = account,
            algorithm = algorithm,
            digits = digits,
            periodSeconds = period,
        )
    }
}
