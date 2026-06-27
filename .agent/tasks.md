# 任务板（细粒度）

状态：TODO / DOING / DONE / BLOCKED。任务 ID = 层-序号。每个任务自带验收证据。

说明：实现层的功能任务（如 KBD-01、VAULT-01）已在各实现文档第 9 节定义，这里只做**排期与依赖**，不重复其内容。分层与门禁见 [execution-plan.md](execution-plan.md)。

## L0 工程基础

| ID | 任务 | 状态 | 依赖 | 验收证据 |
| --- | --- | --- | --- | --- |
| L0-01 | 工程结构与构建系统设计（Gradle+Cargo+UniFFI） | DONE | D-006 | [工程基础](../docs/technical/engineering-foundation.md) §3 |
| L0-02 | SDK/工具链基线（minSdk 等） | DONE（待锁 D-011） | L0-01 | §4 |
| L0-03 | 工程规范（命名/lint/禁记秘密/StrictMode/离线） | DONE | L0-01 | §5 |
| L0-04 | CI 雏形设计 | DONE | L0-01 | §6 |
| L0-05a | Cargo workspace + `vault-core` crate 骨架（含 UniFFI 空接口） | DONE | L0-01..04 + 用户同意 | `cargo build` 通过（uniffi 0.31.2 `setup_scaffolding!()` 空接口，2026-06-24） |
| L0-05b | UniFFI 脚手架：生成 Kotlin 绑定，导出一个平凡函数往返 | DONE | L0-05a | `ping` 导出；Kotlin 绑定生成（`fun ping(...)`）；Rust 测试 `ping_roundtrips_input` 通过；Kotlin 运行期 FFI 调用在 L0-05d 验证（本机无 kotlinc）。2026-06-24 |
| L0-05c | Rust→Android 交叉编译流水线：NDK r27d + 4 Rust targets + cargo-ndk → 4 ABI `.so` 入 core-bindings/jniLibs；uniffi-bindgen 生成 Kotlin 绑定 | DONE | L0-05b | 4 ABI `libvault_core.so`（arm64-v8a/armeabi-v7a/x86_64/x86）+ `vault_core.kt` 已生成就位（2026-06-24） |
| L0-05d | Gradle 多模块工程（app/keyboard/core-bindings + version catalog + wrapper）+ cargo-ndk/uniffi-bindgen 接为 Gradle 任务；`:app:assembleDebug` 端到端绿；app 经绑定调 `ping`，模拟器 `connectedDebugAndroidTest` 通过 | DONE | L0-05c | assembleDebug 绿（APK 含 4 ABI .so）；`PingInstrumentedTest` 在 API 36 模拟器通过（2026-06-24） |
| L0-05e | CI：构建 + clippy + detekt + 日志扫描 + 无 INTERNET 校验 | TODO | L0-05d | CI 全绿 |

注：L0 纸面设计已完成（L0-01..04）。L0-05a/b/c/d 已落地并验证（2026-06-24；完整 Rust→UniFFI→Kotlin→Android 链路在 API 36 模拟器跑通 ping）；仅 L0-05e（CI）待续。

## L1 架构落地

| ID | 任务 | 状态 | 依赖 | 验收证据 |
| --- | --- | --- | --- | --- |
| L1-01 | 模块结构 + 核心↔原生层间契约 + 依赖方向 | DONE | D-006 | [模块架构与层间契约](../docs/technical/module-architecture.md) 成型 |
| L1-02 | 核心 API 草案细化为 UniFFI UDL/接口 | TODO | L1-01、L0 | UDL 或 proc-macro 接口定义（L0/L2 落地时） |

共享核心语言已锁（D-006）。

## L2 横切基础（Rust 核心地基 + FFI 脚手架）

对应 [安全实现设计](../docs/technical/security-implementation-design.md) §2-4、[模块架构](../docs/technical/module-architecture.md) §4-5。

| ID | 任务 | 状态 | 依赖 | 验收证据 |
| --- | --- | --- | --- | --- |
| L2-01 | 秘密与卫生类型（Rust） | DONE | L0-05a | `secret.rs`：`SecretKey`/`SecretBytes`（`zeroize::ZeroizeOnDrop` + 遮蔽 Debug + `subtle` 常量时间比较）；测试通过（2026-06-25） |
| L2-02 | 加密原语封装（Argon2id + XChaCha20-Poly1305 + 信封密钥，D-004） | DONE | L2-01 | `crypto.rs`：Argon2id KEK / XChaCha20-Poly1305 / 信封 / HKDF 子密钥（RustCrypto）；10 行为测试（往返/篡改/错口令不可区分/域分离） + clippy 净 + arm64 交叉编译；标准 KAT 向量留 L6-01（2026-06-25） |
| L2-03 | 序列化（CBOR via `ciborium`，保留未知字段，D-005） | TODO | L2-01 | 往返一致且保未知字段 |
| L2-04 | 错误模型与结果类型（跨 FFI） | TODO | L0-05a | 口令错/篡改/不兼容/损坏 枚举经 UniFFI 正确映射 |
| L2-05 | CSPRNG 与无偏置采样 | TODO | L2-01 | 分布近似均匀 |
| L2-06 | `VaultCore` FFI 表面骨架（签名齐全、空实现） | TODO | L2-04 | 绑定生成、可调 |

