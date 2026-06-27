# 安全键盘输入法 实现文档

本文件描述 Android 安全键盘输入法（短码 KBD）的实现方式，结构遵循 [实现文档层总览](README.md) 第 3 节模板。它落实 v0.1 原型的 PROTO-003/004/005 与 v0.2 的 MVP-005。

## 1. 范围与目标

实现一个 Android 输入法：默认锁定，用户解锁后从本地保险库主动搜索条目、选择字段，并把被选中的值插入到其他应用的输入框中。

- 范围内：IME 注册、锁定态、解锁交接、搜索与条目/字段选择、`commitText` 插入、切换输入法、会话清退。
- 不在范围内：保险库加密核心（见 VAULT）、主密码与解锁会话内部（见 LOCK）、生物识别（见 BIO）、平台自动填充（兼容增强，单独记录）。
- v0.1 阶段可用最小模拟保险库；MVP-005 接入真实保险库，加密与解锁由 VAULT 和 LOCK 提供。

## 2. 依赖的设计与技术决策

- 交互路径与锁定/解锁规则：[核心交互设计](../product/interaction-design.md) 第 6 节。
- 字段层主动选择与字段属性：[信息架构设计](../product/information-architecture.md) 第 5.3 节。
- 键盘隐私规则（不收集输入、不记录字段值、避免截屏暴露）：[安全模型](../technical/security-model.md) 第 6 节。
- 键盘最小短时会话授权、防截屏、内存与日志卫生：[安全实现设计](../technical/security-implementation-design.md) 第 5.3、6、7 节。
- Android IME 能力与首平台选择：[平台能力设计](../product/platform-capability-design.md)。
- 版本任务：[v0.1 原型计划](../product/v0.1-prototype-plan.md)、[v0.2 离线 MVP 计划](../product/v0.2-mvp-plan.md)。

## 3. 平台与技术栈（Android 优先）

- 语言：Kotlin。
- 核心类：继承 `InputMethodService`。
- 注册：在 manifest 声明输入法服务，权限 `BIND_INPUT_METHOD`，intent-filter 匹配 `android.view.InputMethod`，并提供 `@xml/method` 元数据；可提供设置 Activity。
- 切换输入法：在输入法 XML 声明 `supportsSwitchingToNextInputMethod="true"`。

manifest 片段（按官方结构）：

```xml
<service
    android:name=".keyboard.SecureKeyboardService"
    android:label="@string/keyboard_label"
    android:permission="android.permission.BIND_INPUT_METHOD"
    android:exported="true">
    <intent-filter>
        <action android:name="android.view.InputMethod" />
    </intent-filter>
    <meta-data
        android:name="android.view.im"
        android:resource="@xml/method" />
</service>
```

## 4. 接口与数据结构

键盘只读取展示与插入所需的最小信息，不持有主密码，不缓存明文。

```kotlin
// 键盘状态
sealed interface KeyboardState {
    data object Locked : KeyboardState
    data class Unlocked(val session: VaultSession) : KeyboardState
}

// 面向键盘的只读保险库访问（实现见 VAULT / LOCK）
interface KeyboardVaultAccess {
    fun search(query: String): List<EntrySummary>
    fun fields(entryId: EntryId): List<FieldSummary>
    // 仅在用户点击插入时调用，返回短时持有、用后清零的值
    fun revealForInsert(fieldId: FieldId): SecretValue
}

// 列表项不含明文秘密
data class EntrySummary(val id: EntryId, val title: String, val tags: List<String>)
data class FieldSummary(
    val id: FieldId,
    val label: String,
    val kind: FieldKind,            // username/password/totp/text/...
    val sensitivity: Sensitivity,   // normal/sensitive/high
    val inputBehavior: InputBehavior // insert/copy/revealOnly
)

sealed interface InsertResult {
    data object Inserted : InsertResult
    data class Failed(val reason: InsertFailure) : InsertResult
}
```

插入使用当前 `InputConnection`：

```kotlin
val ic = currentInputConnection ?: return InsertResult.Failed(InsertFailure.NoConnection)
val ok = ic.commitText(secret.asCharSequence(), 1) // 1：光标移到插入文本之后
return if (ok) InsertResult.Inserted else InsertResult.Failed(InsertFailure.Rejected)
```

## 5. 实现步骤

1. 注册：添加 manifest 服务声明与 `res/xml/method.xml`（含 `supportsSwitchingToNextInputMethod`）。
2. 服务骨架：`SecureKeyboardService : InputMethodService()`，实现 `onCreateInputView`、`onStartInputView`、`onFinishInput`。
3. 候选区：`onCreateCandidatesView()` 返回 `null`，不提供词库或建议。
4. 锁定态视图：`onCreateInputView` 默认渲染锁定态，仅含产品标识、解锁入口、切换键盘入口；保险库不存在时提示打开主应用。
5. 解锁交接：调用 LOCK 提供的解锁流程，成功后得到 `VaultSession`，状态转 `Unlocked`。
6. 解锁态视图：聚焦搜索；展示条目列表与字段列表，字段显示标签与类型，不显示明文秘密。
7. 字段选择与插入：用户点击字段 → `revealForInsert` → `commitText(value, 1)` → 按返回值给出反馈 → 立即清零内存中的值。
8. 切换输入法：在合适时机调用 `switchToNextInputMethod(false)` 切到其他键盘。
9. 会话清退：`onFinishInput`、窗口隐藏或超时时清除会话与敏感缓冲。

