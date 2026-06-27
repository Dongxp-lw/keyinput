//! Private Input Vault 共享核心（`vault-core`）。
//!
//! 跨端共享的纯逻辑核心，**不依赖任何平台 API**：保险库格式、密码学、领域模型、
//! TOTP、导出/导入与迁移/同步逻辑（见 `docs/technical/module-architecture.md`）。
//! 平台经 UniFFI（Android/iOS）或 Node-API（HarmonyOS）调用本核心；依赖方向单向：
//! 平台 → 核心，核心绝不 import 任何平台 API。
//!
//! 当前含 L0-05a 骨架 + L0-05b 的 FFI 连通性自检函数（`ping`）；业务接口尚未导出。
//!
//! L2 起新增内部核心模块：[`secret`]（秘密类型与清零）、[`crypto`]（Argon2id KDF /
//! XChaCha20-Poly1305 AEAD / 信封密钥 / HKDF 子密钥）。这些是纯 Rust、可确定性测试的
//! 内部实现，尚未跨 FFI 暴露（FFI 表面见 L2-06）。

pub mod crypto;
pub mod secret;

// UniFFI 脚手架（proc-macro 模式）。业务 API（见 docs/technical/module-architecture.md §4）
// 自 L2-06 起逐步导出；当前仅一个连通性自检用的平凡函数（L0-05b）。
uniffi::setup_scaffolding!();

/// FFI 连通性自检（**非业务接口**）：回显输入并加固定前缀。
/// 用于验证 Rust↔各端绑定（Kotlin/Swift/ArkTS）的往返调用是否打通。
#[uniffi::export]
pub fn ping(message: String) -> String {
    format!("vault-core pong: {message}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_roundtrips_input() {
        assert_eq!(ping("hello".to_string()), "vault-core pong: hello");
    }
}
