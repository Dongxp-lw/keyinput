# 云同步 实现文档

本文件描述云同步（短码 SYNC）的实现方式，结构遵循 [实现文档层总览](README.md) 第 3 节模板。它落实 v1.1 的 SYNC-001..008，复用 [加密导出与导入](encrypted-export-import.md) 与 [跨设备迁移](cross-device-migration.md) 的认证信封与域分离规则。

本文档服从 [产品决策记录](../product/product-decision-record.md) 基线：云同步是可选增强，本地使用永不依赖云；登录只用于云服务、绝不作为本地使用前置；主动选择仍是核心心智模型；平台凭据与自动填充是兼容增强而非主路径。

本文档不另立密码学与复制模型选型，复用 [v1.1 云同步计划](../product/v1.1-cloud-sync-plan.md) 第 3 节已锚定的乐观复制、版本向量冲突检测与 CRDT 结论，以及 [安全实现设计](../technical/security-implementation-design.md) 第 8 节的同步密码学规则，无需新调研。

## 1. 范围与目标

实现端到端加密的可选云同步：本地变更生成认证同步载荷、上传密文、在多设备间收敛，检测并解决并发冲突、保留版本历史，且本地使用永不依赖云。

- 范围内：账号身份（仅云）、端到端加密同步、设备注册与撤销、冲突检测与解决、版本历史与恢复、订阅权益、离线优先保障。
- 不在范围内：改变离线优先或要求登录才能本地使用（违反基线）；云服务端实现本身；多保险库；iOS 与 HarmonyOS 构建。
- 服务端只存储密文，绝不接收主密码、保险库明文、明文 TOTP 种子或派生内容密钥（技术架构可选云服务）。

## 2. 依赖的设计与技术决策

- 同步密码学规则（域分离同步密钥、认证、先校验后使用、只处理密文）：[安全实现设计](../technical/security-implementation-design.md) 第 8 节。
- 零知识与信任边界（服务端不接触明文与主密码）：[安全模型](../technical/security-model.md) 第 7 节、[技术架构](../technical/architecture.md) 可选云服务与第 6 节数据流。
- 同步对象与设备记录：[数据模型](../technical/data-model.md) 第 6、7 节（`SyncObject`、`DeviceRecord`）。
- 乐观复制、版本向量冲突检测、CRDT 与无丢失合并：[v1.1 云同步计划](../product/v1.1-cloud-sync-plan.md) 第 3、5 节。
- 条目与字段级合并所依赖的模型：[条目与字段模型](entry-field-model.md)。
- 认证信封复用：[加密导出与导入](encrypted-export-import.md)、[跨设备迁移](cross-device-migration.md)。
- 版本任务与测试：SYNC-001..008；[测试计划](../testing/test-plan.md) 第 4 节 TP-201..206。

## 3. 平台与技术栈（Android 优先）

- 实现层级：载荷加密/冲突检测/合并逻辑属**核心层（Rust）**；网络与账号 UI 属**原生层（Android Kotlin）**。见 [模块架构](../technical/module-architecture.md)；下文接口为逻辑示意。
- 加密：复用 IMEX/MIGR 的 AEAD 与域分离派生，用独立的同步密钥，不在本文档重定义。
- 复制模型：乐观复制——任意设备可离线编辑，联网后再同步。
- 冲突检测：基于 `SyncObject` 的 `baseVersion` 与 `version`（版本向量），采用追加式版本。

## 4. 接口与数据结构

