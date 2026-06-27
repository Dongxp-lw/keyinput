# 跨设备迁移 实现文档

本文件描述跨设备迁移（短码 MIGR）的实现方式，结构遵循 [实现文档层总览](README.md) 第 3 节模板。它落实 v0.3 的 MIG-001..008，复用 [加密导出与导入](encrypted-export-import.md) 的迁移包格式与 [安全实现设计](../technical/security-implementation-design.md) 第 8 节的迁移密码学规则。

本文档服从 [产品决策记录](../product/product-decision-record.md) 基线：迁移由用户自主控制、不需要第一方云账号；主动选择仍是核心心智模型；平台凭据与自动填充是兼容增强而非主路径。

本文档不另立密码学选型与二维码容量结论，复用 [v0.3 跨设备迁移计划](../product/v0.3-migration-plan.md) 第 3.1 节已锚定的二维码容量调研（二维码只承载配对密钥、不承载完整载荷）与 IMEX 的认证信封，无需新调研。

## 1. 范围与目标

实现设备到设备的保险库迁移：发送端用迁移包封装内容，通过加密包文件或本地配对通道传输；接收端先校验、再预览、按冲突策略可恢复地导入，失败回滚。

- 范围内：迁移包（复用 IMEX）、设备配对（二维码/手动）、用户自主控制的本地传输、校验与预览、冲突处理（替换/合并/保留）、可恢复导入与回滚、离线与隐私验证。
- 不在范围内：第一方云账号与实时同步（见 SYNC）；多保险库与高级迁移工具；桌面/CLI 工具仅做可行性评估（MIG-007）。
- 首平台 Android；iOS 与 HarmonyOS 在 v0.3 不构建。

## 2. 依赖的设计与技术决策

- 迁移密码学规则（域分离迁移密钥、先校验后使用、失败回滚、应用前校验兼容性）：[安全实现设计](../technical/security-implementation-design.md) 第 8 节。
- 迁移包信封与认证加密（复用）：[加密导出与导入](encrypted-export-import.md)。
- 迁移包结构 `TransferPackage` 与设备记录 `DeviceRecord`、架构迁移规则：[数据模型](../technical/data-model.md) 第 5、7、8 节。
- 二维码容量与配对结论、迁移模型与任务：[v0.3 跨设备迁移计划](../product/v0.3-migration-plan.md) 第 3.1、4、6 节。
- 先预览不覆盖、可恢复、错误保留上下文：[核心交互设计](../product/interaction-design.md) 第 3 节。
- 合并所依赖的条目与字段模型：[条目与字段模型](entry-field-model.md)。
- 版本任务与测试：MIG-001..008；[测试计划](../testing/test-plan.md) 第 3 节 TP-301..308。

## 3. 平台与技术栈（Android 优先）

- 实现层级：迁移包/校验/合并逻辑属**核心层（Rust）**；二维码扫描与本地传输属**原生层（Android Kotlin）**。见 [模块架构](../technical/module-architecture.md)；下文接口为逻辑示意。
- 配对：二维码承载短时配对密钥或引导信息，扫描或手动输入兜底；二维码不承载完整加密载荷（v0.3 计划第 3.1 节）。
- 本地传输：用户自主控制的本地通道，只传密文，不依赖账号与第一方云；具体通道实现保持抽象（见第 10 节）。

## 4. 接口与数据结构

