# AGENTS.md — 给 AI 助手与协作者的工作约定

本文件给在本仓库工作的 **AI 编码助手**（及人类协作者）提供稳定的上下文与规则，使工作更可控、质量更高。它是工作约定，不替代正式文档；权威设计始终在 `docs/`。

## 真相来源（按优先级）

1. **`docs/`** — 产品、技术、安全、测试的权威设计（中文，保留英文术语）。先读 [docs/README.md](docs/README.md) 的文档地图。
2. **`.agent/`** — 开发执行管理（**非**架构权威）：
   - `state.md` — 当前状态，**每次接续工作先读**。
   - `tasks.md` — 细粒度任务板（L0–L6）。
   - `decisions.md` — 决策登记（ADR-lite，LOCKED/PENDING）。
   - `execution-plan.md` — 自顶向下分层计划。
3. **`CONTRIBUTING.md` / `docs/technical/coding-standards.md` / `release-process.md`** — 协作与发布规范。

## 硬性红线（不可违反）

- **绝不记录秘密**：主密码、密钥、字段值、TOTP 种子与码不进日志、`println!`、`Log.*`、`toString()`、崩溃报告或任何输出。
- **不自实现密码学原语**；只用审计过的库（[D-008](.agent/decisions.md)）。
- **离线按构造成立**：核心模块不声明 `INTERNET` 权限（[D-015](.agent/decisions.md)）。仅在功能确需联网时才声明，并运行时告知用户。
- **仓库内绝不放真实密钥/口令/keystore/真实测试秘密。**

## 协作约定

- **不要擅自 `git commit` 或 `git push`**，除非用户明确要求。
- 当前**仅本地 git**，不推送任何远端（当前登录的 GitHub 账号非本人，见 [D-018](.agent/decisions.md)）。可配置项集中在 [project-config.toml](project-config.toml)。
- **不要新建总结类 / changelog 类 .md 文件**来"汇报改动"，除非用户要求（`CHANGELOG.md` 例外，按规范维护）。
- 诚实区分「**编译通过**」与「**运行期已验证**」；不夸大完成度，未验证就标未验证。
- 不在事实之外迎合；上游/用户有误要直接指出。
- 高风险/不可逆/越权操作前先征求许可（见根级《Agent Constitution》）。

## 文档与语言

- 文档以**中文**为主，**保留英文技术术语**（IME、commitText、Argon2id、XChaCha20-Poly1305、TOTP、MASVS、SemVer 等）。
- 每篇设计文档声明服从 [产品决策记录](docs/product/product-decision-record.md)，并含反漂移句（自动填充/凭据 API 是兼容增强，非主路径）。
- 平台能力未证实的写「待验证」，不臆造。

## 构建与验证命令（本机已验证，详见 `.agent/state.md`）

```powershell
# Rust 核心（cargo 在 %USERPROFILE%\.cargo\bin）
cargo test -p vault-core --lib
cargo clippy -p vault-core --lib --tests -- -D warnings

# Android（Gradle，需 cargo+MinGW+ANDROID_NDK_HOME 入 PATH/env，见 state.md）
& "$env:USERPROFILE\gradle-dist\gradle-8.9\bin\gradle.bat" -p apps/android :app:assembleDebug --console=plain
```

- 提交前：`cargo fmt --all`、`cargo clippy -- -D warnings`、`cargo test`、`cargo deny check`、`:app:assembleDebug`。
- 提交信息用 **Conventional Commits** + DCO `Signed-off-by`（见 [CONTRIBUTING](CONTRIBUTING.md)）。

## 提交前自检（最小集）

- [ ] 未引入 `INTERNET` 权限（除非按 D-015 处理）。
- [ ] 未记录任何秘密、未提交任何真实秘密。
- [ ] `clippy -D warnings` 净、相关测试通过、`:app:assembleDebug` 绿。
- [ ] 用户可见变更已记入 `CHANGELOG.md` 的 `[Unreleased]`。
- [ ] 相关 `docs/` 或 `.agent/` 已同步。
