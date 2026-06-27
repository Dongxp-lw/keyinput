//! 加密导出与导入（L3-IMEX，实现文档 `docs/implementation/encrypted-export-import.md` IMEX-01..06）。
//!
//! 把解锁后的 [`VaultContent`] 用**导出口令**以信封方式加密为**独立于本地保险库文件**的迁移包
//! （`TransferPackage`）；导入时先认证校验、版本兼容检查，再解密。复用 L2 加密原语与 VAULT 的
//! [`CryptoProfile`]；迁移子密钥用 [`SUBKEY_MIGRATION`] 与本地内容密钥**域分离**（安全实现设计 §8）。
//!
//! 错口令与篡改不可区分（[`VaultError::WrongPasswordOrTampered`]）；损坏=[`VaultError::Corrupt`]；
//! 版本不兼容=[`VaultError::IncompatibleVersion`]。[`import`] **无副作用**：失败只返回错误，绝不影响
//! 调用方现有保险库（原子替换/回滚由平台的文件写入负责）。口令/密钥用后清零、绝不记录（§7）。

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::codec;
use crate::crypto::{self, KdfParams, SUBKEY_MIGRATION};
use crate::entry::{VaultContent, CURRENT_SCHEMA_VERSION};
use crate::error::{VaultError, VaultResult};
use crate::vault::CryptoProfile;

/// 迁移包类型标识（明文头部，作 AAD 认证）。
pub const TRANSFER_MAGIC: &str = "private-input-vault:transfer";
/// 迁移包格式版本。
pub const TRANSFER_PACKAGE_VERSION: u32 = 1;

/// 迁移包明文头部（数据模型 §5）。不含秘密，但经 AAD 认证。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferPackageHeader {
    pub magic: String,
    pub package_version: u32,
    pub schema_version: u32,
    pub created_at: i64,
    /// 可选来源设备标签（明文元数据，注意暴露程度；实现文档 §10）。
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub source_device_label: Option<String>,
    pub vault_id: String,
    pub crypto_profile: CryptoProfile,
}

/// 落盘的迁移包容器（CBOR）。`wrappedPackageDek` 自带认证标签；`payload` = `nonce||ct||tag`。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransferPackage {
    header: TransferPackageHeader,
    #[serde(with = "serde_bytes")]
    wrapped_package_dek: Vec<u8>,
    #[serde(with = "serde_bytes")]
    payload: Vec<u8>,
}

/// 导出选项（明文头部元数据；时间由上层注入）。
#[derive(Clone, Debug)]
pub struct ExportOptions {
    pub vault_id: String,
    pub source_device_label: Option<String>,
    pub created_at: i64,
}

/// 头部规范序列化为 AAD（与 VAULT 同法；头部固定字段、无 flatten，CBOR 编码确定）。
fn header_aad(header: &TransferPackageHeader) -> VaultResult<Vec<u8>> {
    Ok(codec::to_cbor(header)?)
}

/// 明文头部门禁：magic 不符即 [`VaultError::Corrupt`]；版本/架构过新即 [`VaultError::IncompatibleVersion`]。
fn validate_header(header: &TransferPackageHeader) -> VaultResult<()> {
    if header.magic != TRANSFER_MAGIC {
        return Err(VaultError::Corrupt);
    }
    if header.package_version > TRANSFER_PACKAGE_VERSION
        || header.schema_version > CURRENT_SCHEMA_VERSION
    {
        return Err(VaultError::IncompatibleVersion);
    }
    Ok(())
}

/// 用生产默认 KDF 参数把内容导出为加密迁移包字节。
pub fn export(
    content: &VaultContent,
    passphrase: &[u8],
    options: &ExportOptions,
) -> VaultResult<Vec<u8>> {
    export_with_params(
        content,
        passphrase,
        options,
        KdfParams::production_default()?,
    )
}

/// 用指定 KDF 参数导出（供低内存降级与测试注入快参）。
pub fn export_with_params(
    content: &VaultContent,
    passphrase: &[u8],
    options: &ExportOptions,
    params: KdfParams,
) -> VaultResult<Vec<u8>> {
    // 独立 salt 与随机 package DEK：导出包自包含，可在仅有导出口令的全新安装上恢复。
    let package_kek = crypto::derive_kek(passphrase, &params)?;
    let package_dek = crypto::random_dek()?;
    let wrapped_package_dek = crypto::wrap_dek(&package_dek, &package_kek)?;

    let header = TransferPackageHeader {
        magic: TRANSFER_MAGIC.to_string(),
        package_version: TRANSFER_PACKAGE_VERSION,
        schema_version: content.schema_version,
        created_at: options.created_at,
        source_device_label: options.source_device_label.clone(),
        vault_id: options.vault_id.clone(),
        crypto_profile: CryptoProfile::from_params(&params),
    };
    let aad = header_aad(&header)?;
    // 迁移子密钥与本地内容密钥域分离（SUBKEY_MIGRATION ≠ SUBKEY_VAULT_CONTENT）。
    let transfer_key = crypto::derive_subkey(&package_dek, SUBKEY_MIGRATION)?;
    let mut plaintext = content.to_bytes()?;
    let payload = crypto::seal(&transfer_key, &aad, &plaintext);
    plaintext.zeroize();

    let package = TransferPackage {
        header,
        wrapped_package_dek,
        payload: payload?,
    };
    Ok(codec::to_cbor(&package)?)
}

