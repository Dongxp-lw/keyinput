# 编码规范

本文件汇总 Private Input Vault 的**代码风格、命名、错误处理、安全编码与测试约定**，是 [工程基础](engineering-foundation.md) §5 的落地细化，供 [贡献指南](../../CONTRIBUTING.md) 引用。

## 1. 通用

- 编辑器基础风格由 [.editorconfig](../../.editorconfig) 统一：UTF-8、LF、行尾去空白、文件末尾留空行、4 空格缩进（配置/数据文件 2 空格）。
- 单一职责、小步提交；公共 API 与非显然逻辑写注释说明**为什么**，而非复述代码。
- 不留无用代码、不留 `TODO` 而无 issue 跟踪。

## 2. Rust（共享核心）

- 格式：`cargo fmt --all`（CI 校验；阶段性目标是把 fmt 门禁从建议性转为阻断，见下「现状」）。
- 静态检查：`cargo clippy --all-targets -- -D warnings` 必须无告警。
- 错误处理：对外统一 `VaultError` / `VaultResult`（[error.rs]）；**错口令与篡改不可区分**（都映射 `WrongPasswordOrTampered`，安全要求，有测试）。
- 不 `unwrap()`/`expect()` 于可恢复路径；`panic!` 仅用于不可能发生的不变量。
- 秘密类型：用 `zeroize`（`Zeroizing`/`ZeroizeOnDrop`）承载，`Debug` 必须**遮蔽**（不得 derive 出会打印明文的 `Debug`）；常量时间比较用 `subtle`。
- 不自实现密码学原语（[D-008](../../.agent/decisions.md)）。
- 跨版本兼容：序列化保留未知字段（`#[serde(flatten)]` + `BTreeMap<String, ciborium::Value>`，[D-005](../../.agent/decisions.md)）。
- 时间由上层注入，核心不读时钟（便于测试与确定性）。
- 测试：领域逻辑配单元测试；**密码学必须用权威标准向量 KAT**（RFC/草案原文，非自洽生成）。

## 3. Kotlin / Android

- 风格：ktlint / detekt（官方 4 空格风格）；Android lint 纳入 CI（建议性起步）。
- 秘密：用 `ByteArray`/`CharArray`，**不用 `String`**；用后 `fill(0)` 清零；不 `toString()` 暴露。
- 不在主线程做磁盘/网络 IO（开发构建 `StrictMode` 会抓）。
- Compose：状态上提、副作用进 `LaunchedEffect`/`DisposableEffect`；避免在组合期做 IO。
- 协程：IO/CPU 切到合适 dispatcher（核心调用经 `:vault-data` 切 `Dispatchers.Default`）。
- 异常：`VaultException` 映射为可读中文提示，不向用户泄漏内部细节。

## 4. 命名与命名空间

见 [工程基础](engineering-foundation.md) §5.1：应用 ID `com.lincdkeyinput`（[D-012](../../.agent/decisions.md)）；模块 `…app` / `…keyboard` / `…data`；Rust crate `vault-core`、UniFFI 包 `vault_core`。功能短码：`KBD VAULT LOCK BIO ENTRY GEN TOTP IMEX CLIP MIGR SYNC`。

## 5. 安全编码红线（硬性）

见 [贡献指南](../../CONTRIBUTING.md) §8 与 [安全实现设计](security-implementation-design.md) §7：

- **绝不记录秘密**：主密码/密钥/字段值/TOTP 种子与码不进日志、`println!`、`Log.*`、`toString()`、崩溃报告。
- **离线按构造成立**：核心模块不声明 `INTERNET`（[D-015](../../.agent/decisions.md)）。
- **仓库内绝不放真实密钥/口令/keystore**。
- 导入/迁移**失败不破坏现有库**（原子写入）。

## 6. 提交与 PR

提交信息用 **Conventional Commits**、带 DCO `Signed-off-by`；PR 流程与自检清单见 [贡献指南](../../CONTRIBUTING.md) §4–7。

## 7. 现状与待办

- `rustfmt`：存量代码尚未做过一次性 `cargo fmt --all`，故 CI 的 fmt 门禁**暂为建议性**；待一次性格式化后转为阻断。`clippy -D warnings` 已是阻断项且通过。
- detekt/ktlint 配置文件待接入（CI 中预留）。
- `cargo deny`：配置见 [deny.toml](../../deny.toml)，已接入 CI 供应链 job。

[error.rs]: ../../core/vault-core/src/error.rs
