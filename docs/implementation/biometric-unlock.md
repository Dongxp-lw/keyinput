# 生物识别解锁 实现文档

本文件描述生物识别解锁（短码 BIO）的实现方式，结构遵循 [实现文档层总览](README.md) 第 3 节模板。它落实 v0.2 的 MVP-003，用 Android Keystore 与 BiometricPrompt 包装本地解锁材料，产出与 [主密码与解锁会话](master-password-unlock.md) 等价的会话。

## 1. 范围与目标

实现一条便利的解锁路径：通过 BiometricPrompt（Class 3）触发硬件密钥库中的密钥，解开本地解锁材料，建立与主密码解锁等价的会话。

- 范围内：可用性检查、Keystore 密钥生成、启用流程、生物识别解锁流程、关闭与主密码兜底、注册变更失效处理。
- 不在范围内：保险库加密内部（见 VAULT）；会话生命周期（见 LOCK，BIO 产出同一会话）；界面视觉设计。
- 生物识别是便利机制，不替代主密码这一根秘密。

## 2. 依赖的设计与技术决策

- 生物识别解锁与平台密钥库绑定：[安全实现设计](../technical/security-implementation-design.md) 第 5.2、6 节。
- 会话与解锁管理（BIO 产出同一会话）：[主密码与解锁会话](master-password-unlock.md)。
- 解开的本地解锁材料来源：[保险库加密核心](vault-crypto-core.md)。
- 发布门禁相关项 MASVS-AUTH（加密绑定、注册失效、显式确认）：[v1.0 发布计划](../product/v1.0-release-plan.md) 第 4 节。
- 版本任务：[v0.2 离线 MVP 计划](../product/v0.2-mvp-plan.md)（MVP-003）。

## 3. 平台与技术栈（Android 优先）

- 语言：Kotlin。
- 库：`androidx.biometric`（`BiometricPrompt`、`BiometricPrompt.PromptInfo`、`BiometricManager`、`BiometricPrompt.CryptoObject`）。
- 密钥：AndroidKeyStore 生成的 AES 密钥，`setUserAuthenticationRequired(true)`、`setInvalidatedByBiometricEnrollment(true)`、认证类型 `BIOMETRIC_STRONG`（Class 3）；可选 StrongBox。
- 加密绑定：用 `CryptoObject(cipher)` 让生物识别成功在密码学上成为必要条件，而不是 UI 布尔值。

## 4. 接口与数据结构

```kotlin
interface BiometricUnlock {
    fun availability(): BiometricAvailability
    fun enable(session: VaultSession): EnableResult   // 在已解锁状态下启用
    fun unlock(host: FragmentActivity): UnlockResult  // 产出 LOCK 的 VaultSession
    fun disable()
}

enum class BiometricAvailability { Available, NoneEnrolled, NoHardware, Unavailable }

// 静态存储的生物识别信封（不含明文秘密）
data class BiometricEnvelope(
    val keystoreAlias: String,
    val wrappedUnlockKey: ByteArray, // 被 Keystore 密钥加密的生物识别解锁密钥
    val iv: ByteArray,               // Keystore 加密的 IV
    val dekBinding: ByteArray        // 被解锁密钥包装的 DEK 材料
)
```

Keystore 密钥生成（按官方结构，模式见第 10 节）：

```kotlin
KeyGenParameterSpec.Builder(alias, PURPOSE_ENCRYPT or PURPOSE_DECRYPT)
    .setBlockModes(/* 认证模式，见第 10 节 */)
    .setUserAuthenticationRequired(true)
    .setInvalidatedByBiometricEnrollment(true)
    // 可选 setIsStrongBoxBacked(true)
    .build()
```

提示框：`PromptInfo.Builder().setTitle(...).setSubtitle(...).setNegativeButtonText("使用主密码").setConfirmationRequired(true).build()`。

## 5. 实现步骤

1. 可用性：`BiometricManager.from(ctx).canAuthenticate(BIOMETRIC_STRONG)`，必要时用 `Settings.ACTION_BIOMETRIC_ENROLL` 引导注册。
2. 启用（已用主密码解锁时）：生成 Keystore 密钥；生成随机生物识别解锁密钥；用它包装 DEK 材料；以生物识别授权的 `CryptoObject` 用 Keystore 密钥加密该解锁密钥；保存 `BiometricEnvelope`。
3. 解锁：用 Keystore 密钥和存储的 IV 初始化解密 `Cipher`；`biometricPrompt.authenticate(promptInfo, CryptoObject(cipher))`；成功后 `result.cryptoObject.cipher` 解出解锁密钥 → 解包 DEK 材料 → 经 LOCK 的解锁管理建立同一会话。
4. 关闭：删除 Keystore 密钥与信封，回退为仅主密码。
5. 兜底：提示框“使用主密码”按钮 → 走 LOCK 主密码路径。
6. 注册失效：`setInvalidatedByBiometricEnrollment(true)`，新增生物识别注册后密钥失效，解锁抛 `KeyPermanentlyInvalidatedException` → 要求主密码并可重新启用。
7. 确认：敏感场景保持 `setConfirmationRequired(true)`。

