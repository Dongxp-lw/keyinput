# 加密导出与导入 实现文档

本文件描述加密导出与导入（短码 IMEX）的实现方式，结构遵循 [实现文档层总览](README.md) 第 3 节模板。它落实 v0.2 的 MVP-008，复用 [保险库加密核心](vault-crypto-core.md) 的密码学原语与 [条目与字段模型](entry-field-model.md) 的序列化载荷。

本文档服从 [产品决策记录](../product/product-decision-record.md) 基线：导出与导入是用户主动发起的本地备份/恢复操作，明文不离开设备，云端与网络不接触明文，平台凭据与自动填充是兼容增强而非主路径。

本文档不另立密码学选型，完整复用 [安全实现设计](../technical/security-implementation-design.md) 第 2、3、4、8 节已锚定的决策（Argon2id、XChaCha20-Poly1305、信封密钥、域分离迁移子密钥、头部作 AAD、失败回滚），无需新调研。

## 1. 范围与目标

实现加密导出包的生成与导入恢复：把解锁后的保险库内容序列化、用导出口令以信封方式加密为独立的迁移包，并在导入时先认证校验、版本兼容检查、失败可回滚。

- 范围内：导出包格式（`TransferPackage`）、导出口令派生与信封包装、迁移子密钥域分离、载荷认证加密、导入校验与解密、版本与架构兼容检查、失败回滚。
- 不在范围内：保险库本地文件格式与原语实现（见 VAULT，本文档复用）；条目字段序列化模式（见 ENTRY）；跨设备配对迁移（见 MIGR）；云同步（见 SYNC）。
- 导出包必须独立于本地保险库文件进行加密和认证保护（数据模型第 5 节）。

## 2. 依赖的设计与技术决策

- 迁移包密码学规则（域分离子密钥、先校验后使用、失败回滚、应用前校验兼容性）：[安全实现设计](../technical/security-implementation-design.md) 第 8 节。
- KDF、AEAD、信封密钥层级与文件格式：[安全实现设计](../technical/security-implementation-design.md) 第 2、3、4 节。
- 复用的加解密原语（`deriveKek`、`wrapDek`/`unwrapDek`、`seal`/`open`、随机源）：[保险库加密核心](vault-crypto-core.md) 第 4 节。
- 序列化载荷（二进制、保留未知字段）：[条目与字段模型](entry-field-model.md) 第 4、5 节。
- 迁移包结构与架构迁移规则：[数据模型](../technical/data-model.md) 第 5、8 节。
- 版本任务与验收证据：[v0.2 离线 MVP 计划](../product/v0.2-mvp-plan.md)（MVP-008）；测试 TP-105、TP-106、TP-108。

## 3. 平台与技术栈（Android 优先）

- 实现语言：打包/校验逻辑属**核心层（Rust）**；文件选择等 IO 在**原生层**。见 [模块架构](../technical/module-architecture.md)；下文接口为逻辑示意。
- 复用 VAULT 的加密原语（Rust crate，见 [保险库加密核心](vault-crypto-core.md) 与安全实现设计 §2.4），不在本文档另选库。
- 随机源：CSPRNG 生成导出包独立的 salt、nonce 与 package DEK。
- 导出包是独立于本地保险库文件的自包含容器：自带 `cryptoProfile`，可在仅有导出口令的全新安装上恢复。

## 4. 接口与数据结构

导出包结构对应数据模型第 5 节 `TransferPackage`，头部不含秘密但作为 AAD 受认证保护。

