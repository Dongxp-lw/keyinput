package com.lincdkeyinput.data

/**
 * RFC 4648 Base32 解码（TOTP 种子常用 Base32 文本表示）。核心的 TOTP 接口接受**解码后的原始字节**
 * （`otpauth://` / Base32 导入在核心层「待确认」），故在平台侧做解码。
 *
 * 忽略空白与大小写，丢弃 `=` 填充。遇到非字母表字符返回 `null`（交由上层提示重输）。
 */
object Base32 {
    private const val ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"

    fun decodeOrNull(input: String): ByteArray? {
        val cleaned = input.trim().replace(" ", "").replace("-", "").trimEnd('=').uppercase()
        if (cleaned.isEmpty()) return null
        val out = ArrayList<Byte>(cleaned.length * 5 / 8)
        var buffer = 0
        var bitsLeft = 0
        for (c in cleaned) {
            val v = ALPHABET.indexOf(c)
            if (v < 0) return null
            buffer = (buffer shl 5) or v
            bitsLeft += 5
            if (bitsLeft >= 8) {
                bitsLeft -= 8
                out.add(((buffer shr bitsLeft) and 0xFF).toByte())
            }
        }
        return out.toByteArray()
    }
}
