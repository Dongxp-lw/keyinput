//! 保险库加密核心：文件格式 + 创建/打开/保存/改口令（L3-VAULT，实现文档
//! `docs/implementation/vault-crypto-core.md` VAULT-01..08）。
//!
//! 组合 L2 原语：Argon2id 派生 KEK（[`crypto::derive_kek`]）→ 随机 DEK 信封包装
//! （[`crypto::wrap_dek`]）→ 由 DEK 经 HKDF 域分离出内容子密钥（[`crypto::derive_subkey`] +
//! [`SUBKEY_VAULT_CONTENT`]）→ XChaCha20-Poly1305 认证加密明文载荷（[`VaultContent`] 的 CBOR），
//! **头部作为 AAD**。安全要求见安全实现设计 §3-4、§12：头部认证、解密前验签、错误口令与篡改
//! 不可区分、失败不输出部分明文、版本不兼容不改动入参。
//!
//! 密钥卫生：[`Vault`] 仅持有会话期 DEK（[`SecretKey`]，Drop 清零）；KEK 仅在 create/open/
//! change_password 内临时存在并随作用域结束清零；解密出的明文载荷用后显式 `zeroize`。

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use core::fmt;

use crate::codec;
use crate::crypto::{self, KdfParams, SUBKEY_VAULT_CONTENT};
use crate::entry::{VaultContent, CURRENT_SCHEMA_VERSION};
use crate::error::{VaultError, VaultResult};
use crate::repository::EntryRepository;
use crate::secret::SecretKey;

/// 文件类型标识（明文头部，作 AAD 认证）。
pub const VAULT_MAGIC: &str = "private-input-vault";
/// 文件格式版本。重大不兼容演进时递增。
pub const VAULT_FORMAT_VERSION: u32 = 1;
/// KDF 标识（对应 [`crypto::derive_kek`] 固定的 Argon2id）。
const KDF_ID: &str = "argon2id";
/// Argon2 版本号（0x13 = 1.3，与 crypto 层一致）。
const ARGON2_VERSION: u32 = 0x13;
/// AEAD 标识。
const AEAD_ID: &str = "xchacha20poly1305";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KdfProfile {
    id: String,
    version: u32,
    m_kib: u32,
    t: u32,
    p: u32,
    #[serde(with = "serde_bytes")]
    salt: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AeadProfile {
    id: String,
}

/// 密码学档案（KDF + AEAD 标识与参数）。VAULT 文件头与 IMEX 迁移包头共用同一结构（IMEX 复用）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CryptoProfile {
    kdf: KdfProfile,
    aead: AeadProfile,
}

impl CryptoProfile {
    pub fn from_params(params: &KdfParams) -> Self {
        Self {
            kdf: KdfProfile {
                id: KDF_ID.to_string(),
                version: ARGON2_VERSION,
                m_kib: params.m_kib,
                t: params.t,
                p: params.p,
                salt: params.salt.clone(),
            },
            aead: AeadProfile {
                id: AEAD_ID.to_string(),
            },
        }
    }

    /// 校验算法标识/版本被本实现支持，并取出 KDF 参数。不识别即不兼容（[`VaultError::IncompatibleVersion`]）。
    pub fn to_params(&self) -> VaultResult<KdfParams> {
        if self.kdf.id != KDF_ID || self.kdf.version != ARGON2_VERSION || self.aead.id != AEAD_ID {
            return Err(VaultError::IncompatibleVersion);
        }
        Ok(KdfParams {
            m_kib: self.kdf.m_kib,
            t: self.kdf.t,
            p: self.kdf.p,
            salt: self.kdf.salt.clone(),
        })
    }
}

/// 保险库文件的明文头部。固定字段、无 flatten，确保 CBOR 编码确定（可作稳定 AAD）。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VaultHeader {
    magic: String,
    format_version: u32,
    schema_version: u32,
    crypto_profile: CryptoProfile,
}

