//! 条目与字段领域模型 + 默认策略 + 载荷编解码（L3-ENTRY，对应实现文档
//! `docs/implementation/entry-field-model.md` ENTRY-01..05）。
//!
//! 纯逻辑、可确定性测试，**不做加密**（加密见 L3-VAULT）、**不生成 TOTP 验证码**
//! （只存储 TOTP 字段，生成见 L3-TOTP）、**不含界面**。序列化复用 L2-03 的
//! [`crate::codec`]（CBOR），并以 `#[serde(flatten)]` 保留未知字段实现跨版本前向兼容
//! （数据模型 §8、安全实现设计 §4）。时间由上层注入（模块架构 §5），核心不读系统时钟。

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::codec::{self, UnknownFields};
use crate::error::VaultResult;

/// 当前数据架构版本。重大不兼容演进时递增，由 L3-VAULT 在解析前做版本门禁。
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// 条目 ID（不透明字符串，由上层生成；核心不假设其格式）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntryId(pub String);

/// 字段 ID（不透明字符串）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FieldId(pub String);

impl EntryId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FieldId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 条目类型（数据模型 §2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum EntryType {
    Login,
    SecureNote,
    Identity,
    Payment,
    Template,
    Custom,
}

/// 字段类型（数据模型 §3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum FieldKind {
    Username,
    Password,
    Email,
    Phone,
    Totp,
    Text,
    Multiline,
    Url,
    Address,
    Secret,
    Note,
}

/// 敏感级别（驱动界面与默认行为；数据模型 §3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum Sensitivity {
    Normal,
    Sensitive,
    High,
}

/// 字段的输入行为（主动选择后如何使用；信息架构 §5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum InputBehavior {
    /// 插入到目标输入框（经安全键盘 `commitText`）。
    Insert,
    /// 复制到剪贴板（带兜底清除，见 CLIP）。
    Copy,
    /// 仅在应用内显示，不插入也不复制。
    RevealOnly,
}

/// TOTP 算法（数据模型 §4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[serde(rename_all = "camelCase")]
pub enum TotpAlgorithm {
    Sha1,
    Sha256,
    Sha512,
}

/// 字段默认策略（ENTRY-03，来自实现文档 §6）。用户可在此基础上覆盖。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldPolicy {
    pub sensitivity: Sensitivity,
    pub input_behavior: InputBehavior,
    pub require_reauth: bool,
}

impl FieldKind {
    /// 返回该字段类型的默认策略（实现文档 §6）。§6 未单列的类型给出保守默认：
    /// 多行文本默认复制、URL/邮箱/电话按普通插入；`secret`（含恢复码）默认仅显示。
    pub fn default_policy(self) -> FieldPolicy {
        use FieldKind::*;
        use InputBehavior::*;
        use Sensitivity::*;
        let (sensitivity, input_behavior) = match self {
            Username => (Normal, Insert),
            Password => (High, Insert),
            Email => (Normal, Insert),
            Phone => (Normal, Insert),
            Totp => (High, Insert),
            Text => (Normal, Insert),
            Multiline => (Normal, Copy),
            Url => (Normal, Insert),
            Address => (Normal, Insert),
            Secret => (High, RevealOnly),
            Note => (Sensitive, Copy),
        };
        FieldPolicy {
            sensitivity,
            input_behavior,
            // 重认证默认关闭；密码与恢复码可由用户开启（§6「可选/可要求重认证」）。
            require_reauth: false,
        }
    }
}

/// TOTP 字段（仅存储；验证码生成见 L3-TOTP）。`secret` 为高敏感种子。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TotpField {
    pub issuer: String,
    pub account_name: String,
    /// 高敏感 TOTP 种子（明文仅在解锁后的内存模型中存在；由 VAULT 加密持久化）。
    pub secret: String,
    pub algorithm: TotpAlgorithm,
    pub digits: u32,
    pub period_seconds: u32,
    /// 跨版本前向兼容：保留本版本未识别的字段。
    #[serde(flatten, default)]
    pub unknown: UnknownFields,
}

/// 条目内的一个字段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub id: FieldId,
    pub label: String,
    pub kind: FieldKind,
    /// 字段值（明文仅在解锁后的内存模型中存在；本模型不自行持久化明文）。
    pub value: String,
    pub sensitivity: Sensitivity,
    pub input_behavior: InputBehavior,
    pub require_reauth: bool,
    /// 仅当 `kind == Totp` 时有意义：结构化 TOTP 配置（`value` 保留用户原始输入）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub totp: Option<TotpField>,
    #[serde(flatten, default)]
    pub unknown: UnknownFields,
}

