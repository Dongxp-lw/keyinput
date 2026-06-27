# 贡献指南（Contributing）

感谢你愿意为 **Private Input Vault** 贡献。本项目是一个**离线优先的私密输入键盘与加密保险库**，安全是第一要务。请在动手前通读本指南。

参与即表示你同意遵守 [行为准则](CODE_OF_CONDUCT.md)。

---

## 1. 在你开始之前

- 先读懂产品基线：[产品决策记录](docs/product/product-decision-record.md)（离线优先、用户主动选择输入、本地优先加密、不承诺识别钓鱼）。任何贡献不得偏离基线。
- 大改动请先开一个 issue 讨论方案，再动手，避免做无用功。
- 安全漏洞**不要**走公开 issue / PR，见 [SECURITY.md](SECURITY.md)。

## 2. 开发环境

完整工具链基线见 [工程基础](docs/technical/engineering-foundation.md) §4；本机已验证的版本与构建命令见 `.agent/state.md`「开发环境」。要点：

- **Rust**：stable toolchain（`rust-toolchain.toml` 固定）。
- **Android**：SDK（compileSdk/targetSdk 35、minSdk 28）、NDK r27、JDK 17。
- **构建集成**：`cargo-ndk` 交叉编译 4 个 ABI 的 `.so`，`uniffi-bindgen` 生成 Kotlin 绑定，由 Gradle 任务驱动。

常用命令：

```bash
# Rust 共享核心
cargo test -p vault-core --lib
cargo clippy -p vault-core --lib --tests -- -D warnings
cargo fmt --all                # 提交前格式化

# Android（在 apps/android 下）
./gradlew :app:assembleDebug
./gradlew :app:lintDebug

# 供应链（需 cargo install cargo-deny）
cargo deny check
```

## 3. 分支策略

- `main` 为受保护的主分支，始终保持可构建、CI 绿。
- 从 `main` 切**短期特性分支**，命名 `type/简短描述`，与提交类型一致：
  - `feat/totp-issuer-field`、`fix/export-autolock`、`docs/release-process`、`chore/bump-deps` 等。
- 通过 **Pull Request** 合入 `main`，**squash merge**（保持线性历史）。
- 直接向 `main` push 仅限维护者在极少数情况下。

## 4. 提交信息：Conventional Commits

采用 [Conventional Commits 1.0.0](https://www.conventionalcommits.org/zh-hans/v1.0.0/)：

```
<type>(<scope>): <subject>

<body 可选：动机与对比>

<footer 可选：BREAKING CHANGE / Closes #123 / Signed-off-by>
```

- **type**：`feat` `fix` `docs` `style` `refactor` `perf` `test` `build` `ci` `chore` `revert`。
- **scope**（可选）：用功能短码 `KBD VAULT LOCK BIO ENTRY GEN TOTP IMEX CLIP MIGR SYNC`，或模块名 `core` `app` `keyboard` `vault-data` `ci`。
- **subject**：祈使句、简洁；中文或英文皆可；结尾不加句号。
- 破坏性变更：footer 写 `BREAKING CHANGE: …`，或 type 后加 `!`（如 `feat(core)!: …`）。

示例：

```
feat(TOTP): 支持 SHA-256/512 与 8 位验证码
fix(IMEX): 导出/导入时豁免一次后台自动锁定，修复 0 字节导出
docs: 新增发布流程与版本号策略
```

## 5. 开发者来源证明（DCO / Sign-off）

提交需带 `Signed-off-by`，表示你声明 [DCO](https://developercertificate.org/)（你有权贡献这段代码）：

```bash
git commit -s -m "fix(IMEX): ..."
```

## 6. 提交前自检

- [ ] `cargo fmt --all` 已格式化；`cargo clippy -- -D warnings` 无告警；`cargo test` 通过（涉及 Rust）。
- [ ] `./gradlew :app:assembleDebug` 通过（涉及 Android）。
- [ ] **未引入 `INTERNET` 权限**（除非该功能确需联网且按 [D-015](.agent/decisions.md) 在运行时告知用户）。
- [ ] **未记录任何秘密**（见 §8）。
- [ ] **未提交任何真实密钥/口令/keystore/真实测试秘密**。
- [ ] 用户可见变更已更新 `CHANGELOG.md` 的 `[Unreleased]`。
- [ ] 相关文档已更新（`docs/`，或实现文档第 9 节）。

## 7. Pull Request 流程

1. 小步提交，一个 PR 聚焦一件事。
2. 填写 PR 模板，关联 issue。
3. CI 必须全绿；至少 **1 名维护者**评审通过（安全相关路径见 `CODEOWNERS`，需安全负责人评审）。
4. 维护者 squash merge。

## 8. 安全红线（硬性要求）

这是安全产品，以下为不可逾越的红线：

- **绝不记录秘密**：主密码、密钥、字段值、TOTP 种子与生成码**绝不**进日志、`println!`、`Log.*`、`toString()`、崩溃报告。秘密用可清零、`Debug` 被遮蔽的类型承载（Rust：`zeroize` + 遮蔽 `Debug`；Kotlin：`ByteArray`/`CharArray`，用后 `fill(0)`）。详见 [安全实现设计](docs/technical/security-implementation-design.md) §7、[工程基础](docs/technical/engineering-foundation.md) §5.3。
- **不自实现密码学原语**：只在审计过的库间选择（[D-008](.agent/decisions.md)）。
- **离线按构造成立**：核心模块不声明网络权限（[D-015](.agent/decisions.md)）。
- **仓库内绝不放真实密钥/口令**。

## 9. 代码规范

详见 [编码规范](docs/technical/coding-standards.md)。要点：Rust 用 `rustfmt`+`clippy -D warnings`；Kotlin 用 ktlint/detekt；命名见 [工程基础](docs/technical/engineering-foundation.md) §5.1；错误经统一 `VaultError`/`VaultException`，错口令与篡改不可区分。

## 10. 测试

- 核心密码学变更必须配标准向量 KAT（权威来源，非自洽生成）。
- 领域逻辑配单元测试；平台交互必要时配 instrumented 测试。
- 测试约定见 [测试策略](docs/testing/test-strategy.md) 与 [测试计划](docs/testing/test-plan.md)。

## 11. 文档约定

- 文档以**中文**为主，**保留英文技术术语**（IME、commitText、Argon2id、TOTP、MASVS 等）。
- 架构/产品/安全的权威文档在 `docs/`；`.agent/` 是开发执行管理（state/tasks/decisions），不替代 `docs/`。

## 12. 许可

你提交的贡献将按本项目的 [Apache License 2.0](LICENSE) 授权（见 LICENSE §5 Submission of Contributions）。
