# 工程基础：构建系统、工具链与规范（L0）

本文件是工程基础（L0）的纸面设计：定义**构建系统、工具链与 SDK 基线、工程规范（含不记录秘密的强制手段）、CI 雏形与 L0 门禁**。它服从 [产品决策记录](../product/product-decision-record.md) 基线，落实已锁定的 Rust 共享核心 + 各端原生 UI（[技术架构](architecture.md) 第 2 节）与模块结构（[模块架构与层间契约](module-architecture.md)）。

本阶段不写代码，产出"怎么搭、用什么版本、守什么规范"的设计；具体配置文件（`build.gradle.kts`、`Cargo.toml`、CI yaml 等）属落地（L0-05）。

## 1. 范围与目标

- 范围内：Gradle + Cargo + UniFFI 的构建集成、SDK/工具链基线、命名与代码规范、静态检查、不记录秘密的强制手段、离线强制、Git 卫生、CI 雏形、L0 门禁。
- 不在范围内：具体配置文件内容与代码（落地阶段）；Rust 加密 crate 的最终版本（L2 锁定）。

## 2. 依赖的决策与文档

- 架构与模块：[技术架构](architecture.md) 第 2 节、[模块架构与层间契约](module-architecture.md)。
- 卫生与日志要求：[安全实现设计](security-implementation-design.md) 第 7 节。
- 离线与隐私验证、日志扫描：[v0.2 离线 MVP 计划](../product/v0.2-mvp-plan.md)、[测试计划](../testing/test-plan.md)。
- 决策：D-006（Rust 核心 + 原生 UI）、D-004（加密走 Rust crate）；本文件新增 D-011（SDK 基线）、D-012（命名空间），均待用户确认锁定。

## 3. 构建系统架构（Gradle + Cargo + UniFFI）

### 3.1 模块与构建图

```text
vault-core (Cargo crate, Rust)
   │  uniffi-bindgen 生成 Kotlin 绑定
   │  cargo-ndk 交叉编译各 ABI 的 .so
   ▼
apps/android/core-bindings (Gradle 模块)
   │  打包生成的 Kotlin + jniLibs/<abi>/libvault_core.so
   ▼
apps/android/app  与  apps/android/keyboard (Gradle 模块，依赖 core-bindings)
```

- 单一 mono-repo，Gradle 管 Android 侧，Cargo 管 Rust 核心。
- iOS（v1）：同一 `vault-core` 经 UniFFI 产出 XCFramework + Swift 绑定。HarmonyOS（v1）：`.so` 经 Node-API 包装。

### 3.2 Rust → Android 构建流程

- 用 `cargo-ndk` 为 `aarch64/armv7/x86_64/i686-linux-android` 交叉编译 `vault-core` 的 `cdylib`。
- 用 `uniffi-bindgen` 由 `vault-core` 生成 Kotlin 绑定源码。
- 用 Gradle 插件（如 `cargo-ndk-android-gradle`）把 cargo 构建挂到 Gradle 生命周期，使 `./gradlew assemble` 自动触发 Rust 构建并把 `.so` 放入 `jniLibs`。
- 参考完整范式：UniFFI 官方的 `uniffi-starter`（含 Android 库构建与 XCFramework 脚本）。具体插件与版本在落地时锁定。

### 3.3 依赖管理

- Gradle 用 version catalog（`gradle/libs.versions.toml`）集中管理依赖与版本。
- Cargo 用 `Cargo.toml` + `Cargo.lock`（提交 lock 以可复现）。
- 第三方库遵循 [技术架构](architecture.md) 第 2 节的开源组件选型表；加密走 Rust crate（D-004）。

## 4. 工具链与 SDK 基线

待用户确认后锁定（D-011）。撰写时建议：

| 项 | 建议 | 说明 |
| --- | --- | --- |
| Android `compileSdk` / `targetSdk` | 最新稳定（撰写时约 36） | 随平台发布跟进。 |
| Android `minSdk` | **28（Android 9.0，已锁定 D-011）** | 安全优先：StrongBox 硬件密钥、更强生物识别、更干净的安全 API；覆盖 2026 年绝大多数设备。 |
| AGP / Gradle / Kotlin | 最新稳定且互相兼容 | 落地时按当时稳定版锁定到 version catalog。 |
| Android NDK | 与 `cargo-ndk` 兼容的稳定版 | 锁定具体版本。 |
| Rust edition / toolchain | 2021（或当时最新）+ 固定 toolchain | 用 `rust-toolchain.toml` 固定；声明 MSRV。 |
| Rust Android targets | aarch64/armv7/x86_64/i686-linux-android | 由 `cargo-ndk` 管理。 |

## 5. 工程规范

### 5.1 命名与命名空间