## L3 领域核心（Rust，纯逻辑可测）

排期 ENTRY → VAULT → LOCK → GEN → TOTP → IMEX；功能细分见各实现文档第 9 节（用 Rust 实现，非 Kotlin）。

| ID | 任务 | 状态 | 依赖 | 实现文档（细分任务） | 验收证据 |
| --- | --- | --- | --- | --- | --- |
| L3-ENTRY | 条目/字段模型 + 编解码 + CRUD + 本地搜索 | TODO | L2-03 | [条目与字段模型](../docs/implementation/entry-field-model.md) §9（ENTRY-01..07） | 往返一致、保未知字段、可搜索 |
| L3-VAULT | 保险库格式 + KDF + 信封 + AEAD + 篡改检测 | TODO | L2-02、L3-ENTRY | [保险库加密核心](../docs/implementation/vault-crypto-core.md) §9（VAULT-01..08） | 已知答案；篡改/错误口令安全失败 |
| L3-LOCK | 解锁会话密钥逻辑（核心侧） | TODO | L3-VAULT | [主密码与解锁会话](../docs/implementation/master-password-unlock.md) §9 | 错误口令不可区分；会话密钥最小持有 |
| L3-GEN | 密码生成器 | TODO | L2-05 | [密码生成器](../docs/implementation/password-generator.md) §9（GEN-01..04） | 无明显偏置；不记日志 |
| L3-TOTP | TOTP 生成 | TODO | L2-02 | [基础 TOTP](../docs/implementation/totp-generation.md) §9（TOTP-01..05） | RFC 6238 向量通过 |
| L3-IMEX | 加密导出/导入包 | TODO | L3-VAULT、L3-ENTRY | [加密导出与导入](../docs/implementation/encrypted-export-import.md) §9（IMEX-01..06） | 篡改拒绝；失败回滚 |

## L4 平台集成（Android 原生）

| ID | 任务 | 状态 | 依赖 | 实现文档 | 验收证据 |
| --- | --- | --- | --- | --- | --- |
| L4-FFI | core-bindings 消费 + 会话桥接 | TODO | L0-05d、L3-VAULT | [模块架构](../docs/technical/module-architecture.md) §4-5 | Kotlin 调 `VaultCore`；解锁→读字段链路通 |
| L4-KBD | 安全键盘 IME | TODO | L4-FFI | [安全键盘输入法](../docs/implementation/secure-keyboard-ime.md) §9 | 主动选择插入；锁定态不泄露；不记日志 |
| L4-BIO | 生物识别解锁 | TODO | L4-FFI | [生物识别解锁](../docs/implementation/biometric-unlock.md) §9 | 可解锁、可回退主密码；新注册失效 |
| L4-CLIP | 剪贴板兜底 | TODO | L4-FFI | [剪贴板兜底](../docs/implementation/clipboard-fallback.md) §9（CLIP-01..04） | 超时清除；敏感标记 |
| L4-APP | 主应用 UI + 持久化接线 | TODO | L4-FFI | 信息架构/核心交互 | E2E：离线创建/解锁/管理/生成/导入导出 |

## L5 跨设备与云

| ID | 任务 | 状态 | 依赖 | 实现文档 | 验收证据 |
| --- | --- | --- | --- | --- | --- |
| L5-MIGR | 跨设备迁移（v0.3） | TODO | L3-IMEX、L4-APP | [跨设备迁移](../docs/implementation/cross-device-migration.md) §9（MIGR-01..07） | TP-301..308 |
| L5-SYNC | 云同步（v1.1） | TODO | L5-MIGR | [云同步](../docs/implementation/cloud-sync.md) §9（SYNC-01..08） | TP-201..206 |

## L6 测试与发布门禁

| ID | 任务 | 状态 | 依赖 | 验收证据 |
| --- | --- | --- | --- | --- |
| L6-01 | 核心已知答案测试套件（KDF/AEAD/TOTP） | TODO | L2-02、L3-TOTP | 测试向量全通过 |
| L6-02 | 离线与隐私验证（无 INTERNET、抓包、日志扫描、StrictMode） | TODO | L4-APP | MVP-010、TP-102 通过 |
| L6-03 | v0.2 MVP 退出标准回归 | TODO | L4-* | v0.2 退出标准全满足 |
| L6-04 | 发布门禁（MASVS 对齐） | TODO | L6-* | 见 [v1.0 发布计划](../docs/product/v1.0-release-plan.md)，门禁通过 |
