# 共享核心 / 跨平台 架构选项分析

状态：分析中，待与用户讨论后在 [decisions.md](../decisions.md) 锁定 D-006/D-004。
背景：用户问"加密库是否适用于所有端"——这个问题牵出共享核心这一架构决策。

## 关键事实（已核实 / 待验证）

- lazysodium-android 是 **Android 专用**（JVM/JNA，min SDK 24），**不直接适用于 iOS/HarmonyOS**（已核实）。
- libsodium 本体是可移植 C 库，各平台有不同绑定：iOS 有 swift-sodium；Android 有 lazysodium；HarmonyOS 的移植/绑定 **待验证**。
- **没有单一库**原生覆盖 Android+iOS+HarmonyOS；要"一套加密实现覆盖所有端"，只能靠**共享核心**。
- 安全产品硬要求：保险库格式 + 加密 + 序列化必须**全平台逐字节一致**，否则跨设备迁移(MIGR)/同步(SYNC)会失败。这要求核心**只实现一次**，而非每端重写。

## 选项

### A. 每端原生、无共享核心
Android=Kotlin+lazysodium，iOS=Swift+swift-sodium，HarmonyOS=ArkTS+?。
- 优点：各端最简、起步快。
- 缺点：安全核心重写 3 次，跨端一致性风险高。**加密/格式核心不推荐此法。**

### B. Rust 共享核心（[技术架构](../../docs/technical/architecture.md) §2 的推荐）
一份 Rust 库实现格式/加密/序列化/冲突逻辑，经 FFI 编译到各端（Android=JNI/UniFFI，iOS=C-FFI/Swift，HarmonyOS=native）。
- 优点：安全核心**只实现一次**，跨端一致性最强；Rust 内存安全贴近密码学。
- 缺点：FFI 复杂度；HarmonyOS 的 Rust FFI **待验证**；起步比纯 Android 慢。

### C. Kotlin Multiplatform（KMP）共享核心
用 Kotlin 在 Android+iOS 共享核心；加密用 expect/actual 委托各端 libsodium 绑定或 KMP 加密库。
- 优点：一套 Kotlin 核心覆盖 Android+iOS；复用 Kotlin 技能；iOS 产出 framework。
- 缺点：**HarmonyOS 不是 KMP 目标**（仍需单独处理）；KMP 加密库成熟度 **待验证**；Kotlin/Native iOS 互操作有粗糙处。

## 核实结果（2026-06-24）

- **UniFFI**（Mozilla，MPL-2.0，"ready for production use"，Firefox 移动端大规模在用）：Rust 核心写一次，自动生成 Kotlin(Android)/Swift(iOS) 绑定。**Android+iOS 的 Rust FFI 是成熟、经生产验证的路径。**
- **HarmonyOS Node-API（NDK）**：华为官方支持把 C/C++ 能力编译成 `.so` 暴露给 ArkTS。Rust 编译到 C ABI/`.so` 即可经此接入。**第三端的原生核心路径已确认**；唯一 spike 是 Rust→HarmonyOS 目标工具链。

## 结论与推荐（已核实，待用户确认锁定）

**推荐：分层架构 = 一套 Rust 共享核心 + 各端原生 UI。**

- 既不是"三套代码"，也不是"一套到底"，而是**分层**：把安全关键、风险高、价值高的核心**只写一次**（Rust），把必须原生的 UI 各端各写。
- 这正是"兼顾"：质量（单一审计核心 + Rust 内存安全 + 原生 UI）与复用（核心写一次喂三端）**同时拿到**，不是二选一。
- 因为质量优先：核心从一开始就用 Rust 写，**不**先用 Kotlin 写再重写（避免重复实现安全核心）。

分层（谁在哪写）：

- **Rust 核心（写一次）**：保险库格式、KDF+AEAD+信封密钥、序列化、条目/字段领域模型与 CRUD/搜索、TOTP、导出/导入打包、迁移/同步冲突逻辑。
- **各端原生（各写）**：UI、键盘/IME 服务、生物识别提示、安全存储（Keystore/Keychain/HUKS）、剪贴板、生命周期。

为什么不是 KMP：KMP **不支持 HarmonyOS**，且 KMP 不提供"单一加密实现"（加密仍按端委托）。三端 + 质量优先下，Rust 严格更优。
为什么不是 C/C++ 核心：同样可移植，但贴近密码学处**内存不安全**，质量上 Rust 胜。

对 D-004（加密库）的影响：选 Rust 核心后，加密用 **Rust crate**（RustCrypto 的 argon2 + chacha20poly1305，或 libsodium 经 libsodium-sys），不再是 Android 专用的 lazysodium-android。

## 成本与风险（诚实）

- 上手复杂度更高：Rust + UniFFI + NDK 构建接线（但久经实践：Firefox、Matrix SDK、多款密码管理器/钱包）。
- v0.1 Android 起步比纯 Kotlin 慢（要先搭 Rust+JNI 脚手架），但避免日后更贵的核心重写。
- HarmonyOS Rust 目标工具链 = 待做的可行性 spike（Node-API 路径已确认）；最坏回退：薄 C/C++ Node-API shim 包 Rust `.so`。仅影响 HarmonyOS（本就 v1 末尾）。

## 待用户确认

- 是否锁定"Rust 共享核心 + 各端原生 UI"为架构基线？确认后写进 [技术架构](../../docs/technical/architecture.md) 并锁 D-006/D-004。
