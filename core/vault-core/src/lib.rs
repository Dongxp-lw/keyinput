//! Private Input Vault 共享核心（`vault-core`）。
//!
//! 跨端共享的纯逻辑核心，**不依赖任何平台 API**：保险库格式、密码学、领域模型、
//! TOTP、导出/导入与迁移/同步逻辑（见 `docs/technical/module-architecture.md`）。
//! 平台经 UniFFI（Android/iOS）或 Node-API（HarmonyOS）调用本核心；依赖方向单向：
//! 平台 → 核心，核心绝不 import 任何平台 API。
//!
//! 当前含 L0-05a 骨架 + L0-05b 的 FFI 连通性自检函数（`ping`）；业务接口尚未导出。
//!
//! L2 起新增核心模块：[`secret`]（秘密类型与清零）、[`crypto`]（Argon2id KDF /
//! XChaCha20-Poly1305 AEAD / 信封密钥 / HKDF 子密钥）、[`codec`]（CBOR 序列化，保未知字段）、
//! [`error`]（对外统一错误 [`error::VaultError`]，经 UniFFI 映射）。L3 起新增领域核心：
//! [`entry`]（条目/字段模型 + 默认策略 + 载荷编解码）、[`repository`]（条目 CRUD + 本地搜索）、
//! [`vault`]（保险库文件格式 + 创建/打开/保存/改口令，用 L2 加密 + L2-04 错误）、
//! [`lock`]（解锁会话：最小持有会话密钥 + 显式锁定清零 + 纯空闲判定）、[`rng`]（CSPRNG +
//! 无偏置采样）、[`generator`]（密码生成器）、[`totp`]（RFC 6238 TOTP 生成）、
//! [`imex`]（加密导出/导入：独立迁移包，跨设备复用域分离子密钥）、[`ffi`]（L2-06：`VaultCore`
//! 经 UniFFI 导出的业务表面）。
//! 其中 `secret`/`crypto`/`codec` 是纯 Rust 内部实现。

pub mod codec;
pub mod crypto;
pub mod entry;
pub mod error;
pub mod ffi;
pub mod generator;
pub mod imex;
pub mod lock;
pub mod repository;
pub mod rng;
pub mod secret;
pub mod totp;
pub mod vault;

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
