# 条目与字段模型 实现文档

本文件描述条目与字段模型（短码 ENTRY）的实现方式，结构遵循 [实现文档层总览](README.md) 第 3 节模板。它落实 v0.2 的 MVP-004，并定义 [保险库加密核心](vault-crypto-core.md) 所加密的载荷内容。

## 1. 范围与目标

实现保险库的领域模型与序列化：条目、字段、类型、敏感级别、默认策略、增删改查与本地搜索，并产出 VAULT 加密的序列化载荷。

- 范围内：领域模型、默认字段策略、CRUD、本地搜索、前向兼容序列化。
- 不在范围内：加密（见 VAULT）；TOTP 验证码生成（见 TOTP，本文档只存储 TOTP 字段）；键盘界面（见 KBD）；导入合并与同步冲突（见 IMEX、MIGR、SYNC）。
- 平台匹配信息只用于提示，不表示目标可信。

## 2. 依赖的设计与技术决策

- 条目、字段、TOTP 字段、子对象与架构迁移规则：[数据模型](../technical/data-model.md) 第 2、3、4、8 节。
- 对象模型与字段层主动选择：[信息架构设计](../product/information-architecture.md) 第 5 节。
- 保存流程与默认字段策略：[核心交互设计](../product/interaction-design.md) 第 5、5.4 节。
- 载荷由谁加密：[保险库加密核心](vault-crypto-core.md)。
- 版本任务：[v0.2 离线 MVP 计划](../product/v0.2-mvp-plan.md)（MVP-004）。

## 3. 平台与技术栈（Android 优先）

- 实现语言：**Rust**（核心层，见 [模块架构](../technical/module-architecture.md)）；下文 Kotlin 接口为逻辑示意。
- 序列化候选：Rust 的 prost（Protocol Buffers）为主、serde_cbor（CBOR）备选，二进制编码、保留未知字段（前向兼容）、以保留字段号管理演进；保留未知字段的具体 crate 与最终选择待评审（见 [技术架构](../technical/architecture.md) §2 提醒）。
- 前向兼容前提：必须用二进制格式并以消息级复制保留未知字段；序列化到 JSON 或逐字段复制会丢失未知字段。
- 注意：Protobuf 序列化不保证规范化字节，这对“序列化后加密、解密后反序列化”的用法可接受，但不应假设重新序列化字节完全一致。
- 本模型是 KBD 的 `EntrySummary`/`FieldSummary` 的来源。

## 4. 接口与数据结构

```kotlin
data class Entry(
    val id: EntryId,
    val title: String,
    val type: EntryType,         // login/secureNote/identity/payment/template/custom
    val fields: List<Field>,
    val tags: List<String>,
    val favorite: Boolean,
    val archived: Boolean,
    val createdAt: Long,
    val updatedAt: Long,
    val deletedAt: Long?         // 软删除
)

data class Field(
    val id: FieldId,
    val label: String,
    val kind: FieldKind,         // username/password/email/phone/totp/text/multiline/url/address/secret/note
    val value: String,           // 明文仅在解锁后的内存模型中存在
    val sensitivity: Sensitivity,// normal/sensitive/high
    val inputBehavior: InputBehavior, // insert/copy/revealOnly
    val requireReauth: Boolean
)

data class TotpField(
    val issuer: String,
    val accountName: String,
    val secret: String,          // 高敏感，加密存储
    val algorithm: TotpAlgorithm,// SHA1/SHA256/SHA512
    val digits: Int,
    val periodSeconds: Int
)

// VAULT 加密的载荷
data class VaultContent(val schemaVersion: Int, val entries: List<Entry>, val settings: Settings)

interface VaultPayloadCodec {
    fun serialize(content: VaultContent): ByteArray   // 二进制，保留未知字段
    fun deserialize(bytes: ByteArray): VaultContent   // 保留未知字段
}

interface EntryRepository {
    fun list(): List<Entry>
    fun get(id: EntryId): Entry?
    fun search(query: String): List<Entry> // 仅本地
    fun upsert(entry: Entry)
    fun delete(id: EntryId)
}
```

## 5. 实现步骤

1. 定义领域模型：Entry、Field、TotpField、Tag、PlatformMatch、UsagePolicy，及类型、敏感级别、输入行为枚举。
2. 定义默认字段策略（见第 6 节策略表）。
3. 定义序列化模式：带 schemaVersion，按保留字段号管理演进，保留未知字段。
4. 实现 `VaultPayloadCodec` 的序列化与反序列化（二进制，保留未知字段）。
5. 实现 `EntryRepository` 在解锁后的内存模型上做 CRUD；持久化时序列化后交 VAULT 保存。
6. 实现本地搜索（标题、字段、标签），不上传搜索词。
7. 创建字段时应用默认策略，用户可覆盖。
8. 支持软删除与归档。

