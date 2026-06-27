# 平台能力设计

本文档定义 Private Input Vault 在 Android、iOS 和 HarmonyOS 上的能力边界、平台限制、验证任务和版本取舍。它承接 [产品决策记录：主动选择式私密输入](product-decision-record.md)、[信息架构设计](information-architecture.md) 和 [核心交互设计](interaction-design.md)。

本文档不写平台代码，也不最终确定加密算法参数。它的目标是回答：各平台能否支撑主动选择式私密输入，哪些能力可以作为兼容增强，哪些限制必须在产品中明确呈现。

## 1. 平台能力设计目标

平台能力设计必须服务产品基线：本地保险库是信任中心，用户主动选择是核心交互，安全键盘是主动私密输入的关键入口，平台凭据和自动填充是标准登录场景的兼容增强。

设计目标：

- 确认各平台是否支持安全键盘或等价输入法能力。
- 确认各平台的凭据、自动填充和 TOTP 填充能力可以怎样增强标准登录场景。
- 确认本地解锁、生物识别、设备凭据和安全存储能力的可用性和限制。
- 确认剪贴板、截图保护、共享容器和扩展通信的风险边界。
- 为 v0.1 原型选择首个平台和最小验证范围。
- 为平台不支持或受限的场景定义清楚兜底路径。

## 2. 调研来源与可信度

本轮调研优先使用官方文档。

| 平台 | 已查询资料 | 结论可信度 |
| --- | --- | --- |
| Android | IME、Autofill、IME inline suggestions、Credential Provider、BiometricPrompt、Android Keystore、剪贴板、FLAG_SECURE/HIDE_OVERLAY_WINDOWS。 | 高。官方资料覆盖关键能力。 |
| iOS | Custom Keyboard、Password AutoFill、OTP AutoFill、LocalAuthentication、Keychain、UIPasteboard、App Groups。 | 高。官方资料覆盖关键能力和限制。 |
| HarmonyOS | User Authentication Kit、Asset Store Kit、window API、InputMethodKit 页面。 | 中。认证和安全存储资料明确；InputMethodKit 页面抓取内容不足，必须真机/SDK spike 验证。 |

调研结论必须按以下规则使用：

- 官方明确说明的能力可以进入平台能力矩阵。
- 抓取不完整或未验证的能力必须标注为“待验证”。
- 即使平台提供自动填充或凭据能力，也不能把它写成产品主路径。

## 3. 总体能力矩阵

| 能力 | Android | iOS | HarmonyOS | 产品含义 |
| --- | --- | --- | --- | --- |
| 主应用 | 可行。 | 可行。 | 可行。 | 三平台都应支持本地保险库管理。 |
| 安全键盘/输入法 | 可行，基于 Android IME。 | 可行但强限制，基于 Keyboard Extension。 | 待验证，需确认 InputMethodKit 能力。 | Android 最适合做 v0.1 输入原型。 |
| 密码/凭据自动填充 | 可行，Autofill API 26+，Credential Manager Provider Android 14+。 | 可行，AutoFill Credential Provider。 | 待验证。 | 兼容增强，不是主路径。 |
| TOTP 自动填充 | 可通过平台能力评估，Android 需具体验证。 | 可行，Credential Provider 可声明提供 OTP。 | 待验证。 | 标准验证码场景增强。 |
| 生物识别/设备凭据 | 可行，BiometricPrompt/DEVICE_CREDENTIAL。 | 可行，LocalAuthentication。 | 可行，User Authentication Kit。 | 便利解锁和重新认证，不替代主密码。 |
| 本地安全存储 | 可行，Android Keystore。 | 可行，Keychain。 | 可行，Asset Store Kit/HUKS 方向。 | 用于包装或保护本地解锁材料。 |
| 剪贴板兜底 | 可行但有风险和版本差异。 | 可行但有用户通知、localOnly/expiration 能力。 | 待验证。 | 只能作为兜底，需自动清理和风险说明。 |
| 截图/录屏保护 | 可行但不完全可靠，FLAG_SECURE/HIDE_OVERLAY_WINDOWS。 | 需平台方案另行验证。 | window API 待验证。 | 保护敏感界面，但不能承诺绝对防录屏。 |
| 跨组件共享状态 | 可通过应用内部存储/服务设计。 | 可用 App Groups，但会增加信任和数据边界责任。 | 待验证。 | 仅共享最小授权状态。 |

