//! 密码生成器（L3-GEN，实现文档 `docs/implementation/password-generator.md` GEN-01..04）。
//!
//! 基于 CSPRNG（[`crate::rng::uniform_index`] 的无偏置采样）按策略生成强密码：可配置长度与字符类、
//! 可选排除易混淆字符。生成值按秘密处理——返回 [`Zeroizing<String>`]，Drop 时清零；**绝不记录**
//! （安全实现设计 §2.3、实现文档 §7）。生成值的存储由 ENTRY/VAULT 负责。

use zeroize::Zeroizing;

use crate::error::{VaultError, VaultResult};
use crate::rng;

const LOWERCASE: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPERCASE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.?";
/// 易混淆字符（启用排除时从字符集剔除）：大写 O、数字 0、小写 l、大写 I、数字 1。
const AMBIGUOUS: &str = "O0lI1";
/// 生成长度下限（安全基线；低于此拒绝，提升交由上层 UX）。
pub const MIN_LENGTH: usize = 8;

/// 密码生成策略（实现文档 §4）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasswordPolicy {
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
    pub exclude_ambiguous: bool,
}

impl Default for PasswordPolicy {
    /// 稳健默认：长度 20、全字符类、不排除易混淆。
    fn default() -> Self {
        Self {
            length: 20,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: false,
        }
    }
}

/// 按策略组装允许字符集（§5.1）。
fn charset(policy: &PasswordPolicy) -> Vec<char> {
    let mut s = String::new();
    if policy.lowercase {
        s.push_str(LOWERCASE);
    }
    if policy.uppercase {
        s.push_str(UPPERCASE);
    }
    if policy.digits {
        s.push_str(DIGITS);
    }
    if policy.symbols {
        s.push_str(SYMBOLS);
    }
    if policy.exclude_ambiguous {
        s.retain(|c| !AMBIGUOUS.contains(c));
    }
    s.chars().collect()
}

/// 按策略生成密码，返回 [`Zeroizing<String>`]（Drop 时清零）。
///
/// 校验失败返回 [`VaultError::InvalidInput`]：未选任何字符类、长度低于 [`MIN_LENGTH`]、或排除
/// 易混淆后字符集为空。映射用拒绝采样避免取模偏置（§5.3、§7）。
pub fn generate(policy: &PasswordPolicy) -> VaultResult<Zeroizing<String>> {
    let any_class = policy.lowercase || policy.uppercase || policy.digits || policy.symbols;
    if !any_class {
        return Err(VaultError::InvalidInput);
    }
    if policy.length < MIN_LENGTH {
        return Err(VaultError::InvalidInput);
    }
    let set = charset(policy);
    if set.is_empty() {
        return Err(VaultError::InvalidInput);
    }
    let bound = set.len() as u32;
    let mut out = String::with_capacity(policy.length);
    for _ in 0..policy.length {
        let idx = rng::uniform_index(bound)? as usize;
        out.push(set[idx]);
    }
    Ok(Zeroizing::new(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respects_length_and_charset() {
        let policy = PasswordPolicy::default();
        let pw = generate(&policy).unwrap();
        assert_eq!(pw.chars().count(), policy.length);
        let allowed = charset(&policy);
        assert!(pw.chars().all(|c| allowed.contains(&c)));
    }

    #[test]
    fn only_digits_when_only_digits_selected() {
        let policy = PasswordPolicy {
            length: 16,
            lowercase: false,
            uppercase: false,
            digits: true,
            symbols: false,
            exclude_ambiguous: false,
        };
        let pw = generate(&policy).unwrap();
        assert!(pw.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn exclude_ambiguous_removes_those_chars() {
        let policy = PasswordPolicy {
            length: 200,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: false,
            exclude_ambiguous: true,
        };
        let pw = generate(&policy).unwrap();
        assert!(pw.chars().all(|c| !AMBIGUOUS.contains(c)));
    }

    #[test]
    fn rejects_no_character_class() {
        let policy = PasswordPolicy {
            length: 16,
            lowercase: false,
            uppercase: false,
            digits: false,
            symbols: false,
            exclude_ambiguous: false,
        };
        assert_eq!(generate(&policy).unwrap_err(), VaultError::InvalidInput);
    }

    #[test]
    fn rejects_length_below_minimum() {
        let policy = PasswordPolicy {
            length: MIN_LENGTH - 1,
            ..PasswordPolicy::default()
        };
        assert_eq!(generate(&policy).unwrap_err(), VaultError::InvalidInput);
    }

    #[test]
    fn generated_chars_have_no_obvious_bias() {
        // 仅数字字符集（10 类）；长样本下各数字应接近均匀（GEN-04）。
        let policy = PasswordPolicy {
            length: 10_000,
            lowercase: false,
            uppercase: false,
            digits: true,
            symbols: false,
            exclude_ambiguous: false,
        };
        let pw = generate(&policy).unwrap();
        let mut counts = [0u32; 10];
        for c in pw.chars() {
            counts[c.to_digit(10).unwrap() as usize] += 1;
        }
        let expected = 1_000u32; // 10000 / 10
        for c in counts {
            assert!(
                c > expected * 4 / 5 && c < expected * 6 / 5,
                "digit count {c} outside ±20% of {expected}"
            );
        }
    }
}
