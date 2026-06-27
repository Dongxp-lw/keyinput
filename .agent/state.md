# 当前状态

最后更新：2026-06-25。**接续工作先读本文件。**

## 阶段

- 文档阶段：完成（产品/技术/版本/测试/11 份实现文档/开源组件选型）。
- L0 纸面设计：完成（工程基础、模块架构、任务板 L0–L6）。
- **开发阶段：已开始**（用户 2026-06-24 同意）。工具链已装并验证；L0-05a 空骨架已落地，`cargo build` 通过。

## 进行中

- L2 核心地基：L2-01（秘密类型）、L2-02（加密原语）已完成并验证（13 测试 + clippy `-D warnings` + arm64 交叉编译）；继续 L2-03（序列化）、L2-04（错误模型），再 L3-ENTRY/VAULT。

## 下一步

- 继续核心实现：L2-03 序列化（保未知字段，D-005）→ L2-04 错误模型 → L3-ENTRY（条目/字段模型 + 编解码 + CRUD + 本地搜索）→ L3-VAULT（保险库格式 + create/open/save，用 L2-02 加密）。L0-05e CI 可并行或稍后。
- 已锁定 D-011（minSdk 28）。D-012 暂定：应用 ID `com.lincdkeyinput`（工作值，发布前定稿）；最终品牌待产品成形后定；产品**不**整体改名，文档仍称 Private Input Vault。

## 已确认（用户 2026-06-24）

- 架构锁定：**一套 Rust 共享核心 + 各端原生 UI**（D-006）；加密走 Rust crate（D-004）。已写入 [技术架构](../docs/technical/architecture.md) §2。
- 商业模式：免费层保证正常使用（密码、2FA、本地保险库、导出与手动迁移）；增值（云同步、键盘皮肤）账号绑定、延后 MVP、不影响正常使用（D-010）。已写入 [版本与权益计划](../docs/product/version-plan.md)。
- 开发阶段已授权（2026-06-24）：同意安装工具链并开始落地（“你可以直接安装，我没有意见”）。
- 工作区 `.agent/` 采用（D-009）。

## 开发环境（本机已装，2026-06-24）

- Rust：stable **1.96.0**，host **x86_64-pc-windows-gnu**（`rust-toolchain.toml` 暂用 `stable`，精确版推迟到 CI）。cargo/rustc 在 `%USERPROFILE%\.cargo\bin`（已入用户 PATH）。
- MinGW-w64：**WinLibs POSIX/MSVCRT**（winget `BrechtSanders.WinLibs.POSIX.MSVCRT`），提供 `dlltool.exe`——gnu host 编译 `windows-sys`（uniffi 依赖树）必需；bin 已入用户 PATH。
- Android SDK：`%LOCALAPPDATA%\Android\Sdk`（build-tools 35/36、platform android-35、platform-tools、emulator）；**无 cmdline-tools/sdkmanager**；`ANDROID_HOME`/`ANDROID_NDK_HOME` 已设。
- Android NDK：**r27d（27.3.13750724）**，SHA-1 校验通过，装于 `…\Sdk\ndk\27.3.13750724`（= `ANDROID_NDK_HOME`）。
- Rust Android targets：aarch64/armv7/x86_64/i686-linux-android 已装；`cargo-ndk 4.1.2` 已装。
- Gradle：8.9（SHA-256 校验）解压于 `%USERPROFILE%\gradle-dist`（仓库外）；工程 wrapper 已生成。AVD 已有（Medium_Phone_API_36 等），WHPX 加速可用，可跑 instrumented 测试。
- JDK：Oracle 17 + Microsoft 21（Gradle 实跑用 21）；git 已装。
- 说明：host 工具链仅用于本机开发/测试核心；Android 交叉编译用 NDK clang（与 gnu/MinGW 无关）。

## 待验证（不阻塞当前）

- HarmonyOS 的 Rust 目标工具链可行性 spike（Node-API 路径已确认）。

## 最近完成

- **L2-01 / L2-02 落地（加密核心）**：`core/vault-core/src/secret.rs`（`SecretKey`/`SecretBytes`：`zeroize` 清零 + 遮蔽 `Debug` + `subtle` 常量时间比较）、`crypto.rs`（Argon2id KEK / XChaCha20-Poly1305 seal·open / 信封 wrap·unwrap / HKDF-SHA256 子密钥），全用审计过的 RustCrypto crate（D-004/D-008）；13 测试通过、clippy `-D warnings` 净、arm64 交叉编译通过。标准 KAT 向量留 L6-01。2026-06-25。
- **L0-05d 落地（完整链路验证）**：搭 Gradle 多模块工程（AGP 8.7.3/Gradle 8.9/Kotlin 2.1.0/compileSdk 35）+ wrapper；cargo-ndk/uniffi-bindgen 接为增量 Gradle 任务；`:app:assembleDebug` 绿（APK 含 4 ABI .so）；JNA 用 `@aar`；API 36 模拟器上 `PingInstrumentedTest` 通过（Kotlin→Rust 运行期往返）。2026-06-24。
- **L0-05c 落地**：装 NDK r27d（SHA-1 校验）+ 4 个 Rust Android target + `cargo-ndk 4.1.2`；交叉编译出 4 个 ABI 的 `libvault_core.so` 入 core-bindings/jniLibs；`uniffi-bindgen` 生成 Kotlin 绑定入 core-bindings。Gradle 工程与 `assemble` 归入 L0-05d。2026-06-24。
- **L0-05b 落地**：导出平凡函数 `ping` + Rust 测试通过；`uniffi-bindgen`（feature 门控 `bindgen`，保持 cdylib 精简）生成 Kotlin 绑定（`fun ping(...)`）。Kotlin 运行期 FFI 调用留到 L0-05d（本机无 kotlinc）。2026-06-24。
- **L0-05a 落地**：Cargo 工作区 + `vault-core` crate（lib+cdylib）+ UniFFI 空脚手架（`uniffi 0.31.2`）；`cargo build` 通过（2026-06-24）。
- 安装并验证 Rust 工具链（gnu host）+ WinLibs MinGW（`dlltool`）。
- 开源组件选型调研，写入 [技术架构](../docs/technical/architecture.md) §2 + [安全实现设计](../docs/technical/security-implementation-design.md) §2.4。
- 11/11 实现文档。

## 来源指引

- 计划：[execution-plan.md](execution-plan.md)；任务：[tasks.md](tasks.md)；决策：[decisions.md](decisions.md)。
