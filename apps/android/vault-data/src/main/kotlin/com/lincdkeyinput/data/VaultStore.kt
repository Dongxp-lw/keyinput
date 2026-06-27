package com.lincdkeyinput.data

import android.content.Context
import java.io.File

/**
 * 本地保险库文件的持久化（L4-FFI）。核心只产出/消费字节，文件 IO 在平台（模块架构 §5）。
 *
 * 写入采用「临时文件 → 原子重命名」，避免半截写入损坏上一份有效保险库（安全实现设计 §8 失败回滚）。
 */
class VaultStore(
    context: Context,
    private val fileName: String = DEFAULT_VAULT_FILE,
) {
    private val dir: File = context.applicationContext.filesDir
    private val file: File get() = File(dir, fileName)

    fun exists(): Boolean = file.exists()

    fun read(): ByteArray = file.readBytes()

    /** 原子写入：写临时文件再重命名替换，失败不破坏现有文件。 */
    fun write(bytes: ByteArray) {
        val tmp = File(dir, "$fileName.tmp")
        tmp.writeBytes(bytes)
        if (!tmp.renameTo(file)) {
            // 极少数文件系统重命名失败时退回直接写（仍尽量保证完整写入）。
            file.writeBytes(bytes)
            tmp.delete()
        }
    }

    fun delete(): Boolean = file.delete()

    private companion object {
        const val DEFAULT_VAULT_FILE = "vault.pivault"
    }
}
