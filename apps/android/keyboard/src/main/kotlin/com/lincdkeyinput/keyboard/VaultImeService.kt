package com.lincdkeyinput.keyboard

import android.graphics.Color
import android.inputmethodservice.InputMethodService
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.widget.Toast
import com.lincdkeyinput.data.Base32
import com.lincdkeyinput.data.VaultManager
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import uniffi.vault_core.FfiField
import uniffi.vault_core.FfiTotpParams
import uniffi.vault_core.FieldKind
import uniffi.vault_core.VaultException

/**
 * 安全键盘 IME（L4-KBD）。
 *
 * 设计要点：
 * - 独立会话：本服务持有自己的 [VaultManager]（独立于 App 的单例），单独解锁。
 *   因为 App 在后台会自动锁定，而键盘往往是在「别的 App」里使用，必须能独立解锁。
 * - 内置解锁键盘：锁定时用自绘按键输入主密码（不依赖其它输入法），避免明文经由系统剪贴板。
 * - 直接填入：选择条目字段后用 [android.view.inputmethod.InputConnection.commitText] 把值直接写入目标输入框，
 *   全程不经过系统剪贴板（这正是「安全键盘」的核心价值）。
 * - 空闲自动锁定：每次显示输入视图时若超过空闲阈值则自动锁定；销毁时锁定。
 *
 * 视图全部以代码构建（不引入 Compose），规避 IME 窗口下的生命周期复杂度。
 */
class VaultImeService : InputMethodService() {

    private enum class Mode { LOCKED, LIST, DETAIL }

    private val manager by lazy { VaultManager(applicationContext) }
    private val scope = MainScope()

    private lateinit var root: LinearLayout
    private var maskedView: TextView? = null

    private val pwd = StringBuilder()
    private var shift = false
    private var symbols = false
    private var mode = Mode.LOCKED
    private var detailEntryId: String? = null
    private var statusText: String = ""

