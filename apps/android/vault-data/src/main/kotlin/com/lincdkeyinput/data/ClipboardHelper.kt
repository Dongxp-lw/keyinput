package com.lincdkeyinput.data

import android.content.ClipData
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.os.PersistableBundle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * 剪贴板兜底（L4-CLIP）：当字段不便直接插入时，复制到剪贴板并在超时后自动清除，降低秘密驻留。
 *
 * - API 33+ 标记 `EXTRA_IS_SENSITIVE`，系统不在界面预览明文。
 * - 超时后若剪贴板仍是本次复制的内容则清空（避免误清用户后续复制的其他内容）。
 * - 不记录被复制的值（安全实现设计 §5.4、剪贴板兜底实现文档）。
 */
object ClipboardHelper {

    private const val DEFAULT_CLEAR_AFTER_MS = 30_000L
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)

    fun copySensitive(
        context: Context,
        label: String,
        value: String,
        clearAfterMs: Long = DEFAULT_CLEAR_AFTER_MS,
    ) {
        val cm = context.applicationContext
            .getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        val clip = ClipData.newPlainText(label, value)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            clip.description.extras = PersistableBundle().apply {
                putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true)
            }
        }
        cm.setPrimaryClip(clip)

        scope.launch {
            delay(clearAfterMs)
            if (currentClipMatches(cm, value)) {
                clearClipboard(cm)
            }
        }
    }

    private fun currentClipMatches(cm: ClipboardManager, value: String): Boolean {
        val current = cm.primaryClip ?: return false
        if (current.itemCount == 0) return false
        return current.getItemAt(0).text?.toString() == value
    }

    private fun clearClipboard(cm: ClipboardManager) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            cm.clearPrimaryClip()
        } else {
            cm.setPrimaryClip(ClipData.newPlainText("", ""))
        }
    }
}