/// 仅解析明文头部并校验兼容性，**不解密**（无需口令）。
pub fn inspect(bytes: &[u8]) -> VaultResult<TransferPackageHeader> {
    let package: TransferPackage = codec::from_cbor(bytes).map_err(|_| VaultError::Corrupt)?;
    validate_header(&package.header)?;
    Ok(package.header)
}

/// 导入迁移包：先版本门禁，再派生 KEK、解包 DEK、域分离子密钥、验签解密、反序列化。
///
/// 错口令/篡改返回 [`VaultError::WrongPasswordOrTampered`]（不可区分）；损坏=[`VaultError::Corrupt`]；
/// 版本不兼容=[`VaultError::IncompatibleVersion`]。**无副作用**：失败不改动调用方现有保险库。
pub fn import(bytes: &[u8], passphrase: &[u8]) -> VaultResult<VaultContent> {
    let package: TransferPackage = codec::from_cbor(bytes).map_err(|_| VaultError::Corrupt)?;
    validate_header(&package.header)?;

    let params = package.header.crypto_profile.to_params()?;
    let package_kek = crypto::derive_kek(passphrase, &params)?;
    let package_dek = crypto::unwrap_dek(&package.wrapped_package_dek, &package_kek)?;

    let aad = header_aad(&package.header)?;
    let transfer_key = crypto::derive_subkey(&package_dek, SUBKEY_MIGRATION)?;
    let mut plaintext = crypto::open(&transfer_key, &aad, &package.payload)?;
    let content = VaultContent::from_bytes(&plaintext);
    plaintext.zeroize();
    content
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{Entry, EntryId, EntryType, Field, FieldId, FieldKind};

    fn fast_params() -> KdfParams {
        KdfParams {
            m_kib: 4096,
            t: 1,
            p: 1,
            salt: vec![0x33; 16],
        }
    }

    fn options() -> ExportOptions {
        ExportOptions {
            vault_id: "vault-1".into(),
            source_device_label: Some("pixel".into()),
            created_at: 1_700_000_000,
        }
    }

    fn sample_content() -> VaultContent {
        let mut c = VaultContent::new();
        let mut e = Entry::new(EntryId::new("e1"), "GitHub", EntryType::Login, 1);
        e.fields.push(Field::with_defaults(
            FieldId::new("u"),
            "Username",
            FieldKind::Username,
            "octocat",
        ));
        e.fields.push(Field::with_defaults(
            FieldId::new("p"),
            "Password",
            FieldKind::Password,
            "p@ss-do-not-leak",
        ));
        c.entries.push(e);
        c
    }

    fn export_sample() -> Vec<u8> {
        export_with_params(&sample_content(), b"export pass", &options(), fast_params()).unwrap()
    }

    fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    #[test]
    fn export_import_roundtrips() {
        let restored = import(&export_sample(), b"export pass").unwrap();
        assert_eq!(restored, sample_content());
    }

    #[test]
    fn exported_bytes_are_not_plaintext_readable() {
        let bytes = export_sample();
        // 字段明文值不得以明文出现在导出包中（TP-105）。
        assert!(!contains_subslice(&bytes, b"octocat"));
        assert!(!contains_subslice(&bytes, b"p@ss-do-not-leak"));
    }

    #[test]
    fn import_wrong_passphrase_is_indistinguishable() {
        assert_eq!(
            import(&export_sample(), b"WRONG").unwrap_err(),
            VaultError::WrongPasswordOrTampered
        );
    }

    #[test]
    fn import_tampered_payload_fails_safely() {
        let mut bytes = export_sample();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
        assert_eq!(
            import(&bytes, b"export pass").unwrap_err(),
            VaultError::WrongPasswordOrTampered
        );
    }

    #[test]
    fn truncated_or_wrong_magic_is_corrupt() {
        let bytes = export_sample();
        assert_eq!(
            import(&bytes[..bytes.len() / 2], b"export pass").unwrap_err(),
            VaultError::Corrupt
        );
        let mut pkg: TransferPackage = codec::from_cbor(&bytes).unwrap();
        pkg.header.magic = "nope".into();
        assert_eq!(
            import(&codec::to_cbor(&pkg).unwrap(), b"export pass").unwrap_err(),
            VaultError::Corrupt
        );
    }

    #[test]
    fn future_version_is_incompatible_in_import_and_inspect() {
        let bytes = export_sample();
        let mut pkg: TransferPackage = codec::from_cbor(&bytes).unwrap();
        pkg.header.package_version = TRANSFER_PACKAGE_VERSION + 1;
        let mutated = codec::to_cbor(&pkg).unwrap();
        assert_eq!(
            import(&mutated, b"export pass").unwrap_err(),
            VaultError::IncompatibleVersion
        );
        assert_eq!(
            inspect(&mutated).unwrap_err(),
            VaultError::IncompatibleVersion
        );
    }

    #[test]
    fn inspect_returns_header_without_passphrase() {
        let header = inspect(&export_sample()).unwrap();
        assert_eq!(header.magic, TRANSFER_MAGIC);
        assert_eq!(header.vault_id, "vault-1");
        assert_eq!(header.source_device_label.as_deref(), Some("pixel"));
        assert_eq!(header.schema_version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn import_failure_is_side_effect_free() {
        // 核心 import 无副作用：失败只返回 Err（回滚性质在核心边界天然满足）。
        let mut tampered = export_sample();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        assert!(import(&tampered, b"export pass").is_err());
        // 同一份合法导出仍可正常导入（前一次失败未污染任何状态）。
        assert_eq!(
            import(&export_sample(), b"export pass").unwrap(),
            sample_content()
        );
    }
}
