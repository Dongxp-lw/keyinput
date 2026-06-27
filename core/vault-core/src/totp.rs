//! 基础 TOTP 生成（L3-TOTP，实现文档 `docs/implementation/totp-generation.md` TOTP-01..05；
//! RFC 6238 / RFC 4226）。
//!
//! 由 TOTP 种子与参数、按**注入的设备时间**生成临时验证码。**只生成、不存储/不加密**（种子的
//! 加密存储见 ENTRY/VAULT）；种子为高敏感，中间缓冲用后清零、绝不记录种子或验证码（§7）。
//! 时间由调用方注入（便于已知答案测试，且核心不读系统时钟）。算法复用 [`TotpAlgorithm`]。

use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use sha2::{Sha256, Sha512};
use zeroize::Zeroize;

use crate::entry::TotpAlgorithm;
use crate::error::{VaultError, VaultResult};

/// 最短种子：128-bit（§6；RFC 4226 推荐 160-bit）。
const MIN_SECRET_LEN: usize = 16;

/// TOTP 生成参数（实现文档 §4）。`secret` 为解码后的原始密钥字节（高敏感，由调用方管理生命周期）。
#[derive(Clone)]
pub struct TotpParameters {
    pub secret: Vec<u8>,
    pub algorithm: TotpAlgorithm,
    /// 验证码位数：仅接受 6 或 8。
    pub digits: u32,
    /// 时间步 X（秒），默认 30；必须 > 0。
    pub period_seconds: u32,
    /// T0（秒），默认 0（Unix 纪元）。
    pub t0_seconds: i64,
}

/// 生成的验证码与步内刷新信息（实现文档 §4）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TotpCode {
    /// 左侧补零到 `digits` 位的验证码。
    pub code: String,
    pub valid_until_epoch_seconds: i64,
    /// 当前时间步剩余秒数，供界面在步边界刷新。
    pub seconds_remaining: u32,
}

/// HOTP(K, C) = Truncate(HMAC(K, C))；`counter` 编码为 8 字节大端（RFC 4226 §5.3）。
/// `digits` 必须已被校验为 6 或 8。
fn hotp(key: &[u8], counter: u64, algorithm: TotpAlgorithm, digits: u32) -> VaultResult<String> {
    let msg = counter.to_be_bytes();
    let mut hash = match algorithm {
        TotpAlgorithm::Sha1 => {
            let mut mac = Hmac::<Sha1>::new_from_slice(key).map_err(|_| VaultError::Internal)?;
            mac.update(&msg);
            mac.finalize().into_bytes().to_vec()
        }
        TotpAlgorithm::Sha256 => {
            let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| VaultError::Internal)?;
            mac.update(&msg);
            mac.finalize().into_bytes().to_vec()
        }
        TotpAlgorithm::Sha512 => {
            let mut mac = Hmac::<Sha512>::new_from_slice(key).map_err(|_| VaultError::Internal)?;
            mac.update(&msg);
            mac.finalize().into_bytes().to_vec()
        }
    };

    // 动态截断（RFC 4226 §5.3）：offset 取末字节低 4 位；取 4 字节并清最高位得 31 位整数。
    let offset = (hash[hash.len() - 1] & 0x0f) as usize;
    let binary = ((u32::from(hash[offset]) & 0x7f) << 24)
        | (u32::from(hash[offset + 1]) << 16)
        | (u32::from(hash[offset + 2]) << 8)
        | u32::from(hash[offset + 3]);
    hash.zeroize();

    let otp = binary % 10u32.pow(digits);
    Ok(format!("{otp:0width$}", width = digits as usize))
}

