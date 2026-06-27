//! 秘密类型与卫生（L2-01）。
//!
//! 安全清零由第三方 `zeroize` crate 提供硬保证（`ZeroizeOnDrop`）；本模块只把它包成
//! 带类型安全与遮蔽 `Debug` 的领域类型，**不实现任何安全原语**。常量时间比较用
//! `subtle`。规则见 docs/technical/security-implementation-design.md 第 7 节。

use core::fmt;

use subtle::ConstantTimeEq;
use zeroize::ZeroizeOnDrop;

/// 定长 32 字节密钥材料（KEK / DEK / 子密钥）。Drop 时清零，`Debug` 遮蔽。
#[derive(ZeroizeOnDrop)]
pub struct SecretKey([u8; 32]);

impl SecretKey {
    /// 由 32 字节构造。
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// 暴露原始字节供加密原语使用。调用方不得复制后泄露或写入日志。
    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }

    /// 常量时间相等比较（避免计时侧信道；用于密钥相等判断与测试）。
    pub fn ct_eq(&self, other: &SecretKey) -> bool {
        self.0.ct_eq(&other.0).into()
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKey(***)")
    }
}

/// 变长秘密字节（主密码 UTF-8、明文载荷、字段值等）。Drop 时清零，`Debug` 遮蔽。
#[derive(ZeroizeOnDrop)]
pub struct SecretBytes(Vec<u8>);

impl SecretBytes {
    /// 由字节向量构造（构造后原向量所有权转入，随本类型一并清零）。
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// 暴露原始字节。调用方不得复制后泄露或写入日志。
    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretBytes(*** {} bytes)", self.0.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_key_debug_is_redacted() {
        let k = SecretKey::new([7u8; 32]);
        let d = format!("{k:?}");
        assert_eq!(d, "SecretKey(***)");
        assert!(!d.contains('7'));
    }

    #[test]
    fn secret_bytes_debug_redacts_value_but_shows_len() {
        let s = SecretBytes::new(b"hunter2".to_vec());
        let d = format!("{s:?}");
        assert_eq!(d, "SecretBytes(*** 7 bytes)");
        assert!(!d.contains("hunter2"));
        assert_eq!(s.len(), 7);
        assert!(!s.is_empty());
    }

    #[test]
    fn secret_key_ct_eq_matches_value_equality() {
        assert!(SecretKey::new([1u8; 32]).ct_eq(&SecretKey::new([1u8; 32])));
        assert!(!SecretKey::new([1u8; 32]).ct_eq(&SecretKey::new([2u8; 32])));
    }
}