- 应用 ID（**暂定 D-012**，尚无最终产品故为工作值，发布前定稿）：**`com.lincdkeyinput`**（全局唯一、发布后不可改；发布前确认 Play 未被占用）；模块命名空间用 `com.lincdkeyinput.app` / `com.lincdkeyinput.keyboard` / `com.lincdkeyinput.core`。
- 应用显示名 / 品牌：**暂未确定**（尚无真正产品；项目内仍称 Private Input Vault）；最终品牌待产品成形后再定。骨架阶段可暂用项目名作占位显示名。
- Rust crate 名 `vault-core`，UniFFI 包名 `vault_core`（内部名，与品牌无关）。

### 5.2 代码风格与静态检查

- Kotlin：ktlint 或 detekt 统一风格与静态检查。
- Rust：`rustfmt` 统一格式、`clippy` 做静态检查（CI 中 `-D warnings`）。
- 这些检查纳入 CI，未通过即失败。

### 5.3 不记录秘密的强制手段（多层）

安全产品的硬要求：主密码、密钥、字段值、TOTP 种子与生成码绝不进日志/崩溃报告（[安全实现设计](security-implementation-design.md) 第 7 节）。多层强制：

- **类型层**：秘密用可清零、`Debug`/`toString` 被遮蔽的类型。Rust 用 `zeroize::Zeroizing` + 自定义 `Secret` newtype（`Debug` 输出 `***`）；Kotlin 用 `CharArray`/`ByteArray`（不用 `String`），必要时包 `Secret` 类（`toString()` 返回 `***`）。
- **静态检查层**：detekt 自定义/禁用规则，禁止对秘密类型调用 `android.util.Log`、`println`、`printStackTrace`；Rust 用 lint/`#[deny]` 禁止对秘密 `println!`/`dbg!`。
- **构建层**：release 用 R8/ProGuard 去除调试日志；默认不接入遥测，若接入须用户选择加入并排除秘密。
- **CI 门禁层**：日志扫描脚本对禁用模式（如打印字段值）做启发式扫描；并跑实现文档/测试计划里的"日志不含秘密"测试（TP-005、TP-102 等）。
- **运行期（开发）**：自定义日志封装拒绝秘密类型；崩溃报告在源头清理字段值与保险库标识。

### 5.4 离线强制与 StrictMode

- **离线强制**：MVP 的 `app`/`keyboard` 模块**不声明** `android.permission.INTERNET`；云同步（v1.1）放在**独立模块**，仅在云功能启用时引入网络权限——使核心离线"按构造成立"。
- 开发构建启用 StrictMode（主线程磁盘/网络、泄漏检测）。
- 网络安全配置关闭明文；发布前抓包验证无流量（MVP-010、TP-102）。

### 5.5 Git 卫生与 .gitignore

- 忽略：Gradle/Cargo 构建输出（`build/`、`target/`、`.gradle/`）、`local.properties`、签名/密钥材料（`*.jks`、`*.keystore`）、生成的 `.so` 与绑定产物、IDE 私有文件。
- 提交 `Cargo.lock` 与 version catalog 以可复现。
- 仓库内绝不放任何真实密钥、口令或测试用真实秘密。

## 6. CI 雏形（设计）

落地时实现；阶段门：

- 构建：`./gradlew assemble` + `cargo build`（各 Android target）。
- 测试：Rust 核心单元测试（含已知答案测试）+ Android 单元/instrumented 测试。
- 静态检查：`clippy -D warnings`、detekt/ktlint、Android lint。
- 安全门：日志扫描无秘密；MVP 模块无 `INTERNET` 权限校验。

## 7. 验证与门禁（L0 gate）

- 模块与包结构、构建集成方案经评审（本文件）。
- 落地后（L0-05）：`./gradlew assemble` 通过，`.so` 正确打入各 ABI，UniFFI 绑定可被 `app`/`keyboard` 调用。
- 规范可执行：CI 跑通格式、静态检查、日志扫描与无网络权限校验。

## 8. 待确认与不在范围

- D-011 SDK 基线：已锁定 `minSdk` = 28；`compileSdk`/`targetSdk` 取最新稳定，具体工具版本落地时锁定到 version catalog。
- D-012 命名空间/品牌：应用 ID 暂定 `com.lincdkeyinput`（工作值，发布前定稿）；最终品牌/显示名待产品成形后定；产品**不**整体改名，文档仍称 Private Input Vault。
- 具体构建插件与版本（cargo-ndk 插件、AGP/Gradle/Kotlin/NDK 版本）在落地时锁定到 version catalog。
- 实际配置文件与代码属落地（L0-05），不在本设计内。
- iOS/HarmonyOS 的构建脚本放到 v1 阶段末尾。
