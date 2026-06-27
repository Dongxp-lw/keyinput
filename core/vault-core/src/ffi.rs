//! FFI 表面（L2-06）：经 UniFFI 把核心能力导出给各端原生 UI（Android Kotlin / iOS Swift）。
//!
//! [`VaultCore`] 是有状态句柄（内部 `Mutex<Option<Session>>`），平台持有一个实例：先 `create` 或
//! `unlock` 建立会话，再做条目 CRUD / 读字段 / 导出导入，最后 `save` 取回加密字节由平台写文件，
//! 或 `lock` 清零。无状态能力（密码生成、TOTP、检视导出包头）以独立函数导出。
//!
//! 边界约定：口令/密钥用 `Vec<u8>`（平台可传可清零的字节数组）并在用后清零；时间 `now` 由平台
//! 注入；领域类型经 FFI DTO（[`FfiEntry`] 等）映射，**未知字段不过 FFI，但在 upsert 时按 id 保留**
//! （维持跨版本前向兼容）。返回的明文（字段值、生成密码）跨 FFI 后应由平台尽快清理——UniFFI 返回值
//! 的清零是平台侧限制（待优化）。锁定/算法不可替换等错误经 [`VaultError`] → Kotlin `VaultException`。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zeroize::Zeroize;

use crate::codec::UnknownFields;
use crate::entry::{
    Entry, EntryId, EntryType, Field, FieldId, FieldKind, InputBehavior, Sensitivity, TotpAlgorithm,
    TotpField,
};
use crate::error::{VaultError, VaultResult};
use crate::generator::{self, PasswordPolicy};
use crate::imex::{self, ExportOptions, TransferPackageHeader};
use crate::lock::Session;
use crate::totp::{self, TotpParameters};

// ---------- FFI DTO（uniffi::Record / 复用 entry 的 uniffi::Enum） ----------

/// 条目摘要（列表/搜索用，不含字段值）。
#[derive(uniffi::Record)]
pub struct EntrySummary {
    pub id: String,
    pub title: String,
    pub entry_type: EntryType,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub archived: bool,
    pub field_count: u32,
}

/// TOTP 字段（FFI）。
#[derive(uniffi::Record)]
pub struct FfiTotpField {
    pub issuer: String,
    pub account_name: String,
    pub secret: String,
    pub algorithm: TotpAlgorithm,
    pub digits: u32,
    pub period_seconds: u32,
}

/// 字段（FFI）。
#[derive(uniffi::Record)]
pub struct FfiField {
    pub id: String,
    pub label: String,
    pub kind: FieldKind,
    pub value: String,
    pub sensitivity: Sensitivity,
    pub input_behavior: InputBehavior,
    pub require_reauth: bool,
    pub totp: Option<FfiTotpField>,
}