/// 落盘的保险库文件容器（CBOR）。`wrappedDek` 自带认证标签；`payload` = `nonce||ct||tag`。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VaultFile {
    header: VaultHeader,
    #[serde(with = "serde_bytes")]
    wrapped_dek: Vec<u8>,
    #[serde(with = "serde_bytes")]
    payload: Vec<u8>,
}

/// 头部规范序列化为 AAD（安全实现设计 §4）。头部为固定字段、无 flatten，CBOR 编码确定。
fn header_aad(header: &VaultHeader) -> VaultResult<Vec<u8>> {
    Ok(codec::to_cbor(header)?)
}

/// 已打开（解锁）的保险库：持有会话期 DEK 与解密后的内容仓库（[`EntryRepository`]）。
pub struct Vault {
    crypto_profile: CryptoProfile,
    wrapped_dek: Vec<u8>,
    dek: SecretKey,
    repo: EntryRepository,
}

/// 遮蔽 `Debug`：绝不打印 DEK、wrappedDEK、salt 或任何字段值（避免秘密泄漏到日志）。
impl fmt::Debug for Vault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vault")
            .field("format_version", &VAULT_FORMAT_VERSION)
            .field("schema_version", &self.repo.content().schema_version)
            .field("entries", &self.repo.list().len())
            .finish_non_exhaustive()
    }
}

impl Vault {
    /// 用生产默认 KDF 参数新建空保险库（随机 salt/DEK）。
    pub fn create(master_password: &[u8]) -> VaultResult<Self> {
        Self::create_with_params(master_password, KdfParams::production_default()?)
    }

    /// 用指定 KDF 参数新建空保险库（供低内存设备降级档与测试注入快参数；实现文档 §6）。
    pub fn create_with_params(master_password: &[u8], params: KdfParams) -> VaultResult<Self> {
        let kek = crypto::derive_kek(master_password, &params)?;
        let dek = crypto::random_dek()?;
        let wrapped_dek = crypto::wrap_dek(&dek, &kek)?;
        Ok(Self {
            crypto_profile: CryptoProfile::from_params(&params),
            wrapped_dek,
            dek,
            repo: EntryRepository::new(),
        })
    }

    /// 打开既有保险库字节：先做格式/版本门禁，再派生 KEK、解包 DEK、解密验签。
    ///
    /// 错误口令与篡改均返回 [`VaultError::WrongPasswordOrTampered`]（不可区分）；损坏返回
    /// [`VaultError::Corrupt`]；版本/算法不兼容返回 [`VaultError::IncompatibleVersion`]。
    /// 任何失败都不输出部分明文、不改动入参。
    pub fn open(bytes: &[u8], master_password: &[u8]) -> VaultResult<Self> {
        // 解析（损坏 → Corrupt），随后在动用密码学之前做廉价的格式/版本门禁。
        let file: VaultFile = codec::from_cbor(bytes).map_err(|_| VaultError::Corrupt)?;
        if file.header.magic != VAULT_MAGIC {
            return Err(VaultError::Corrupt);
        }
        if file.header.format_version > VAULT_FORMAT_VERSION
            || file.header.schema_version > CURRENT_SCHEMA_VERSION
        {
            return Err(VaultError::IncompatibleVersion);
        }
        let params = file.header.crypto_profile.to_params()?;

        // 派生 KEK → 解包 DEK（失败即错误口令/篡改，不可区分）。
        let kek = crypto::derive_kek(master_password, &params)?;
        let dek = crypto::unwrap_dek(&file.wrapped_dek, &kek)?;

        // 头部作 AAD，解密并验签载荷（失败即篡改/错误口令，不可区分；不输出部分明文）。
        let aad = header_aad(&file.header)?;
        let content_key = crypto::derive_subkey(&dek, SUBKEY_VAULT_CONTENT)?;
        let mut plaintext = crypto::open(&content_key, &aad, &file.payload)?;
        let content = VaultContent::from_bytes(&plaintext);
        plaintext.zeroize();
        let content = content?;

        Ok(Self {
            crypto_profile: file.header.crypto_profile,
            wrapped_dek: file.wrapped_dek,
            dek,
            repo: EntryRepository::from_content(content),
        })
    }

