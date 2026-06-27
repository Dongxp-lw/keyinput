//! 加密原语封装（L2-02）。
//!
//! 所有密码学原语来自审计过的 RustCrypto crate（D-004 / D-008）：`argon2`（Argon2id）、
//! `chacha20poly1305`（XChaCha20-Poly1305）、`hkdf` + `sha2`（HKDF-SHA256）。本模块只做
//! **参数固定、信封组装与密钥卫生**，不实现任何密码学原语本身。方案见
//! docs/technical/security-implementation-design.md 第 2-4 节。

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    Key, XChaCha20Poly1305, XNonce,
};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroize;

use crate::secret::SecretKey;

/// 密钥长度：256-bit。
const KEY_LEN: usize = 32;
/// XChaCha20 nonce：192-bit（24 字节），允许安全使用随机 nonce。
const NONCE_LEN: usize = 24;
/// Poly1305 认证标签长度。
const TAG_LEN: usize = 16;

/// 域分离上下文标签（HKDF `info`），确保子密钥用途隔离（安全实现设计 §3）。
pub const SUBKEY_VAULT_CONTENT: &[u8] = b"private-input-vault:subkey:vault-content:v1";
pub const SUBKEY_MIGRATION: &[u8] = b"private-input-vault:subkey:migration:v1";
pub const SUBKEY_SYNC: &[u8] = b"private-input-vault:subkey:sync:v1";

/// 信封包装 DEK 时绑定的 AAD 域标签（把 wrappedDEK 绑定到其用途）。
const WRAP_DEK_AAD: &[u8] = b"private-input-vault:wrap-dek:v1";

/// 加密层错误。**口令错误与篡改都表现为 [`CryptoError::Aead`]**（不可区分，避免信息泄露）。
#[derive(Debug, PartialEq, Eq)]
pub enum CryptoError {
    /// CSPRNG 取随机失败。
    Rng,
    /// Argon2id 参数非法（如低于库约束）。
    KdfParams,
    /// 密钥派生失败。
    Kdf,
    /// AEAD 失败：验签不过（篡改 / 错误密钥 / 错误 AAD）或密文格式错误。
    Aead,
}

/// KDF 参数。写入保险库头部以便升级与重哈希判断（安全实现设计 §2.1、§4）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KdfParams {
    /// 内存代价（KiB）。
    pub m_kib: u32,
    /// 迭代次数。
    pub t: u32,
    /// 并行度。
    pub p: u32,
    /// 每库独立随机 salt（≥16 字节）。
    pub salt: Vec<u8>,
}

impl KdfParams {
    /// OWASP 下限（低内存设备降级档）：m=19 MiB、t=2、p=1。
    pub const OWASP_MIN_M_KIB: u32 = 19 * 1024;
    /// OWASP 下限迭代次数。
    pub const OWASP_MIN_T: u32 = 2;

    /// 生产默认：m=64 MiB、t=3、p=1，随机 16 字节 salt（安全实现设计 §2.1）。
    pub fn production_default() -> Result<Self, CryptoError> {
        Ok(Self {
            m_kib: 64 * 1024,
            t: 3,
            p: 1,
            salt: random_bytes(16)?,
        })
    }

    fn to_argon2(&self) -> Result<Params, CryptoError> {
        Params::new(self.m_kib, self.t, self.p, Some(KEY_LEN)).map_err(|_| CryptoError::KdfParams)
    }
}

/// 取 `n` 字节加密安全随机（CSPRNG）。
fn random_bytes(n: usize) -> Result<Vec<u8>, CryptoError> {
    let mut buf = vec![0u8; n];
    getrandom::getrandom(&mut buf).map_err(|_| CryptoError::Rng)?;
    Ok(buf)
}

/// 从主密码与参数用 Argon2id 派生 32 字节 KEK（安全实现设计 §2.1）。
pub fn derive_kek(password: &[u8], params: &KdfParams) -> Result<SecretKey, CryptoError> {
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params.to_argon2()?);
    let mut out = [0u8; KEY_LEN];
    argon
        .hash_password_into(password, &params.salt, &mut out)
        .map_err(|_| CryptoError::Kdf)?;
    let key = SecretKey::new(out);
    out.zeroize();
    Ok(key)
}

/// 生成 256-bit 随机 DEK（安全实现设计 §3）。
pub fn random_dek() -> Result<SecretKey, CryptoError> {
    let mut dek = [0u8; KEY_LEN];
    getrandom::getrandom(&mut dek).map_err(|_| CryptoError::Rng)?;
    let key = SecretKey::new(dek);
    dek.zeroize();
    Ok(key)
}

/// 用 `key` 与随机 24 字节 nonce 做 XChaCha20-Poly1305 认证加密；`aad` 一并认证。
/// 返回 `nonce(24) || ciphertext || tag(16)`。
pub fn seal(key: &SecretKey, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.expose()));
    let nonce_bytes = random_bytes(NONCE_LEN)?;
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, Payload { msg: plaintext, aad })
        .map_err(|_| CryptoError::Aead)?;
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// [`seal`] 的逆操作。验签失败（篡改 / 错误密钥 / 错误 AAD）返回 [`CryptoError::Aead`]，
/// 不输出任何部分明文。
pub fn open(key: &SecretKey, aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if sealed.len() < NONCE_LEN + TAG_LEN {
        return Err(CryptoError::Aead);
    }
    let (nonce_bytes, ct) = sealed.split_at(NONCE_LEN);
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.expose()));
    let nonce = XNonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, Payload { msg: ct, aad })
        .map_err(|_| CryptoError::Aead)
}