```kotlin
// 数据模型第 6、7 节
data class SyncObject(
    val objectId: String,
    val vaultId: String,
    val deviceId: String,
    val baseVersion: Long,
    val version: Long,
    val createdAt: Long,
    val encryptedPayload: ByteArray,   // 域分离同步密钥加密，仅密文
    val authenticationTag: ByteArray
)

data class DeviceRecord(
    val deviceId: String,
    val displayName: String,
    val platform: String,
    val registeredAt: Long,
    val lastSyncAt: Long?,
    val publicSyncKey: ByteArray?
)

enum class SyncConflict { NONE, CONCURRENT }                     // 基于 baseVersion/version
enum class ConflictResolution { KEEP_LOCAL, KEEP_REMOTE, MERGE } // 用户可见，默认不静默覆盖
enum class SyncFailure { TAMPERED, NETWORK, UNAUTHORIZED, ENTITLEMENT, INCOMPATIBLE_VERSION, CORRUPT }

data class ConflictItem(val entryId: String, val field: String?)

sealed interface SyncResult {
    data class Converged(val applied: Int) : SyncResult
    data class ConflictPending(val items: List<ConflictItem>) : SyncResult // 待用户解决
    data class Failed(val reason: SyncFailure) : SyncResult                 // 降级为本地可用
}

interface SyncEngine {
    fun createPayload(localChange: VaultContent): SyncObject       // 域分离同步密钥、认证
    fun detectConflict(local: SyncObject, remote: SyncObject): SyncConflict
    fun push(): SyncResult                                          // 上传密文与版本元数据
    fun pull(): SyncResult                                          // 下载、校验、解密、合并
    fun resolve(item: ConflictItem, resolution: ConflictResolution): SyncResult
}

// 仅云服务，不参与本地解锁与保险库解密
interface CloudSession {
    fun login(): Boolean
    fun logout()
    fun registerDevice(record: DeviceRecord)
    fun revokeDevice(deviceId: String)
    fun entitlementActive(): Boolean
}
```

## 5. 实现步骤

推送（本地变更上云）：

1. 本地变更后，核心创建端到端加密同步载荷：用域分离同步密钥（独立于本地内容密钥）加密并认证，附 `baseVersion`/`version`。
2. 平台上传密文与版本元数据；服务端只存带版本的加密对象，绝不接收主密码或明文。
3. 同步失败（网络/未授权/权益）降级为本地可用、稍后重试，不阻断本地编辑与键盘输入。

拉取与合并（云端到本地）：

4. 下载远端密文与版本元数据。
5. 先校验认证标签（先校验后使用）；失败安全失败（`TAMPERED`/`INCOMPATIBLE_VERSION`），不改动本地有效库。
6. 用 `baseVersion`/`version` 检测并发：两端从同一基线编辑判为并发冲突，先后编辑不误报。
7. 解密后按条目/字段级合并；无冲突直接收敛；真正冲突标为待解决，默认不静默覆盖。
8. 用户可见地解决冲突（保留本地/远端/合并）；版本历史保留双方、避免静默丢失；合并失败可回滚。
9. 清零同步密钥与明文缓冲；不记录主密码、密钥、字段值、TOTP 种子与同步密钥。

账号与设备：

10. 登录只解锁云功能；本地创建/解锁/编辑/键盘输入不要求登录、不参与解密。
11. 基于 `DeviceRecord` 注册、列出、撤销设备；撤销后不再获得新密文访问。
12. 权益只约束云功能；权益异常不阻断本地；权益校验不暴露保险库内容。

## 6. 边界条件与错误处理

| 场景 | 处理 |
| --- | --- |
| 同步载荷被篡改 | 验签失败；`TAMPERED`，不改动本地有效库。 |
| 离线编辑 | 乐观复制；联网后进入检测与同步。 |
| 并发编辑（同一基线） | 判为冲突；条目/字段级、用户可见解决，默认不覆盖。 |
| 同步失败（网络/未授权/权益） | 降级本地可用，稍后重试，不阻断本地。 |
| 退出登录 | 本地保险库仍完全可用；只关闭云功能。 |
| 设备被撤销 | 不再获得新密文访问。 |
| 版本历史恢复 | 历史为密文；恢复不破坏当前有效保险库。 |
| 架构不兼容 | 先校验兼容性；不兼容安全失败，不破坏本地。 |

## 7. 安全与隐私要求