/// 条目（FFI）。
#[derive(uniffi::Record)]
pub struct FfiEntry {
    pub id: String,
    pub title: String,
    pub entry_type: EntryType,
    pub fields: Vec<FfiField>,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub archived: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 密码生成策略（FFI）。
#[derive(uniffi::Record)]
pub struct FfiPasswordPolicy {
    pub length: u32,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
    pub exclude_ambiguous: bool,
}

/// TOTP 生成参数（FFI）。`secret` 为解码后的原始密钥字节。
#[derive(uniffi::Record)]
pub struct FfiTotpParams {
    pub secret: Vec<u8>,
    pub algorithm: TotpAlgorithm,
    pub digits: u32,
    pub period_seconds: u32,
    pub t0_seconds: i64,
}

/// TOTP 验证码（FFI）。
#[derive(uniffi::Record)]
pub struct FfiTotpCode {
    pub code: String,
    pub valid_until_epoch_seconds: i64,
    pub seconds_remaining: u32,
}

/// 导出选项（FFI）。
#[derive(uniffi::Record)]
pub struct FfiExportOptions {
    pub vault_id: String,
    pub source_device_label: Option<String>,
    pub created_at: i64,
}

/// 迁移包明文头部（FFI，检视用）。
#[derive(uniffi::Record)]
pub struct FfiTransferHeader {
    pub magic: String,
    pub package_version: u32,
    pub schema_version: u32,
    pub created_at: i64,
    pub source_device_label: Option<String>,
    pub vault_id: String,
}

// ---------- 域 → FFI ----------

impl EntrySummary {
    fn from_entry(e: &Entry) -> Self {
        Self {
            id: e.id.0.clone(),
            title: e.title.clone(),
            entry_type: e.entry_type,
            tags: e.tags.clone(),
            favorite: e.favorite,
            archived: e.archived,
            field_count: e.fields.len() as u32,
        }
    }
}

impl FfiField {
    fn from_field(f: &Field) -> Self {
        Self {
            id: f.id.0.clone(),
            label: f.label.clone(),
            kind: f.kind,
            value: f.value.clone(),
            sensitivity: f.sensitivity,
            input_behavior: f.input_behavior,
            require_reauth: f.require_reauth,
            totp: f.totp.as_ref().map(|t| FfiTotpField {
                issuer: t.issuer.clone(),
                account_name: t.account_name.clone(),
                secret: t.secret.clone(),
                algorithm: t.algorithm,
                digits: t.digits,
                period_seconds: t.period_seconds,
            }),
        }
    }
}

impl FfiEntry {
    fn from_entry(e: &Entry) -> Self {
        Self {
            id: e.id.0.clone(),
            title: e.title.clone(),
            entry_type: e.entry_type,
            fields: e.fields.iter().map(FfiField::from_field).collect(),
            tags: e.tags.clone(),
            favorite: e.favorite,
            archived: e.archived,
            created_at: e.created_at,
            updated_at: e.updated_at,
        }
    }
}

impl FfiTransferHeader {
    fn from_header(h: TransferPackageHeader) -> Self {
        Self {
            magic: h.magic,
            package_version: h.package_version,
            schema_version: h.schema_version,
            created_at: h.created_at,
            source_device_label: h.source_device_label,
            vault_id: h.vault_id,
        }
    }
}

// ---------- FFI → 域（upsert 时按 id 保留未知字段，维持前向兼容） ----------

fn ffi_entry_to_domain(ffi: FfiEntry, existing: Option<&Entry>, now: i64) -> Entry {
    // 既有字段按 id 索引，用于保留字段级与 TOTP 级未知字段。
    let prior_fields: HashMap<&str, &Field> = existing
        .map(|e| e.fields.iter().map(|f| (f.id.0.as_str(), f)).collect())
        .unwrap_or_default();

    let fields = ffi
        .fields
        .into_iter()
        .map(|ff| {
            let prior = prior_fields.get(ff.id.as_str()).copied();
            let field_unknown: UnknownFields = prior.map(|f| f.unknown.clone()).unwrap_or_default();
            let totp = ff.totp.map(|t| {
                let totp_unknown: UnknownFields = prior
                    .and_then(|f| f.totp.as_ref())
                    .map(|pt| pt.unknown.clone())
                    .unwrap_or_default();
                TotpField {
                    issuer: t.issuer,
                    account_name: t.account_name,
                    secret: t.secret,
                    algorithm: t.algorithm,
                    digits: t.digits,
                    period_seconds: t.period_seconds,
                    unknown: totp_unknown,
                }
            });
            Field {
                id: FieldId::new(ff.id),
                label: ff.label,
                kind: ff.kind,
                value: ff.value,
                sensitivity: ff.sensitivity,
                input_behavior: ff.input_behavior,
                require_reauth: ff.require_reauth,
                totp,
                unknown: field_unknown,
            }
        })
        .collect();

    Entry {
        id: EntryId::new(ffi.id),
        title: ffi.title,
        entry_type: ffi.entry_type,
        fields,
        tags: ffi.tags,
        favorite: ffi.favorite,
        archived: ffi.archived,
        // 既有条目保留其创建时间与软删除/未知字段；新条目用入参创建时间。
        created_at: existing.map_or(ffi.created_at, |e| e.created_at),
        updated_at: now,
        deleted_at: existing.and_then(|e| e.deleted_at),
        unknown: existing.map(|e| e.unknown.clone()).unwrap_or_default(),
    }
}

// ---------- VaultCore（有状态 FFI 对象） ----------

/// 经 UniFFI 导出的核心句柄。内部持有至多一个已解锁会话。
#[derive(uniffi::Object)]
pub struct VaultCore {
    inner: Mutex<Option<Session>>,
}

impl VaultCore {
    fn with_session<T>(&self, f: impl FnOnce(&Session) -> VaultResult<T>) -> VaultResult<T> {
        let guard = self.inner.lock().expect("vault mutex poisoned");
        f(guard.as_ref().ok_or(VaultError::Locked)?)
    }

