package com.lincdkeyinput.app.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

/** 主密码强度分级（仅作引导提示，不阻止提交；真正的长度下限校验仍在调用方）。 */
enum class PasswordStrength(val label: String, val fraction: Float, val color: Color) {
    WEAK("弱", 0.34f, Color(0xFFD32F2F)),
    MEDIUM("中", 0.67f, Color(0xFFF9A825)),
    STRONG("强", 1f, Color(0xFF388E3C)),
}

/**
 * 轻量启发式强度估算（不引入 zxcvbn 等重依赖）：综合长度与字符类别多样性。
 * 返回 null 表示空输入（调用方不显示）。这是 UI 引导，不是安全判定。
 */
fun estimatePasswordStrength(password: String): PasswordStrength? {
    if (password.isEmpty()) return null
    var classes = 0
    if (password.any { it.isLowerCase() }) classes++
    if (password.any { it.isUpperCase() }) classes++
    if (password.any { it.isDigit() }) classes++
    if (password.any { !it.isLetterOrDigit() }) classes++
    val len = password.length
    var score = 0
    if (len >= 8) score++
    if (len >= 12) score++
    if (len >= 16) score++
    if (classes >= 2) score++
    if (classes >= 3) score++
    return when {
        len < 8 || score <= 2 -> PasswordStrength.WEAK
        score == 3 -> PasswordStrength.MEDIUM
        else -> PasswordStrength.STRONG
    }
}

/** 主密码强度指示条：随输入实时显示弱/中/强。空输入时不渲染。 */
@Composable
fun PasswordStrengthBar(password: String, modifier: Modifier = Modifier) {
    val strength = estimatePasswordStrength(password) ?: return
    Column(modifier = modifier.fillMaxWidth()) {
        LinearProgressIndicator(
            progress = { strength.fraction },
            color = strength.color,
            modifier = Modifier.fillMaxWidth(),
        )
        Text(
            "强度：${strength.label}",
            style = MaterialTheme.typography.bodySmall,
            color = strength.color,
            modifier = Modifier.padding(top = 4.dp),
        )
    }
}
