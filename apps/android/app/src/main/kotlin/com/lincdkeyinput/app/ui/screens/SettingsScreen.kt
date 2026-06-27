package com.lincdkeyinput.app.ui.screens

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.lincdkeyinput.app.ui.PasswordStrengthBar
import com.lincdkeyinput.app.ui.components.PasswordField

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    busy: Boolean,
    onBack: () -> Unit,
    onChangePassword: (old: ByteArray, new: ByteArray) -> Unit,
    onExport: (uri: Uri, passphrase: ByteArray) -> Unit,
    onImport: (uri: Uri, passphrase: ByteArray) -> Unit,
    biometricSupported: Boolean,
    biometricEnabled: Boolean,
    onEnableBiometric: (masterPassword: ByteArray) -> Unit,
    onDisableBiometric: () -> Unit,
    onArmSafPicker: () -> Unit,
) {
    // 待用户输入口令后再触发的 SAF 操作。
    var pendingExportPass by remember { mutableStateOf<ByteArray?>(null) }
    var importUri by remember { mutableStateOf<Uri?>(null) }
    var showExportDialog by remember { mutableStateOf(false) }
    var showBiometricDialog by remember { mutableStateOf(false) }

    val createDoc = rememberLauncherForActivityResult(
        ActivityResultContracts.CreateDocument("application/octet-stream"),
    ) { uri ->
        val pass = pendingExportPass
        pendingExportPass = null
        if (uri != null && pass != null) onExport(uri, pass) else pass?.fill(0)
    }
    val openDoc = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument(),
    ) { uri -> importUri = uri }

    Scaffold(
        topBar = {
            androidx.compose.material3.TopAppBar(
                title = { Text("设置") },
                navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "返回") } },
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            ChangePasswordCard(busy, onChangePassword)

            if (biometricSupported) {
                Card(modifier = Modifier.fillMaxWidth()) {
                    Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text("生物识别解锁", style = MaterialTheme.typography.titleMedium)
                        Text(
                            if (biometricEnabled) {
                                "已启用。下次锁定后可用指纹/人脸快速解锁。"
                            } else {
                                "启用后，主密码会用受硬件保护的密钥加密保存，可用指纹/人脸解锁。"
                            },
                            style = MaterialTheme.typography.bodySmall,
                        )
                        if (biometricEnabled) {
                            OutlinedButton(onClick = onDisableBiometric, enabled = !busy, modifier = Modifier.fillMaxWidth()) { Text("关闭生物识别解锁") }
                        } else {
                            OutlinedButton(onClick = { showBiometricDialog = true }, enabled = !busy, modifier = Modifier.fillMaxWidth()) { Text("启用生物识别解锁") }
                        }
                    }
                }
            }

            Card(modifier = Modifier.fillMaxWidth()) {
                Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("加密备份", style = MaterialTheme.typography.titleMedium)
                    Text("导出独立于本地库的加密备份（用单独的导出口令保护），可在新设备恢复。", style = MaterialTheme.typography.bodySmall)
                    OutlinedButton(onClick = { showExportDialog = true }, enabled = !busy, modifier = Modifier.fillMaxWidth()) { Text("导出加密备份") }
                    OutlinedButton(onClick = { onArmSafPicker(); openDoc.launch(arrayOf("*/*")) }, enabled = !busy, modifier = Modifier.fillMaxWidth()) { Text("从备份导入（整库恢复）") }
                }
            }
            HorizontalDivider()
            Text("离线优先：本应用不申请网络权限，明文不离开设备。", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }

    if (showExportDialog) {
        PassphraseDialog(
            title = "设置导出口令",
            confirmLabel = "选择保存位置",
            onConfirm = { pass ->
                showExportDialog = false
                pendingExportPass = pass
                onArmSafPicker()
                createDoc.launch("vault-backup.pivpkg")
            },
            onDismiss = { showExportDialog = false },
        )
    }
    importUri?.let { uri ->
        PassphraseDialog(
            title = "输入导出口令以导入",
            confirmLabel = "导入",
            onConfirm = { pass -> importUri = null; onImport(uri, pass) },
            onDismiss = { importUri = null },
        )
    }
    if (showBiometricDialog) {
        PassphraseDialog(
            title = "输入主密码以启用",
            confirmLabel = "启用",
            label = "主密码",
            onConfirm = { pass -> showBiometricDialog = false; onEnableBiometric(pass) },
            onDismiss = { showBiometricDialog = false },
        )
    }
}

@Composable
private fun ChangePasswordCard(busy: Boolean, onChangePassword: (ByteArray, ByteArray) -> Unit) {
    var old by remember { mutableStateOf("") }
    var new by remember { mutableStateOf("") }
    var confirm by remember { mutableStateOf("") }
    val canSubmit = old.isNotEmpty() && new.length >= 8 && new == confirm && !busy

    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("修改主密码", style = MaterialTheme.typography.titleMedium)
            PasswordField(old, { old = it }, "当前主密码")
            PasswordField(new, { new = it }, "新主密码（至少 8 位）")
            PasswordStrengthBar(new)
            PasswordField(confirm, { confirm = it }, "确认新主密码")
            Button(
                onClick = {
                    onChangePassword(old.toByteArray(Charsets.UTF_8), new.toByteArray(Charsets.UTF_8))
                    old = ""; new = ""; confirm = ""
                },
                enabled = canSubmit,
                modifier = Modifier.fillMaxWidth(),
            ) { Text("修改") }
        }
    }
}

@Composable
private fun PassphraseDialog(
    title: String,
    confirmLabel: String,
    onConfirm: (ByteArray) -> Unit,
    onDismiss: () -> Unit,
    label: String = "导出口令",
) {
    var pass by remember { mutableStateOf("") }
    androidx.compose.material3.AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = { PasswordField(pass, { pass = it }, label) },
        confirmButton = {
            androidx.compose.material3.TextButton(
                onClick = { onConfirm(pass.toByteArray(Charsets.UTF_8)); pass = "" },
                enabled = pass.isNotEmpty(),
            ) { Text(confirmLabel) }
        },
        dismissButton = { androidx.compose.material3.TextButton(onClick = onDismiss) { Text("取消") } },
    )
}