    override fun onCreateInputView(): View {
        root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setBackgroundColor(Color.parseColor("#FFECEFF1"))
            setPadding(dp(4), dp(4), dp(4), dp(6))
        }
        renderRootMode()
        return root
    }

    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        if (manager.isUnlocked() && manager.isIdleExpired(IDLE_TIMEOUT_MS)) {
            manager.lock()
        }
        pwd.setLength(0)
        shift = false
        symbols = false
        statusText = ""
        detailEntryId = null
        mode = if (manager.isUnlocked()) Mode.LIST else Mode.LOCKED
        renderRootMode()
    }

    override fun onDestroy() {
        manager.lock()
        scope.cancel()
        super.onDestroy()
    }

    // ---- 渲染 ----

    private fun renderRootMode() {
        if (!::root.isInitialized) return
        root.removeAllViews()
        maskedView = null
        root.addView(header())
        when (mode) {
            Mode.LOCKED -> renderLocked()
            Mode.LIST -> renderList()
            Mode.DETAIL -> renderDetail()
        }
    }

    private fun header(): LinearLayout {
        val bar = horizontal()
        val title = TextView(this).apply {
            text = when (mode) {
                Mode.LOCKED -> "🔒 安全键盘"
                Mode.LIST -> "🔓 选择要填入的条目"
                Mode.DETAIL -> "🔓 选择字段填入"
            }
            textSize = 14f
            setPadding(dp(6), dp(6), dp(6), dp(6))
            layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 5f)
        }
        bar.addView(title)
        bar.addView(key("⌨", 1f) { switchKeyboard() })
        if (manager.isUnlocked()) {
            bar.addView(
                key("锁定", 1.4f) {
                    manager.lock()
                    pwd.setLength(0)
                    statusText = ""
                    mode = Mode.LOCKED
                    renderRootMode()
                },
            )
        }
        return bar
    }

    private fun renderLocked() {
        if (!manager.vaultExists()) {
            root.addView(infoText("还没有保险库。请先在 Private Input Vault 应用中创建后再使用安全键盘。"))
            return
        }
        val masked = TextView(this).apply {
            text = if (statusText.isNotEmpty()) statusText else "输入主密码解锁"
            textSize = 16f
            setPadding(dp(10), dp(10), dp(10), dp(10))
        }
        maskedView = masked
        root.addView(masked)
        root.addView(buildKeyboard())
    }

    private fun renderList() {
        val entries = manager.listEntries()
        if (entries.isEmpty()) {
            root.addView(infoText("还没有任何条目，请先在应用中添加。"))
            return
        }
        val scroll = ScrollView(this).apply {
            layoutParams = LinearLayout.LayoutParams(MATCH, dp(240))
        }
        val col = vertical()
        for (e in entries) {
            col.addView(
                key(e.title.ifBlank { "（未命名）" }, 1f) {
                    detailEntryId = e.id
                    mode = Mode.DETAIL
                    renderRootMode()
                }.fullWidth(),
            )
        }
        scroll.addView(col)
        root.addView(scroll)
    }

    private fun renderDetail() {
        val entry = detailEntryId?.let { manager.getEntry(it) }
        if (entry == null) {
            mode = Mode.LIST
            renderRootMode()
            return
        }
        root.addView(
            key("‹ 返回条目列表", 1f) {
                mode = Mode.LIST
                renderRootMode()
            }.fullWidth(),
        )
        root.addView(infoText(entry.title.ifBlank { "（未命名）" }))
        val scroll = ScrollView(this).apply {
            layoutParams = LinearLayout.LayoutParams(MATCH, dp(220))
        }
        val col = vertical()
        if (entry.fields.isEmpty()) {
            col.addView(infoText("该条目没有字段。"))
        }
        for (f in entry.fields) {
            val label = f.label.ifBlank { fieldKindLabel(f.kind) }
            col.addView(
                key("填入：$label", 1f) { commitField(f) }.fullWidth(),
            )
        }
        scroll.addView(col)
        root.addView(scroll)
    }

    // ---- 内置解锁键盘 ----

    private fun buildKeyboard(): LinearLayout {
        val kb = vertical()
        kb.addView(charRow("1234567890"))
        if (!symbols) {
            kb.addView(charRow("qwertyuiop"))
            kb.addView(charRow("asdfghjkl"))
            kb.addView(specialRow("zxcvbnm", withShift = true))
        } else {
            kb.addView(charRow("@#\$%&*-+()"))
            kb.addView(charRow("=/\\:;!?\"'"))
            kb.addView(specialRow(",._~|<>[]", withShift = false))
        }
        kb.addView(bottomRow())
        return kb
    }

    private fun charRow(chars: String): LinearLayout {
        val row = horizontal()
        for (c in chars) row.addView(key(display(c), 1f) { appendChar(c) })
        return row
    }

    private fun specialRow(chars: String, withShift: Boolean): LinearLayout {
        val row = horizontal()
        if (withShift) row.addView(key(if (shift) "⇧●" else "⇧", 1.5f) { shift = !shift; renderRootMode() })
        for (c in chars) row.addView(key(display(c), 1f) { appendChar(c) })
        row.addView(key("⌫", 1.5f) { backspace() })
        return row
    }

    private fun bottomRow(): LinearLayout {
        val row = horizontal()
        row.addView(key(if (symbols) "ABC" else "?#", 1.5f) { symbols = !symbols; shift = false; renderRootMode() })
        row.addView(key("空格", 4f) { appendChar(' ') })
        row.addView(key("解锁", 2f) { doUnlock() })
        return row
    }

    private fun display(c: Char): String =
        if (!symbols && shift && c in 'a'..'z') c.uppercaseChar().toString() else c.toString()

    private fun appendChar(c: Char) {
        val ch = if (shift && c in 'a'..'z') c.uppercaseChar() else c
        pwd.append(ch)
        statusText = ""
        if (shift) {
            shift = false
            renderRootMode()
        } else {
            maskedView?.text = mask()
        }
    }

    private fun backspace() {
        if (pwd.isNotEmpty()) pwd.deleteCharAt(pwd.length - 1)
        maskedView?.text = mask()
    }

    private fun mask(): String = if (pwd.isEmpty()) "输入主密码解锁" else "•".repeat(pwd.length)

    private fun doUnlock() {
        if (pwd.isEmpty()) {
            setLockedStatus("请输入主密码")
            return
        }
        val bytes = pwd.toString().toByteArray(Charsets.UTF_8)
        pwd.setLength(0)
        setLockedStatus("解锁中…")
        scope.launch {
            try {
                manager.unlock(bytes)
                bytes.fill(0)
                statusText = ""
                mode = Mode.LIST
                renderRootMode()
            } catch (e: VaultException) {
                bytes.fill(0)
                setLockedStatus(e.userMessage())
            } catch (e: Exception) {
                bytes.fill(0)
                setLockedStatus("解锁失败：${e.message ?: "未知错误"}")
            }
        }
    }

    private fun setLockedStatus(msg: String) {
        statusText = msg
        if (mode == Mode.LOCKED) maskedView?.text = msg
    }

    // ---- 填入目标输入框 ----

    private fun commitField(f: FfiField) {
        val value = fieldCommitValue(f)
        if (value == null) {
            Toast.makeText(this, "无法获取该字段的值", Toast.LENGTH_SHORT).show()
            return
        }
        currentInputConnection?.commitText(value, 1)
        manager.touch()
    }

    private fun fieldCommitValue(f: FfiField): String? {
        if (f.kind == FieldKind.TOTP) {
            val totp = f.totp ?: return null
            val secret = Base32.decodeOrNull(totp.secret) ?: return null
            return try {
                manager.totp(
                    FfiTotpParams(secret, totp.algorithm, totp.digits, totp.periodSeconds, 0L),
                ).code
            } catch (e: Exception) {
                null
            }
        }
        return f.value
    }

    // ---- 杂项 ----

    private fun switchKeyboard() {
        if (!switchToPreviousInputMethod()) {
            (getSystemService(INPUT_METHOD_SERVICE) as InputMethodManager).showInputMethodPicker()
        }
    }

    private fun fieldKindLabel(kind: FieldKind): String = when (kind.name) {
        "USERNAME" -> "用户名"
        "PASSWORD" -> "密码"
        "EMAIL" -> "邮箱"
        "URL" -> "网址"
        "TOTP" -> "动态验证码"
        "PHONE" -> "电话"
        "PIN" -> "PIN 码"
        "NOTE" -> "备注"
        "TEXT" -> "文本"
        else -> kind.name
    }

    private fun VaultException.userMessage(): String = when (this) {
        is VaultException.WrongPasswordOrTampered -> "主密码错误，或数据已被篡改"
        is VaultException.Corrupt -> "文件已损坏"
        is VaultException.IncompatibleVersion -> "版本不兼容"
        is VaultException.Locked -> "已锁定"
        else -> "无法解锁"
    }

    // ---- 视图构建辅助 ----

    private fun horizontal(): LinearLayout = LinearLayout(this).apply {
        orientation = LinearLayout.HORIZONTAL
        layoutParams = LinearLayout.LayoutParams(MATCH, LinearLayout.LayoutParams.WRAP_CONTENT)
    }

    private fun vertical(): LinearLayout = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        layoutParams = LinearLayout.LayoutParams(MATCH, LinearLayout.LayoutParams.WRAP_CONTENT)
    }

    private fun infoText(msg: String): TextView = TextView(this).apply {
        text = msg
        textSize = 14f
        setPadding(dp(10), dp(10), dp(10), dp(10))
    }

    private fun key(text: String, weight: Float, onClick: () -> Unit): Button = Button(this).apply {
        this.text = text
        isAllCaps = false
        textSize = 16f
        setPadding(dp(2), dp(6), dp(2), dp(6))
        minWidth = 0
        minimumWidth = 0
        layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, weight).apply {
            setMargins(dp(2), dp(2), dp(2), dp(2))
        }
        setOnClickListener { onClick() }
    }

    private fun Button.fullWidth(): Button = apply {
        layoutParams = LinearLayout.LayoutParams(MATCH, LinearLayout.LayoutParams.WRAP_CONTENT).apply {
            setMargins(dp(2), dp(2), dp(2), dp(2))
        }
    }

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()

    private companion object {
        const val MATCH = LinearLayout.LayoutParams.MATCH_PARENT
        const val IDLE_TIMEOUT_MS = 60_000L
    }
}