    fn with_session_mut<T>(&self, f: impl FnOnce(&mut Session) -> VaultResult<T>) -> VaultResult<T> {
        let mut guard = self.inner.lock().expect("vault mutex poisoned");
        f(guard.as_mut().ok_or(VaultError::Locked)?)
    }
}

#[uniffi::export]
impl VaultCore {
    /// 新建一个锁定状态的核心句柄。
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(None),
        })
    }

    /// 当前是否已解锁。
    pub fn is_unlocked(&self) -> bool {
        self.inner.lock().expect("vault mutex poisoned").is_some()
    }

    /// 用主密码新建一个空保险库会话（生产 KDF 参数）。
    pub fn create(&self, mut master_password: Vec<u8>, now: i64) -> VaultResult<()> {
        let session = Session::create(&master_password, now);
        master_password.zeroize();
        *self.inner.lock().expect("vault mutex poisoned") = Some(session?);
        Ok(())
    }

    /// 用主密码解锁已有保险库字节，建立会话。
    pub fn unlock(&self, bytes: Vec<u8>, mut master_password: Vec<u8>, now: i64) -> VaultResult<()> {
        let session = Session::unlock(&bytes, &master_password, now);
        master_password.zeroize();
        *self.inner.lock().expect("vault mutex poisoned") = Some(session?);
        Ok(())
    }

    /// 锁定并清零会话（字段明文 + TOTP 种子 + DEK）。
    pub fn lock(&self) {
        self.inner.lock().expect("vault mutex poisoned").take();
    }

    /// 序列化并加密当前会话内容，返回保险库字节供平台写文件。
    pub fn save(&self) -> VaultResult<Vec<u8>> {
        self.with_session(|s| s.vault().save())
    }

    /// 改主密码：返回重新加密后的保险库字节。
    pub fn change_password(&self, mut old: Vec<u8>, mut new: Vec<u8>) -> VaultResult<Vec<u8>> {
        let result = self.with_session_mut(|s| s.vault_mut().change_password(&old, &new));
        old.zeroize();
        new.zeroize();
        result
    }

    /// 列出未删除条目的摘要。
    pub fn list_entries(&self) -> VaultResult<Vec<EntrySummary>> {
        self.with_session(|s| {
            Ok(s.vault()
                .entries()
                .list()
                .into_iter()
                .map(EntrySummary::from_entry)
                .collect())
        })
    }

    /// 取单条条目（不存在返回 `None`）。
    pub fn get_entry(&self, id: String) -> VaultResult<Option<FfiEntry>> {
        self.with_session(|s| {
            Ok(s.vault()
                .entries()
                .get(&EntryId::new(id))
                .map(FfiEntry::from_entry))
        })
    }

    /// 本地搜索（标题/标签/字段标签/普通敏感度字段值），返回摘要。
    pub fn search(&self, query: String) -> VaultResult<Vec<EntrySummary>> {
        self.with_session(|s| {
            Ok(s.vault()
                .entries()
                .search(&query)
                .into_iter()
                .map(EntrySummary::from_entry)
                .collect())
        })
    }

    /// 插入或更新条目（按 id 保留未知字段）；刷新空闲计时。
    pub fn upsert_entry(&self, entry: FfiEntry, now: i64) -> VaultResult<()> {
        self.with_session_mut(|s| {
            let id = EntryId::new(entry.id.clone());
            let existing = s.vault().entries().get(&id).cloned();
            let domain = ffi_entry_to_domain(entry, existing.as_ref(), now);
            s.vault_mut().entries_mut().upsert(domain);
            s.touch(now);
            Ok(())
        })
    }

    /// 软删除条目；返回是否命中。
    pub fn delete_entry(&self, id: String, now: i64) -> VaultResult<bool> {
        self.with_session_mut(|s| Ok(s.vault_mut().entries_mut().soft_delete(&EntryId::new(id), now)))
    }

    /// 读取某字段的明文值（不存在返回 `None`）。返回值为秘密，平台用后尽快清理。
    pub fn get_field_value(
        &self,
        entry_id: String,
        field_id: String,
    ) -> VaultResult<Option<String>> {
        self.with_session(|s| {
            Ok(s.vault().entries().get(&EntryId::new(entry_id)).and_then(|e| {
                e.fields
                    .iter()
                    .find(|f| f.id.0 == field_id)
                    .map(|f| f.value.clone())
            }))
        })
    }

    /// 平台在使用会话时调用，刷新空闲计时基准。
    pub fn touch(&self, now: i64) {
        if let Some(s) = self
            .inner
            .lock()
            .expect("vault mutex poisoned")
            .as_mut()
        {
            s.touch(now);
        }
    }

    /// 纯空闲判定（供平台自动锁定计时）。锁定态返回 `false`。
    pub fn is_idle_expired(&self, now: i64, idle_timeout: i64) -> bool {
        self.inner
            .lock()
            .expect("vault mutex poisoned")
            .as_ref()
            .is_some_and(|s| s.is_idle_expired(now, idle_timeout))
    }

    /// 用导出口令把当前会话内容导出为加密迁移包字节。
    pub fn export_package(
        &self,
        mut passphrase: Vec<u8>,
        options: FfiExportOptions,
    ) -> VaultResult<Vec<u8>> {
        let opts = ExportOptions {
            vault_id: options.vault_id,
            source_device_label: options.source_device_label,
            created_at: options.created_at,
        };
        let result = self.with_session(|s| imex::export(s.vault().entries().content(), &passphrase, &opts));
        passphrase.zeroize();
        result
    }

    /// 用导出口令导入迁移包，整库恢复到当前已解锁会话（沿用当前主密码）。
    pub fn import_package(&self, bytes: Vec<u8>, mut passphrase: Vec<u8>) -> VaultResult<()> {
        let imported = imex::import(&bytes, &passphrase);
        passphrase.zeroize();
        let content = imported?;
        self.with_session_mut(|s| {
            s.vault_mut().replace_content(content);
            Ok(())
        })
    }
}