## 4. Android 能力设计

### 4.1 安全键盘

Android 官方 IME 框架支持第三方输入法。实现方式是声明继承 `InputMethodService` 的服务，并通过系统设置启用。IME 可以通过 `InputConnection.commitText()` 向当前输入框提交文本。

已确认能力：

- 可以实现系统级输入法。
- 可以根据输入框 `EditorInfo.inputType` 调整 UI。
- 可以向当前输入框提交文本。
- 可以提供输入法设置页。
- 可以提供切换到下一个输入法的入口。
- Android 11 起，IME 可以支持 inline autofill suggestions。

限制与风险：

- 用户必须在系统设置中启用并切换到该输入法。
- 同一时间只有一个 IME 生效。
- IME 生命周期可能频繁创建和销毁，需要短时授权和快速恢复。
- Android 文档提醒密码字段需要隐藏密码，不应在 UI 或候选区明文展示。
- 输入法应保留切换到其他输入法的能力。

产品策略：

- Android 是 v0.1 首选平台。
- v0.1 应验证安全键盘锁定态、解锁态、搜索条目、字段选择和 `commitText()` 插入。
- v0.1 不要求实现完整 Autofill 或 Credential Provider，但要记录与后续兼容层的关系。

### 4.2 Autofill 与 IME inline suggestions

Android Autofill Framework 从 Android 8.0 API 26 起可用。密码管理器可以作为 Autofill Service。Android 11 起，IME 和 Autofill Service 可以配合展示 inline suggestions；文档说明建议内容在用户选择前不会暴露给 IME。

已确认能力：

- Autofill Service 可为其他应用填充数据。
- IME inline suggestions 可在键盘候选区展示自动填充建议。
- 如果 IME 或 Autofill Service 不支持 inline suggestions，系统可回退到菜单展示。

限制与风险：

- Autofill 的可用性取决于目标应用字段、系统版本、用户设置和服务启用状态。
- Android 文档提示 IME 不保证一定使用 Autofill Service 提供的 inline suggestions。
- Autofill 适合标准登录和表单字段，不覆盖全部私密输入场景。

产品策略：

- Autofill 是标准登录场景的兼容增强。
- 不应把 Autofill 候选写成目标可信判断。
- Autofill 失败时必须回到安全键盘或复制路径。

### 4.3 Credential Manager Provider

Android Credential Manager Provider 面向 Android 14+，支持密码和 Passkey 等凭据类型。官方文档描述了查询阶段和选择阶段：系统先绑定 provider 获取候选，用户选择后通过 `PendingIntent` 进入 provider activity 完成认证和返回结果。

已确认能力：

- 可声明 `CredentialProviderService`。
- 可声明支持密码和 Passkey 能力。
- 可在锁定状态下返回认证动作。
- 可提供 provider 设置入口。
- 可通过系统设置启用凭据提供方。

限制与风险：

- Android 14+ 才是 Credential Provider 主要目标。
- 需要处理不同请求类型、调用方信息和浏览器代理来源。
- Passkey 私钥必须加密保存。
- 该能力只能覆盖标准凭据场景，不覆盖全部私密字段。

产品策略：

- v0.1 只调研，不作为原型阻断项。
- v0.2 或 v1.0 前评估是否加入最小密码提供方能力。
- Passkey 支持应晚于本地保险库和安全键盘稳定之后。

### 4.4 生物识别与设备凭据

Android BiometricPrompt 支持 `BIOMETRIC_STRONG`、`BIOMETRIC_WEAK` 和 `DEVICE_CREDENTIAL`。官方建议初次登录可用 Credential Manager，后续重新授权可用 BiometricPrompt 或 Credential Manager。Android Keystore 可将密钥使用绑定到用户认证。

已确认能力：

- 可检测设备是否支持指定认证方式。
- 可调用系统认证 UI。
- 可知道用户使用的是生物识别还是设备凭据。
- 可把密钥使用限制到最近认证或每次操作认证。

限制与风险：

