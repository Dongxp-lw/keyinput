# 剪贴板兜底 实现文档

本文件描述剪贴板兜底（短码 CLIP）的实现方式，结构遵循 [实现文档层总览](README.md) 第 3 节模板。它落实 v0.2 的 MVP-009，在 [安全键盘输入法](secure-keyboard-ime.md) 插入不可用时提供复制路径。

本文档服从 [产品决策记录](../product/product-decision-record.md) 基线：复制是用户主动选择的延伸，不在后台自动复制；它不改变主动选择主路径，平台凭据与自动填充是兼容增强而非主路径。

本文档就 Android 剪贴板平台行为做了权威核对（Android 官方 copy-paste 指南与 `ClipDescription` 参考）。关键结论：`EXTRA_IS_SENSITIVE` 自 API 33（Android 13）起，用于让复制内容不出现在系统复制预览中，但它只是渲染提示、不增加额外安全性，不替代清除；Android 13+ 复制时系统会显示带预览的确认，Android 12+ 读取剪贴板会触发系统提示；平台没有内置的超时清除接口，需由应用自行调度清除。

## 1. 范围与目标

实现一个把字段值复制到系统剪贴板的兜底能力，作为键盘插入或平台填充不可用时的恢复路径，并尽量降低剪贴板暴露风险。

- 范围内：用户主动触发的复制、配置化自动清除、平台支持时标记敏感、风险说明、不记录复制值。
- 不在范围内：键盘插入主路径（见 KBD）；字段与敏感级别模型（见 ENTRY）；粘贴/读取他人剪贴板内容（不做）。
- 兜底能力必须有自动清除和风险说明（信息架构第 6.5 节）。

## 2. 依赖的设计与技术决策

- 剪贴板限时清除、平台支持时标记敏感、不记录秘密：[安全实现设计](../technical/security-implementation-design.md) 第 7 节。
- 兜底定位（平台填充失败可转安全键盘或复制）：[核心交互设计](../product/interaction-design.md) 第 2.2、3.3 节。
- 剪贴板能力必须有自动清除与风险说明：[信息架构设计](../product/information-architecture.md) 第 6.5 节。
- 复制是允许的用户主动动作、按字段敏感级别处理：[条目与字段模型](entry-field-model.md) 第 6 节、[核心交互设计](../product/interaction-design.md) 第 3.1 节。
- 会话结束（锁定、超时、进入后台）时的清除时机：[主密码与解锁会话](master-password-unlock.md)。
- 版本任务与验收证据：[v0.2 离线 MVP 计划](../product/v0.2-mvp-plan.md)（MVP-009）；测试 TP-109。

## 3. 平台与技术栈（Android 优先）

- 语言：Kotlin。
- 剪贴板：`ClipboardManager`（`getSystemService(CLIPBOARD_SERVICE)`）、`ClipData.newPlainText`、`setPrimaryClip`、`clearPrimaryClip`。
- 敏感标记：Android 13（API 33）及以上在 `ClipDescription` 上加布尔 extra `EXTRA_IS_SENSITIVE`；低 SDK 用字符串字面量 `"android.content.extra.IS_SENSITIVE"`，官方要求所有 App 不论目标 API 都应设置。
- 自动清除由应用调度（平台无内置超时清除），清除时仅处理本应用仍拥有的主内容。

## 4. 接口与数据结构

```kotlin
data class ClipboardPolicy(
    val autoClearSeconds: Int,  // 配置化自动清除超时；0 表示不自动清除（需 UI 明确风险）
    val markSensitive: Boolean  // 平台支持时标记敏感（Android 13+）
)

interface SecureClipboard {
    // value 按秘密处理；复制后按策略调度自动清除
    fun copy(value: CharSequence, policy: ClipboardPolicy)
    // 仅清除本应用写入且未被其他应用覆盖的主内容
    fun clearIfOwn()
}
```

## 5. 实现步骤

1. 仅在用户对已选中字段显式触发复制时调用（主动选择延伸），不在后台自动复制。
2. 用 `ClipData.newPlainText` 构造剪贴，标签不含敏感信息（用空标签或通用标签，绝不放字段值或条目名）。
3. 平台支持时（API 33+）在 `ClipDescription` 上加布尔 extra `EXTRA_IS_SENSITIVE=true`，使复制确认预览不显示明文；低 SDK 用字符串字面量。
4. 调用 `setPrimaryClip(clip)`。
5. 按 `autoClearSeconds` 调度延时清除：到时若主内容仍是本应用写入的同一内容，则 `clearPrimaryClip()` 或覆盖为无害空内容；不清除他人内容。
6. 在锁定、超时、进入后台等会话结束时机也调用 `clearIfOwn()`，与解锁会话卫生一致。
7. 不记录复制值；UI 仅提示“已复制（将在 N 秒后清除）”，提示文本不含字段值，并说明剪贴板可被其他应用读取的风险。

