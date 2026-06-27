# 主密码与解锁会话 实现文档

本文件描述主密码解锁与解锁会话（短码 LOCK）的实现方式，结构遵循 [实现文档层总览](README.md) 第 3 节模板。它落实 v0.2 的 MVP-002，使用 [保险库加密核心](vault-crypto-core.md) 的派生与解包能力，并为 [安全键盘输入法](secure-keyboard-ime.md) 提供 `VaultSession`。

## 1. 范围与目标

实现以主密码解锁本地保险库，并管理只驻留内存的解锁会话：建立会话、自动锁定、清零密钥、向键盘下发最小授权。

- 范围内：解锁流程、会话模型与生命周期、自动锁定（后台与空闲）、键盘最小短时授权、失败处理、内存清零。
- 不在范围内：保险库加密内部（见 VAULT）；生物识别解锁（见 BIO，作为另一条产生同等会话的路径）；条目和字段读取（见 ENTRY、KBD）；界面视觉设计。

## 2. 依赖的设计与技术决策

- 解锁与会话模型、键盘会话授权、会话与内存清除：[安全实现设计](../technical/security-implementation-design.md) 第 5、5.3、5.4、7 节。
- 派生 KEK 与解包 DEK：[保险库加密核心](vault-crypto-core.md)。
- 创建与解锁交互、可恢复原则（解锁失败保留任务）：[核心交互设计](../product/interaction-design.md) 第 4、3.3 节。
- 版本任务：[v0.2 离线 MVP 计划](../product/v0.2-mvp-plan.md)（MVP-002）。

## 3. 平台与技术栈（Android 优先）

- 实现层级：密钥/会话逻辑属**核心层（Rust）**；主密码输入边界与自动锁定属**原生层（Android Kotlin）**。见 [模块架构](../technical/module-architecture.md)；下文接口为逻辑示意。
- 主密码以 `CharArray`（Kotlin 侧）/ 可清零字节（Rust 侧）传递，使用后立即清空；不用不可清零的 `String`。
- 自动锁定（原生）：监听 `ProcessLifecycleOwner`（`androidx.lifecycle:lifecycle-process`）的 `ON_STOP`，应用进入后台时触发锁定或宽限计时；该事件在配置变更时有延迟，避免误锁。
- 密钥缓冲（核心）：Rust 用 `zeroize` 清零、可选 libsodium（经 `libsodium-sys`）的 mlock；mlock 在移动端能力有限，作为尽力而为，待评审。
- 空闲超时：用计时器，使用时重置。

## 4. 接口与数据结构

```kotlin
// 解锁会话，只驻留内存，可清零
interface VaultSession {
    val createdAt: Long
    var lastUsedAt: Long
    fun <T> withContentKey(block: (Dek) -> T): T // 使用期间持有，用后不外泄
    fun close()                                   // 清零并失效
}

interface UnlockManager {
    fun unlockWithPassword(masterPassword: CharArray): UnlockResult
    fun currentSession(): VaultSession?
    fun lock(trigger: LockTrigger)
    val isLocked: Boolean
}

// 下发给键盘的最小短时授权，不含主密码
interface KeyboardAuthorization {
    fun grantForKeyboard(ttlMillis: Long): KeyboardGrant
}

sealed interface UnlockResult {
    data class Unlocked(val session: VaultSession) : UnlockResult
    data class WrongPassword(val attempts: Int) : UnlockResult // 不含可区分信息
    data class Error(val reason: UnlockError) : UnlockResult
}

enum class LockTrigger { Manual, IdleTimeout, Background, BiometricInvalidated }
```

## 5. 实现步骤

1. 口令输入：以 `CharArray` 接收主密码，传给 VAULT 派生后立即清空数组。
2. 解锁：`deriveKek` → `unwrapDek` → 构建 `VaultSession`（持有最小密钥材料）；解包失败返回 `WrongPassword`，记录尝试次数，不泄露区分信息。
3. 会话存储：只在内存中；不持久化 KEK 或 DEK；解包后可丢弃 KEK，按安全实现设计保留内容密钥。
4. 自动锁定：注册 `ProcessLifecycleOwner` 观察者，`ON_STOP` 时锁定或启动宽限计时；空闲超时计时器在每次使用时重置；提供显式锁定。
5. 锁定：清零会话密钥缓冲（`sodium_memzero`/`munlock`），清除引用，转入锁定态。
6. 键盘授权：签发短时 `KeyboardGrant`，作用域仅限读取被选中字段；键盘不接收主密码；授权在超时、锁定或切换应用后失效。
7. 高敏感重认证：高敏感字段显示或插入前可要求重新解锁或生物识别。
8. 失败处理：计数并可选退避；不区分口令错误与数据错误。