```kotlin
enum class MigrationChannel { PACKAGE_FILE, DEVICE_PAIRING }
enum class ConflictResolution { REPLACE, MERGE, KEEP_BOTH }
enum class MigrationFailure { TAMPERED, WRONG_KEY, INCOMPATIBLE_VERSION, TRANSPORT_INTERRUPTED, CORRUPT }

data class PairingInfo(
    val pairingKeyId: String,
    val shortLivedSecret: ByteArray,    // 短时配对密钥/引导信息，经二维码或手动传递，用后清零
    val expiresAtEpochSeconds: Long,
    val sourceDeviceLabel: String?
)

data class ConflictItem(val entryId: String, val kind: String) // 与现有库的冲突项
data class MigrationPreview(
    val entryCount: Int,
    val fieldCount: Int,
    val schemaCompatible: Boolean,
    val conflicts: List<ConflictItem>
)

sealed interface MigrationResult {
    data class Success(val applied: ConflictResolution) : MigrationResult
    data class Failed(val reason: MigrationFailure) : MigrationResult // 失败回滚，不改动现有库
}

interface MigrationSender {
    fun createPackage(content: VaultContent): ByteArray          // 复用 IMEX 认证信封
    fun startPairing(sourceDeviceLabel: String?): PairingInfo     // 生成短时配对密钥
    fun sendOverLocalChannel(packageBytes: ByteArray, pairing: PairingInfo) // 只传密文
}

interface MigrationReceiver {
    fun acceptPairing(info: PairingInfo): Boolean                 // 两端用户确认且未过期
    fun receiveAndVerify(packageBytes: ByteArray, pairing: PairingInfo?): MigrationPreview // 先校验
    fun importWithResolution(resolution: ConflictResolution): MigrationResult              // 可回滚
}
```

## 5. 实现步骤

发送：

1. 序列化保险库内容，复用 IMEX 生成受迁移密钥保护的认证迁移包（域分离迁移密钥、AEAD、头部作 AAD）。
2. 通道 A（`PACKAGE_FILE`）：把迁移包作为加密文件交用户搬运（复用 v0.2 导出/导入）。
3. 通道 B（`DEVICE_PAIRING`）：生成短时配对密钥与引导信息，经二维码或手动配对在两端确认；二维码只承载配对密钥，不承载完整载荷。
4. 通道 B：通过用户自主控制的本地通道只传输密文，不依赖账号或第一方云；传输中断可重试或回退通道 A。

接收：

5. 校验迁移包认证标签与 `packageVersion`/`schemaVersion` 兼容性；失败安全失败（`TAMPERED`/`INCOMPATIBLE_VERSION`），不改动现有库。
6. 在隔离位置解密并构建预览：条目与字段计数、架构兼容性、与现有库的冲突项。
7. 展示预览与冲突；默认不覆盖；用户选择 `REPLACE`/`MERGE`/`KEEP_BOTH`。
8. 事务化应用所选策略：写入临时副本→校验→原子替换或合并；任何失败回滚，不留损坏中间状态，可重试。
9. 清零迁移密钥、配对密钥与明文缓冲；不记录秘密与迁移/配对密钥。

冲突策略：

- `REPLACE`：用迁移内容替换现有库，仍先经可回滚写入。
- `MERGE`：合并双方、保留双方数据、无静默丢失；按条目去重的具体规则待评审（见第 10 节）。
- `KEEP_BOTH`：保留两份，不覆盖现有库。

## 6. 边界条件与错误处理

| 场景 | 处理 |
| --- | --- |
| 迁移包被篡改 | AEAD 验签失败；`TAMPERED`，不输出任何部分明文，不改动现有库。 |
| 架构不兼容 | 先按数据模型迁移规则；不兼容 `INCOMPATIBLE_VERSION`，给可理解说明，不改动现有库。 |
| 二维码容量不足以承载完整载荷 | 二维码只承载配对密钥；载荷走文件或本地通道（v0.3 计划第 3.1 节）。 |
| 本地传输中断 | `TRANSPORT_INTERRUPTED`；可重试或回退通道 A 文件迁移。 |
| 配对密钥过期 | 拒绝并要求重新配对（短时失效）。 |
| 接收端已有保险库 | 默认不覆盖；先预览，由用户决策替换/合并/保留。 |
| 合并产生重复或冲突 | 保留双方、不静默丢失；冲突项在预览中标出。 |
| 导入中途失败或中断 | 回滚；现有库保持有效，可重试。 |

## 7. 安全与隐私要求

