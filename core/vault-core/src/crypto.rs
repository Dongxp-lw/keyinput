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

    // ===================================================================
    // L6-01 标准已知答案测试（KAT）。向量取自权威来源（RFC / IETF draft），
    // 非自洽生成；逐字节核对官方文本。证明本库密码学原语与标准互通。
    // ===================================================================

    /// Argon2id 原语对 **RFC 9106 §5.3** 标准向量（含 secret K 与关联数据 X）。
    /// 与 `derive_kek` 同算法（Argon2id）/版本（0x13），证明依赖实现符合 RFC。
    #[test]
    fn argon2id_matches_rfc9106_vector() {
        use argon2::{AssociatedData, ParamsBuilder};
        let params = ParamsBuilder::new()
            .m_cost(32)
            .t_cost(3)
            .p_cost(4)
            .data(AssociatedData::new(&[0x04u8; 12]).unwrap())
            .build()
            .unwrap();
        let ctx =
            Argon2::new_with_secret(&[0x03u8; 8], Algorithm::Argon2id, Version::V0x13, params)
                .unwrap();
        let mut out = [0u8; KEY_LEN];
        ctx.hash_password_into(&[0x01u8; 32], &[0x02u8; 16], &mut out)
            .unwrap();
        // RFC 9106 §5.3 Tag[32]
        let expected: [u8; 32] = [
            0x0d, 0x64, 0x0d, 0xf5, 0x8d, 0x78, 0x76, 0x6c, 0x08, 0xc0, 0x37, 0xa3, 0x4a, 0x8b,
            0x53, 0xc9, 0xd0, 0x1e, 0xf0, 0x45, 0x2d, 0x75, 0xb6, 0x5e, 0xb5, 0x25, 0x20, 0xe9,
            0x6b, 0x01, 0xe6, 0x59,
        ];
        assert_eq!(out, expected);
    }

    /// `derive_kek` 接线检验：与同参数（无 secret/AD）的 Argon2id(0x13) 直算一致，
    /// 证明 `derive_kek` 正确传入 m/t/p/taglen 与算法/版本。结合上一个 RFC KAT 即证其符合 RFC。
    #[test]
    fn derive_kek_wires_argon2id_v0x13() {
        let kdf = KdfParams { m_kib: 64, t: 2, p: 1, salt: vec![0x10; 16] };
        let pw = b"kat-wiring-password";
        let got = derive_kek(pw, &kdf).unwrap();
        let params = Params::new(64, 2, 1, Some(KEY_LEN)).unwrap();
        let ctx = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut expected = [0u8; KEY_LEN];
        ctx.hash_password_into(pw, &kdf.salt, &mut expected).unwrap();
        assert!(got.ct_eq(&SecretKey::new(expected)));
    }

    /// 我们的 `open` 对 **XChaCha20-Poly1305** 标准向量
    /// （draft-arciszewski-xchacha-03 附录 A.1）。sealed = nonce(24)||ciphertext||tag(16)。
    #[test]
    fn open_decrypts_xchacha20poly1305_draft_vector() {
        let key = SecretKey::new([
            0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x8b, 0x8c, 0x8d,
            0x8e, 0x8f, 0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0x9b,
            0x9c, 0x9d, 0x9e, 0x9f,
        ]);
        let nonce: [u8; NONCE_LEN] = [
            0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d,
            0x4e, 0x4f, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57,
        ];
        let aad: [u8; 12] = [
            0x50, 0x51, 0x52, 0x53, 0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7,
        ];
        let ciphertext: &[u8] = &[
            0xbd, 0x6d, 0x17, 0x9d, 0x3e, 0x83, 0xd4, 0x3b, 0x95, 0x76, 0x57, 0x94, 0x93, 0xc0,
            0xe9, 0x39, 0x57, 0x2a, 0x17, 0x00, 0x25, 0x2b, 0xfa, 0xcc, 0xbe, 0xd2, 0x90, 0x2c,
            0x21, 0x39, 0x6c, 0xbb, 0x73, 0x1c, 0x7f, 0x1b, 0x0b, 0x4a, 0xa6, 0x44, 0x0b, 0xf3,
            0xa8, 0x2f, 0x4e, 0xda, 0x7e, 0x39, 0xae, 0x64, 0xc6, 0x70, 0x8c, 0x54, 0xc2, 0x16,
            0xcb, 0x96, 0xb7, 0x2e, 0x12, 0x13, 0xb4, 0x52, 0x2f, 0x8c, 0x9b, 0xa4, 0x0d, 0xb5,
            0xd9, 0x45, 0xb1, 0x1b, 0x69, 0xb9, 0x82, 0xc1, 0xbb, 0x9e, 0x3f, 0x3f, 0xac, 0x2b,
            0xc3, 0x69, 0x48, 0x8f, 0x76, 0xb2, 0x38, 0x35, 0x65, 0xd3, 0xff, 0xf9, 0x21, 0xf9,
            0x66, 0x4c, 0x97, 0x63, 0x7d, 0xa9, 0x76, 0x88, 0x12, 0xf6, 0x15, 0xc6, 0x8b, 0x13,
            0xb5, 0x2e,
        ];
        let tag: [u8; TAG_LEN] = [
            0xc0, 0x87, 0x59, 0x24, 0xc1, 0xc7, 0x98, 0x79, 0x47, 0xde, 0xaf, 0xd8, 0x78, 0x0a,
            0xcf, 0x49,
        ];
        let mut sealed = Vec::new();
        sealed.extend_from_slice(&nonce);
        sealed.extend_from_slice(ciphertext);
        sealed.extend_from_slice(&tag);
        let plaintext = open(&key, &aad, &sealed).unwrap();
        assert_eq!(
            plaintext,
            b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it."
                .to_vec()
        );
    }

    /// HKDF-SHA256 原语对 **RFC 5869 附录 A** 标准向量（Test Case 1 与 Test Case 3）。
    /// TC3 的 salt 为空，与 `derive_subkey` 使用的 `salt=None`（HMAC 零填充）等价。
    #[test]
    fn hkdf_sha256_matches_rfc5869_vectors() {
        // Test Case 1：13 字节 salt + 10 字节 info。
        let ikm = [0x0bu8; 22];
        let salt1: [u8; 13] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c,
        ];
        let info1: [u8; 10] = [0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9];
        let mut okm1 = [0u8; 42];
        Hkdf::<Sha256>::new(Some(&salt1), &ikm)
            .expand(&info1, &mut okm1)
            .unwrap();
        let expected1: [u8; 42] = [
            0x3c, 0xb2, 0x5f, 0x25, 0xfa, 0xac, 0xd5, 0x7a, 0x90, 0x43, 0x4f, 0x64, 0xd0, 0x36,
            0x2f, 0x2a, 0x2d, 0x2d, 0x0a, 0x90, 0xcf, 0x1a, 0x5a, 0x4c, 0x5d, 0xb0, 0x2d, 0x56,
            0xec, 0xc4, 0xc5, 0xbf, 0x34, 0x00, 0x72, 0x08, 0xd5, 0xb8, 0x87, 0x18, 0x58, 0x65,
        ];
        assert_eq!(okm1, expected1);

        // Test Case 3：salt/info 皆空；`salt=None` 与 derive_subkey 同路径。
        let mut okm3 = [0u8; 42];
        Hkdf::<Sha256>::new(None, &ikm)
            .expand(&[], &mut okm3)
            .unwrap();
        let expected3: [u8; 42] = [
            0x8d, 0xa4, 0xe7, 0x75, 0xa5, 0x63, 0xc1, 0x8f, 0x71, 0x5f, 0x80, 0x2a, 0x06, 0x3c,
            0x5a, 0x31, 0xb8, 0xa1, 0x1f, 0x5c, 0x5e, 0xe1, 0x87, 0x9e, 0xc3, 0x45, 0x4e, 0x5f,
            0x3c, 0x73, 0x8d, 0x2d, 0x9d, 0x20, 0x13, 0x95, 0xfa, 0xa4, 0xb6, 0x1a, 0x96, 0xc8,
        ];
        assert_eq!(okm3, expected3);
    }

    /// `derive_subkey` 接线检验：与 `Hkdf::<Sha256>`（salt=None）直算一致。
    /// 结合上一个 RFC KAT 即证其为 RFC 5869 的 HKDF-SHA256。
    #[test]
    fn derive_subkey_wires_hkdf_sha256() {
        let dek = SecretKey::new([0x5a; KEY_LEN]);
        let context = b"private-input-vault:kat-context";
        let got = derive_subkey(&dek, context).unwrap();
        let mut expected = [0u8; KEY_LEN];
        Hkdf::<Sha256>::new(None, dek.expose())
            .expand(context, &mut expected)
            .unwrap();
        assert!(got.ct_eq(&SecretKey::new(expected)));
    }
}