- Android 10 及以下对某些 `DEVICE_CREDENTIAL` 组合支持有限。
- 强生物识别和设备凭据的组合策略需要按 API 版本处理。
- 生物识别是便利机制，不应替代主密码。

产品策略：

- v0.2 支持生物识别/设备凭据解锁。
- 高敏感字段可使用重新认证策略。
- 认证提示必须说明正在授权的动作。

### 4.5 Android Keystore

Android Keystore 可让密钥材料更难被提取，并支持硬件绑定、StrongBox、用途限制和用户认证限制。

已确认能力：

- Key material 可保持不可导出。
- 可绑定到 TEE 或 StrongBox，取决于设备支持。
- 可限制密钥用途和认证要求。
- 可设置认证后有效期或每次操作认证。

限制与风险：

- StrongBox 并非所有设备支持，且性能和并发能力有限。
- Keystore 不适合直接保存整份保险库内容。
- 密码学操作不应阻塞 UI 主线程。

产品策略：

- 使用 Keystore 包装本地解锁密钥或保护设备绑定材料。
- 保险库内容仍由共享核心加密格式管理。
- StrongBox 可作为高安全选项，不作为默认要求。

### 4.6 剪贴板与屏幕保护

Android 剪贴板存在版本差异。Android 10 之前后台应用可能访问剪贴板；Android 12 起访问剪贴板有提示；Android 支持敏感剪贴板标记；Android 13 起系统会自动清理剪贴板内容。`FLAG_SECURE` 可降低截图和非安全显示风险，`HIDE_OVERLAY_WINDOWS` 可降低覆盖攻击风险。

产品策略：

- 剪贴板仅作为兜底路径。
- 复制敏感字段时设置敏感标记。
- 尽可能自动清理剪贴板，并说明平台限制。
- 敏感界面使用截图保护，但不承诺绝对防录屏。

## 5. iOS 能力设计

### 5.1 自定义键盘

iOS Custom Keyboard 可作为键盘扩展插入文本，但官方文档明确限制较多。

已确认能力：

- 可创建 Keyboard Extension。
- 可通过 `textDocumentProxy.insertText` 插入文本。
- 必须提供切换到其他键盘的入口。
- 默认无网络访问，默认不能与 containing app 共享容器。
- 若启用 `RequestsOpenAccess`，可获得共享容器、网络和 UIPasteboard 等能力，但用户信任责任显著增加。

限制与风险：

- secure text input 中，系统会临时替换为系统键盘。
- phone pad 类输入对象中，系统会替换为标准键盘。
- 应用开发者可以完全拒绝第三方键盘。
- 自定义键盘不能选择文本，也不能访问宿主应用的编辑菜单。
- open access 会让用户知道键入内容可被键盘开发者访问，信任成本高。

产品策略：

- iOS 安全键盘必须被设计为“受限但有价值”的主动输入入口。
- iOS 上不能承诺密码字段普遍可用键盘插入。
- iOS 上复制、主应用和 AutoFill Provider 是重要兜底。
- 是否启用 open access 必须单独安全评审。

### 5.2 Password AutoFill 与 Credential Provider

iOS Password AutoFill 支持第三方密码管理器通过 AutoFill Credential Provider Extension 提供凭据。Apple 文档建议应用通过 associated domains 和正确的 text content type 提升 AutoFill 体验。

已确认能力：

- 第三方密码管理器可通过 Credential Provider Extension 接入 Password AutoFill。
- 系统可在兼容的用户名和密码字段中提供候选。
- 用户访问凭据前需要 Face ID 或 Touch ID 等认证。
- 与 associated domains 和字段语义有关。

限制与风险：

- AutoFill 只覆盖兼容登录和网页/应用字段。
- 它不是非登录私密字段的通用输入方案。
- 关联域和字段标注由目标应用/网页影响，无法完全由本产品控制。

产品策略：

- iOS 标准登录场景优先评估 AutoFill Provider。
- 安全键盘负责非标准私密输入和平台限制场景。
- AutoFill 候选不能被表述成目标可信判断。

### 5.3 TOTP AutoFill

Apple AuthenticationServices 支持 Credential Provider Extension 提供 OTP/TOTP。扩展可声明 `ProvidesOneTimeCodes`，系统可请求 OTP；若需要用户交互，扩展可要求展示界面。

已确认能力：