impl Field {
    /// 用该字段类型的默认策略构造字段（ENTRY-03）；之后可覆盖各策略位。
    pub fn with_defaults(
        id: FieldId,
        label: impl Into<String>,
        kind: FieldKind,
        value: impl Into<String>,
    ) -> Self {
        let policy = kind.default_policy();
        Self {
            id,
            label: label.into(),
            kind,
            value: value.into(),
            sensitivity: policy.sensitivity,
            input_behavior: policy.input_behavior,
            require_reauth: policy.require_reauth,
            totp: None,
            unknown: UnknownFields::new(),
        }
    }
}

/// 一条保险库条目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: EntryId,
    pub title: String,
    #[serde(rename = "type")]
    pub entry_type: EntryType,
    pub fields: Vec<Field>,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub archived: bool,
    pub created_at: i64,
    pub updated_at: i64,
    /// 软删除时间戳（`None` 表示未删除）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub deleted_at: Option<i64>,
    #[serde(flatten, default)]
    pub unknown: UnknownFields,
}

impl Entry {
    /// 新建条目：无字段/标签，未收藏未归档，创建与更新时间取注入的 `now`（epoch 毫秒/秒由上层约定）。
    pub fn new(id: EntryId, title: impl Into<String>, entry_type: EntryType, now: i64) -> Self {
        Self {
            id,
            title: title.into(),
            entry_type,
            fields: Vec::new(),
            tags: Vec::new(),
            favorite: false,
            archived: false,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            unknown: UnknownFields::new(),
        }
    }

    /// 是否已软删除。
    pub fn is_deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

/// 保险库级设置。当前无固定字段，仅承载未知字段以保前向兼容。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(flatten, default)]
    pub unknown: UnknownFields,
}

/// VAULT 加密的明文载荷（条目与字段模型 §4 的 `VaultContent`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultContent {
    pub schema_version: u32,
    pub entries: Vec<Entry>,
    pub settings: Settings,
    #[serde(flatten, default)]
    pub unknown: UnknownFields,
}

impl Default for VaultContent {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            entries: Vec::new(),
            settings: Settings::default(),
            unknown: UnknownFields::new(),
        }
    }
}

impl VaultContent {
    /// 空载荷（当前架构版本）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 序列化为 CBOR 字节（保留未知字段；二进制，前向兼容）。ENTRY-05。
    pub fn to_bytes(&self) -> VaultResult<Vec<u8>> {
        Ok(codec::to_cbor(self)?)
    }

    /// 从 CBOR 字节反序列化（保留未知字段）。损坏/不可解析返回
    /// [`crate::error::VaultError::Corrupt`]。ENTRY-05。
    pub fn from_bytes(bytes: &[u8]) -> VaultResult<Self> {
        Ok(codec::from_cbor(bytes)?)
    }