## 6. 边界条件与错误处理

| 场景 | 处理 |
| --- | --- |
| 平台不支持敏感标记（API < 33） | 仍复制并调度清除；标记为不可用，UI 给出风险说明。 |
| 自动清除到时但剪贴板已被其他应用覆盖 | 不清除他人内容；仅清除本应用仍拥有的主内容。 |
| 进程在清除定时器触发前被回收 | 自动清除可能不执行（平台限制，待验证）；下次解锁或启动时尽力清除并提示风险。 |
| 后台访问剪贴板受限（Android 10+） | 清除在前台/会话有效时进行；后台清除可能不可用（待验证）。 |
| Android 12+ 读取剪贴板触发系统提示 | 本功能只写不读他人内容，读取仅限本应用数据，避免触发“已粘贴”提示。 |
| autoClearSeconds 配置为 0 或关闭 | 不自动清除；UI 必须明确告知剪贴板不会自动清除的风险。 |

## 7. 安全与隐私要求

- 复制值按秘密处理，使用后尽快从剪贴板清除（安全实现设计第 7 节）。
- 平台支持时标记敏感（Android 13+ `EXTRA_IS_SENSITIVE`），避免明文出现在系统复制预览；注意这是渲染提示、不增加额外安全性，不替代清除（已核对 Android 文档）。
- 配置化自动清除：默认开启，超时值可配（默认值待评审）。
- 不记录复制值、字段值、TOTP 种子、条目名；剪贴板标签不含敏感信息（MASVS-PRIVACY、MASVS-STORAGE）。
- 复制是用户主动选择的延伸，不在后台自动复制，不改变主动选择主路径。
- 兜底能力必须有风险说明：剪贴板内容可被其他应用读取（信息架构第 6.5 节）。

## 8. 测试映射

| 测试/任务 | 关联 |
| --- | --- |
| 版本任务 MVP-009 | CLIP-01..CLIP-04 |
| TP-109 复制后在配置超时内自动清除（平台允许时） | CLIP-03 |
| 敏感标记在 Android 13+ 生效（预览不显示明文） | CLIP-02 |
| 复制值不写日志 | CLIP-04 |
| 兜底路径有文档说明 | 本文档第 1、5 节 |

验证方式：手动或 instrumentation 验证复制后在配置超时内、平台允许时主内容被清除；在 Android 13+ 上确认复制确认预览不显示明文；日志扫描确认无复制值；兜底路径在文档中有说明。

## 9. AI 任务拆分

| 任务 ID | 目的 | 输入 | 产出物 | 约束 | 验收证据 | 依赖 |
| --- | --- | --- | --- | --- | --- | --- |
| CLIP-01 | 安全复制封装 | §4、§5.1-4 | `SecureClipboard.copy`、`ClipData` 构造 | 标签不含敏感信息；仅主动触发 | 复制成功且标签无敏感信息 | 无 |
| CLIP-02 | 敏感标记 | §5.3、Android 文档 | `EXTRA_IS_SENSITIVE`（API 33+ 与低 SDK 字面量） | 渲染提示，非安全控制 | API 33+ 预览不显示明文 | CLIP-01 |
| CLIP-03 | 配置化自动清除 | §5.5-6、§6 | 延时清除、`clearIfOwn`、会话结束清除 | 不清除他人内容 | 超时后本应用内容被清 | CLIP-01 |
| CLIP-04 | 卫生与风险说明 | §7 | 不记日志、UI 风险提示 | 不记录复制值 | 日志无复制值；UI 有风险说明 | CLIP-01 |

## 10. 待验证与不在范围

- 自动清除在进程被回收、后台剪贴板访问受限（Android 10+）下的可靠性，待验证。
- `clearPrimaryClip()` 的最低 API 与各厂商系统的剪贴板历史行为，待验证。
- 默认自动清除超时值（如 30 或 60 秒），待评审。
- iOS（`UIPasteboard` 的过期与本地范围选项）与 HarmonyOS 剪贴板能力：放到 v1 阶段末尾，待验证。
- 键盘插入主路径见 KBD；复制仅作兜底，不作为主输入方式。
