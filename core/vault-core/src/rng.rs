//! CSPRNG 与无偏置采样（L2-05）。
//!
//! 提供加密安全随机源上的**均匀整数采样**：用拒绝采样丢弃落在截断高区间的随机值，避免取模
//! 偏置（安全实现设计 §2.3；RFC 4226 对截断取模偏置的说明）。供密码生成器（L3-GEN）等使用。
//! 密钥/nonce/salt 的随机仍在 [`crate::crypto`] 内就地取用（各自的错误域）。

use crate::error::{VaultError, VaultResult};

/// 取一个加密安全随机 `u32`（CSPRNG）。
fn random_u32() -> VaultResult<u32> {
    let mut b = [0u8; 4];
    getrandom::getrandom(&mut b).map_err(|_| VaultError::Internal)?;
    Ok(u32::from_le_bytes(b))
}

/// 返回 `[0, bound)` 上的均匀随机整数；用拒绝采样避免取模偏置。`bound` 为 0 返回
/// [`VaultError::InvalidInput`]。
///
/// 原理：可接受区间取小于 `2^32` 的最大 `bound` 整数倍 `[0, usable]`，落入被截断高区间的随机值
/// 丢弃后重取，使 `% bound` 映射严格均匀。期望迭代次数 < 2。
pub fn uniform_index(bound: u32) -> VaultResult<u32> {
    if bound == 0 {
        return Err(VaultError::InvalidInput);
    }
    // 2^32 mod bound，无溢出地计算（2^32 = u32::MAX + 1）。
    let rem = (u32::MAX % bound + 1) % bound;
    // 最大可接受值（含）：[0, usable] 共 2^32 - rem 个，恰为 bound 的整数倍。
    let usable = u32::MAX - rem;
    loop {
        let r = random_u32()?;
        if r <= usable {
            return Ok(r % bound);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bound_zero_is_invalid() {
        assert_eq!(uniform_index(0).unwrap_err(), VaultError::InvalidInput);
    }

    #[test]
    fn bound_one_always_zero() {
        for _ in 0..100 {
            assert_eq!(uniform_index(1).unwrap(), 0);
        }
    }

    #[test]
    fn samples_stay_in_range() {
        for _ in 0..10_000 {
            assert!(uniform_index(7).unwrap() < 7);
        }
    }

    #[test]
    fn distribution_is_approximately_uniform() {
        // 非 2 的幂的 bound（=6）最易暴露取模偏置；大样本下各桶应接近均匀。
        let bound = 6u32;
        let samples = 60_000u32;
        let mut counts = [0u32; 6];
        for _ in 0..samples {
            counts[uniform_index(bound).unwrap() as usize] += 1;
        }
        let expected = samples / bound; // 10_000
                                        // 容差 ±10%（约 11σ，统计上几乎不会误报）。
        for c in counts {
            assert!(
                c > expected * 9 / 10 && c < expected * 11 / 10,
                "bucket {c} outside ±10% of {expected}"
            );
        }
    }
}
