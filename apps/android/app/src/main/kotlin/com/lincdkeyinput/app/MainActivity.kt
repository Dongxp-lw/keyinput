package com.lincdkeyinput.app

import android.app.Activity
import android.os.Bundle
import android.util.Log
import uniffi.vault_core.ping

/**
 * L0-05d 骨架 Activity：调用 Rust 共享核心的连通性自检函数 [ping]，
 * 验证 Kotlin↔Rust（经 UniFFI + JNA + jniLibs 中的 libvault_core.so）的运行期往返。
 * 真正的业务 UI 在 L4 实现。
 */
class MainActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val result = ping("android")
        Log.i(TAG, "ping -> $result")
    }

    private companion object {
        const val TAG = "VaultCorePing"
    }
}