```kotlin
data class TransferPackageHeader(
    val magic: String,               // 包类型标识
    val packageVersion: Int,         // 包格式版本
    val schemaVersion: Int,          // 数据架构版本
    val createdAt: Long,
    val sourceDeviceLabel: String?,  // 可选，注意暴露程度（见第 10 节）
    val vaultId: String,
    val cryptoProfile: CryptoProfile // kdf{ id,version,m,t,p,salt } + aead{ id }，与 VAULT 同结构
)

data class TransferPackage(
    val header: TransferPackageHeader,
    val wrappedPackageDek: ByteArray, // 被导出口令 KEK 包装的随机 package DEK
    val nonce: ByteArray,             // 载荷加密的 24 字节随机 nonce
    val encryptedPayload: ByteArray,
    val authenticationTag: ByteArray
)

enum class ImportFailure { WRONG_PASSPHRASE, TAMPERED, INCOMPATIBLE_VERSION, CORRUPT }

sealed interface ImportResult {
    data class Success(val content: VaultContent) : ImportResult
    data class Failed(val reason: ImportFailure) : ImportResult // 失败不改动现有库
}

interface VaultExporter {
    // passphrase 为导出口令（是否复用主密码待评审，见第 10 节）
    fun export(content: VaultContent, passphrase: CharArray): ByteArray
}

interface VaultImporter {
    fun inspect(bytes: ByteArray): TransferPackageHeader            // 仅解析明文头部，不解密
    fun import(bytes: ByteArray, passphrase: CharArray): ImportResult
}
```

## 5. 实现步骤

导出：

1. 序列化保险库内容（复用 ENTRY 的 `VaultPayloadCodec`，二进制、保留未知字段）。
2. 生成导出包独立的随机 salt 与随机 package DEK（256-bit）。
3. Argon2id（passphrase、salt、参数）派生 package KEK，参数写入头部 `cryptoProfile`。
4. 用 package KEK 以 AEAD 包装 package DEK 得 `wrappedPackageDek`。
5. 用 package DEK 经域分离派生迁移子密钥（上下文标签如 `transfer`，区别于保险库内容标签）。
6. 生成 24 字节随机 nonce，用迁移子密钥以 XChaCha20-Poly1305 加密序列化载荷，头部规范序列化作为 AAD。
7. 组装 `TransferPackage` 字节（明文头部 + wrappedPackageDek + nonce + ciphertext + authTag）。
8. 清零 passphrase、KEK、DEK、子密钥与明文缓冲。

导入：

1. `inspect` 解析明文头部，校验 magic、packageVersion、schemaVersion 兼容性；不兼容返回 `INCOMPATIBLE_VERSION`，不改动现有库。
2. Argon2id 用头部参数派生 package KEK。
3. 用 KEK 解包 `wrappedPackageDek`；失败即 `WRONG_PASSPHRASE`，不泄露区分细节。
4. 域分离派生迁移子密钥。
5. 以头部作为 AAD 验签并解密 `encryptedPayload`；验签失败即 `TAMPERED`，终止且不输出任何部分明文。
6. 反序列化为 `VaultContent`（保留未知字段）。
7. 应用前在隔离位置完成解密与校验，采用可回滚写入（临时副本→校验→原子替换/合并）；任何失败回滚，不覆盖上一份有效保险库。
8. 清零所有密钥与明文缓冲。

## 6. 边界条件与错误处理

| 场景 | 处理 |
| --- | --- |
| 错误导出口令 | 解包 package DEK 认证失败；返回不可区分的 `WRONG_PASSPHRASE`。 |
| 导出包被篡改 | AEAD 验签失败；`TAMPERED`，终止，不输出任何部分明文。 |
| packageVersion/schemaVersion 不兼容 | 先按数据模型迁移规则；不兼容返回 `INCOMPATIBLE_VERSION`，不改动现有库。 |
| 导入中途失败或中断 | 可回滚，不覆盖上一份有效保险库。 |
| 文件截断或损坏 | 返回 `CORRUPT`，拒绝。 |
| 序列化遇到未知字段 | 保留，不丢弃（前向兼容）。 |
| 加密库加载失败 | 安全失败并上报，不退化为明文。 |

## 7. 安全与隐私要求

