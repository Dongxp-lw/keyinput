# 基础 TOTP 实现文档

本文件描述基础 TOTP（短码 TOTP）的实现方式，结构遵循 [实现文档层总览](README.md) 第 3 节模板。它落实 v0.2 的 MVP-007，并依赖 [条目与字段模型](entry-field-model.md) 定义的 `TotpField`（本文档只负责由种子生成验证码，不负责存储与加密）。

本文档服从 [产品决策记录](../product/product-decision-record.md) 基线：TOTP 验证码生成是离线的本地能力，验证码的插入遵循用户主动选择，平台凭据与自动填充是兼容增强而非主路径。

本文档的 TOTP 事实复用 [v0.2 离线 MVP 计划](../product/v0.2-mvp-plan.md) 第 3.1 节已锚定的 RFC 6238 / RFC 4226 调研，并在本轮向 RFC 6238 原文核对了附录 B 测试向量与动态截断逻辑，无需新调研。

## 1. 范围与目标

实现一个符合 RFC 6238 的 TOTP 生成器：由 `TotpField` 的种子和参数，按当前设备时间生成临时验证码，供用户主动选择后插入或复制。

- 范围内：HOTP 动态截断、时间步计算、6/8 位、HMAC-SHA-1/256/512、2038 年后大于 32 位的时间步、步内剩余时间提示、已知答案测试。
- 不在范围内：种子的存储与加密（见 ENTRY、VAULT）；验证方/服务端校验与 resync（产品是生成方）；`otpauth://` 导入与二维码扫描（待确认，见第 10 节）。
- 产品定位是生成方（prover），验证码依赖设备时间，时钟漂移会影响结果。

## 2. 依赖的设计与技术决策

- TOTP 算法、时间步、2038 年要求与附录 B 测试向量：RFC 6238 第 4 节与附录 B；HOTP 与动态截断见 RFC 4226；已在 [v0.2 离线 MVP 计划](../product/v0.2-mvp-plan.md) 第 3.1 节锚定。
- `TotpField` 字段（issuer、accountName、secret、algorithm、digits、periodSeconds）：[数据模型](../technical/data-model.md) 第 4 节、[条目与字段模型](entry-field-model.md) 第 4 节。
- 种子作为高敏感字段加密存储、不记日志、最小暴露：[安全模型](../technical/security-model.md)、[安全实现设计](../technical/security-implementation-design.md)、[数据模型](../technical/data-model.md) 第 3 节。
- TOTP 字段默认行为（生成临时验证码、不保存生成码）：[条目与字段模型](entry-field-model.md) 第 6 节、[核心交互设计](../product/interaction-design.md) 第 5.4 节。
- 验证码插入遵循主动选择（生成属于允许的自动化）：[核心交互设计](../product/interaction-design.md) 第 3.1 节。
- 版本任务：v0.2 MVP-007；测试 TP-104。

## 3. 平台与技术栈（Android 优先）

- 实现语言：**Rust**（核心层，见 [模块架构](../technical/module-architecture.md)）；下文 Java/Kotlin API 为逻辑示意。
- HMAC：Rust 的 `hmac` + `sha1`/`sha2` crates（HMAC-SHA1/256/512）；（原 Java 示意为 `javax.crypto.Mac`）。
- 时间源：设备 Unix 时间（秒）；本组件不自带时钟，时间由调用方注入，便于测试与已知答案验证。
- 时间步与计数器用 64 位 `Long` 表示，计数器编码为 8 字节大端，满足 2038 年后大于 32 位的要求。

## 4. 接口与数据结构

```kotlin
enum class TotpAlgorithm { SHA1, SHA256, SHA512 }

data class TotpParameters(
    val secret: ByteArray,        // 原始密钥字节（解码后），高敏感，用后清零
    val algorithm: TotpAlgorithm, // 默认 SHA1，兼容性最好
    val digits: Int,              // 6 或 8
    val periodSeconds: Int,       // 时间步 X，默认 30
    val t0Seconds: Long           // T0，默认 0（Unix 纪元）
)

data class TotpCode(
    val code: String,             // 左侧补零到 digits 位
    val validUntilEpochSeconds: Long,
    val secondsRemaining: Int     // 当前时间步剩余秒数，用于界面刷新
)

interface TotpGenerator {
    // nowEpochSeconds 由调用方注入设备时间，便于已知答案测试
    fun generate(params: TotpParameters, nowEpochSeconds: Long): TotpCode
}

// 内部：HOTP(K, C) = Truncate(HMAC(K, C))，C 为 8 字节大端计数器
internal fun hotp(
    key: ByteArray,
    counter: Long,
    algorithm: TotpAlgorithm,
    digits: Int
): String
```

## 5. 实现步骤

1. 计算时间步 `T = floorDiv(nowEpochSeconds - t0Seconds, periodSeconds)`，用 `Math.floorDiv` 保证负方向也向下取整。
2. 把 `T` 编码为 8 字节大端计数器 `C`（最高位在前），即使 `T` 超过 32 位也用完整 64 位表示。
3. 用所选算法计算 `hash = HMAC(key, C)`；`key` 为解码后的原始密钥字节。
4. 动态截断（RFC 4226）：`offset = hash[最后一字节] & 0x0f`；取 `hash[offset..offset+3]`，清除最高位得到 31 位整数 `binary`。
5. `otp = binary mod 10^digits`；转十进制字符串并左侧补零到 `digits` 位。
6. 计算 `validUntilEpochSeconds = (T + 1) * periodSeconds + t0Seconds` 与 `secondsRemaining`，供界面在步边界刷新。
7. 用后清零密钥与中间缓冲；不记录种子与生成的验证码。