- 迁移包对所有内容加密并认证；迁移密钥与本地内容密钥域分离、独立派生（安全实现设计第 8 节）。
- 先校验后使用：解密与导入前先验签；任何明文输出前完成认证。
- 配对密钥短时有效、需两端用户确认；二维码不承载完整加密载荷。
- 传输只传密文；不需要账号或第一方云；迁移路径离线可用（不声明 `INTERNET` 依赖）。
- 默认不覆盖现有库；导入事务化、失败回滚。
- 不记录主密码、密钥、字段值、TOTP 种子、迁移密钥与配对密钥（MASVS-PRIVACY、MASVS-STORAGE）。
- 迁移由用户自主控制，是主动选择心智的延续；平台凭据与自动填充是兼容增强而非主路径。

## 8. 测试映射

| 测试/任务 | 关联 |
| --- | --- |
| 版本任务 MIG-001..008 | MIGR-01..MIGR-07 |
| TP-302 迁移包加密且带认证标签 | MIGR-01 |
| TP-303 被篡改的迁移包被拒绝 | MIGR-01、MIGR-04 |
| TP-304 二维码只承载配对密钥 | MIGR-02 |
| TP-301 不依赖第一方云完成设备到设备迁移 | MIGR-02、MIGR-03 |
| TP-305 导入前校验并预览内容与冲突 | MIGR-04 |
| TP-306 已有保险库时默认不被覆盖 | MIGR-05 |
| TP-307 导入失败不破坏现有保险库 | MIGR-06 |
| TP-308 迁移路径离线可用且不泄露秘密 | MIGR-07 |

验证方式：迁移包为密文且带认证标签；翻转任意字节后导入安全失败；二维码只含配对信息；无云路径可完成设备到设备迁移；导入前预览条目数与冲突；已有库默认不覆盖；故障注入后现有库保持有效；抓包无云依赖、日志无秘密。

## 9. AI 任务拆分

| 任务 ID | 目的 | 输入 | 产出物 | 约束 | 验收证据 | 依赖 |
| --- | --- | --- | --- | --- | --- | --- |
| MIGR-01 | 迁移包格式与加密（复用 IMEX） | §3、安全实现设计 §8、数据模型 §5 | 迁移包生成/校验，域分离迁移密钥 | 认证；只传密文 | 篡改被拒；已知答案测试通过 | IMEX-03 |
| MIGR-02 | 设备配对 | §5 发送 3、v0.3 §3.1 | 二维码/手动配对、短时配对密钥、两端确认 | 二维码不承载完整载荷；短时失效 | 可配对；二维码仅配对信息 | MIGR-01 |
| MIGR-03 | 本地传输通道 | §5 发送 4 | 用户自主本地通道、只传密文、可重试 | 不依赖云/账号 | 密文传输；中断可重试 | MIGR-02 |
| MIGR-04 | 校验与预览 | §5 接收 5-6 | 完整性校验、计数、架构兼容、冲突预览 | 先校验后使用 | 校验失败被拒；预览正确 | MIGR-01 |
| MIGR-05 | 冲突处理 | §5 接收 7、§6 | 替换/合并/保留，默认不覆盖 | 无静默丢失 | 默认不覆盖；三策略可选 | MIGR-04 |
| MIGR-06 | 可恢复导入与回滚 | §5 接收 8 | 事务化导入、失败回滚、可重试 | 失败不损坏现有库 | 故障注入后库仍有效 | MIGR-05 |
| MIGR-07 | 离线与隐私验证 | §7 | 无云依赖核对、日志扫描 | 不记录秘密与迁移/配对密钥 | 无云；日志无秘密 | MIGR-03、MIGR-06 |

## 10. 待验证与不在范围

- 配对的密钥建立方式（导出口令复用、配对密钥包装迁移包密钥、或基于 PAKE 的认证密钥交换）待评审；约束：必须保持迁移包认证与迁移密钥域分离。
- 本地传输通道的具体实现（本地网络、近场、文件中转等）待评审；v0.3 计划刻意保持抽象、不依赖云。
- 合并（`MERGE`）的去重与冲突规则（按条目 id / updatedAt / 字段级）待评审，关联 ENTRY 与未来 SYNC。
- 桌面或 CLI 打包辅助工具（MIG-007）仅做可行性评估，不在本文档实现；纳入须不削弱加密与认证、不引入云依赖。
- iOS 与 HarmonyOS 实现：放到 v1 阶段末尾。
