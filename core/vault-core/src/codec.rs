//! CBOR 序列化（L2-03）。
//!
//! 核心数据的二进制序列化统一用 `ciborium`（CBOR，RFC 8949；D-005）。选 CBOR 而非
//! protobuf 的关键原因：CBOR 自描述，配合 serde `#[serde(flatten)]` 能把本版本未识别的
//! 字段收进一张 map，**在跨版本读写时保留未知字段**——这是安全实现设计 §4 与数据模型 §8
//! 的硬需求：旧版本读到新版本写入的字段时不得丢弃，否则跨版本迁移会丢数据。`prost`
//! (protobuf) 解析会丢弃未知字段，故不选（D-005）。
//!
//! 本模块只做编解码薄封装与未知字段载体，**不定义任何领域模型**（领域模型见 L3-ENTRY /
//! L3-VAULT）。未知字段用 [`BTreeMap`] 承载，键有序、输出确定，便于测试与（后续）头部认证。

use std::collections::BTreeMap;

/// CBOR 动态值（`ciborium` 的 `Value`）。用作未知字段载体，按 CBOR 原样保留，不做解释。
pub use ciborium::value::Value as CborValue;

/// 未知字段载体：领域结构体以 `#[serde(flatten)] unknown: UnknownFields` 内嵌，
/// 解码时收集本版本未声明的字段，编码时原样写回，从而实现跨版本前向兼容（保未知字段）。
pub type UnknownFields = BTreeMap<String, CborValue>;

/// 序列化层错误。FFI 映射在 L2-04 统一处理，这里只区分编码 / 解码两类。
#[derive(Debug, PartialEq, Eq)]
pub enum CodecError {
    /// 编码失败（序列化到内存缓冲；对 `Vec<u8>` 写入而言通常不可达）。
    Encode,
    /// 解码失败：字节非合法 CBOR，或其结构与目标类型不兼容。
    Decode,
}

/// 把可序列化值编码为 CBOR 字节。
pub fn to_cbor<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, CodecError> {
    let mut buf = Vec::new();
    ciborium::into_writer(value, &mut buf).map_err(|_| CodecError::Encode)?;
    Ok(buf)
}

/// 从 CBOR 字节解码为目标类型。截断或类型不兼容均返回 [`CodecError::Decode`]。
pub fn from_cbor<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, CodecError> {
    ciborium::from_reader(bytes).map_err(|_| CodecError::Decode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Simple {
        id: u32,
        name: String,
        tags: Vec<String>,
    }

    #[test]
    fn roundtrips_simple_struct() {
        let v = Simple {
            id: 7,
            name: "vault".into(),
            tags: vec!["a".into(), "b".into()],
        };
        let bytes = to_cbor(&v).unwrap();
        let back: Simple = from_cbor(&bytes).unwrap();
        assert_eq!(v, back);
    }

    // 旧版本 schema：只认识字段 `a`，其余进未知字段载体。
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct EntryV1 {
        a: u32,
        #[serde(flatten)]
        unknown: UnknownFields,
    }

    // 新版本 schema：认识 `a` 与 `b`。
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct EntryV2 {
        a: u32,
        b: String,
        #[serde(flatten)]
        unknown: UnknownFields,
    }

    #[test]
    fn preserves_unknown_fields_across_versions() {
        // 新版本写出含 `b` 的记录。
        let v2 = EntryV2 {
            a: 1,
            b: "new-field".into(),
            unknown: UnknownFields::new(),
        };
        let wire = to_cbor(&v2).unwrap();

        // 旧版本读取：不认识 `b`，应收进 unknown 而非丢弃。
        let v1: EntryV1 = from_cbor(&wire).unwrap();
        assert_eq!(v1.a, 1);
        assert_eq!(
            v1.unknown.get("b"),
            Some(&CborValue::Text("new-field".into()))
        );

        // 旧版本原样写回。
        let rewire = to_cbor(&v1).unwrap();

        // 新版本再次读取：`b` 经旧版本往返后仍在（前向兼容核心保证）。
        let back: EntryV2 = from_cbor(&rewire).unwrap();
        assert_eq!(back, v2);
    }

    #[test]
    fn no_unknown_fields_when_schema_matches() {
        let v2 = EntryV2 {
            a: 3,
            b: "x".into(),
            unknown: UnknownFields::new(),
        };
        let wire = to_cbor(&v2).unwrap();
        let back: EntryV2 = from_cbor(&wire).unwrap();
        assert!(back.unknown.is_empty());
    }

    #[test]
    fn unknown_fields_are_deterministically_ordered() {
        // BTreeMap 键有序 → 同样内容的编码字节稳定（便于测试与后续头部认证）。
        let mut a = EntryV1 {
            a: 0,
            unknown: UnknownFields::new(),
        };
        a.unknown.insert("z".into(), CborValue::Integer(2.into()));
        a.unknown.insert("y".into(), CborValue::Integer(1.into()));

        let mut b = EntryV1 {
            a: 0,
            unknown: UnknownFields::new(),
        };
        b.unknown.insert("y".into(), CborValue::Integer(1.into()));
        b.unknown.insert("z".into(), CborValue::Integer(2.into()));

        assert_eq!(to_cbor(&a).unwrap(), to_cbor(&b).unwrap());
    }

    #[test]
    fn rejects_incompatible_or_truncated_cbor() {
        // 把一个整数当结构体解析：类型不匹配。
        let int_bytes = to_cbor(&42u32).unwrap();
        assert_eq!(
            from_cbor::<Simple>(&int_bytes).unwrap_err(),
            CodecError::Decode
        );

        // 截断合法字节：解析应失败而非返回部分结果。
        let mut good = to_cbor(&Simple {
            id: 1,
            name: "x".into(),
            tags: vec![],
        })
        .unwrap();
        good.truncate(good.len() - 1);
        assert_eq!(from_cbor::<Simple>(&good).unwrap_err(), CodecError::Decode);
    }
}
