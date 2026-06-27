package com.lincdkeyinput.app

import android.app.Application
import android.net.Uri
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.lincdkeyinput.data.Base32
import com.lincdkeyinput.data.BiometricGate
import com.lincdkeyinput.data.ClipboardHelper
import com.lincdkeyinput.data.UnlockThrottle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.vault_core.EntrySummary
import uniffi.vault_core.FfiEntry
import uniffi.vault_core.FfiExportOptions
import uniffi.vault_core.FfiField
import uniffi.vault_core.FfiPasswordPolicy
import uniffi.vault_core.FfiTotpCode
import uniffi.vault_core.FfiTotpParams
import uniffi.vault_core.VaultException

/** 顶层界面状态。 */
sealed interface VaultUiState {
    data object Loading : VaultUiState
    data object Onboarding : VaultUiState
    data object Locked : VaultUiState
    data object Unlocked : VaultUiState
}

/** 已解锁时的子页面。 */
sealed interface Screen {
    data object List : Screen
    data class Detail(val entryId: String) : Screen
    data class Edit(val entryId: String?) : Screen
    data object Settings : Screen
}

/**
 * 应用状态机与动作（L4-APP）。所有耗时/IO 动作经 [com.lincdkeyinput.data.VaultManager] 切后台；
 * 核心抛出的 `VaultException` 在此映射为可读中文提示。密码 `ByteArray` 用后立即 `fill(0)` 清零。
 */
class VaultViewModel(app: Application) : AndroidViewModel(app) {

    private val manager by lazy { getApplication<VaultApplication>().vaultManager }
    private val gate = BiometricGate(app)

    private val _uiState = MutableStateFlow<VaultUiState>(VaultUiState.Loading)
    val uiState: StateFlow<VaultUiState> = _uiState.asStateFlow()

    private val _screen = MutableStateFlow<Screen>(Screen.List)
    val screen: StateFlow<Screen> = _screen.asStateFlow()

    private val _entries = MutableStateFlow<List<EntrySummary>>(emptyList())
    val entries: StateFlow<List<EntrySummary>> = _entries.asStateFlow()

    private val _query = MutableStateFlow("")
    val query: StateFlow<String> = _query.asStateFlow()

    private val _busy = MutableStateFlow(false)
    val busy: StateFlow<Boolean> = _busy.asStateFlow()

    private val _message = MutableStateFlow<String?>(null)
    val message: StateFlow<String?> = _message.asStateFlow()

    private val throttle = UnlockThrottle(app)
    private val _failedAttempts = MutableStateFlow(0)
    val failedAttempts: StateFlow<Int> = _failedAttempts.asStateFlow()
    private val _lockoutRemainingSec = MutableStateFlow(0)
    val lockoutRemainingSec: StateFlow<Int> = _lockoutRemainingSec.asStateFlow()
    private var lockoutJob: Job? = null

    private val _biometricEnabled = MutableStateFlow(false)
    val biometricEnabled: StateFlow<Boolean> = _biometricEnabled.asStateFlow()

    /** 设备硬件是否支持强生物识别。 */
    fun biometricHardwareAvailable(): Boolean = gate.isHardwareAvailable()

    /** 解锁界面是否应展示生物识别入口（硬件可用且用户已启用）。 */
    fun biometricUnlockReady(): Boolean = gate.isHardwareAvailable() && gate.isEnabled()

    init {
        // 启动期的磁盘读取（native 库加载、保险库是否存在、失败计数、生物识别开关）放到
        // 后台线程，避免主线程 IO（StrictMode）；期间 UI 停留在 Loading。
        viewModelScope.launch {
            val now = System.currentTimeMillis()
            _failedAttempts.value = withContext(Dispatchers.Default) { throttle.failedAttempts() }
            _biometricEnabled.value = withContext(Dispatchers.Default) { gate.isEnabled() }
            // 首次访问 manager（lazy）会构造 VaultCore 并加载 native .so —— 放后台线程。
            val unlocked = withContext(Dispatchers.Default) { manager.isUnlocked() }
            val exists = !unlocked && withContext(Dispatchers.Default) { manager.vaultExists() }
            when {
                unlocked -> {
                    _uiState.value = VaultUiState.Unlocked
                    refresh()
                }
                exists -> {
                    _uiState.value = VaultUiState.Locked
                    if (withContext(Dispatchers.Default) { throttle.remainingLockMs(now) } > 0) startLockoutCountdown()
                }
                else -> _uiState.value = VaultUiState.Onboarding
            }
        }
    }

