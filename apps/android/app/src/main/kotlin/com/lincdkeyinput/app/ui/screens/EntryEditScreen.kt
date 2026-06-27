package com.lincdkeyinput.app.ui.screens

import android.Manifest
import android.content.pm.PackageManager
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.AutoFixHigh
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.QrCodeScanner
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MenuAnchorType
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import com.lincdkeyinput.app.ui.defaultInputBehavior
import com.lincdkeyinput.app.ui.defaultSensitivity
import com.lincdkeyinput.app.ui.entryTypeLabel
import com.lincdkeyinput.app.ui.fieldKindLabel
import com.lincdkeyinput.data.OtpAuth
import uniffi.vault_core.EntryType
import uniffi.vault_core.FfiEntry
import uniffi.vault_core.FfiField
import uniffi.vault_core.FfiPasswordPolicy
import uniffi.vault_core.FfiTotpField
import uniffi.vault_core.FieldKind
import uniffi.vault_core.TotpAlgorithm
import java.util.UUID

private class EditFieldState(
    val id: String,
    label: String,
    kind: FieldKind,
    value: String,
    issuer: String = "",
    accountName: String = "",
    algorithm: TotpAlgorithm = TotpAlgorithm.SHA1,
    digits: UInt = 6u,
    periodSeconds: UInt = 30u,
) {
    var label by mutableStateOf(label)
    var kind by mutableStateOf(kind)
    var value by mutableStateOf(value)
    var issuer by mutableStateOf(issuer)
    var accountName by mutableStateOf(accountName)
    var algorithm by mutableStateOf(algorithm)
    var digits by mutableStateOf(digits)
    var periodSeconds by mutableStateOf(periodSeconds)
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun EntryEditScreen(
    initial: FfiEntry?,
    onBack: () -> Unit,
    onSave: (FfiEntry) -> Unit,
    onGenerate: (FfiPasswordPolicy) -> String?,
) {
    val entryId = remember { initial?.id ?: UUID.randomUUID().toString() }
    var title by remember { mutableStateOf(initial?.title ?: "") }
    var entryType by remember { mutableStateOf(initial?.entryType ?: EntryType.LOGIN) }
    var tags by remember { mutableStateOf(initial?.tags?.joinToString(" ") ?: "") }
    val fields = remember {
        mutableStateListOf<EditFieldState>().apply {
            initial?.fields?.forEach {
                val t = it.totp
                add(
                    EditFieldState(
                        id = it.id,
                        label = it.label,
                        kind = it.kind,
                        value = if (it.kind == FieldKind.TOTP) (t?.secret ?: it.value) else it.value,
                        issuer = t?.issuer ?: "",
                        accountName = t?.accountName ?: "",
                        algorithm = t?.algorithm ?: TotpAlgorithm.SHA1,
                        digits = t?.digits ?: 6u,
                        periodSeconds = t?.periodSeconds ?: 30u,
                    ),
                )
            }
        }
    }
    var generatorFor by remember { mutableStateOf<EditFieldState?>(null) }

    val context = LocalContext.current
    var scanFor by remember { mutableStateOf<EditFieldState?>(null) }
    val scanLauncher = rememberLauncherForActivityResult(ScanContract()) { result ->
        val target = scanFor
        scanFor = null
        val contents = result.contents
        if (target != null && contents != null) {
            val parsed = OtpAuth.parse(contents)
            if (parsed != null) {
                target.value = parsed.secret
                target.issuer = parsed.issuer
                target.accountName = parsed.accountName
                target.algorithm = parsed.algorithm
                target.digits = parsed.digits
                target.periodSeconds = parsed.periodSeconds
            } else {
                Toast.makeText(context, "不是有效的 2FA 二维码", Toast.LENGTH_SHORT).show()
            }
        }
    }
    val cameraPermissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted ->
        if (granted) {
            scanLauncher.launch(buildScanOptions())
        } else {
            Toast.makeText(context, "需要相机权限才能扫码", Toast.LENGTH_SHORT).show()
        }
    }
    fun startScan(field: EditFieldState) {
        scanFor = field
        if (ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
            PackageManager.PERMISSION_GRANTED
        ) {
            scanLauncher.launch(buildScanOptions())
        } else {
            cameraPermissionLauncher.launch(Manifest.permission.CAMERA)
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(if (initial == null) "新建条目" else "编辑条目") },
                navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "返回") } },
                actions = {
                    IconButton(onClick = {
                        onSave(buildEntry(entryId, title, entryType, tags, fields, initial))
                    }) { Icon(Icons.Filled.Check, "保存") }
                },
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            OutlinedTextField(title, { title = it }, label = { Text("标题") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            EnumDropdown("类型", EntryType.entries, entryType, ::entryTypeLabel) { entryType = it }
            OutlinedTextField(tags, { tags = it }, label = { Text("标签（空格分隔）") }, singleLine = true, modifier = Modifier.fillMaxWidth())

            Text("字段", style = MaterialTheme.typography.titleMedium)
            fields.forEachIndexed { index, f ->
                FieldEditor(
                    field = f,
                    onRemove = { fields.removeAt(index) },
                    onGenerate = { generatorFor = f },
                    onScan = { startScan(f) },
                )
            }
            OutlinedButton(
                onClick = { fields.add(EditFieldState(UUID.randomUUID().toString(), "", FieldKind.TEXT, "")) },
                modifier = Modifier.fillMaxWidth(),
            ) {
                Icon(Icons.Filled.Add, null); Text("  添加字段")
            }
        }
    }

    generatorFor?.let { target ->
        GeneratorDialog(
            onGenerate = onGenerate,
            onPick = { target.value = it },
            onDismiss = { generatorFor = null },
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun FieldEditor(field: EditFieldState, onRemove: () -> Unit, onGenerate: () -> Unit, onScan: () -> Unit) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(fieldKindLabel(field.kind), style = MaterialTheme.typography.labelMedium, modifier = Modifier.weight(1f))
                IconButton(onClick = onRemove) { Icon(Icons.Filled.Delete, "删除字段") }
            }
            OutlinedTextField(field.label, { field.label = it }, label = { Text("名称") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            EnumDropdown("类型", FieldKind.entries, field.kind, ::fieldKindLabel) { field.kind = it }
            OutlinedTextField(
                value = field.value,
                onValueChange = { input ->
                    val parsed = if (field.kind == FieldKind.TOTP) OtpAuth.parse(input) else null
                    if (parsed != null) {
                        field.value = parsed.secret
                        field.issuer = parsed.issuer
                        field.accountName = parsed.accountName
                        field.algorithm = parsed.algorithm
                        field.digits = parsed.digits
                        field.periodSeconds = parsed.periodSeconds
                    } else {
                        field.value = input
                    }
                },
                label = { Text(if (field.kind == FieldKind.TOTP) "TOTP 种子（Base32，或粘贴 otpauth:// 链接）" else "值") },
                singleLine = field.kind != FieldKind.MULTILINE && field.kind != FieldKind.NOTE,
                modifier = Modifier.fillMaxWidth(),
                trailingIcon = when (field.kind) {
                    FieldKind.PASSWORD -> {
                        { IconButton(onClick = onGenerate) { Icon(Icons.Filled.AutoFixHigh, "生成密码") } }
                    }
                    FieldKind.TOTP -> {
                        { IconButton(onClick = onScan) { Icon(Icons.Filled.QrCodeScanner, "扫码导入") } }
                    }
                    else -> null
                },
            )
            if (field.kind == FieldKind.TOTP) {
                OutlinedTextField(field.issuer, { field.issuer = it }, label = { Text("发行方（可留空，默认用标题）") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                OutlinedTextField(field.accountName, { field.accountName = it }, label = { Text("账户名（可留空）") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                EnumDropdown("算法", TotpAlgorithm.entries, field.algorithm, ::totpAlgorithmLabel) { field.algorithm = it }
                EnumDropdown("位数", listOf(6u, 8u), field.digits, { "$it 位" }) { field.digits = it }
                OutlinedTextField(
                    value = field.periodSeconds.toString(),
                    onValueChange = { v -> field.periodSeconds = v.toUIntOrNull()?.takeIf { it > 0u } ?: field.periodSeconds },
                    label = { Text("周期（秒）") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun <T> EnumDropdown(label: String, options: List<T>, selected: T, labelOf: (T) -> String, onSelect: (T) -> Unit) {
    var expanded by remember { mutableStateOf(false) }
    ExposedDropdownMenuBox(expanded = expanded, onExpandedChange = { expanded = it }) {
        OutlinedTextField(
            value = labelOf(selected),
            onValueChange = {},
            readOnly = true,
            label = { Text(label) },
            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded) },
            modifier = Modifier.menuAnchor(MenuAnchorType.PrimaryNotEditable).fillMaxWidth(),
        )
        ExposedDropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
            options.forEach { opt ->
                DropdownMenuItem(text = { Text(labelOf(opt)) }, onClick = { onSelect(opt); expanded = false })
            }
        }
    }
}

@Composable
private fun GeneratorDialog(
    onGenerate: (FfiPasswordPolicy) -> String?,
    onPick: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var length by remember { mutableFloatStateOf(20f) }
    var symbols by remember { mutableStateOf(true) }
    var excludeAmbiguous by remember { mutableStateOf(false) }
    var preview by remember { mutableStateOf("") }

    fun policy() = FfiPasswordPolicy(length.toInt().toUInt(), true, true, true, symbols, excludeAmbiguous)
    fun regen() { preview = onGenerate(policy()) ?: "" }

    LaunchedEffect(Unit) { regen() }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("生成密码") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(preview.ifBlank { "…" }, style = MaterialTheme.typography.titleLarge)
                Text("长度：${length.toInt()}")
                Slider(value = length, onValueChange = { length = it }, valueRange = 8f..64f)
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Checkbox(checked = symbols, onCheckedChange = { symbols = it }); Text("包含符号")
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Checkbox(checked = excludeAmbiguous, onCheckedChange = { excludeAmbiguous = it }); Text("排除易混淆字符")
                }
                TextButton(onClick = { regen() }) { Text("重新生成") }
            }
        },
        confirmButton = { TextButton(onClick = { if (preview.isNotBlank()) onPick(preview); onDismiss() }) { Text("使用") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}

private fun buildScanOptions(): ScanOptions = ScanOptions().apply {
    setDesiredBarcodeFormats(ScanOptions.QR_CODE)
    setPrompt("对准 2FA 二维码")
    setBeepEnabled(false)
    setOrientationLocked(false)
}

private fun totpAlgorithmLabel(a: TotpAlgorithm): String = when (a) {
    TotpAlgorithm.SHA1 -> "SHA-1"
    TotpAlgorithm.SHA256 -> "SHA-256"
    TotpAlgorithm.SHA512 -> "SHA-512"
}

private fun buildEntry(
    id: String,
    title: String,
    entryType: EntryType,
    tags: String,
    fields: List<EditFieldState>,
    initial: FfiEntry?,
): FfiEntry {
    val now = System.currentTimeMillis()
    val ffiFields = fields.map { f ->
        FfiField(
            id = f.id,
            label = f.label,
            kind = f.kind,
            value = f.value,
            sensitivity = defaultSensitivity(f.kind),
            inputBehavior = defaultInputBehavior(f.kind),
            requireReauth = false,
            totp = if (f.kind == FieldKind.TOTP) {
                FfiTotpField(
                    issuer = f.issuer.ifBlank { title },
                    accountName = f.accountName,
                    secret = f.value,
                    algorithm = f.algorithm,
                    digits = f.digits,
                    periodSeconds = f.periodSeconds,
                )
            } else {
                null
            },
        )
    }
    return FfiEntry(
        id = id,
        title = title,
        entryType = entryType,
        fields = ffiFields,
        tags = tags.split(" ", "\n").map { it.trim() }.filter { it.isNotEmpty() },
        favorite = initial?.favorite ?: false,
        archived = initial?.archived ?: false,
        createdAt = initial?.createdAt ?: now,
        updatedAt = now,
    )
}