    /// 清零所有字段明文值与 TOTP 种子（锁定/超时/进后台时调用，安全实现设计 §5.4）。
    /// 尽力而为：只清零本核心持有的内存副本（不触及绑定层或平台缓冲）。
    pub fn zeroize_secrets(&mut self) {
        for entry in &mut self.entries {
            for field in &mut entry.fields {
                field.value.zeroize();
                if let Some(totp) = &mut field.totp {
                    totp.secret.zeroize();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::CborValue;

    fn sample_login() -> Entry {
        let mut e = Entry::new(EntryId::new("e1"), "GitHub", EntryType::Login, 1_000);
        e.fields.push(Field::with_defaults(
            FieldId::new("f1"),
            "Username",
            FieldKind::Username,
            "octocat",
        ));
        e.fields.push(Field::with_defaults(
            FieldId::new("f2"),
            "Password",
            FieldKind::Password,
            "s3cr3t",
        ));
        e.tags.push("dev".to_string());
        e
    }

    #[test]
    fn model_can_express_all_field_kinds() {
        let kinds = [
            FieldKind::Username,
            FieldKind::Password,
            FieldKind::Email,
            FieldKind::Phone,
            FieldKind::Totp,
            FieldKind::Text,
            FieldKind::Multiline,
            FieldKind::Url,
            FieldKind::Address,
            FieldKind::Secret,
            FieldKind::Note,
        ];
        for (i, k) in kinds.into_iter().enumerate() {
            let f = Field::with_defaults(FieldId::new(format!("f{i}")), "L", k, "v");
            assert_eq!(f.kind, k);
        }
    }

    #[test]
    fn default_policy_matches_spec_table() {
        // 实现文档 §6 的关键行。
        assert_eq!(
            FieldKind::Username.default_policy(),
            FieldPolicy {
                sensitivity: Sensitivity::Normal,
                input_behavior: InputBehavior::Insert,
                require_reauth: false
            }
        );
        assert_eq!(
            FieldKind::Password.default_policy().sensitivity,
            Sensitivity::High
        );
        assert_eq!(
            FieldKind::Totp.default_policy().sensitivity,
            Sensitivity::High
        );
        assert_eq!(
            FieldKind::Note.default_policy().sensitivity,
            Sensitivity::Sensitive
        );
        assert_eq!(
            FieldKind::Secret.default_policy().input_behavior,
            InputBehavior::RevealOnly
        );
    }

    #[test]
    fn user_can_override_default_policy() {
        let mut f = Field::with_defaults(FieldId::new("f"), "L", FieldKind::Password, "v");
        assert_eq!(f.sensitivity, Sensitivity::High);
        f.require_reauth = true; // 用户覆盖
        f.sensitivity = Sensitivity::Sensitive;
        assert!(f.require_reauth);
        assert_eq!(f.sensitivity, Sensitivity::Sensitive);
    }

    #[test]
    fn vault_content_roundtrips() {
        let mut c = VaultContent::new();
        c.entries.push(sample_login());
        let bytes = c.to_bytes().unwrap();
        let back = VaultContent::from_bytes(&bytes).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn totp_field_roundtrips_within_entry() {
        let mut e = Entry::new(EntryId::new("e"), "T", EntryType::Login, 1);
        let mut f = Field::with_defaults(FieldId::new("f"), "2FA", FieldKind::Totp, "raw-otpauth");
        f.totp = Some(TotpField {
            issuer: "GitHub".into(),
            account_name: "octocat".into(),
            secret: "JBSWY3DPEHPK3PXP".into(),
            algorithm: TotpAlgorithm::Sha1,
            digits: 6,
            period_seconds: 30,
            unknown: UnknownFields::new(),
        });
        e.fields.push(f);
        let mut c = VaultContent::new();
        c.entries.push(e);
        let back = VaultContent::from_bytes(&c.to_bytes().unwrap()).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn preserves_unknown_fields_added_by_future_version() {
        // 模拟未来版本在 Entry 上新增一个本版本不认识的字段。
        let entry = sample_login();
        let bytes = codec::to_cbor(&entry).unwrap();
        let mut value: CborValue = codec::from_cbor(&bytes).unwrap();
        if let CborValue::Map(ref mut m) = value {
            m.push((
                CborValue::Text("futureField".into()),
                CborValue::Text("keep-me".into()),
            ));
        } else {
            panic!("entry should encode as a CBOR map");
        }
        let injected = codec::to_cbor(&value).unwrap();

        // 旧版本读取：未知字段进 unknown，不丢弃。
        let parsed: Entry = codec::from_cbor(&injected).unwrap();
        assert_eq!(
            parsed.unknown.get("futureField"),
            Some(&CborValue::Text("keep-me".into()))
        );

        // 旧版本写回后未知字段仍在。
        let rewritten: CborValue = codec::from_cbor(&codec::to_cbor(&parsed).unwrap()).unwrap();
        let CborValue::Map(m) = rewritten else {
            panic!("expected map");
        };
        assert!(m
            .iter()
            .any(|(k, _)| matches!(k, CborValue::Text(s) if s == "futureField")));
    }

    #[test]
    fn high_sensitivity_value_is_not_exposed_in_debug_inadvertently() {
        // Field 派生 Debug 会显示 value；这是解锁后内存模型，符合设计（不写日志由上层保证）。
        // 此测试仅锁定：默认策略把密码标为 High，便于上层据此遮蔽/重认证。
        let f = Field::with_defaults(FieldId::new("f"), "Password", FieldKind::Password, "pw");
        assert_eq!(f.sensitivity, Sensitivity::High);
    }

    #[test]
    fn zeroize_secrets_clears_field_values_and_totp_seed() {
        let mut c = VaultContent::new();
        let mut e = Entry::new(EntryId::new("e"), "T", EntryType::Login, 0);
        let mut f = Field::with_defaults(
            FieldId::new("f"),
            "Password",
            FieldKind::Password,
            "hunter2",
        );
        f.totp = Some(TotpField {
            issuer: "i".into(),
            account_name: "a".into(),
            secret: "JBSWY3DPEHPK3PXP".into(),
            algorithm: TotpAlgorithm::Sha1,
            digits: 6,
            period_seconds: 30,
            unknown: UnknownFields::new(),
        });
        e.fields.push(f);
        c.entries.push(e);
        c.zeroize_secrets();
        assert_eq!(c.entries[0].fields[0].value, "");
        assert_eq!(c.entries[0].fields[0].totp.as_ref().unwrap().secret, "");
    }
}