// ---------- 无状态能力（独立导出函数） ----------

/// 按策略生成强密码（CSPRNG + 无偏置采样）。返回值为秘密，平台用后尽快清理。
#[uniffi::export]
pub fn generate_password(policy: FfiPasswordPolicy) -> VaultResult<String> {
    let domain = PasswordPolicy {
        length: policy.length as usize,
        lowercase: policy.lowercase,
        uppercase: policy.uppercase,
        digits: policy.digits,
        symbols: policy.symbols,
        exclude_ambiguous: policy.exclude_ambiguous,
    };
    Ok(generator::generate(&domain)?.as_str().to_owned())
}

/// 按注入时间生成 TOTP 验证码（RFC 6238）。
#[uniffi::export]
pub fn totp_now(params: FfiTotpParams, now_epoch_seconds: i64) -> VaultResult<FfiTotpCode> {
    let domain = TotpParameters {
        secret: params.secret,
        algorithm: params.algorithm,
        digits: params.digits,
        period_seconds: params.period_seconds,
        t0_seconds: params.t0_seconds,
    };
    let code = totp::generate(&domain, now_epoch_seconds)?;
    Ok(FfiTotpCode {
        code: code.code,
        valid_until_epoch_seconds: code.valid_until_epoch_seconds,
        seconds_remaining: code.seconds_remaining,
    })
}