    /// 序列化并加密为保险库字节。每次保存用全新随机 nonce（安全实现设计 §5.7）。
    pub fn save(&self) -> VaultResult<Vec<u8>> {
        let header = self.header();
        let aad = header_aad(&header)?;
        let content_key = crypto::derive_subkey(&self.dek, SUBKEY_VAULT_CONTENT)?;
        let mut plaintext = self.repo.content().to_bytes()?;
        let payload = crypto::seal(&content_key, &aad, &plaintext);
        plaintext.zeroize();
        let file = VaultFile {
            header,
            wrapped_dek: self.wrapped_dek.clone(),
            payload: payload?,
        };
        Ok(codec::to_cbor(&file)?)
    }

    /// 改主密码：校验旧口令后仅用新口令重新包装同一 DEK（不重加密整库；salt 不变），再保存。
    /// 旧口令错误返回 [`VaultError::WrongPasswordOrTampered`]。
    pub fn change_password(
        &mut self,
        old_password: &[u8],
        new_password: &[u8],
    ) -> VaultResult<Vec<u8>> {
        let params = self.crypto_profile.to_params()?;
        // 验证旧口令：用旧 KEK 解包 DEK 必须成功（失败即旧口令错/篡改，不可区分）。解出的 DEK 即弃（Drop 清零）。
        let old_kek = crypto::derive_kek(old_password, &params)?;
        crypto::unwrap_dek(&self.wrapped_dek, &old_kek)?;
        // 仅用新口令重新派生 KEK 并重包装同一 DEK。
        let new_kek = crypto::derive_kek(new_password, &params)?;
        self.wrapped_dek = crypto::wrap_dek(&self.dek, &new_kek)?;
        self.save()
    }

    /// 借出条目仓库（只读）。
    pub fn entries(&self) -> &EntryRepository {
        &self.repo
    }

    /// 借出条目仓库（可变，用于 CRUD）；之后调用 [`Vault::save`] 持久化。
    pub fn entries_mut(&mut self) -> &mut EntryRepository {
        &mut self.repo
    }

    /// 用新内容替换保险库条目（如 IMEX 整库恢复后导入到当前已解锁保险库）。沿用当前 DEK/口令。
    pub fn replace_content(&mut self, content: VaultContent) {
        self.repo = EntryRepository::from_content(content);
    }