    /** 回到前台时若已被自动锁定，则切到解锁界面。 */
    fun syncLockState() {
        if (_uiState.value == VaultUiState.Unlocked && !manager.isUnlocked()) {
            _uiState.value = VaultUiState.Locked
            _entries.value = emptyList()
            _query.value = ""
        }
    }

    fun createVault(password: ByteArray) = launchBusy {
        manager.create(password)
        password.fill(0)
        enterUnlocked()
    }

    fun unlock(password: ByteArray) {
        viewModelScope.launch {
            val remaining = withContext(Dispatchers.Default) { throttle.remainingLockMs(System.currentTimeMillis()) }
            if (remaining > 0) {
                password.fill(0)
                _message.value = "尝试过于频繁，请 ${(remaining + 999) / 1000} 秒后再试"
                startLockoutCountdown()
                return@launch
            }
            _busy.value = true
            try {
                manager.unlock(password)
                password.fill(0)
                withContext(Dispatchers.Default) { throttle.recordSuccess() }
                _failedAttempts.value = 0
                _lockoutRemainingSec.value = 0
                enterUnlocked()
            } catch (e: VaultException.WrongPasswordOrTampered) {
                password.fill(0)
                val s = withContext(Dispatchers.Default) { throttle.recordFailure(System.currentTimeMillis()) }
                _failedAttempts.value = s.failedAttempts
                _message.value = wrongPasswordMessage(s)
                if (s.lockedUntilMs > System.currentTimeMillis()) startLockoutCountdown()
            } catch (e: VaultException) {
                password.fill(0)
                _message.value = e.toUserMessage()
            } catch (e: Exception) {
                password.fill(0)
                _message.value = "操作失败：${e.message ?: e.javaClass.simpleName}"
            } finally {
                _busy.value = false
            }
        }
    }

    private fun wrongPasswordMessage(s: UnlockThrottle.State): String {
        val base = "密码错误（已失败 ${s.failedAttempts} 次）"
        val remMs = (s.lockedUntilMs - System.currentTimeMillis()).coerceAtLeast(0L)
        return if (remMs > 0) "$base，请 ${(remMs + 999) / 1000} 秒后再试" else base
    }

    private fun startLockoutCountdown() {
        lockoutJob?.cancel()
        lockoutJob = viewModelScope.launch {
            while (true) {
                val remMs = withContext(Dispatchers.Default) { throttle.remainingLockMs(System.currentTimeMillis()) }
                val sec = ((remMs + 999) / 1000).toInt()
                _lockoutRemainingSec.value = sec
                if (sec <= 0) break
                delay(1000)
            }
        }
    }

    /** 用生物识别解锁：认证成功后取回主密码字节，复用主密码解锁路径，用完即清零。 */
    fun biometricUnlock(activity: FragmentActivity) {
        viewModelScope.launch {
            gate.unlock(
                activity = activity,
                onSuccess = { masterPassword -> unlock(masterPassword) },
                onError = { msg -> _message.value = msg },
            )
        }
    }

    /** 在设置中启用生物识别：需要用户重新输入主密码以便加密落盘。 */
    fun enableBiometric(activity: FragmentActivity, masterPassword: ByteArray) {
        gate.enable(activity, masterPassword) { ok, err ->
            _biometricEnabled.value = gate.isEnabled()
            _message.value = if (ok) "已启用生物识别解锁" else (err ?: "启用失败")
        }
    }

    /** 关闭生物识别解锁。 */
    fun disableBiometric() {
        gate.disable()
        _biometricEnabled.value = gate.isEnabled()
        _message.value = "已关闭生物识别解锁"
    }

    /** 由平台（生物识别成功）提供已解锁的会话来源时调用：此处简单走主密码路径。 */
    fun lock() {
        manager.lock()
        _uiState.value = VaultUiState.Locked
        _entries.value = emptyList()
        _query.value = ""
        _screen.value = Screen.List
    }

    fun refresh() {
        if (!manager.isUnlocked()) return
        val q = _query.value
        _entries.value = if (q.isBlank()) manager.listEntries() else manager.search(q)
        manager.touch()
    }

    fun onQueryChange(q: String) {
        _query.value = q
        refresh()
    }