- 端到端加密：同步载荷用域分离同步密钥（独立于本地内容密钥）加密并认证；服务端只存密文（安全实现设计第 8 节、安全模型第 7 节）。
- 先校验后使用：合并前先验签；任何明文输出前完成认证。
- 服务端绝不接收主密码、保险库明文、明文 TOTP 种子或派生内容密钥；主密码仍是根秘密、留客户端；云账号密码重置不解密保险库。
- 登录只用于账号身份、设备管理、权益与云对象访问，不参与本地解锁与保险库解密。
- 默认不静默覆盖；追加式版本、版本历史、合并可回滚，避免数据丢失。
- 冲突与版本元数据可对服务端可见，但内容保持加密；元数据最小化。
- 不记录主密码、密钥、字段值、TOTP 种子与同步密钥（MASVS-PRIVACY、MASVS-STORAGE）。
- 云同步是可选增强，本地使用永不依赖云；主动选择仍是核心，平台凭据与自动填充是兼容增强而非主路径。

## 8. 测试映射

| 测试/任务 | 关联 |
| --- | --- |
| 版本任务 SYNC-001..008 | SYNC-01..SYNC-08 |
| TP-201 登录后启用云同步 | SYNC-01、SYNC-02 |
| TP-202 退出登录保留本地保险库 | SYNC-08 |
| TP-203 同步只上传密文 | SYNC-02 |
| TP-204 设备 A 与 B 同步收敛 | SYNC-02、SYNC-03 |
| TP-205 离线编辑稍后同步 | SYNC-04、SYNC-08 |
| TP-206 冲突被检测并解决、无数据丢失 | SYNC-04、SYNC-05 |

验证方式：服务端载荷无明文字段且带认证标签；同步密钥独立于本地内容密钥；两设备同步后状态收敛；离线编辑联网后上传；同基线并发编辑被判为冲突并由用户解决、无丢失；退出登录或同步失败时本地仍可用。

## 9. AI 任务拆分

| 任务 ID | 目的 | 输入 | 产出物 | 约束 | 验收证据 | 依赖 |
| --- | --- | --- | --- | --- | --- | --- |
| SYNC-01 | 账号与身份（仅云） | §5 步 10、v1.1 §7 | `CloudSession` 登录/登出 | 登录不参与解密 | 登录只解锁云功能 | 无 |
| SYNC-02 | 端到端加密同步 | §5 步 1-5、安全实现设计 §8 | `createPayload`/`push`/`pull` 密文 | 域分离同步密钥；服务端只存密文 | 服务端无明文；密钥独立 | SYNC-01 |
| SYNC-03 | 设备列表与注册 | §5 步 11、数据模型 §7 | `DeviceRecord` 注册/列出/撤销 | 撤销后无新密文访问 | 设备可注册；撤销生效 | SYNC-02 |
| SYNC-04 | 冲突检测 | §5 步 6、v1.1 §3.2 | `baseVersion`/`version` 并发检测 | 先后编辑不误报 | 同基线编辑判为冲突 | SYNC-02 |
| SYNC-05 | 冲突解决与无丢失合并 | §5 步 7-8、v1.1 §3.3 | 条目/字段级合并、用户可见解决 | 无静默丢失；可回滚 | 真实冲突由用户决策 | SYNC-04 |
| SYNC-06 | 版本历史与恢复 | §5、v1.1 §7 | 版本历史、按版本恢复 | 历史只含密文 | 可恢复；不破坏当前库 | SYNC-05 |
| SYNC-07 | 订阅与权益校验 | §5 步 12、v1.1 §7 | 权益状态与校验 | 权益只约束云功能 | 权益异常不阻断本地 | SYNC-01 |
| SYNC-08 | 离线优先保障 | §5 步 3、§7 | 退出/离线/同步失败降级 | 本地永不依赖云 | 退出或离线本地可用 | SYNC-02 |

## 10. 待验证与不在范围

- CRDT 式自动合并作为后续可选项；v1.1 以条目/字段级 + 用户可见解决 + 版本历史为主（v1.1 计划第 3.3 节）。
- 云服务端实现、账号身份协议与传输层安全：不在本实现文档，按 v1.1 计划与技术架构单独设计。
- 同步密钥的具体派生与设备间分发（`DeviceRecord.publicSyncKey` 的用途）待评审；约束：域分离、服务端不接触明文。
- 元数据最小化的具体字段暴露程度待评审。
- 改变离线优先或要求登录才能本地使用：明确不在范围（违反基线）。
- iOS 与 HarmonyOS 实现：放到 v1 阶段末尾。