    fn header(&self) -> VaultHeader {
        VaultHeader {
            magic: VAULT_MAGIC.to_string(),
            format_version: VAULT_FORMAT_VERSION,
            schema_version: self.repo.content().schema_version,
            crypto_profile: self.crypto_profile.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{Entry, EntryId, EntryType};

    /// 仅供测试加速；**远低于生产 / OWASP 档，切勿用于生产**。
    fn fast_params() -> KdfParams {
        KdfParams {
            m_kib: 4096,
            t: 1,
            p: 1,
            salt: vec![0x11; 16],
        }
    }

    fn new_vault_with_entry() -> Vault {
        let mut v = Vault::create_with_params(b"correct horse battery", fast_params()).unwrap();
        v.entries_mut().upsert(Entry::new(
            EntryId::new("e1"),
            "GitHub",
            EntryType::Login,
            1_000,
        ));
        v
    }

    #[test]
    fn create_save_open_roundtrips() {
        let bytes = new_vault_with_entry().save().unwrap();
        let opened = Vault::open(&bytes, b"correct horse battery").unwrap();
        assert_eq!(opened.entries().list().len(), 1);
        assert_eq!(
            opened.entries().get(&EntryId::new("e1")).unwrap().title,
            "GitHub"
        );
    }

    #[test]
    fn open_with_wrong_password_is_indistinguishable() {
        let bytes = new_vault_with_entry().save().unwrap();
        assert_eq!(
            Vault::open(&bytes, b"WRONG").unwrap_err(),
            VaultError::WrongPasswordOrTampered
        );
    }

    #[test]
    fn tampering_payload_fails_safely() {
        let mut bytes = new_vault_with_entry().save().unwrap();
        let last = bytes.len() - 1; // 落在 payload 字节串末尾（AEAD tag）
        bytes[last] ^= 0x01;
        assert_eq!(
            Vault::open(&bytes, b"correct horse battery").unwrap_err(),
            VaultError::WrongPasswordOrTampered
        );
    }

    #[test]
    fn truncated_or_garbage_is_corrupt() {
        let bytes = new_vault_with_entry().save().unwrap();
        assert_eq!(
            Vault::open(&bytes[..bytes.len() / 2], b"correct horse battery").unwrap_err(),
            VaultError::Corrupt
        );
        assert_eq!(
            Vault::open(&[0xff, 0x00, 0x13], b"x").unwrap_err(),
            VaultError::Corrupt
        );
    }

    #[test]
    fn wrong_magic_is_corrupt() {
        let bytes = new_vault_with_entry().save().unwrap();
        let mut file: VaultFile = codec::from_cbor(&bytes).unwrap();
        file.header.magic = "not-a-vault".into();
        let mutated = codec::to_cbor(&file).unwrap();
        assert_eq!(
            Vault::open(&mutated, b"correct horse battery").unwrap_err(),
            VaultError::Corrupt
        );
    }

    #[test]
    fn future_format_or_schema_version_is_incompatible() {
        let bytes = new_vault_with_entry().save().unwrap();

        let mut f1: VaultFile = codec::from_cbor(&bytes).unwrap();
        f1.header.format_version = VAULT_FORMAT_VERSION + 1;
        assert_eq!(
            Vault::open(&codec::to_cbor(&f1).unwrap(), b"correct horse battery").unwrap_err(),
            VaultError::IncompatibleVersion
        );

        let mut f2: VaultFile = codec::from_cbor(&bytes).unwrap();
        f2.header.schema_version = CURRENT_SCHEMA_VERSION + 1;
        assert_eq!(
            Vault::open(&codec::to_cbor(&f2).unwrap(), b"correct horse battery").unwrap_err(),
            VaultError::IncompatibleVersion
        );
    }

    #[test]
    fn unknown_crypto_profile_is_incompatible() {
        let bytes = new_vault_with_entry().save().unwrap();
        let mut file: VaultFile = codec::from_cbor(&bytes).unwrap();
        file.header.crypto_profile.kdf.id = "scrypt".into();
        assert_eq!(
            Vault::open(&codec::to_cbor(&file).unwrap(), b"correct horse battery").unwrap_err(),
            VaultError::IncompatibleVersion
        );
    }

    #[test]
    fn change_password_lets_new_open_and_old_fail() {
        let mut v = new_vault_with_entry();
        let bytes = v
            .change_password(b"correct horse battery", b"a new strong phrase")
            .unwrap();
        let opened = Vault::open(&bytes, b"a new strong phrase").unwrap();
        assert_eq!(opened.entries().list().len(), 1);
        assert_eq!(
            Vault::open(&bytes, b"correct horse battery").unwrap_err(),
            VaultError::WrongPasswordOrTampered
        );
    }

    #[test]
    fn change_password_with_wrong_old_fails() {
        let mut v = new_vault_with_entry();
        assert_eq!(
            v.change_password(b"WRONG old", b"new").unwrap_err(),
            VaultError::WrongPasswordOrTampered
        );
    }

    #[test]
    fn header_aad_is_deterministic() {
        let h = new_vault_with_entry().header();
        assert_eq!(header_aad(&h).unwrap(), header_aad(&h).unwrap());
    }

    #[test]
    fn save_uses_fresh_nonce_each_time() {
        let v = new_vault_with_entry();
        // 同内容两次保存因随机 nonce 不同而字节不同。
        assert_ne!(v.save().unwrap(), v.save().unwrap());
    }
}