- 可声明扩展提供一次性验证码。
- 可响应系统 OTP 请求。
- 可提供 OTP 列表供用户选择。
- 可在需要交互时要求系统展示选择界面。

限制与风险：

- 仅覆盖系统识别为 OTP 的场景。
- TOTP 种子必须仍由本地保险库加密保护。
- 生成码是临时值，不得进入日志或同步元数据。

产品策略：

- iOS TOTP AutoFill 是 v1.0 前值得验证的增强能力。
- 免费离线版仍应保留安全键盘 TOTP 输入能力。

### 5.4 LocalAuthentication 与 Keychain

LocalAuthentication 让应用调用 Face ID、Touch ID、Optic ID 或设备密码等本地认证。Apple 文档说明应用不会接触底层认证数据，只获得成功/失败结果。Keychain Services 用于安全存储小型秘密。

已确认能力：

- 可用系统认证保护敏感操作。
- 可用 Keychain 存储小型秘密、密钥和凭据材料。
- 可把 Keychain 项与 Face ID/Touch ID 访问控制结合。

限制与风险：

- LocalAuthentication 不给应用提供生物特征本身。
- Keychain 适合小型秘密，不是整个保险库文件的替代存储。
- 生物识别仍是便利机制，不替代主密码。

产品策略：

- 使用 Keychain 包装本地解锁材料。
- 使用 LocalAuthentication 做便利解锁和高敏感重新认证。
- 主密码仍是根秘密。

### 5.5 UIPasteboard 与 App Groups

UIPasteboard 可在应用间复制粘贴；iOS 14 起，读取其他应用写入的通用剪贴板且缺少明确用户意图时，系统会通知用户。UIPasteboard 支持 localOnly 和 expirationDate 选项。App Groups 可让同一开发者的 app 和 extension 访问共享容器。

产品策略：

- 剪贴板是兜底能力，应设置本地限制和过期时间。
- 不主动读取剪贴板秘密。
- App Groups 只共享最小必要状态，不共享长期明文保险库。
- 共享容器策略必须与 open access 风险一起评审。

## 6. HarmonyOS 能力设计

### 6.1 输入法能力

本轮查询尝试访问 InputMethodKit 官方页面，但抓取内容不足，无法可靠确认具体 API 能力边界。

当前状态：待验证。

必须验证：

- 是否允许第三方输入法注册和系统级切换。
- 输入法是否能向当前输入框提交文本。
- 是否能识别输入框类型或敏感字段限制。
- 输入法是否能与主应用共享状态。
- 输入法权限、签名、分发渠道是否有额外限制。
- 是否存在类似系统自动填充或凭据提供方能力。

产品策略：

- HarmonyOS 不作为 v0.1 首选平台。
- 进入承诺发布日期前必须完成真机/SDK spike。
- 文档和产品中不得提前承诺 HarmonyOS 安全键盘覆盖范围。

### 6.2 用户认证

HarmonyOS User Authentication Kit 官方资料明确支持系统级用户身份认证，统一调用锁屏口令、人脸和指纹，并支持认证可信等级、认证结果复用、系统级认证界面和凭据变化感知。

已确认能力：

- 可调用系统级认证控件。
- 支持锁屏口令、人脸、指纹组合认证。
- 可指定期望认证可信等级。
- 可在一定时间内复用认证结果，最长 5 分钟。
- 不允许三方应用在后台发起身份认证请求。

产品策略：

- 可用于便利解锁和高敏感重新认证。
- 认证可信等级应和字段敏感级别对应。
- 后台自动认证不可作为设计路径。

### 6.3 安全存储

HarmonyOS Asset Store Kit 官方资料说明它用于短敏感数据安全存储，底层依赖通用密钥库系统，关键资产加解密使用 AES256-GCM，并支持属主访问控制、群组访问控制、锁屏状态访问控制、锁屏密码设置状态访问控制和用户认证访问控制。

已确认能力：

- 可存储账号/密码、Token 等短敏感数据。
- 访问控制绑定应用属主身份。
- 可配置群组共享。
- 可设置锁屏状态和用户认证访问控制。
- 支持认证有效期，最长可设置 10 分钟。

限制与风险：

