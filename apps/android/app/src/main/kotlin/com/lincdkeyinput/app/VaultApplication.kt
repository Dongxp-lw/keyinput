package com.lincdkeyinput.app

import android.app.Application
import android.os.StrictMode
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import com.lincdkeyinput.data.VaultManager

/**
 * 应用入口：持有进程内唯一的 [VaultManager]，并在应用进入后台时**自动锁定并清零**会话
 * （安全实现设计 §5.4；`ProcessLifecycleOwner` 已对配置变更去抖，旋转不会误锁）。
 *
 * 键盘 IME 不复用此会话（跨使用场景需在他应用内独立解锁），见 L4-KBD。
 */
class VaultApplication : Application(), DefaultLifecycleObserver {

    val vaultManager: VaultManager by lazy { VaultManager(this) }

    /**
     * 一次性「本次进入后台不自动锁定」标志。仅用于 App **自己发起**的 SAF 文件选择器
     * （加密导出/导入）：选择器（DocumentsUI）属于其它进程，会让本 App 进入后台并触发
     * [onStop]，但这是用户明确发起的同一操作流程，中途锁定会导致选择器返回后会话已锁、
     * 导出/导入失败。置位后仅在下一次 [onStop] 放行一次并立即复位；其它任何进入后台仍
     * 照常自动锁定。（QR 扫码、BiometricPrompt 在本进程/本 Activity 内，不会触发 onStop，无需放行。）
     */
    @Volatile
    var suppressNextBackgroundLock: Boolean = false

    override fun onCreate() {
        enableStrictModeInDebug()
        super<Application>.onCreate()
        ProcessLifecycleOwner.get().lifecycle.addObserver(this)
    }

    override fun onStop(owner: LifecycleOwner) {
        // 进入后台：锁定并清零（即便尚未解锁也无副作用）。
        // 例外：App 自己发起的导出/导入文件选择器，放行一次（见 suppressNextBackgroundLock）。
        if (suppressNextBackgroundLock) {
            suppressNextBackgroundLock = false
            return
        }
        vaultManager.lock()
    }

    /**
     * 仅**开发构建**启用 StrictMode：尽早暴露主线程 IO、资源未关闭等问题（MVP-010）。
     * 生产构建不启用；`penaltyLog` 仅将违规诊断输出到 logcat（不含任何秘密）。
     */
    private fun enableStrictModeInDebug() {
        if (!BuildConfig.DEBUG) return
        StrictMode.setThreadPolicy(
            StrictMode.ThreadPolicy.Builder()
                .detectAll()
                .penaltyLog()
                .build(),
        )
        StrictMode.setVmPolicy(
            StrictMode.VmPolicy.Builder()
                .detectAll()
                .penaltyLog()
                .build(),
        )
    }
}
