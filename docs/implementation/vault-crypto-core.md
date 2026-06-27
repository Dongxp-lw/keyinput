# 保险库加密核心 实现文档

本文件描述本地加密保险库核心（短码 VAULT）的实现方式，结构遵循 [实现文档层总览](README.md) 第 3 节模板。它落实 v0.2 的 MVP-001，并为 [安全键盘输入法](secure-keyboard-ime.md) 与解锁会话（LOCK）提供加解密能力。

## 1. 范围与目标

实现本地保险库文件的加密读写：从主密码派生密钥、信封式保护数据密钥、对保险库载荷做认证加密、定义文件格式与版本。

- 范围内：保险库文件格式与编解码、Argon2id 派生 KEK、随机 DEK 的包装与解包、XChaCha20-Poly1305 载荷加解密、篡改检测、格式与架构版本。
- 不在范围内：解锁会话与 UI、生物识别（见 LOCK、BIO）；条目和字段的序列化模式（见 ENTRY，VAULT 只加密其序列化后的载荷）；迁移包与同步密钥（见 MIGR、SYNC）。
- 算法选型不在本文档重新决定，遵循 [安全实现设计](../technical/security-implementation-design.md)。

## 2. 依赖的设计与技术决策

- 密码学选型、密钥层级、文件格式、卫生与演进：[安全实现设计](../technical/security-implementation-design.md) 第 2、3、4、7、9 节。
- 验证要求（已知答案测试、篡改检测、错误口令）：安全实现设计第 12 节。
- 保险库逻辑模型与架构迁移规则：[数据模型](../technical/data-model.md) 第 1、8 节。
- 版本任务与退出标准：[v0.2 离线 MVP 计划](../product/v0.2-mvp-plan.md)（MVP-001）。

## 3. 平台与技术栈（Android 优先）

- 实现语言：**Rust**（本功能属共享核心层，D-006，见 [模块架构](../technical/module-architecture.md)）；下文 Kotlin 接口为逻辑示意，实际以 Rust 实现。
- 加密库（已锁定 D-004）：Rust crate——RustCrypto 的 `argon2`（Argon2id）+ `chacha20poly1305`（XChaCha20-Poly1305），或 libsodium 经 `libsodium-sys`；最终 crate 与版本在实现评审锁定。
- 随机源：Rust 的 `getrandom`/RustCrypto 加密安全随机生成 salt、nonce、DEK。

## 4. 接口与数据结构

保险库文件结构遵循安全实现设计第 4 节。头部不含秘密，但作为 AAD 受认证保护。

```text
VaultFile
- magic               文件类型标识
- formatVersion       文件格式版本
- schemaVersion       数据架构版本
- cryptoProfile
    - kdf: { id, version, m, t, p, salt }
    - aead: { id }
- wrappedDEK          被 KEK 包装的数据密钥及认证标签
- nonce               载荷加密随机 nonce（XChaCha20 为 24 字节）
- ciphertext          加密后的保险库载荷
- authTag             载荷认证标签
```

接口（逻辑示意，实际以 Rust 实现；密钥类型可清零）：

```kotlin
interface VaultCrypto {
    fun deriveKek(masterPassword: CharArray, kdf: KdfParams): Kek
    fun wrapDek(dek: Dek, kek: Kek): WrappedDek
    fun unwrapDek(wrapped: WrappedDek, kek: Kek): Dek            // 失败即口令错误
    fun seal(plain: ByteArray, dek: Dek, aad: ByteArray): Sealed // XChaCha20-Poly1305
    fun open(sealed: Sealed, dek: Dek, aad: ByteArray): ByteArray // 验签失败即篡改
    fun randomDek(): Dek
}

interface VaultFileCodec {
    fun read(bytes: ByteArray): VaultFile      // 解析头部
    fun write(file: VaultFile): ByteArray      // 头部作为 AAD
    fun headerAad(file: VaultFile): ByteArray  // magic+版本+cryptoProfile 的规范序列化
}

interface VaultStore {
    fun create(masterPassword: CharArray): Vault
    fun open(bytes: ByteArray, masterPassword: CharArray): Vault
    fun save(vault: Vault): ByteArray
    fun changePassword(vault: Vault, old: CharArray, new: CharArray): ByteArray
}
```

## 5. 实现步骤

1. 文件格式：实现 `VaultFile` 与 `VaultFileCodec`，把 magic、版本、cryptoProfile 规范序列化为 AAD。
2. KDF：用 Argon2id 从主密码、salt 与参数派生 KEK，参数写入头部。
3. 信封密钥：生成 256-bit 随机 DEK，用 KEK 以 AEAD 包装为 wrappedDEK。
4. 载荷加密：用 DEK 和随机 nonce，以 XChaCha20-Poly1305 加密序列化载荷，头部作为 AAD。
5. 创建流程：新 salt → 派生 KEK → 随机 DEK → 包装 → 加密空载荷 → 写文件。
6. 打开流程：读头部 → 用头部参数派生 KEK → 解包 DEK（失败即口令错误）→ 解密并验签（失败即篡改）。
7. 保存流程：用 DEK 和新 nonce 重新加密载荷；未改口令时 wrappedDEK 不变。
8. 改主密码：派生新 KEK，仅重新包装 DEK，不重加密整库。
9. 演进：从头部读取 kdf/aead 标识；参数低于目标时在打开后重派生并重包装。
10. 清零：KEK、DEK、明文使用后立即清零。

