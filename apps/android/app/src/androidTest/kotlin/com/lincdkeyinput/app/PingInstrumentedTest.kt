package com.lincdkeyinput.app

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.vault_core.ping

/**
 * 在设备/模拟器上验证 Kotlin↔Rust 运行期往返（加载 libvault_core.so，经 UniFFI 调 ping）。
 * 运行：./gradlew :app:connectedDebugAndroidTest
 */
@RunWith(AndroidJUnit4::class)
class PingInstrumentedTest {
    @Test
    fun ping_round_trips_through_ffi() {
        assertEquals("vault-core pong: it-works", ping("it-works"))
    }
}