## 6. 边界条件与错误处理

| 场景 | 处理 |
| --- | --- |
| 错误主密码 | 返回 `WrongPassword`，保留当前任务上下文，不泄露区分信息。 |
| 应用进入后台 | `ON_STOP` 按策略锁定或启动宽限计时。 |
| 进程被销毁 | 会话丢失；返回时要求重新解锁。 |
| 空闲超时 | 锁定并清零。 |
| 新增生物识别注册 | 失效生物识别包装密钥（见 BIO），要求主密码。 |
| 键盘授权过期 | 键盘重新请求解锁。 |
| 配置变更（旋转） | `ProcessLifecycleOwner` 延迟 `ON_STOP`，避免误锁。 |

## 7. 安全与隐私要求

- 主密码以 `CharArray` 处理，派生后立即清空；不用 `String`，绝不记录（MASVS-AUTH、MASVS-STORAGE）。
- 会话密钥只驻留内存；锁定时清零，可行时 mlock（安全实现设计第 7 节）。
- 键盘只获得最小短时授权，绝不接收主密码（安全实现设计第 5.3 节）。
- 生物识别是便利机制，不替代主密码；其产生的会话与主密码会话等价（见 BIO）。
- 后台与空闲自动锁定，锁定即清零。
- 错误口令不返回可区分信息。
- 不记录主密码、密钥、字段值。
- JVM 无法保证不可变对象清零，因此一律用可清零缓冲并尽快清空，作为尽力而为。

## 8. 测试映射

| 测试/任务 | 关联 |
| --- | --- |
| TP-101 不登录创建并解锁 | LOCK-02 |
| TP-107 错误主密码失败且不破坏数据 | LOCK-02、LOCK-06 |
| TP-102 网络关闭时可用 | LOCK-02（离线路径） |
| 版本任务 MVP-002 | LOCK-01..LOCK-07 |
| 支撑 MVP-003（BIO）、MVP-005（KBD 会话） | LOCK-05、LOCK-07 |

验证方式：错误口令返回失败且不泄露区分信息；后台触发锁定；空闲超时锁定并清零；键盘授权不能比锁定存活更久；会话缓冲在可观测范围内被清零。

## 9. AI 任务拆分

| 任务 ID | 目的 | 输入 | 产出物 | 约束 | 验收证据 | 依赖 |
| --- | --- | --- | --- | --- | --- | --- |
| LOCK-01 | 口令输入与清理 | §5.1、§7 | `CharArray` 接入与清空 | 不用 String；派生后清空 | 口令数组用后被清空 | VAULT-02 |
| LOCK-02 | 解锁流程 | §5.2、VAULT | `unlockWithPassword`、`VaultSession` 构建 | 失败不泄露区分信息 | 正确解锁，错误返回 WrongPassword | LOCK-01、VAULT-03 |
| LOCK-03 | 会话模型与清零 | §5.3、§5.5 | 会话内存模型、`close` 清零 | 不持久化密钥 | 锁定后缓冲清零 | LOCK-02 |
| LOCK-04 | 自动锁定 | §5.4、ProcessLifecycleOwner | 后台与空闲锁定 | 配置变更不误锁 | 后台/超时触发锁定 | LOCK-03 |
| LOCK-05 | 键盘最小授权 | §5.6、安全实现设计 §5.3 | `KeyboardGrant` 短时授权 | 不含主密码；可失效 | 授权随锁定/超时失效 | LOCK-03 |
| LOCK-06 | 失败计数与不可区分错误 | §5.8、§6 | 计数与退避 | 不区分口令与数据错误 | 多次失败被计数且不泄露 | LOCK-02 |
| LOCK-07 | 高敏感重认证钩子 | §5.7、BIO | 重认证接口 | 对接生物识别或重解锁 | 高敏感操作可要求重认证 | LOCK-03 |

## 10. 待验证与不在范围

- 自动锁定超时默认值，待产品确认。
- Android 上 mlock 的可用性与限制，待评审；JVM 清零为尽力而为。
- 生物识别解锁内部：见 BIO 实现文档。
- 保险库加密内部：见 VAULT 实现文档。
- iOS 与 HarmonyOS 实现：放到 v1 阶段末尾。
