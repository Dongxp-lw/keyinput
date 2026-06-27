# 决策登记（ADR-lite）

状态：LOCKED（已锁定）/ PENDING（待定）。改 LOCKED 决策须在此更新并记原因，避免反复推翻。

| ID | 决策 | 状态 | 依据/来源 |
| --- | --- | --- | --- |
| D-001 | 平台优先级：Android 优先；iOS/HarmonyOS 放到 v1 末尾 | LOCKED | 产品决策记录基线 |
| D-002 | 进入自顶向下分层构建；用户要求先把架构再细化一层（共享核心/跨平台），**暂不写代码** | LOCKED | 用户 2026-06-24 |
| D-003 | 加密方案：Argon2id 派生 KEK + 随机 DEK 信封 + XChaCha20-Poly1305 + 头部作 AAD | LOCKED | [安全实现设计](../docs/technical/security-implementation-design.md) §2-4 |
| D-004 | 加密：选 Rust 共享核心后，加密原语用 Rust crate（RustCrypto 的 argon2+chacha20poly1305，或 libsodium-sys）；不用 lazysodium-android。**落地（2026-06-25）：argon2 0.5.3 + chacha20poly1305 0.10.1 + hkdf 0.13 + sha2 0.11 + zeroize 1.9 + subtle 2.6 + getrandom 0.2，已实跑+测试+arm64 交叉编译** | LOCKED | 用户 2026-06-24、§2.4、L2-02 实测 |
| D-005 | 序列化（Rust 核心）：需保留未知字段（跨版本前向兼容，安全实现设计 §4）。候选 **`ciborium`**（CBOR，serde，可经"捕获额外字段"保未知字段，**推荐**）；**`prost`（protobuf）会丢未知字段，故不选**。L2-03 落地时锁定 | PENDING（推荐 ciborium，待用户确认） | 安全实现设计 §4、L2-03 |
| D-006 | 架构：**一套 Rust 共享核心 + 各端原生 UI**（Android/iOS 经 UniFFI，HarmonyOS 经 Node-API）；HarmonyOS Rust 工具链待 spike | LOCKED | 用户 2026-06-24、[notes/shared-core-options.md](notes/shared-core-options.md) |
| D-007 | 本地传输：Nearby 依赖 GMS，须 Wi-Fi Direct/文件包回退 | LOCKED | 架构 §2 选型 |
| D-008 | 不自行实现密码学原语，只在审计过的库间选择 | LOCKED | 安全要求、§2.4 |
| D-009 | Agent 工作区文件夹采用 `.agent/` | LOCKED | 用户 2026-06-24 |
| D-010 | 商业模式：免费层保证正常使用（密码、2FA/TOTP、本地保险库、导出与手动迁移）；增值（云同步多端共享、键盘皮肤等）账号绑定、延后 MVP、不影响正常使用 | LOCKED | 用户 2026-06-24、[版本与权益计划](../docs/product/version-plan.md) |
| D-011 | SDK 基线：**minSdk = 28**（Android 9，安全优先）；compileSdk/targetSdk 取最新稳定，工具版本落地时锁定 | LOCKED | 用户 2026-06-24、[工程基础](../docs/technical/engineering-foundation.md) §4 |
| D-012 | 应用 ID **暂定 `com.lincdkeyinput`**（工作值，发布前定稿）；最终品牌/显示名待产品成形后定；产品**不**整体改名，文档仍称 Private Input Vault | PROVISIONAL | 用户 2026-06-24、[工程基础](../docs/technical/engineering-foundation.md) §5.1 |
| D-013 | Android 构建工具链（**已实跑验证** 2026-06-24）：NDK r27d (27.3.13750724) 经 cargo-ndk → 4 ABI `.so`；Gradle 工程 = **AGP 8.7.3 + Gradle 8.9 + Kotlin 2.1.0 + compileSdk 35 + JDK 17/21**（用现成 android-35，避开装 cmdline-tools/android-36）；UniFFI Kotlin 运行期 = **JNA `5.17.0@aar`**（@aar 必需，否则设备上缺 libjnidispatch.so）；预编译 `.so` 经 jniLibs 打包，AGP 不调 NDK；cargo-ndk/uniffi-bindgen 接为 Gradle 任务。`:app:assembleDebug` + 模拟器 `connectedDebugAndroidTest` 均绿 | LOCKED | 实跑验证 2026-06-24（放弃早前拟用的 AGP 9.2 以免多装 SDK） |