## 6. 边界条件与错误处理

| 场景 | 处理 |
| --- | --- |
| 无硬件或未注册生物识别 | 标记不可用，仅主密码；可引导注册。 |
| 新增生物识别注册 | Keystore 密钥失效（`KeyPermanentlyInvalidatedException`）；要求主密码并可重新启用。 |
| 认证错误、失败或取消 | 留在当前任务，提供主密码路径。 |
| Keystore 密钥加载失败 | 回退到主密码。 |
| 设备凭据兜底 | 不与 `CryptoObject` 同用；本产品兜底为主密码，而非设备凭据。 |

## 7. 安全与隐私要求

- 生物识别是便利机制；主密码仍是根秘密，生物识别绝不替代它。
- 加密绑定：使用 `CryptoObject`，使生物识别成功成为密码学必要条件，防止仅靠 UI 布尔被绕过（MASVS-AUTH）。
- Keystore 密钥不可导出、绑定安全硬件（TEE/StrongBox）、要求用户认证（安全实现设计第 6 节）。
- 新增生物识别注册使密钥失效（`setInvalidatedByBiometricEnrollment(true)`）。
- 使用 `BIOMETRIC_STRONG`（Class 3）做加密绑定；敏感操作要求显式确认。
- 生物识别解锁产出与主密码等价的最小会话，沿用 LOCK 的清零。
- 不存储生物特征数据；只存加密信封。系统负责生物识别本身。
- 不记录主密码、密钥、解锁材料。

## 8. 测试映射

| 测试/任务 | 关联 |
| --- | --- |
| 版本任务 MVP-003 | BIO-01..BIO-06 |
| TP-107 主密码路径始终可用 | BIO-05 |
| 支撑 MVP-002（LOCK 会话）、MVP-005（KBD） | BIO-04 |

验证方式：生物识别解锁产出可用会话；关闭后回退主密码；新增注册后密钥失效并要求主密码；加密绑定使绕过 UI 无法解锁；主密码路径始终可用。

## 9. AI 任务拆分

| 任务 ID | 目的 | 输入 | 产出物 | 约束 | 验收证据 | 依赖 |
| --- | --- | --- | --- | --- | --- | --- |
| BIO-01 | 可用性检查 | §5.1 | `availability()` 与引导注册 | 用 BIOMETRIC_STRONG 判定 | 各状态正确返回 | 无 |
| BIO-02 | Keystore 密钥生成 | §3、§4 | 受认证保护的 AES 密钥 | user-auth-required；注册失效 | 密钥按约束生成 | BIO-01 |
| BIO-03 | 启用流程 | §5.2、VAULT、LOCK | 生成解锁密钥、包装 DEK、存信封 | 仅在已解锁时启用 | 信封生成且不含明文 | BIO-02、LOCK-02 |
| BIO-04 | 解锁流程 | §5.3 | `CryptoObject` + BiometricPrompt → 会话 | 加密绑定；产出同一会话 | 生物识别成功后会话可用 | BIO-03 |
| BIO-05 | 关闭与主密码兜底 | §5.4-5 | 关闭与回退 | 主密码始终可用 | 关闭后仅主密码可解锁 | BIO-04 |
| BIO-06 | 注册失效处理 | §5.6、§6 | 失效捕获与重新启用 | 失效后要求主密码 | 新注册后旧密钥失效 | BIO-04 |

## 10. 待验证与不在范围

- Keystore 包装的具体密码模式：官方示例用 AES/CBC/PKCS7，本产品倾向认证加密（如 AES-GCM），最终模式与信封字段待评审。
- StrongBox 偏好与可用性，待评审。
- 会话生命周期与清零内部：见 LOCK；保险库加密内部：见 VAULT。
- 平台自动填充与 Credential Provider：兼容增强，单独记录。
- iOS 与 HarmonyOS 实现：放到 v1 阶段末尾。
