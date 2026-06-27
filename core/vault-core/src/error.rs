//! 错误模型与结果类型（L2-04，跨 FFI）。
//!
//! [`VaultError`] 是核心对外（经 UniFFI）的统一错误类型；内部各层的具体错误
//! （[`crate::crypto::CryptoError`]、[`crate::codec::CodecError`]）经 `From` 收敛到这里，
//! 配合 `?` 运算符在领域逻辑里向上传播。
//!
//! **安全要求**（安全实现设计 §11-12、各实现文档"边界条件与错误处理"）：口令错误与数据
//! 篡改都表现为 AEAD 验签失败，二者**不可区分**——必须合并为同一个
//! [`VaultError::WrongPasswordOrTampered`]，绝不提供能区分"口令错"与"被篡改"的预言机；
//! 认证失败不输出任何部分明文；内部失败一律**安全失败**，绝不退化为明文。

use core::fmt;

use crate::codec::CodecError;
use crate::crypto::CryptoError;

/// 核心对外统一错误（经 UniFFI 映射到各端的错误类型）。
///
/// 变体粒度刻意保持粗：只暴露处理所需的区分，不暴露任何会形成预言机的细节。
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Error)]
pub enum VaultError {
    /// 主密码错误或数据被篡改。**二者不可区分**（安全要求：不提供区分预言机）。
    WrongPasswordOrTampered,
    /// 数据损坏、截断或无法解析为期望格式。
    Corrupt,
    /// 文件格式或数据架构版本不被当前版本支持（应按迁移规则处理，不覆盖现有库）。
    IncompatibleVersion,
    /// 调用方提供的参数非法（如 KDF 参数越界）。
    InvalidInput,
    /// 操作需要已解锁的保险库，但当前为锁定状态（FFI 表面用）。
    Locked,
    /// 内部失败（CSPRNG 取随机失败、密钥派生失败、编码失败等不应发生的情况）。
    /// 安全失败，绝不退化为明文。
    Internal,
}

impl fmt::Display for VaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 文案刻意不暗示"口令错"或"篡改"二者中具体是哪一个（不可区分要求）。
        let msg = match self {
            VaultError::WrongPasswordOrTampered => "wrong password or tampered data",
            VaultError::Corrupt => "corrupt or unparseable data",
            VaultError::IncompatibleVersion => "incompatible format or schema version",
            VaultError::InvalidInput => "invalid input",
            VaultError::Locked => "vault is locked",
            VaultError::Internal => "internal error",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for VaultError {}

impl From<CryptoError> for VaultError {
    fn from(e: CryptoError) -> Self {
        match e {
            // 口令错与篡改都表现为 AEAD 失败 → 合并为单一不可区分错误（安全要求）。
            CryptoError::Aead => VaultError::WrongPasswordOrTampered,
            // 非法 KDF 参数是调用方问题。
            CryptoError::KdfParams => VaultError::InvalidInput,
            // RNG / 派生失败 → 内部安全失败。
            CryptoError::Rng | CryptoError::Kdf => VaultError::Internal,
        }
    }
}

impl From<CodecError> for VaultError {
    fn from(e: CodecError) -> Self {
        match e {
            // 解码失败：字节损坏 / 不可解析。版本不兼容由上层（L3-VAULT）在读出版本字段后另行判定。
            CodecError::Decode => VaultError::Corrupt,
            // 编码失败属内部错误。
            CodecError::Encode => VaultError::Internal,
        }
    }
}

/// 核心统一结果类型。
pub type VaultResult<T> = Result<T, VaultError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrong_password_and_tamper_are_indistinguishable() {
        // 错误口令与篡改在核心内都表现为 CryptoError::Aead，必须映射到同一个对外错误。
        let from_wrong_password = VaultError::from(CryptoError::Aead);
        let from_tamper = VaultError::from(CryptoError::Aead);
        assert_eq!(from_wrong_password, VaultError::WrongPasswordOrTampered);
        assert_eq!(from_wrong_password, from_tamper);
    }

    #[test]
    fn crypto_errors_map_to_expected_public_errors() {
        assert_eq!(
            VaultError::from(CryptoError::KdfParams),
            VaultError::InvalidInput
        );
        assert_eq!(VaultError::from(CryptoError::Rng), VaultError::Internal);
        assert_eq!(VaultError::from(CryptoError::Kdf), VaultError::Internal);
    }

    #[test]
    fn codec_errors_map_to_expected_public_errors() {
        assert_eq!(VaultError::from(CodecError::Decode), VaultError::Corrupt);
        assert_eq!(VaultError::from(CodecError::Encode), VaultError::Internal);
    }

    #[test]
    fn display_does_not_leak_distinguishing_detail() {
        // 不可区分错误的文案不得单独点名"口令错"或"篡改"。
        let msg = VaultError::WrongPasswordOrTampered.to_string();
        assert_eq!(msg, "wrong password or tampered data");
    }

    #[test]
    fn question_mark_operator_converts_internal_errors() {
        // 验证结果类型与 `?` 自动经 From 收敛（领域逻辑的常用写法）。
        fn crypto_path() -> VaultResult<()> {
            Err(CryptoError::Aead)?;
            Ok(())
        }
        fn codec_path() -> VaultResult<()> {
            Err(CodecError::Decode)?;
            Ok(())
        }
        assert_eq!(
            crypto_path().unwrap_err(),
            VaultError::WrongPasswordOrTampered
        );
        assert_eq!(codec_path().unwrap_err(), VaultError::Corrupt);
    }

    #[test]
    fn incompatible_version_is_part_of_public_contract() {
        // L3-VAULT 在解析版本字段后会直接构造该错误；确认其属于对外契约且文案稳定。
        assert_eq!(
            VaultError::IncompatibleVersion.to_string(),
            "incompatible format or schema version"
        );
    }
}
