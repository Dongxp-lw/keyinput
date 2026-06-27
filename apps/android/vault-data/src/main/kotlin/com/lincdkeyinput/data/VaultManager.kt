package com.lincdkeyinput.data

import android.content.Context
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.vault_core.EntrySummary
import uniffi.vault_core.FfiEntry
import uniffi.vault_core.FfiExportOptions
import uniffi.vault_core.FfiPasswordPolicy
import uniffi.vault_core.FfiTotpCode
import uniffi.vault_core.FfiTotpParams
import uniffi.vault_core.FfiTransferHeader
import uniffi.vault_core.VaultCore
import uniffi.vault_core.generatePassword
import uniffi.vault_core.inspectPackage
import uniffi.vault_core.totpNow

/**
 * 核心桥接（L4-FFI）：把 UniFFI 生成的 [VaultCore] 与本地文件持久化 [VaultStore] 组合成应用可用的
 * 单一入口。会话状态（锁定/解锁）保存在 Rust 核心内（最小持有 DEK，见 LOCK）；本类只做编排与持久化。
 *
 * 线程：含 Argon2id 派生或文件 IO 的方法是 `suspend` 并切到 [Dispatchers.Default]（绝不阻塞主线程）；
 * 纯内存读取（列表/取条目/搜索/读字段值）为同步轻量调用。时间由平台注入：会话/条目用 epoch 毫秒，
 * TOTP 用 epoch 秒（RFC 6238）。
 *
 * 抛出的 `VaultException`（错口令/篡改不可区分、损坏、不兼容、锁定等）向上传播给 ViewModel 处理。
 */
class VaultManager(context: Context) {

    private val store = VaultStore(context)
    private val core = VaultCore()

    fun vaultExists(): Boolean = store.exists()

    fun isUnlocked(): Boolean = core.isUnlocked()

    /** 新建空保险库并写盘（生产 KDF 参数，Argon2id 较慢，切后台）。 */
    suspend fun create(masterPassword: ByteArray): Unit = withContext(Dispatchers.Default) {
        core.create(masterPassword, nowMillis())
        persist()
    }

    /** 读盘并用主密码解锁。错口令/篡改抛 `VaultException.WrongPasswordOrTampered`。 */
    suspend fun unlock(masterPassword: ByteArray): Unit = withContext(Dispatchers.Default) {
        val bytes = store.read()
        core.unlock(bytes, masterPassword, nowMillis())
    }

    /** 锁定并清零会话（字段明文 + DEK）。 */
    fun lock() = core.lock()

    // ---- 条目读取（纯内存，轻量） ----

    fun listEntries(): List<EntrySummary> = core.listEntries()

    fun getEntry(id: String): FfiEntry? = core.getEntry(id)

    fun search(query: String): List<EntrySummary> = core.search(query)

    /** 读取字段明文值（秘密，调用方用后尽快清理）。 */
    fun getFieldValue(entryId: String, fieldId: String): String? =
        core.getFieldValue(entryId, fieldId)

    // ---- 条目写入（写后立即持久化） ----

    suspend fun upsertEntry(entry: FfiEntry): Unit = withContext(Dispatchers.Default) {
        core.upsertEntry(entry, nowMillis())
        persist()
    }

    suspend fun deleteEntry(id: String): Boolean = withContext(Dispatchers.Default) {
        val hit = core.deleteEntry(id, nowMillis())
        persist()
        hit
    }

    /** 改主密码：核心返回重新加密的字节，直接写盘。 */
    suspend fun changePassword(old: ByteArray, new: ByteArray): Unit =
        withContext(Dispatchers.Default) {
            val bytes = core.changePassword(old, new)
            store.write(bytes)
        }

    // ---- 导出/导入 ----

    suspend fun exportPackage(passphrase: ByteArray, options: FfiExportOptions): ByteArray =
        withContext(Dispatchers.Default) { core.exportPackage(passphrase, options) }

    fun inspectPackageHeader(bytes: ByteArray): FfiTransferHeader = inspectPackage(bytes)

    /** 整库恢复到当前已解锁会话（沿用当前主密码），写盘。 */
    suspend fun importPackage(bytes: ByteArray, passphrase: ByteArray): Unit =
        withContext(Dispatchers.Default) {
            core.importPackage(bytes, passphrase)
            persist()
        }

    // ---- 无状态能力 ----

    fun generate(policy: FfiPasswordPolicy): String = generatePassword(policy)

    fun totp(params: FfiTotpParams): FfiTotpCode = totpNow(params, nowSeconds())

    // ---- 会话空闲（供自动锁定计时） ----

    fun touch() = core.touch(nowMillis())

    fun isIdleExpired(timeoutMillis: Long): Boolean = core.isIdleExpired(nowMillis(), timeoutMillis)

    private fun persist() = store.write(core.save())

    private companion object {
        fun nowMillis(): Long = System.currentTimeMillis()
        fun nowSeconds(): Long = System.currentTimeMillis() / 1000L
    }
}
