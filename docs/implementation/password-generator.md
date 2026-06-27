# 密码生成器 实现文档

本文件描述密码生成器（短码 GEN）的实现方式，结构遵循 [实现文档层总览](README.md) 第 3 节模板。它落实 v0.2 的 MVP-006。

本文档的密码学事实复用本会话已有调研：CSPRNG 用加密安全随机源（核心层 Rust 的 `getrandom`/RustCrypto；OWASP 加密存储指南的同类 Java 选项为 `SecureRandom`），取模偏置规避见 RFC 4226 对截断取模偏置的说明，整体方向见 [安全实现设计](../technical/security-implementation-design.md) 第 2.3 节与 [v0.2 离线 MVP 计划](../product/v0.2-mvp-plan.md) 第 3.2 节，无需新调研。

## 1. 范围与目标

实现一个基于 CSPRNG 的密码生成器，帮助用户创建强密码：可配置长度与字符集，映射无偏置，可选排除易混淆字符。

- 范围内：CSPRNG 生成、可配置长度与字符类、避免取模偏置、可选排除易混淆字符、不记录生成值。
- 不在范围内：生成值的存储（见 ENTRY、VAULT）；口令短语/词表模式与“必需元素”规则（候选，见第 11 节与 [灵感库](../inspiration.md)）。

## 2. 依赖的设计与技术决策

- CSPRNG 与避免偏置：[安全实现设计](../technical/security-implementation-design.md) 第 2.3 节。
- 可配置长度与字符集、拒绝采样、排除易混淆字符、不记日志：[v0.2 离线 MVP 计划](../product/v0.2-mvp-plan.md) 第 3.2 节。
- 生成器入口与标签：[核心交互设计](../product/interaction-design.md) 第 3.1 节（生成属于允许的自动化）。
- 版本任务：v0.2 MVP-006。

## 3. 平台与技术栈（Android 优先）

- 实现语言：**Rust**（核心层，见 [模块架构](../technical/module-architecture.md)）；下文接口为逻辑示意。
- CSPRNG：Rust 的加密安全随机（`getrandom`/RustCrypto），不使用非加密安全随机。
- 偏置规避：把随机字节映射到字符集时采用拒绝采样，丢弃落在截断高区间的值，避免取模偏置。

## 4. 接口与数据结构

```kotlin
data class PasswordPolicy(
    val length: Int,
    val useLowercase: Boolean,
    val useUppercase: Boolean,
    val useDigits: Boolean,
    val useSymbols: Boolean,
    val excludeAmbiguous: Boolean
    // 候选：val minPerClass: Map<CharClass, Int>  // 必需元素，见灵感库，待评审
)

interface PasswordGenerator {
    // 返回 CharArray 而非 String，便于清零
    fun generate(policy: PasswordPolicy): CharArray
}
```

## 5. 实现步骤

1. 按策略组装允许字符集（小写、大写、数字、符号的并集）；如启用排除易混淆，剔除如 `O 0 l 1 I` 等。
2. 校验：至少选中一个字符类；长度不低于下限。
3. 用 `SecureRandom` 取随机值，按拒绝采样映射到字符集下标，避免取模偏置。
4. 重复直到生成所需长度的字符。
5. 返回 `CharArray`；清理中间缓冲。
6. 生成值不写日志；交由调用方按秘密处理。

## 6. 边界条件与错误处理

| 场景 | 处理 |
| --- | --- |
| 未选中任何字符类 | 拒绝并提示至少选择一类。 |
| 长度低于下限 | 提升到下限或提示。 |
| 排除易混淆后字符集过小 | 提示放宽设置。 |
| 候选“必需元素”各类最小数量之和大于长度 | 拒绝该配置（仅在引入必需元素后）。 |

## 7. 安全与隐私要求

- 使用 CSPRNG（`SecureRandom`），不使用非加密安全随机源（MASVS-CRYPTO）。
- 映射到字符集时避免取模偏置，采用拒绝采样。
- 返回 `CharArray` 并尽快清零；不用 `String`。
- 不记录生成的密码（MASVS-STORAGE、MASVS-PRIVACY）。
- 生成值按秘密处理，存储由 ENTRY/VAULT 负责。

## 8. 测试映射

| 测试/任务 | 关联 |
| --- | --- |
| 版本任务 MVP-006 | GEN-01..GEN-04 |
| 生成结果符合长度与字符集 | GEN-01、GEN-03 |
| 统计上无明显偏置 | GEN-02、GEN-04 |
| 生成值不写日志 | GEN-04 |

验证方式：生成结果落在所选字符集且符合长度；对大量样本做分布检验，确认字符近似均匀无明显偏置；日志扫描确认无生成值。

## 9. AI 任务拆分

| 任务 ID | 目的 | 输入 | 产出物 | 约束 | 验收证据 | 依赖 |
| --- | --- | --- | --- | --- | --- | --- |
| GEN-01 | 字符集与策略 | §4、§5.1-2 | `PasswordPolicy`、字符集组装 | 至少一类；可排除易混淆 | 字符集按策略正确组装 | 无 |
| GEN-02 | CSPRNG 与拒绝采样 | §3、§5.3 | 无偏置映射 | 用 SecureRandom；拒绝采样 | 分布检验近似均匀 | GEN-01 |
| GEN-03 | 生成与清理 | §5.4-6 | `generate` 返回 CharArray | 用后清零 | 生成符合长度与字符集 | GEN-02 |
| GEN-04 | 偏置测试与不记日志 | §7、§8 | 统计测试、日志核对 | 不记录生成值 | 无明显偏置；日志无生成值 | GEN-03 |

## 10. 待验证与不在范围

- 候选“必需元素”规则（保证每个选中类至少 N 个）与其他规则（口令短语/词表模式、避免重复字符），见 [灵感库](../inspiration.md)，待评审。
- 生成值存储：见 ENTRY、VAULT 实现文档。
- iOS 与 HarmonyOS 实现：放到 v1 阶段末尾。