    // ---- 导航 ----
    fun openEntry(id: String) { _screen.value = Screen.Detail(id) }
    fun newEntry() { _screen.value = Screen.Edit(null) }
    fun editEntry(id: String) { _screen.value = Screen.Edit(id) }
    fun openSettings() { _screen.value = Screen.Settings }
    fun back() { _screen.value = Screen.List; refresh() }

    fun getEntry(id: String): FfiEntry? = manager.getEntry(id)

    fun saveEntry(entry: FfiEntry) = launchBusy {
        manager.upsertEntry(entry)
        refresh()
        _screen.value = Screen.Detail(entry.id)
        _message.value = "已保存"
    }

    fun deleteEntry(id: String) = launchBusy {
        manager.deleteEntry(id)
        refresh()
        _screen.value = Screen.List
        _message.value = "已删除"
    }

    fun copyField(entryId: String, fieldId: String, label: String) {
        val value = manager.getFieldValue(entryId, fieldId) ?: return
        ClipboardHelper.copySensitive(getApplication(), label, value)
        manager.touch()
        _message.value = "已复制「$label」（约 30 秒后自动清除）"
    }

    fun revealField(entryId: String, fieldId: String): String? {
        manager.touch()
        return manager.getFieldValue(entryId, fieldId)
    }

    fun generate(policy: FfiPasswordPolicy): String? = try {
        manager.generate(policy)
    } catch (e: VaultException) {
        _message.value = e.toUserMessage()
        null
    }

    /** 计算某 TOTP 字段当前验证码（Base32 解码种子后调用核心）。无效种子返回 null。 */
    fun totpFor(field: FfiField): FfiTotpCode? {
        val totp = field.totp ?: return null
        val secret = Base32.decodeOrNull(totp.secret) ?: return null
        return try {
            manager.totp(
                FfiTotpParams(secret, totp.algorithm, totp.digits, totp.periodSeconds, 0L),
            )
        } catch (e: VaultException) {
            null
        }
    }

    fun changePassword(old: ByteArray, new: ByteArray) = launchBusy {
        manager.changePassword(old, new)
        old.fill(0)
        new.fill(0)
        _message.value = "主密码已修改"
    }

    /**
     * 在打开 App 自己的导出/导入文件选择器（SAF）前调用：放行一次「进入后台自动锁定」，
     * 避免选择器把 App 切到后台时把会话锁掉，导致选择器返回后导出/导入在已锁会话上失败
     * （见 [VaultApplication.suppressNextBackgroundLock]）。
     */
    fun armSafPicker() {
        (getApplication<Application>() as VaultApplication).suppressNextBackgroundLock = true
    }

    fun exportTo(uri: Uri, passphrase: ByteArray) = launchBusy {
        val opts = FfiExportOptions("primary", null, System.currentTimeMillis() / 1000L)
        val bytes = manager.exportPackage(passphrase, opts)
        passphrase.fill(0)
        getApplication<Application>().contentResolver.openOutputStream(uri)?.use { it.write(bytes) }
            ?: error("无法写入所选文件")
        _message.value = "已导出加密备份"
    }

    fun importFrom(uri: Uri, passphrase: ByteArray) = launchBusy {
        val bytes = getApplication<Application>().contentResolver.openInputStream(uri)?.use {
            it.readBytes()
        } ?: error("无法读取所选文件")
        manager.importPackage(bytes, passphrase)
        passphrase.fill(0)
        refresh()
        _screen.value = Screen.List
        _message.value = "已导入备份"
    }

    fun consumeMessage() { _message.value = null }

    private fun enterUnlocked() {
        _uiState.value = VaultUiState.Unlocked
        _screen.value = Screen.List
        refresh()
    }

    private fun launchBusy(block: suspend () -> Unit) {
        viewModelScope.launch {
            _busy.value = true
            try {
                block()
            } catch (e: VaultException) {
                _message.value = e.toUserMessage()
            } catch (e: Exception) {
                _message.value = "操作失败：${e.message ?: e.javaClass.simpleName}"
            } finally {
                _busy.value = false
            }
        }
    }
}

private fun VaultException.toUserMessage(): String = when (this) {
    is VaultException.WrongPasswordOrTampered -> "密码错误，或数据已被篡改"
    is VaultException.Corrupt -> "文件已损坏或无法解析"
    is VaultException.IncompatibleVersion -> "版本不兼容，请更新应用后重试"
    is VaultException.InvalidInput -> "输入无效，请检查后重试"
    is VaultException.Locked -> "保险库已锁定，请重新解锁"
    is VaultException.Internal -> "内部错误"
    else -> "操作失败"
}