## 6. 边界条件与错误处理

| 场景 | 处理 |
| --- | --- |
| 设备时间不可信或漂移 | 验证码依赖设备 Unix 时间；漂移会与验证方不一致；提示用户校时，记录为生成方限制。 |
| 时间步 T 超过 32 位（2038 年后） | 用 64 位 `Long` 与 8 字节大端计数器，正常支持。 |
| digits 非 6 或 8 | 拒绝，仅接受 6 与 8（默认 6）。 |
| periodSeconds ≤ 0 | 拒绝，回退默认 30。 |
| 种子为空或过短 | 拒绝；种子至少 128-bit，RFC 4226 推荐 160-bit。 |
| 种子编码无法识别 | 保留原输入交 ENTRY 修改，不猜测（`otpauth://` 与 Base32 导入待确认）。 |
| 算法不受支持 | 拒绝或回退 SHA-1，不静默改变语义。 |

## 7. 安全与隐私要求

- 种子是高敏感秘密：由 VAULT 加密存储，本组件只在内存中以原始字节使用，用后清零（MASVS-STORAGE、MASVS-CRYPTO）。
- 不记录种子和生成的验证码，不写入日志、崩溃报告或同步元数据（MASVS-PRIVACY）。
- 生成全程离线，不发起任何网络请求。
- 验证码是临时值，不持久化；插入遵循用户主动选择，不在后台自动填充。
- 优先用 `ByteArray` 传递密钥而非 `String`，减少不可清零的副本。

## 8. 测试映射

| 测试/任务 | 关联 |
| --- | --- |
| 版本任务 MVP-007 | TOTP-01..TOTP-05 |
| TP-104 添加 TOTP 字段并生成验证码 | TOTP-02、TOTP-03、TOTP-05 |
| RFC 6238 附录 B 已知答案测试 | TOTP-01、TOTP-02、TOTP-03 |
| 大于 32 位时间步（2038 年后） | TOTP-02 |
| 种子与验证码不写日志 | TOTP-04 |

已知答案测试用 RFC 6238 附录 B 测试向量，密钥为 ASCII `12345678901234567890`、X=30、T0=0，全部为 8 位：

| Time(sec) | T(hex) | SHA1 | SHA256 | SHA512 |
| --- | --- | --- | --- | --- |
| 59 | 0000000000000001 | 94287082 | 46119246 | 90693936 |
| 1111111109 | 00000000023523EC | 07081804 | 68084774 | 25091201 |
| 2000000000 | 0000000003F940AA | 69279037 | 90698825 | 38618901 |
| 20000000000 | 0000000027BC86AA | 65353130 | 77737706 | 47863826 |

注意：RFC 6238 附录 B 对 SHA-256、SHA-512 使用更长的种子（分别为 32 字节、64 字节，由 `12345678901234567890` 循环填充），不是与 SHA-1 相同的 20 字节种子；实现已知答案测试时必须按算法选对种子长度。最后一行 `20000000000`（公元 2603 年）用于验证大于 32 位的时间步。

## 9. AI 任务拆分

| 任务 ID | 目的 | 输入 | 产出物 | 约束 | 验收证据 | 依赖 |
| --- | --- | --- | --- | --- | --- | --- |
| TOTP-01 | HOTP 与动态截断 | §4、§5.3-5、RFC 4226 | `hotp(key, counter, alg, digits)` | 8 字节大端计数器；31 位截断 | RFC 4226/6238 向量通过 | 无 |
| TOTP-02 | 时间步与 TOTP 封装 | §4、§5.1-2、§5.6、RFC 6238 | `TotpGenerator.generate` | 用 floorDiv；支持大于 32 位 T | 附录 B 全部向量通过（含 2603 年） | TOTP-01 |
| TOTP-03 | 多算法与位数 | §3、§4 | SHA1/256/512 与 6/8 位支持 | 默认 SHA-1；按算法选对种子 | 三种算法附录 B 向量通过 | TOTP-01 |
| TOTP-04 | 种子卫生与不记日志 | §6、§7 | 密钥清零、日志核对 | 种子高敏感；用后清零 | 种子与验证码不入日志 | TOTP-02 |
| TOTP-05 | 步内刷新与展示 | §5.6、交互 §5.4 | 剩余秒数与失效刷新 | 验证码临时、主动选择插入 | 跨时间步刷新正确 | TOTP-02 |

## 10. 待验证与不在范围

- `otpauth://` Key URI 导入与种子的 Base32（RFC 4648）解码：v0.2 是否支持待确认（[v0.2 离线 MVP 计划](../product/v0.2-mvp-plan.md) 第 3.1 节）；当前接口以解码后的原始密钥字节为输入。
- 二维码扫描录入：依赖 `otpauth://` 决策，待评审。
- 种子的存储与加密：见 [条目与字段模型](entry-field-model.md)、[保险库加密核心](vault-crypto-core.md)。
- 验证方/服务端校验、resync、漂移容忍窗口：产品是生成方，不在范围。
- HOTP（基于计数器）模式与非标准变体（如 Steam Guard）：不在范围，待评审（见 [灵感库](../inspiration.md)）。
- iOS 与 HarmonyOS 实现：放到 v1 阶段末尾。