## 6. 边界条件与错误处理

| 场景 | 处理 |
| --- | --- |
| `currentInputConnection` 为 null | 视为插入失败，保留字段选择，提供复制或重试。 |
| `commitText` 返回 false | 目标框拒绝插入，保留选择，提供复制或打开主应用。 |
| 目标为密码框（`TYPE_TEXT_VARIATION_PASSWORD`） | 正常插入；键盘 UI 仍不显示明文。 |
| 本地保险库不存在 | 锁定态提示打开主应用创建保险库。 |
| 解锁失败 | 留在键盘任务中，不跳转无关页面。 |
| IME 生命周期清退已解锁状态 | 重新要求解锁；使用短时会话令牌。 |
| 用户需要普通输入 | 提供切换系统键盘入口。 |

## 7. 安全与隐私要求

- 不收集、不上传用户键入文本：键盘只插入被选中的值，不捕获用户打字。
- 不提供默认在线词库，不基于敏感输入训练词库；候选区返回 null。
- 锁定态不展示条目标题、字段标签、最近使用或搜索历史。
- 即使目标是密码框，键盘 UI 与候选区也不显示明文秘密（遵循 Android 安全提示）。
- 不记录主密码、密钥、字段值、TOTP 种子和生成的验证码（MASVS-STORAGE）。
- 敏感键盘界面设置防截屏 FLAG_SECURE（MASVS-PLATFORM，安全实现设计第 6 节）。
- 键盘只获得读取被选中字段所需的最小、短时会话授权，绝不持有主密码（安全实现设计第 5.3 节）。
- 插入后尽快清零内存中的被选中值。
- 不依赖网络；不声明 INTERNET 权限。
- 不根据目标应用上下文自动选择凭据：主动选择是安全边界，平台自动填充是兼容增强不是主路径。

## 8. 测试映射

| 测试/任务 | 关联 |
| --- | --- |
| TP-001 选中用户名插入普通文本框 | KBD-05、KBD-06 |
| TP-002 选中密码插入密码框 | KBD-06 |
| TP-003 网络关闭时键盘可用 | KBD-08 |
| TP-004 默认不提供词库建议 | KBD-02、KBD-05 |
| TP-005 键盘不记录被选中的秘密 | KBD-08 |
| 版本任务 PROTO-003/004/005 | KBD-01..KBD-07 |
| 版本任务 MVP-005 | KBD-03、KBD-08 |

验证方式：instrumented 测试与 UI Automator 跨应用断言插入结果，日志扫描确认无字段值，离线以无 INTERNET 权限与抓包为准。

## 9. AI 任务拆分

| 任务 ID | 目的 | 输入 | 产出物 | 约束 | 验收证据 | 依赖 |
| --- | --- | --- | --- | --- | --- | --- |
| KBD-01 | IME 注册骨架 | 本文档 §3、§5.1-2 | manifest 服务声明、`method.xml`、`SecureKeyboardService` 骨架 | 权限 `BIND_INPUT_METHOD`；不声明 INTERNET | 键盘可在系统设置启用 | 无 |
| KBD-02 | 锁定态视图 | §5.4、交互 §6.3 | 锁定态布局与渲染 | 不展示条目/字段/历史 | 锁定态仅含标识、解锁、切换 | KBD-01 |
| KBD-03 | 解锁交接接入 | §5.5、LOCK 文档 | 解锁调用与 `VaultSession` 接入 | 不持有主密码，最小会话 | 解锁成功转 Unlocked，失败留任务 | KBD-01、LOCK |
| KBD-04 | 搜索与条目列表 | §4、§5.6、IA §5.2 | 搜索框与条目列表 | 搜索只在本地，不上传 | 可搜索并列出条目 | KBD-03 |
| KBD-05 | 字段列表与主动选择 | §4、IA §5.3、交互 §6.5 | 字段列表与选择动作 | 显示标签与类型，不显明文 | 仅用户点击字段才触发插入 | KBD-04 |
| KBD-06 | commitText 插入与兜底 | §4、§6 | 插入实现与失败处理 | 检查返回值；密码框不显明文 | 普通框只插入选中值；失败有兜底 | KBD-05 |
| KBD-07 | 切换输入法与会话清退 | §5.8-9 | 切换入口与清退逻辑 | 窗口隐藏/超时清零 | 可切系统键盘；退出清会话 | KBD-01 |
| KBD-08 | 离线与不记日志验证 | §7、§8 | 日志扫描脚本接入、离线核对 | 不记录秘密；无网络 | 抓包无流量；日志无字段值 | KBD-06 |

## 10. 待验证与不在范围

- 候选区完全禁用与最小提示的取舍，待原型实测确认。
- iOS 与 HarmonyOS 键盘实现：放到 v1 阶段末尾。
- 真实加密保险库与解锁内部：见 VAULT、LOCK 实现文档。
- 平台自动填充与 Credential Provider：兼容增强，单独记录，不进入本主线。
