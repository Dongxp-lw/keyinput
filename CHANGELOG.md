# 更新日志（Changelog）

本文件记录 Private Input Vault 的所有重要变更。

- 格式遵循 [Keep a Changelog 1.1.0](https://keepachangelog.com/zh-CN/1.1.0/)。
- 版本号遵循 [语义化版本 SemVer 2.0.0](https://semver.org/lang/zh-CN/)；版本号与里程碑、发布流程见 [发布流程](docs/technical/release-process.md)。
- 分类：`Added` 新增、`Changed` 变更、`Deprecated` 弃用、`Removed` 移除、`Fixed` 修复、`Security` 安全。

> 说明：项目当前处于开发阶段，尚未正式打 tag 发布。`0.2.0` 为已完成的**开发里程碑**（`versionName=0.2.0`、`versionCode=1`）；首个正式发布将据本文件定版并打 `vX.Y.Z` tag。

## [Unreleased]

### Added
- 工程治理与规范：`LICENSE`（Apache-2.0）、`NOTICE`、`CONTRIBUTING.md`、`SECURITY.md`、`CODE_OF_CONDUCT.md`、`CHANGELOG.md`、`AGENTS.md`、`.editorconfig`；`docs/technical/release-process.md`（发布流程与版本号策略）、`docs/technical/coding-standards.md`（编码规范）。
- GitHub 协作：PR 模板、Issue 模板（bug/feature + 安全报告引导）、`CODEOWNERS`、`dependabot.yml`（Cargo/Gradle/Actions 自动更新）。
- 供应链门禁：`deny.toml`（cargo-deny 许可证白名单 + 安全公告 + 来源校验），并接入 CI。

### Fixed
- 导出/导入：修复「进入后台自动锁定」与 App 自家 SAF 文件选择器（DocumentsUI，跨进程）冲突，导致导出写出 0 字节、导入失败的问题。修法：`VaultApplication` 增加一次性 `suppressNextBackgroundLock` 标志，仅在打开自家导出/导入选择器前置位、`onStop` 放行一次即复位；其它任何进入后台仍照常自动锁定。已在 API 36 模拟器运行期验证（导出 1094B 加密包、导入整库还原、全程不被锁）。

## [0.2.0] - 2026-06-26 — 离线 MVP 开发里程碑

### Added
- **共享加密核心（Rust）**：Argon2id 派生 KEK + 随机 DEK 信封 + XChaCha20-Poly1305 + HKDF 子密钥；TOTP（RFC 6238，SHA-1/256/512）；密码生成器（拒绝采样无偏置）；加密导出/导入包。经 UniFFI 暴露给 Android。核心 88/88 测试 + 标准向量 KAT（RFC 9106 / RFC 5869 / draft-arciszewski-xchacha-03 / RFC 6238）。
- **Android MVP（Jetpack Compose）**：引导创建、主密码解锁（失败计数+退避，持久化）、条目增/改/删与搜索、密码强度提示、密码生成器、详情揭示/复制、TOTP 实时码与倒计时、加密备份导出/导入（SAF）、设置改主密码。进入后台自动锁定。
- **安全键盘 IME**：独立会话 + 内置解锁键盘（自绘，不依赖其它输入法、不过系统剪贴板）+ 选条目/字段后 `commitText` 直填目标输入框；空闲/销毁自动锁定。
- **生物识别解锁**：Android Keystore AES-GCM 包裹主密码，BiometricPrompt + CryptoObject；新注册生物识别使密钥失效 → 自动降级主密码。
- **剪贴板兜底**：API 33+ 敏感标记（剪贴板预览遮蔽）+ 约 30s 自动清除。
- **二维码扫描导入 TOTP**：`CAMERA` 运行时按需授权（`uses-feature` `required=false`）+ zxing-android-embedded（离线、无 GMS）；扫码与粘贴 `otpauth://` 两路复用同一解析。

### Security
- 全程**离线**：`app`/`keyboard` 不声明 `INTERNET` 权限（按构造成立）。
- **零日志**：Rust 核心与 Android Kotlin 均无 `println!`/`Log.*` 等，主密码/密钥/字段值/TOTP 不入日志（运行期 logcat 实测 0 次）。
- 错口令与数据篡改在错误模型上不可区分（`WrongPasswordOrTampered`）。
- 开发构建启用 `StrictMode`；`usesCleartextTraffic=false` + network security config。

<!-- 比较 / 发布链接：打 release tag 后填，例如：
[Unreleased]: https://github.com/Dongxp-lw/keyinput/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/Dongxp-lw/keyinput/releases/tag/v0.2.0
当前尚未打 release tag。 -->