## 6. 默认字段策略

来自核心交互设计第 5.4 节。

| 字段类型 | 默认敏感级别 | 默认行为 |
| --- | --- | --- |
| 用户名 | 普通 | 插入、复制。 |
| 密码 | 高敏感 | 插入、复制，可选重认证。 |
| TOTP | 高敏感 | 生成临时验证码，不保存生成码。 |
| 备注 | 敏感 | 显示或复制，默认不进平台填充。 |
| 地址 | 普通/敏感 | 插入、复制。 |
| 恢复码 | 高敏感 | 默认仅显示或复制，可要求重认证。 |
| 自定义字段 | 由用户选择 | 插入、复制或仅显示。 |

## 7. 边界条件与错误处理

| 场景 | 处理 |
| --- | --- |
| 标题或字段标签为空 | 给出默认名建议，不直接丢弃。 |
| TOTP 种子格式无法识别 | 保留原输入供修改；生成由 TOTP 文档处理。 |
| 序列化遇到未知字段 | 保留，不丢弃。 |
| schemaVersion 高于支持 | 按数据模型迁移规则处理，不兼容时不丢数据。 |
| 重复或合并 | 由 IMEX、MIGR、SYNC 处理，不在本模型。 |

## 8. 安全与隐私要求

- 序列化载荷（标题、标签、字段、密码、TOTP 种子）全部由 VAULT 加密；本模型不自行持久化明文（MASVS-STORAGE）。
- 敏感级别驱动界面与行为：高敏感默认仅显示或重认证。
- 默认行为保持秘密隐藏：密码与恢复码默认仅显示或复制。
- PlatformMatch 只是匹配提示，绝不表示目标可信。
- 保留未知字段，避免跨版本丢数据（数据模型第 8 节）。
- 不记录字段值与 TOTP 种子。

## 9. 测试映射

| 测试/任务 | 关联 |
| --- | --- |
| TP-103 添加登录条目并可搜索 | ENTRY-06、ENTRY-07 |
| 序列化往返与未知字段保留 | ENTRY-05 |
| 默认策略生效 | ENTRY-03 |
| 版本任务 MVP-004 | ENTRY-01..ENTRY-07 |
| 支撑 KBD、TOTP、IMEX/MIGR/SYNC | ENTRY-01、ENTRY-04、ENTRY-05 |

验证方式：条目可保存并可搜索；序列化往返一致且未知字段被保留；默认策略按字段类型生效；纯逻辑有单元测试。

## 10. AI 任务拆分

| 任务 ID | 目的 | 输入 | 产出物 | 约束 | 验收证据 | 依赖 |
| --- | --- | --- | --- | --- | --- | --- |
| ENTRY-01 | 领域模型 | §4、数据模型 §2-3 | Entry/Field 及枚举 | 字段携带类型与敏感级别 | 模型可表达全部字段类型 | 无 |
| ENTRY-02 | TOTP 字段与子对象 | 数据模型 §4、IA §5 | TotpField、Tag、PlatformMatch、UsagePolicy | 种子标记为高敏感 | 子对象可挂到条目 | ENTRY-01 |
| ENTRY-03 | 默认字段策略 | §6、交互 §5.4 | 默认策略实现 | 用户可覆盖 | 各类型默认正确 | ENTRY-01 |
| ENTRY-04 | 序列化模式与版本 | §3、数据模型 §8 | schema 定义、保留字段号 | 保留未知字段 | 新增字段不破坏旧解析 | ENTRY-01 |
| ENTRY-05 | 载荷编解码 | §4、§5.4 | `VaultPayloadCodec` | 二进制；保留未知 | 往返一致且保留未知 | ENTRY-04 |
| ENTRY-06 | 条目 CRUD | §5.5、VAULT | `EntryRepository` | 经 VAULT 持久化 | 增删改查持久生效 | ENTRY-05、VAULT-05 |
| ENTRY-07 | 本地搜索 | §5.6 | 搜索实现 | 仅本地，不上传 | 可按标题/字段/标签搜索 | ENTRY-06 |

## 11. 待验证与不在范围

- 序列化格式最终选择（Protocol Buffers 或 CBOR），待评审；Protobuf 非规范化字节的影响已说明。
- TOTP 验证码生成：见 TOTP 实现文档。
- 加密：见 VAULT 实现文档。
- 导入合并与同步冲突：见 IMEX、MIGR、SYNC 实现文档。
- 平台凭据匹配：兼容增强，单独记录。
- iOS 与 HarmonyOS 实现：放到 v1 阶段末尾。