- 导出包对所有有意义内容加密（标题、标签、自定义字段、密码、TOTP 种子、备注）；头部不含秘密但作 AAD 认证（MASVS-STORAGE）。
- 迁移子密钥与本地保险库内容密钥域分离、独立派生（安全实现设计第 3、8 节）。
- 解密前先校验认证标签；任何明文输出前完成认证（先校验后使用）。
- 导入失败可回滚，绝不破坏现有有效保险库。
- 用 CSPRNG 生成 salt、nonce、DEK；Argon2id 参数写入头部以便升级（MASVS-CRYPTO）。
- 不记录导出口令、密钥、明文载荷、字段值、TOTP 种子，也不记录导出文件路径内容（MASVS-PRIVACY）。
- 明文不离开设备；导出是用户主动发起的本地操作，云端与网络不接触明文。
- 口令用 `CharArray`、密钥用 `ByteArray`，使用后清零。

## 8. 测试映射

| 测试/任务 | 关联 |
| --- | --- |
| 版本任务 MVP-008 | IMEX-01..IMEX-06 |
| TP-105 导出加密保险库（不可明文读取） | IMEX-02、IMEX-03 |
| TP-106 导入与源一致 | IMEX-04、IMEX-05 |
| TP-108 被篡改的导出包被拒绝 | IMEX-04 |
| 导入失败不破坏现有库（回滚） | IMEX-05 |
| 复用 VAULT 的 KDF/AEAD 已知答案测试 | IMEX-01、IMEX-02 |

验证方式：导出包不能按明文读取（无可见字段值）；导入后条目与源保险库一致；翻转导出包任意字节后导入安全失败；错误导出口令不可区分地失败；版本不兼容或中途失败时现有保险库保持不变。

## 9. AI 任务拆分

| 任务 ID | 目的 | 输入 | 产出物 | 约束 | 验收证据 | 依赖 |
| --- | --- | --- | --- | --- | --- | --- |
| IMEX-01 | 包格式与编解码 | §4、安全实现设计 §4/§8、数据模型 §5 | `TransferPackage`、头部编解码、headerAad | 头部规范序列化作 AAD | 头部可读写；AAD 稳定 | VAULT-01 |
| IMEX-02 | 导出口令派生与信封 | §5 导出 2-4、安全实现设计 §2.1/§3 | Argon2id 派生 + 包装 package DEK | 独立 salt；参数写头部 | 同口令同参数派生一致 | VAULT-02、VAULT-03 |
| IMEX-03 | 导出加密与组装 | §5 导出 5-8 | `export()` 生成字节 | 域分离迁移子密钥；随机 nonce；用后清零 | 导出包不可明文读取 | IMEX-01、IMEX-02 |
| IMEX-04 | 导入校验与解密 | §5 导入 1-6、§6 | `inspect()`、`import()` 解密验签 | 先校验后使用；错误口令/篡改安全失败 | 篡改被拒；错误口令不可区分 | IMEX-03 |
| IMEX-05 | 回滚与版本兼容 | §5 导入 7、安全实现设计 §8、数据模型 §8 | 可回滚导入、版本兼容检查 | 失败不覆盖现有库 | 导入与源一致；失败回滚 | IMEX-04 |
| IMEX-06 | 卫生与不记日志 | §7 | 清零、日志核对 | 不记录口令/明文/字段值/TOTP 种子 | 缓冲清零；日志无秘密 | IMEX-03、IMEX-04 |

## 10. 待验证与不在范围

- 导出口令策略：复用主密码还是设置单独的导出口令，待评审（默认允许用户设置导出口令，可与主密码相同）。
- 导出容器布局与明文元数据暴露程度（`createdAt`、`sourceDeviceLabel`、`vaultId` 是否以及如何在明文头部暴露），待评审。
- 导入合并策略（整库恢复 vs 合并去重）：v0.2 以整库恢复为主，合并策略待评审（见 ENTRY、MIGR）。
- 跨设备配对迁移（MIGR）与云同步（SYNC）复用包格式与域分离规则，但不在本文档。
- 最终加密库与序列化格式选择：随 VAULT、ENTRY 评审。
- iOS 与 HarmonyOS 实现：放到 v1 阶段末尾。
