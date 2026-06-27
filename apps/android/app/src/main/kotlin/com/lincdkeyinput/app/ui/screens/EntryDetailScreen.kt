package com.lincdkeyinput.app.ui.screens

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
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Visibility
import androidx.compose.material.icons.filled.VisibilityOff
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.lincdkeyinput.app.ui.entryTypeLabel
import com.lincdkeyinput.app.ui.fieldKindLabel
import com.lincdkeyinput.app.ui.isHidden
import kotlinx.coroutines.delay
import uniffi.vault_core.FfiEntry
import uniffi.vault_core.FfiField
import uniffi.vault_core.FfiTotpCode
import uniffi.vault_core.FieldKind

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun EntryDetailScreen(
    entry: FfiEntry,
    onBack: () -> Unit,
    onEdit: () -> Unit,
    onDelete: () -> Unit,
    onCopy: (fieldId: String, label: String) -> Unit,
    totpFor: (FfiField) -> FfiTotpCode?,
) {
    val revealed = remember { mutableStateMapOf<String, Boolean>() }
    var tick by remember { mutableLongStateOf(System.currentTimeMillis()) }
    var confirmDelete by remember { mutableStateOf(false) }

    LaunchedEffect(entry.id) {
        while (true) {
            delay(1000)
            tick = System.currentTimeMillis()
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(entry.title.ifBlank { "(无标题)" }, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                navigationIcon = {
                    IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "返回") }
                },
                actions = {
                    IconButton(onClick = onEdit) { Icon(Icons.Filled.Edit, "编辑") }
                    IconButton(onClick = { confirmDelete = true }) { Icon(Icons.Filled.Delete, "删除") }
                },
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState())
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(entryTypeLabel(entry.entryType), color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (entry.tags.isNotEmpty()) {
                Text("标签：" + entry.tags.joinToString("  ") { "#$it" }, style = MaterialTheme.typography.bodySmall)
            }
            if (entry.fields.isEmpty()) {
                Text("(没有字段)", color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            entry.fields.forEach { field ->
                FieldCard(
                    field = field,
                    revealed = revealed[field.id] == true,
                    onToggleReveal = { revealed[field.id] = revealed[field.id] != true },
                    onCopy = { onCopy(field.id, field.label.ifBlank { fieldKindLabel(field.kind) }) },
                    totpCode = if (field.kind == FieldKind.TOTP) {
                        remember(tick, field.id) { totpFor(field) }
                    } else {
                        null
                    },
                    period = field.totp?.periodSeconds?.toInt() ?: 30,
                )
            }
        }
    }

    if (confirmDelete) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text("删除条目") },
            text = { Text("确定删除「${entry.title.ifBlank { "(无标题)" }}」？此操作可在下次同步前撤销，但本地立即隐藏。") },
            confirmButton = { TextButton(onClick = { confirmDelete = false; onDelete() }) { Text("删除") } },
            dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text("取消") } },
        )
    }
}

@Composable
private fun FieldCard(
    field: FfiField,
    revealed: Boolean,
    onToggleReveal: () -> Unit,
    onCopy: () -> Unit,
    totpCode: FfiTotpCode?,
    period: Int,
) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(
                text = field.label.ifBlank { fieldKindLabel(field.kind) },
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (field.kind == FieldKind.TOTP) {
                if (totpCode != null) {
                    Text(totpCode.code, style = MaterialTheme.typography.headlineMedium)
                    val remaining = totpCode.secondsRemaining.toInt()
                    LinearProgressIndicator(
                        progress = { remaining.toFloat() / period.coerceAtLeast(1) },
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text("$remaining 秒后刷新", style = MaterialTheme.typography.bodySmall, modifier = Modifier.weight(1f))
                        IconButton(onClick = onCopy) { Icon(Icons.Filled.ContentCopy, "复制验证码") }
                    }
                } else {
                    Text("TOTP 种子无效（需 Base32 文本）", color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
                }
            } else {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        text = if (revealed || !isHidden(field.sensitivity)) {
                            field.value.ifBlank { "(空)" }
                        } else {
                            "••••••••"
                        },
                        modifier = Modifier.weight(1f),
                    )
                    if (isHidden(field.sensitivity)) {
                        IconButton(onClick = onToggleReveal) {
                            Icon(if (revealed) Icons.Filled.VisibilityOff else Icons.Filled.Visibility, "显示/隐藏")
                        }
                    }
                    IconButton(onClick = onCopy) { Icon(Icons.Filled.ContentCopy, "复制") }
                }
            }
        }
    }
}