## 6. 边界条件与错误处理

| 场景 | 处理 |
| --- | --- |
| 错误主密码 | 解包 DEK 认证失败；返回不可区分的口令错误。 |
| 头部或密文被篡改 | AEAD 验签失败；终止，不输出任何部分明文。 |
| 未知 formatVersion/schemaVersion | 按数据模型迁移规则处理；不兼容时给出可理解错误，不覆盖原文件。 |
| 低内存设备 | Argon2id 参数降级到 OWASP 下限档，并写入头部。 |
| 加密库或本地库加载失败 | 安全失败并上报，不退化为明文。 |
| 文件截断或损坏 | 拒绝读取。 |

## 7. 安全与隐私要求

- 加密所有有意义内容：标题、标签、自定义字段、密码、TOTP 种子（MASVS-STORAGE）。
- 头部作为 AAD 认证；任何明文输出前先验签。
- KEK 只驻留内存；DEK 只以包装形式落盘；密钥相互独立；保险库、迁移包、同步用域分离子密钥。
- 用 CSPRNG 生成 salt、nonce、DEK；无硬编码密钥（MASVS-CRYPTO）。
- Argon2id 参数写入头部以便升级与重哈希判断。
- KEK、DEK、明文使用后清零，可行时锁定内存（安全实现设计第 7 节）。
- 不记录主密码、密钥、字段值、TOTP 种子。
- 改主密码只重包装 DEK，不暴露明文，不重加密整库。

## 8. 测试映射

| 测试/任务 | 关联 |
| --- | --- |
| TP-101 不登录创建保险库 | VAULT-05 |
| TP-107 错误主密码失败且不破坏数据 | VAULT-03、VAULT-05 |
| 篡改导致解密失败 | VAULT-04、VAULT-07 |
| 已知答案测试覆盖 KDF 与 AEAD | VAULT-08 |
| 版本任务 MVP-001 | VAULT-01..VAULT-08 |
| 支撑 MVP-002（LOCK）、MVP-008（导出导入复用格式） | VAULT-01..VAULT-06 |

验证方式：用固定测试向量验证 Argon2id 与 XChaCha20-Poly1305；翻转密文任意字节后解密必失败；错误口令解包必失败；头部参数被正确读取并用于打开；改口令后旧库可用新口令打开且未重加密整库。

## 9. AI 任务拆分

| 任务 ID | 目的 | 输入 | 产出物 | 约束 | 验收证据 | 依赖 |
| --- | --- | --- | --- | --- | --- | --- |
| VAULT-01 | 文件格式与编解码 | §4、安全实现设计 §4 | `VaultFile`、`VaultFileCodec`、headerAad | 头部规范序列化为 AAD | 头部可读写；AAD 稳定 | 无 |
| VAULT-02 | Argon2id 派生 KEK | §5.2、安全实现设计 §2.1 | `deriveKek` 与参数读写 | 参数写入头部；不低于 OWASP 下限 | 同参数同口令派生一致 | VAULT-01 |
| VAULT-03 | 信封密钥 | §5.3、安全实现设计 §3 | 随机 DEK、`wrapDek`/`unwrapDek` | DEK 独立随机；只存包装形式 | 错误口令解包失败 | VAULT-02 |
| VAULT-04 | 载荷加解密 | §5.4、安全实现设计 §2.2 | `seal`/`open`（XChaCha20-Poly1305） | 随机 nonce；头部作 AAD | 篡改任意字节解密失败 | VAULT-01、VAULT-03 |
| VAULT-05 | 创建/打开/保存流程 | §5.5-7 | `VaultStore.create/open/save` | 失败不输出部分明文 | 创建并解密读取一致 | VAULT-04 |
| VAULT-06 | 改主密码重包装 | §5.8 | `changePassword` | 仅重包装 DEK | 旧库可用新口令打开 | VAULT-05 |
| VAULT-07 | 篡改检测与版本处理 | §6、数据模型 §8 | 错误处理与版本兼容检查 | 不兼容不覆盖原文件 | 篡改与不兼容均安全失败 | VAULT-05 |
| VAULT-08 | 已知答案测试与清零 | §7、§8 | 测试向量用例、清零确认 | 不记录秘密 | 测试向量通过；缓冲清零 | VAULT-04 |

## 10. 待验证与不在范围

- 最终 Rust 加密 crate（RustCrypto 或 libsodium-sys）与版本，待实现评审锁定。
- Argon2id 具体参数，待目标设备基准测试。
- 解锁会话、UI 与生物识别：见 LOCK、BIO 实现文档。
- 条目和字段序列化模式：见 ENTRY 实现文档。
- 迁移包与同步密钥派生：见 MIGR、SYNC 实现文档。
- iOS 与 HarmonyOS 实现：放到 v1 阶段末尾。
