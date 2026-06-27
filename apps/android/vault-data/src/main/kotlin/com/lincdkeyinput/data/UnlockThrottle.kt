package com.lincdkeyinput.data

import android.content.Context
import java.io.File

/**
 * 解锁失败计数与退避节流（LOCK-06）。
 *
 * 失败次数与"下次可尝试时间"持久化到文件，**进程被杀/重启后仍生效**（防止重启绕过节流）。
 * 不区分口令错误与数据篡改——二者在核心已合并为同一不可区分错误（安全要求）。
 * 策略：前 4 次失败不退避；第 5 次起指数退避（5s、10s、20s…），封顶 5 分钟。成功解锁即清零。
 */
class UnlockThrottle(context: Context) {

    private val appContext = context.applicationContext
    // 惰性解析：filesDir 触发磁盘访问，放到首次真正读写时（已在后台线程），避免构造时主线程 IO（StrictMode）。
    private val file: File by lazy { File(appContext.filesDir, FILE) }

    data class State(val failedAttempts: Int, val lockedUntilMs: Long)

    fun state(): State = try {
        val parts = file.readText().trim().split(',')
        State(parts[0].toInt(), parts[1].toLong())
    } catch (e: Exception) {
        State(0, 0L)
    }

    fun failedAttempts(): Int = state().failedAttempts

    /** 距离可再次尝试的剩余毫秒（<=0 表示可立即尝试）。 */
    fun remainingLockMs(nowMs: Long): Long = (state().lockedUntilMs - nowMs).coerceAtLeast(0L)

    /** 记一次失败：递增计数，达阈值后按指数退避设定下次可尝试时间。返回新状态。 */
    fun recordFailure(nowMs: Long): State {
        val attempts = state().failedAttempts + 1
        val lockedUntil = if (attempts >= THRESHOLD) {
            val over = (attempts - THRESHOLD).coerceAtMost(MAX_SHIFT)
            val delay = (BASE_DELAY_MS shl over).coerceAtMost(MAX_DELAY_MS)
            nowMs + delay
        } else {
            0L
        }
        val next = State(attempts, lockedUntil)
        runCatching { file.writeText("${next.failedAttempts},${next.lockedUntilMs}") }
        return next
    }

    /** 解锁成功：清零计数与退避。 */
    fun recordSuccess() {
        runCatching { file.delete() }
    }

    private companion object {
        const val FILE = "unlock_throttle"
        const val THRESHOLD = 5          // 第 5 次失败起退避
        const val BASE_DELAY_MS = 5_000L // 基础 5 秒
        const val MAX_SHIFT = 6          // 位移上限（5s<<6 = 320s，再被下面封顶）
        const val MAX_DELAY_MS = 300_000L // 封顶 5 分钟
    }
}
