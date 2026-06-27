# 发布流程与版本号策略

本文件定义 Private Input Vault 的**版本号规则、分支与 tag 约定、发布门禁、发布步骤（runbook）、签名与密钥管理、回滚**。它服从 [产品决策记录](../product/product-decision-record.md) 与各版本计划，并与 [v1.0 发布计划](../product/v1.0-release-plan.md) 的发布门禁衔接。

## 1. 版本号策略（SemVer）

采用 [语义化版本 SemVer 2.0.0](https://semver.org/lang/zh-CN/)：`MAJOR.MINOR.PATCH`。

- **MAJOR**：不兼容的变更（保险库文件格式、迁移包格式、对外契约的破坏性变化）。
- **MINOR**：向后兼容的新功能。
- **PATCH**：向后兼容的缺陷/安全修复。
- **1.0.0 之前（0.y.z）**：API 与格式可能变动；以 `MINOR` 表里程碑（见下），`PATCH` 表修复。

文件格式与 schema 另有独立版本（`formatVersion`/`schemaVersion`，见 [数据模型](data-model.md) 与各 `Header`），**不**与应用 SemVer 绑定，但破坏性的格式变更须伴随应用 `MAJOR`（1.0 后）并提供迁移。

### Android 版本字段

- `versionName` = SemVer 字符串（如 `0.2.0`）。
- `versionCode` = **单调递增整数**，发布即 +1，永不回退。建议规则：`MAJOR*10000 + MINOR*100 + PATCH`（如 `0.2.0` → 200；`1.0.0` → 10000），便于人读且单调。
- 二者在 [apps/android/app/build.gradle.kts](../../apps/android/app/build.gradle.kts) `defaultConfig` 中维护。
- 项目级可配置项（版权署名、联系方式、远端仓库、应用 ID 等）集中登记在 [项目配置](../../project-config.toml)，发布前在此统一核对。

### 版本 ↔ 里程碑映射

| 里程碑 | 版本 | 计划 |
| --- | --- | --- |
| 原型 | 0.1.x | [v0.1 原型计划](../product/v0.1-prototype-plan.md) |
| 离线 MVP | 0.2.x | [v0.2 MVP 计划](../product/v0.2-mvp-plan.md) |
| 跨设备迁移 | 0.3.x | [v0.3 迁移计划](../product/v0.3-migration-plan.md) |
| 公开发布 | 1.0.x | [v1.0 发布计划](../product/v1.0-release-plan.md) |
| 云同步 | 1.1.x | [v1.1 云同步计划](../product/v1.1-cloud-sync-plan.md) |

## 2. 分支与 tag

- `main`：受保护、始终可构建、CI 绿。
- 特性分支：短期 `feat/…`、`fix/…` 等（见 [CONTRIBUTING](../../CONTRIBUTING.md) §3）。
- 发布 tag：`vMAJOR.MINOR.PATCH`（如 `v0.2.0`），带注释（annotated）并签名（`git tag -s`）。
- 可选发布分支：仅当需要在主线继续前进的同时维护某条已发布线时，才开 `release/0.2.x`；MVP 阶段通常直接在 `main` 上打 tag。

## 3. 发布前门禁（必须全绿）

- CI 全绿：构建、`clippy -D warnings`、`cargo test`（含 KAT）、Android lint、`cargo deny check`、安全门（日志扫描无秘密、无 `INTERNET` 权限）。
- 退出标准回归：对应版本计划的验收（如 v0.2 见 L6-03），关键流程运行期实测。
- 安全门禁（面向 1.0）：[v1.0 发布计划](../product/v1.0-release-plan.md) 的 MASVS 对齐项——正式签名、关闭 `debuggable`、生产构建移除调试日志、可复现构建、抓包零流量。
- `CHANGELOG.md` 的 `[Unreleased]` 已整理为对应版本小节。
- 文档与决策（`.agent/decisions.md`）无未决冲突。

## 4. 发布步骤（runbook）

1. 确认门禁（§3）全部满足。
2. **定版**：更新 [build.gradle.kts](../../apps/android/app/build.gradle.kts) 的 `versionName` 与 `versionCode`；如有，更新 crate 版本。
3. **更新 CHANGELOG**：把 `[Unreleased]` 改写为 `[X.Y.Z] - YYYY-MM-DD`，并新建空的 `[Unreleased]`。
4. 提 PR（`chore(release): X.Y.Z`）→ 评审 → 合入 `main`。
5. **打 tag**：`git tag -s vX.Y.Z -m "X.Y.Z"` 并推送。
6. **构建 release**：生产配置（关闭 `debuggable`、移除调试日志）；Rust 以 release 交叉编译各 ABI。
7. **签名**：用发布 keystore 对 APK/AAB 正式签名（见 §5）。
8. **验证**：可复现构建校验；运行期抓包零流量；logcat 无敏感串；安装冒烟。
9. **产物校验**：发布产物记录 SHA-256；如有 AAB/APK 一并归档。
10. **分发**：上架（Play / 其它渠道）或附加到 GitHub Release（含 CHANGELOG 摘录与校验和）。
11. **发布后**：确认 `versionCode` 已落、tag 与 Release 对应；必要时公告。

## 5. 签名与密钥管理

- 发布 keystore（`*.jks`/`*.keystore`）与口令**绝不入库**（`.gitignore` 已忽略）。
- 本地由发布者离线保管；CI 签名走仓库/组织的 **encrypted secrets**，不落明文、不打印。
- 应用 ID `com.lincdkeyinput`（[D-012](../../.agent/decisions.md)，发布前定稿）一经发布不可改。
- 丢失 keystore 将无法更新已上架应用——务必离线多副本备份。

## 6. 回滚

- 应用层：发现严重问题时，发补丁版（`PATCH` +1）前滚修复；`versionCode` 只增不减，不能"回退版本号"。
- 数据层：导入/迁移设计为**失败不破坏现有库**（原子写入），用户数据风险可控。
- 渠道层：必要时在商店下架问题版本并尽快发修复版。

## 7. 当前状态

- 截至 2026-06-27，仓库**尚未打任何 release tag**；`0.2.0` 为开发里程碑（`versionCode=1`）。
- 首个正式发布将据本流程定版、更新 `CHANGELOG.md` 并打 `v0.2.0`（或届时的版本）。