/// 用 KEK 以 AEAD 包装 DEK（信封加密，安全实现设计 §3）。
pub fn wrap_dek(dek: &SecretKey, kek: &SecretKey) -> Result<Vec<u8>, CryptoError> {
    seal(kek, WRAP_DEK_AAD, dek.expose())
}

/// 用 KEK 解包 DEK。失败即口令错误（与篡改不可区分）。
pub fn unwrap_dek(wrapped: &[u8], kek: &SecretKey) -> Result<SecretKey, CryptoError> {
    let mut pt = open(kek, WRAP_DEK_AAD, wrapped)?;
    if pt.len() != KEY_LEN {
        pt.zeroize();
        return Err(CryptoError::Aead);
    }
    let mut arr = [0u8; KEY_LEN];
    arr.copy_from_slice(&pt);
    pt.zeroize();
    let key = SecretKey::new(arr);
    arr.zeroize();
    Ok(key)
}

/// 由 DEK 经 HKDF-SHA256 域分离派生子密钥；`context` 为用途标签（安全实现设计 §3）。
pub fn derive_subkey(dek: &SecretKey, context: &[u8]) -> Result<SecretKey, CryptoError> {
    let hk = Hkdf::<Sha256>::new(None, dek.expose());
    let mut out = [0u8; KEY_LEN];
    hk.expand(context, &mut out).map_err(|_| CryptoError::Kdf)?;
    let key = SecretKey::new(out);
    out.zeroize();
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 仅供单元测试加速；**远低于生产 / OWASP 档，切勿用于生产**。
    fn fast_params() -> KdfParams {
        KdfParams {
            m_kib: 4096,
            t: 1,
            p: 1,
            salt: vec![0x42; 16],
        }
    }

    #[test]
    fn kek_derivation_is_deterministic() {
        let p = fast_params();
        let a = derive_kek(b"correct horse battery staple", &p).unwrap();
        let b = derive_kek(b"correct horse battery staple", &p).unwrap();
        assert!(a.ct_eq(&b));
    }

    #[test]
    fn kek_changes_with_password_and_salt() {
        let p = fast_params();
        let base = derive_kek(b"pw-1", &p).unwrap();
        assert!(!base.ct_eq(&derive_kek(b"pw-2", &p).unwrap()));
        let mut p2 = p.clone();
        p2.salt = vec![0x99; 16];
        assert!(!base.ct_eq(&derive_kek(b"pw-1", &p2).unwrap()));
    }

    #[test]
    fn seal_open_roundtrips_with_aad() {
        let key = random_dek().unwrap();
        let sealed = seal(&key, b"header-aad", b"top secret payload").unwrap();
        assert_eq!(open(&key, b"header-aad", &sealed).unwrap(), b"top secret payload");
    }

    #[test]
    fn open_rejects_tampered_ciphertext() {
        let key = random_dek().unwrap();
        let mut sealed = seal(&key, b"aad", b"data").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0x01;
        assert_eq!(open(&key, b"aad", &sealed), Err(CryptoError::Aead));
    }

    #[test]
    fn open_rejects_wrong_aad_and_wrong_key() {
        let key = random_dek().unwrap();
        let sealed = seal(&key, b"aad-1", b"data").unwrap();
        assert_eq!(open(&key, b"aad-2", &sealed), Err(CryptoError::Aead));
        assert_eq!(open(&random_dek().unwrap(), b"aad-1", &sealed), Err(CryptoError::Aead));
    }

    #[test]
    fn open_rejects_truncated_input() {
        assert_eq!(open(&random_dek().unwrap(), b"", b"short"), Err(CryptoError::Aead));
    }

    #[test]
    fn wrap_unwrap_roundtrips() {
        let kek = random_dek().unwrap();
        let dek = random_dek().unwrap();
        let wrapped = wrap_dek(&dek, &kek).unwrap();
        assert!(dek.ct_eq(&unwrap_dek(&wrapped, &kek).unwrap()));
    }

    #[test]
    fn unwrap_fails_with_wrong_kek() {
        let dek = random_dek().unwrap();
        let wrapped = wrap_dek(&dek, &random_dek().unwrap()).unwrap();
        assert!(matches!(
            unwrap_dek(&wrapped, &random_dek().unwrap()),
            Err(CryptoError::Aead)
        ));
    }

    #[test]
    fn subkeys_are_domain_separated_and_deterministic() {
        let dek = random_dek().unwrap();
        let content = derive_subkey(&dek, SUBKEY_VAULT_CONTENT).unwrap();
        let migration = derive_subkey(&dek, SUBKEY_MIGRATION).unwrap();
        let sync = derive_subkey(&dek, SUBKEY_SYNC).unwrap();
        assert!(!content.ct_eq(&migration));
        assert!(!content.ct_eq(&sync));
        assert!(!migration.ct_eq(&sync));
        assert!(content.ct_eq(&derive_subkey(&dek, SUBKEY_VAULT_CONTENT).unwrap()));
    }
}
