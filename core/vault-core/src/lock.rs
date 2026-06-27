//! 解锁会话（L3-LOCK，核心侧；实现文档 `docs/implementation/master-password-unlock.md`）。
//!
//! 核心只负责：用主密码解锁建立会话、**最小持有会话密钥**（仅保 DEK，随会话 Drop / 锁定清零；
//! 绝不持有 KEK 或主密码）、锁定时清零密钥与敏感字段值、以及供平台自动锁定用的**纯空闲判定**。
//! 自动锁定计时与生命周期监听（`ProcessLifecycleOwner`）、键盘授权生命周期、生物识别、失败退避
//! UI 均在平台（L4，模块架构 §6）。
//!
//! 错误口令与数据篡改不可区分（继承 [`Vault::open`] 的 [`crate::error::VaultError::WrongPasswordOrTampered`]）。

use core::fmt;

use crate::error::VaultResult;
use crate::vault::Vault;

/// 只驻留内存的解锁会话：持有已解锁的 [`Vault`]（含 DEK 与解密内容）与会话时间戳。
///
/// 无 `Clone`、`Debug` 遮蔽：避免会话密钥与字段明文被复制或打印。Drop 时清零字段明文值与 TOTP 种子，
/// 且 [`Vault`] 内 DEK 经其 [`crate::secret::SecretKey`] 自动清零（即使未显式 [`Session::lock`]）。
pub struct Session {
    vault: Vault,
    created_at: i64,
    last_used_at: i64,
}

/// 遮蔽 `Debug`：只显示非敏感的时间戳与条目数，绝不打印 DEK 或字段明文。
impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("created_at", &self.created_at)
            .field("last_used_at", &self.last_used_at)
            .field("entries", &self.vault.entries().list().len())
            .finish_non_exhaustive()
    }
}

impl Session {
    /// 用主密码解锁 `bytes` 建立会话。`now` 由平台注入（epoch，单位由上层约定）。
    /// 错误口令 / 篡改返回 [`crate::error::VaultError::WrongPasswordOrTampered`]（不可区分）。
    pub fn unlock(bytes: &[u8], master_password: &[u8], now: i64) -> VaultResult<Self> {
        Ok(Self::from_vault(Vault::open(bytes, master_password)?, now))
    }

    /// 新建空保险库会话（生产默认 KDF 参数）。
    pub fn create(master_password: &[u8], now: i64) -> VaultResult<Self> {
        Ok(Self::from_vault(Vault::create(master_password)?, now))
    }

    /// 把已打开/新建的 [`Vault`] 包装为会话。
    pub fn from_vault(vault: Vault, now: i64) -> Self {
        Self {
            vault,
            created_at: now,
            last_used_at: now,
        }
    }

    /// 只读访问已解锁保险库（读条目/字段、保存等）。
    pub fn vault(&self) -> &Vault {
        &self.vault
    }

    /// 可变访问已解锁保险库（CRUD）。平台应在使用后调用 [`Session::touch`] 刷新空闲计时。
    pub fn vault_mut(&mut self) -> &mut Vault {
        &mut self.vault
    }

    /// 会话建立时间。
    pub fn created_at(&self) -> i64 {
        self.created_at
    }

    /// 最近一次使用时间（空闲计时基准）。
    pub fn last_used_at(&self) -> i64 {
        self.last_used_at
    }

    /// 平台在每次使用会话时调用，刷新空闲计时基准。
    pub fn touch(&mut self, now: i64) {
        self.last_used_at = now;
    }

    /// 纯空闲判定：自 `last_used_at` 起经过 `idle_timeout` 即视为应锁定。供平台自动锁定计时调用。
    pub fn is_idle_expired(&self, now: i64, idle_timeout: i64) -> bool {
        now.saturating_sub(self.last_used_at) > idle_timeout
    }

    /// 显式锁定：消费会话并清零密钥与敏感值。等价于丢弃会话（清零在 [`Drop`] 中完成）。
    pub fn lock(self) {
        // 消费 self → 触发 Drop：清零字段明文值与 TOTP 种子，DEK 随 Vault 清零。
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // 锁定/超时/进后台/作用域结束都清零敏感值（安全实现设计 §5.4）；DEK 由 Vault 的 SecretKey 清零。
        self.vault.entries_mut().content_mut().zeroize_secrets();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::KdfParams;
    use crate::entry::{Entry, EntryId, EntryType, Field, FieldId, FieldKind};
    use crate::error::VaultError;
    use crate::vault::Vault;

    fn fast_params() -> KdfParams {
        KdfParams {
            m_kib: 4096,
            t: 1,
            p: 1,
            salt: vec![0x22; 16],
        }
    }

    fn sealed_vault_bytes() -> Vec<u8> {
        let mut v = Vault::create_with_params(b"master pw", fast_params()).unwrap();
        let mut e = Entry::new(EntryId::new("e1"), "GitHub", EntryType::Login, 1);
        e.fields.push(Field::with_defaults(
            FieldId::new("u"),
            "Username",
            FieldKind::Username,
            "octocat",
        ));
        v.entries_mut().upsert(e);
        v.save().unwrap()
    }

    #[test]
    fn unlock_builds_session_and_reads() {
        let s = Session::unlock(&sealed_vault_bytes(), b"master pw", 1_000).unwrap();
        assert_eq!(s.vault().entries().list().len(), 1);
        assert_eq!(s.created_at(), 1_000);
        assert_eq!(s.last_used_at(), 1_000);
    }

    #[test]
    fn unlock_wrong_password_is_indistinguishable() {
        assert_eq!(
            Session::unlock(&sealed_vault_bytes(), b"WRONG", 0).unwrap_err(),
            VaultError::WrongPasswordOrTampered
        );
    }

    #[test]
    fn idle_expiry_and_touch() {
        let mut s = Session::unlock(&sealed_vault_bytes(), b"master pw", 1_000).unwrap();
        assert!(!s.is_idle_expired(1_500, 600)); // 空闲 500 < 600
        assert!(s.is_idle_expired(2_000, 600)); // 空闲 1000 > 600
        s.touch(2_000);
        assert_eq!(s.last_used_at(), 2_000);
        assert!(!s.is_idle_expired(2_100, 600)); // touch 后空闲 100 < 600
    }

    #[test]
    fn session_edits_persist_through_save_and_reunlock() {
        let bytes = sealed_vault_bytes();
        let mut s = Session::unlock(&bytes, b"master pw", 0).unwrap();
        s.vault_mut().entries_mut().upsert(Entry::new(
            EntryId::new("e2"),
            "GitLab",
            EntryType::Login,
            5,
        ));
        s.touch(5);
        let resaved = s.vault().save().unwrap();
        let s2 = Session::unlock(&resaved, b"master pw", 10).unwrap();
        assert_eq!(s2.vault().entries().list().len(), 2);
    }

    #[test]
    fn explicit_lock_consumes_session() {
        let s = Session::unlock(&sealed_vault_bytes(), b"master pw", 0).unwrap();
        s.lock(); // 消费；不应再可用（编译期保证）。
    }
}