/// 按参数与注入时间生成 TOTP 验证码（RFC 6238 §4）。
///
/// 校验失败返回 [`VaultError::InvalidInput`]：`digits` 非 6/8、`period_seconds == 0`、或种子短于
/// 128-bit（§6，不静默回退以免改变语义）。
pub fn generate(params: &TotpParameters, now_epoch_seconds: i64) -> VaultResult<TotpCode> {
    if params.digits != 6 && params.digits != 8 {
        return Err(VaultError::InvalidInput);
    }
    if params.period_seconds == 0 {
        return Err(VaultError::InvalidInput);
    }
    if params.secret.len() < MIN_SECRET_LEN {
        return Err(VaultError::InvalidInput);
    }

    let period = i64::from(params.period_seconds);
    // T = floorDiv(now - T0, X)；div_euclid 在除数为正时等于向下取整，2038 年后仍用 64 位。
    let time_step = (now_epoch_seconds - params.t0_seconds).div_euclid(period);
    let code = hotp(&params.secret, time_step as u64, params.algorithm, params.digits)?;

    let valid_until = (time_step + 1) * period + params.t0_seconds;
    let seconds_remaining = (valid_until - now_epoch_seconds).max(0) as u32;
    Ok(TotpCode {
        code,
        valid_until_epoch_seconds: valid_until,
        seconds_remaining,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238 附录 B 种子：ASCII "1234567890" 循环填充到指定长度（SHA1=20、SHA256=32、SHA512=64）。
    fn seed(len: usize) -> Vec<u8> {
        b"1234567890".iter().cycle().take(len).copied().collect()
    }

    fn params(algorithm: TotpAlgorithm, secret: Vec<u8>) -> TotpParameters {
        TotpParameters {
            secret,
            algorithm,
            digits: 8,
            period_seconds: 30,
            t0_seconds: 0,
        }
    }

    #[test]
    fn rfc6238_appendix_b_known_answers() {
        // (秒, SHA1, SHA256, SHA512)；含 20000000000（公元 2603，验证 64 位时间步）。
        let cases: &[(i64, &str, &str, &str)] = &[
            (59, "94287082", "46119246", "90693936"),
            (1111111109, "07081804", "68084774", "25091201"),
            (2000000000, "69279037", "90698825", "38618901"),
            (20000000000, "65353130", "77737706", "47863826"),
        ];
        for &(t, c_sha1, c_sha256, c_sha512) in cases {
            assert_eq!(
                generate(&params(TotpAlgorithm::Sha1, seed(20)), t).unwrap().code,
                c_sha1,
                "SHA1 @ {t}"
            );
            assert_eq!(
                generate(&params(TotpAlgorithm::Sha256, seed(32)), t).unwrap().code,
                c_sha256,
                "SHA256 @ {t}"
            );
            assert_eq!(
                generate(&params(TotpAlgorithm::Sha512, seed(64)), t).unwrap().code,
                c_sha512,
                "SHA512 @ {t}"
            );
        }
    }

    #[test]
    fn six_digit_codes_are_six_chars() {
        let mut p = params(TotpAlgorithm::Sha1, seed(20));
        p.digits = 6;
        assert_eq!(generate(&p, 59).unwrap().code.len(), 6);
    }

    #[test]
    fn seconds_remaining_and_valid_until_track_step_boundary() {
        // t=59、X=30、T0=0 → T=1，validUntil=60，剩余 1 秒。
        let code = generate(&params(TotpAlgorithm::Sha1, seed(20)), 59).unwrap();
        assert_eq!(code.valid_until_epoch_seconds, 60);
        assert_eq!(code.seconds_remaining, 1);
        // 步起点 t=30 → validUntil=60，剩余 30 秒。
        let code = generate(&params(TotpAlgorithm::Sha1, seed(20)), 30).unwrap();
        assert_eq!(code.valid_until_epoch_seconds, 60);
        assert_eq!(code.seconds_remaining, 30);
    }

    #[test]
    fn rejects_invalid_digits_period_and_short_secret() {
        let mut bad_digits = params(TotpAlgorithm::Sha1, seed(20));
        bad_digits.digits = 7;
        assert_eq!(
            generate(&bad_digits, 0).unwrap_err(),
            VaultError::InvalidInput
        );

        let mut zero_period = params(TotpAlgorithm::Sha1, seed(20));
        zero_period.period_seconds = 0;
        assert_eq!(
            generate(&zero_period, 0).unwrap_err(),
            VaultError::InvalidInput
        );

        let short = params(TotpAlgorithm::Sha1, seed(15)); // < 128-bit
        assert_eq!(generate(&short, 0).unwrap_err(), VaultError::InvalidInput);
    }
}