- Asset Store Kit 面向短敏感数据，不适合直接存储完整保险库。
- 批量查询受 IPC 缓冲区限制，建议超过 40 条分批查询。
- 群组共享和持久属性有额外限制。

产品策略：

- Asset Store 可用于保护本地解锁材料或小型关键资产。
- 完整保险库仍应由共享核心加密格式管理。
- 群组共享需谨慎评估，不默认开放。

### 6.4 剪贴板与窗口保护

本轮查询的 HarmonyOS Clipboard 和 window API 页面抓取内容不足，无法可靠确认敏感剪贴板、过期、local only、截图保护或隐私窗口的完整能力。

当前状态：待验证。

必须验证：

- 是否支持设置剪贴板敏感标记。
- 是否支持剪贴板自动过期或本地设备限制。
- 是否支持清空剪贴板。
- 是否支持防截图、防录屏或隐私窗口标记。
- 这些能力在模拟器和真机上的差异。

## 7. 首平台与版本取舍

### 7.1 v0.1 首平台建议

首平台建议：Android。

理由：

- Android IME 能力官方资料明确。
- 可直接验证安全键盘主动选择和字段插入。
- Android 同时具备 Autofill、Credential Provider、BiometricPrompt 和 Keystore 的后续演进路径。
- iOS 键盘限制会干扰主动输入原型判断。
- HarmonyOS 输入法能力仍需独立验证。

### 7.2 v0.1 必须验证

Android v0.1 必须验证：

- IME 注册、启用和切换。
- 锁定态不泄露条目和字段。
- 主应用解锁后键盘获得短时授权。
- 搜索条目和字段。
- `commitText()` 插入用户名、密码、自定义字段、备注。
- 普通文本框、密码框、浏览器、WebView 中的行为。
- 网络关闭时完整输入路径可用。
- 日志不输出字段值。

### 7.3 v0.1 应并行调研

- Android Autofill Service 最小可行性。
- Android Credential Manager Provider 最小可行性和版本门槛。
- iOS Keyboard Extension 在安全字段中的实际替换行为。
- iOS AutoFill Credential Provider 和 TOTP AutoFill 的接入门槛。
- HarmonyOS InputMethodKit 真机/SDK 能力。

### 7.4 v0.2 建议进入

- Android 本地保险库完整闭环。
- Android 生物识别/设备凭据便利解锁。
- Android 加密导出/导入。
- Android 剪贴板兜底和自动清理。
- Android 平台兼容状态页。
- 首个平台 Autofill 或 Credential Provider 的取舍结论。

## 8. 平台限制兜底策略

| 限制 | 用户看到的路径 | 产品处理 |
| --- | --- | --- |
| 第三方键盘不能插入 | 打开主应用、复制字段、切换系统键盘。 | 明确平台限制，不归咎用户。 |
| 自动填充没有候选 | 打开安全键盘、搜索保险库。 | 不自动猜测凭据。 |
| 凭据提供方未启用 | 跳转系统设置或继续本地使用。 | 不要求登录。 |
| 生物识别不可用 | 使用主密码或设备凭据。 | 不阻断本地保险库。 |
| 剪贴板不可自动清理 | 告知风险，建议手动清除。 | 不承诺平台不支持的能力。 |
| 截图保护不可靠 | 提供最佳努力保护。 | 不承诺绝对防录屏。 |
| HarmonyOS 能力未验证 | 不承诺发布日期。 | 完成 spike 后再进入版本计划。 |

## 9. 平台能力验收清单

每个平台进入实现前，必须回答以下问题：

- 是否能在不登录、不联网时创建和解锁本地保险库？
- 是否能实现用户主动选择字段并输入？
- 是否存在会阻断安全键盘的系统限制？
- 是否能提供清楚的键盘切换或兜底路径？
- 是否能用系统认证做便利解锁和重新认证？
- 是否能用平台安全存储保护本地解锁材料？
- 是否能安全处理剪贴板兜底？
- 是否能保护敏感界面截图或至少说明限制？
- 是否能接入平台凭据/自动填充作为兼容增强？
- 是否会把任何平台推荐或匹配误表达为目标可信？

## 10. 后续任务

完成本文档后，下一步建议是“v0.1 原型计划”。它应把 Android 首平台验证拆成可执行任务，并定义每个任务的验收证据。