/// 仅解析迁移包明文头部并校验兼容性（不解密、不需口令）。
#[uniffi::export]
pub fn inspect_package(bytes: Vec<u8>) -> VaultResult<FfiTransferHeader> {
    imex::inspect(&bytes).map(FfiTransferHeader::from_header)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::CborValue;
    use crate::crypto::KdfParams;
    use crate::vault::Vault;

    /// 测试用快 KDF 参数（避免生产 Argon2id 在 debug 下过慢）。
    fn fast_params() -> KdfParams {
        KdfParams {
            m_kib: 4096,
            t: 1,
            p: 1,
            salt: vec![0x44; 16],
        }
    }

    impl VaultCore {
        /// 测试辅助：用快参建立会话，绕过 FFI `create` 的生产 KDF。
        fn set_test_session(&self, session: Session) {
            *self.inner.lock().unwrap() = Some(session);
        }
    }

    fn fast_core() -> Arc<VaultCore> {
        let core = VaultCore::new();
        let vault = Vault::create_with_params(b"master pw", fast_params()).unwrap();
        core.set_test_session(Session::from_vault(vault, 0));
        core
    }

    fn sample_entry() -> FfiEntry {
        FfiEntry {
            id: "e1".into(),
            title: "GitHub".into(),
            entry_type: EntryType::Login,
            fields: vec![FfiField {
                id: "u".into(),
                label: "Username".into(),
                kind: FieldKind::Username,
                value: "octocat".into(),
                sensitivity: Sensitivity::Normal,
                input_behavior: InputBehavior::Insert,
                require_reauth: false,
                totp: None,
            }],
            tags: vec!["dev".into()],
            favorite: false,
            archived: false,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn save_then_unlock_via_ffi_roundtrips() {
        let core = fast_core();
        core.upsert_entry(sample_entry(), 0).unwrap();
        assert_eq!(core.list_entries().unwrap().len(), 1);
        let bytes = core.save().unwrap();
        core.lock();
        assert!(!core.is_unlocked());

        // unlock 用文件头里的快参 → 快。
        let core2 = VaultCore::new();
        core2
            .unlock(bytes, b"master pw".to_vec(), 1)
            .unwrap();
        assert_eq!(
            core2
                .get_field_value("e1".into(), "u".into())
                .unwrap()
                .as_deref(),
            Some("octocat")
        );
    }

    #[test]
    fn locked_operations_return_locked_error() {
        let core = VaultCore::new();
        assert_eq!(core.save().unwrap_err(), VaultError::Locked);
        assert!(matches!(core.list_entries(), Err(VaultError::Locked)));
        assert!(!core.is_unlocked());
    }

    #[test]
    fn unlock_wrong_password_is_indistinguishable() {
        let bytes = fast_core().save().unwrap();
        let core = VaultCore::new();
        assert_eq!(
            core.unlock(bytes, b"WRONG".to_vec(), 0).unwrap_err(),
            VaultError::WrongPasswordOrTampered
        );
    }

    #[test]
    fn search_and_delete_via_ffi() {
        let core = fast_core();
        core.upsert_entry(sample_entry(), 0).unwrap();
        assert_eq!(core.search("hub".into()).unwrap().len(), 1);
        assert!(core.delete_entry("e1".into(), 1).unwrap());
        assert!(core.list_entries().unwrap().is_empty());
        assert!(core.get_entry("e1".into()).unwrap().is_none());
    }

    #[test]
    fn upsert_preserves_unknown_fields() {
        let core = fast_core();
        // 直接在底层放一个带未知字段的条目（模拟未来版本写入）。
        {
            let mut guard = core.inner.lock().unwrap();
            let repo = guard.as_mut().unwrap().vault_mut().entries_mut();
            let mut e = Entry::new(EntryId::new("e1"), "GitHub", EntryType::Login, 0);
            e.unknown
                .insert("futureField".into(), CborValue::Text("keep".into()));
            let mut f = Field::with_defaults(FieldId::new("u"), "User", FieldKind::Username, "octocat");
            f.unknown.insert("ff".into(), CborValue::Bool(true));
            e.fields.push(f);
            repo.upsert(e);
        }
        // 经 FFI 取出、编辑、写回（DTO 不带未知字段）。
        let mut ffi = core.get_entry("e1".into()).unwrap().unwrap();
        ffi.title = "GitHub (edited)".into();
        core.upsert_entry(ffi, 5).unwrap();
        // 未知字段应仍在底层。
        let guard = core.inner.lock().unwrap();
        let e = guard
            .as_ref()
            .unwrap()
            .vault()
            .entries()
            .get(&EntryId::new("e1"))
            .unwrap();
        assert_eq!(e.title, "GitHub (edited)");
        assert_eq!(
            e.unknown.get("futureField"),
            Some(&CborValue::Text("keep".into()))
        );
        assert_eq!(e.fields[0].unknown.get("ff"), Some(&CborValue::Bool(true)));
    }

    #[test]
    fn generate_password_and_totp_via_ffi() {
        let pw = generate_password(FfiPasswordPolicy {
            length: 16,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: false,
            exclude_ambiguous: false,
        })
        .unwrap();
        assert_eq!(pw.chars().count(), 16);

        let code = totp_now(
            FfiTotpParams {
                secret: b"12345678901234567890".to_vec(),
                algorithm: TotpAlgorithm::Sha1,
                digits: 8,
                period_seconds: 30,
                t0_seconds: 0,
            },
            59,
        )
        .unwrap();
        assert_eq!(code.code, "94287082"); // RFC 6238 附录 B
    }

    #[test]
    fn inspect_and_import_package_via_ffi() {
        // 用快参直接造导出包（避免生产 KDF）。
        let mut content = crate::entry::VaultContent::new();
        let mut e = Entry::new(EntryId::new("e1"), "GitHub", EntryType::Login, 0);
        e.fields.push(Field::with_defaults(
            FieldId::new("u"),
            "User",
            FieldKind::Username,
            "octocat",
        ));
        content.entries.push(e);
        let pkg = imex::export_with_params(
            &content,
            b"export pass",
            &ExportOptions {
                vault_id: "v1".into(),
                source_device_label: None,
                created_at: 100,
            },
            fast_params(),
        )
        .unwrap();

        let header = inspect_package(pkg.clone()).unwrap();
        assert_eq!(header.vault_id, "v1");
        assert_eq!(header.magic, crate::imex::TRANSFER_MAGIC);

        let core = fast_core();
        core.import_package(pkg, b"export pass".to_vec()).unwrap();
        assert_eq!(core.list_entries().unwrap().len(), 1);
        assert_eq!(
            core.get_field_value("e1".into(), "u".into())
                .unwrap()
                .as_deref(),
            Some("octocat")
        );
    }
}
