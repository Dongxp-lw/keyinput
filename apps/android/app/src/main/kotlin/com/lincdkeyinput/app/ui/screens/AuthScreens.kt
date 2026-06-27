package com.lincdkeyinput.app.ui.screens

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Fingerprint
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.lincdkeyinput.app.ui.PasswordStrengthBar
import com.lincdkeyinput.app.ui.components.PasswordField

private const val MIN_PASSWORD_LEN = 8

/** 首次创建保险库：设置主密码。 */
@Composable
fun OnboardingScreen(busy: Boolean, onCreate: (ByteArray) -> Unit) {
    var pw by remember { mutableStateOf("") }
    var confirm by remember { mutableStateOf("") }
    val mismatch = confirm.isNotEmpty() && pw != confirm
    val canSubmit = pw.length >= MIN_PASSWORD_LEN && pw == confirm && !busy

    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("创建保险库", style = MaterialTheme.typography.headlineMedium)
        Text(
            "用主密码加密本地保险库。主密码是唯一的根秘密，「无法找回」，请务必牢记。",
            style = MaterialTheme.typography.bodyMedium,
        )
        PasswordField(pw, { pw = it }, "主密码（至少 $MIN_PASSWORD_LEN 位）")
        PasswordStrengthBar(pw)
        PasswordField(confirm, { confirm = it }, "确认主密码")
        if (mismatch) {
            Text("两次输入不一致", color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
        }
        Button(
            onClick = {
                onCreate(pw.toByteArray(Charsets.UTF_8))
                pw = ""; confirm = ""
            },
            enabled = canSubmit,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("创建保险库")
        }
        if (busy) {
            CircularProgressIndicator(modifier = Modifier.align(Alignment.CenterHorizontally))
            Text(
                "正在用 Argon2id 派生密钥…",
                style = MaterialTheme.typography.bodySmall,
                textAlign = TextAlign.Center,
                modifier = Modifier.fillMaxWidth(),
            )
        }
    }
}

/** 解锁已有保险库：输入主密码，或（若已启用）生物识别。 */
@Composable
fun UnlockScreen(
    busy: Boolean,
    biometricAvailable: Boolean,
    failedAttempts: Int,
    lockoutRemainingSec: Int,
    onUnlock: (ByteArray) -> Unit,
    onBiometric: () -> Unit,
) {
    var pw by remember { mutableStateOf("") }
    val lockedOut = lockoutRemainingSec > 0
    val canSubmit = pw.isNotEmpty() && !busy && !lockedOut

    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("解锁保险库", style = MaterialTheme.typography.headlineMedium)
        PasswordField(pw, { pw = it }, "主密码")
        if (lockedOut) {
            Text(
                "尝试过于频繁，请 $lockoutRemainingSec 秒后再试",
                color = MaterialTheme.colorScheme.error,
                style = MaterialTheme.typography.bodySmall,
            )
        } else if (failedAttempts > 0) {
            Text(
                "已失败 $failedAttempts 次",
                color = MaterialTheme.colorScheme.error,
                style = MaterialTheme.typography.bodySmall,
            )
        }
        Button(
            onClick = {
                onUnlock(pw.toByteArray(Charsets.UTF_8))
                pw = ""
            },
            enabled = canSubmit,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("解锁")
        }
        if (biometricAvailable) {
            OutlinedButton(
                onClick = onBiometric,
                enabled = !busy && !lockedOut,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Icon(Icons.Filled.Fingerprint, contentDescription = null)
                Text("  用生物识别解锁")
            }
        }
        if (busy) {
            CircularProgressIndicator(modifier = Modifier.align(Alignment.CenterHorizontally))
        }
    }
}
