# 模块架构与层间契约（L1）

本文件是架构落地（L1）的产出：把已锁定的"一套 Rust 共享核心 + 各端原生 UI"（[技术架构](architecture.md) 第 2 节）落为**具体的模块结构、层间契约（核心暴露的 API + 平台提供的端口）、依赖方向规则**。它服从 [产品决策记录](../product/product-decision-record.md) 基线：主动选择是核心，离线优先，平台凭据与自动填充是兼容增强而非主路径。

本阶段不写代码，产出可落地的接口清单；具体 Rust 代码与 cargo/gradle 配置属于 L0/L2。

## 1. 范围与目标

- 范围内：mono-repo 模块划分、核心与原生的职责边界、核心对外 API 草案、核心所需的平台端口、依赖方向与隔离规则、对版本计划的映射。
- 不在范围内：具体 Rust/Kotlin 代码、构建脚本、UniFFI 类型映射细节（L0/L2 落地时确定）。

## 2. 依赖的决策与文档

- 架构基线（Rust 核心 + 原生 UI、绑定路径、核心/原生分工）：[技术架构](architecture.md) 第 1、2、3 节。
- 密码学、密钥层级、文件格式、会话、卫生：[安全实现设计](security-implementation-design.md)。
- 领域数据结构：[数据模型](data-model.md)。
- 各功能接口（核心 API 的来源）：`docs/implementation/` 的 11 份实现文档第 4 节。
- 决策：D-006（架构）、D-004（加密走 Rust crate）。

## 3. 模块结构（mono-repo）

```text
core/
  vault-core/         Rust crate：共享核心（无任何平台 API）
                      经 UniFFI 导出 Kotlin/Swift 绑定；HarmonyOS 经 Node-API 包装
apps/
  android/
    app/              主应用（Kotlin，原生 UI）
    keyboard/         安全键盘 IME（Kotlin，InputMethodService）
    core-bindings/    UniFFI 生成的 Kotlin 绑定 + 打包的 .so
  ios/                （v1）Swift 应用 + 键盘扩展 + UniFFI Swift 绑定
  harmony/            （v1）ArkTS 应用 + 输入法 + Node-API 绑定
tools/                打包 / CLI 辅助（评估中，见 v0.3 计划）
```

依赖方向：`apps/*` → `core-bindings` → `vault-core`。**核心不依赖任何平台模块。**

## 4. 层间契约 A：核心暴露的 API（平台调用核心）

这是 FFI 边界，把 11 个功能的接口归并为核心对外的表面。下面是**语言中立草案**，最终用 UniFFI 的 UDL 或 proc-macro 表达。值类型（`Entry`/`Field`/`TotpField`/`PasswordPolicy`/`TransferPackage`/`SyncObject` 等）在核心定义，经绑定映射到各端。

```text
VaultCore（核心对外对象，方法按功能归并）
  // 保险库生命周期（VAULT）
  create_vault(master_password) -> VaultHandle
  open(bytes, master_password) -> VaultHandle            // 失败即口令错误/篡改
  save(handle) -> bytes
  change_password(handle, old, new) -> bytes
  // 解锁会话（LOCK）
  lock(handle)
  unlock_with_password(bytes, master_password) -> VaultHandle
  unlock_with_platform_key(bytes, unwrapped_key) -> VaultHandle  // 生物识别：平台解包后的密钥
  // 条目与字段（ENTRY）
  list_entries(handle) -> [EntrySummary]
  get_entry(handle, id) -> Entry
  search(handle, query) -> [EntrySummary]                // 仅本地
  upsert_entry(handle, entry)
  delete_entry(handle, id)
  get_field_value(handle, entryId, fieldId) -> secret
  // 生成器（GEN）
  generate_password(policy) -> secret
  // TOTP
  totp_now(field, now_epoch_seconds) -> TotpCode
  // 导出/导入（IMEX）
  export_package(handle, passphrase) -> bytes
  inspect_package(bytes) -> TransferPackageHeader
  import_package(handle, bytes, passphrase, resolution) -> ImportResult
  // 迁移（MIGR）
  create_transfer_package(handle, passphrase) -> bytes
  start_pairing(label) -> PairingInfo
  receive_and_verify(bytes, pairing) -> MigrationPreview
  import_with_resolution(resolution) -> MigrationResult
  // 同步（SYNC）
  create_sync_payload(change) -> SyncObject
  detect_conflict(local, remote) -> SyncConflict
  apply_pull(objects) -> SyncResult
  resolve_conflict(item, resolution) -> SyncResult
```

说明：这些一一对应各实现文档第 4 节的接口，只是收敛到单一 FFI 表面。秘密（密码、字段值、TOTP 码、迁移/同步明文）尽量留在核心内存；跨 FFI 传出时最小化并要求调用方用后清零。

## 5. 层间契约 B：核心所需的平台端口（平台提供给核心）

核心保持平台无关，平台能力以"端口"形式提供，核心不直接调用平台 API：

| 端口 | 谁实现 | 说明 |
| --- | --- | --- |
| 安全存储 / 硬件绑定（BIO） | 平台 | 平台用 Keystore/Keychain/HUKS 包装与解包"本地解锁密钥"；核心只接收解包后的密钥字节（`unlock_with_platform_key`），硬件绑定与 `BiometricPrompt` 全在平台。 |
| 时间源（TOTP） | 平台注入 | 核心接受注入的 `now_epoch_seconds`，便于已知答案测试；生产由平台传设备时间。 |
| 随机源（CSPRNG） | 核心内 | 在 Rust 核心内（RustCrypto/`getrandom`），不依赖平台。 |
| 文件持久化 | 平台 | 核心只做 bytes 进/出（编解码 + 加解密）；写文件/读文件、目录选择由平台负责，核心无文件 IO 平台依赖。 |
| 网络传输（SYNC/MIGR 通道） | 平台 | 核心只产出/消费密文；上传下载、本地传输通道、账号会话在平台。 |
| UI / 键盘插入 / 剪贴板 | 平台 | 纯平台，不进核心。 |

## 6. 谁在核心 / 谁在原生（对照 11 功能）

| 功能 | 核心（Rust，写一次） | 原生（各端各写） |
| --- | --- | --- |
| [KBD 安全键盘](../implementation/secure-keyboard-ime.md) | — | IME：解锁交接、调核心搜索、`commitText` 插入 |
| [VAULT 加密核心](../implementation/vault-crypto-core.md) | 全部（KDF/AEAD/信封/格式） | — |
| [LOCK 解锁会话](../implementation/master-password-unlock.md) | 密钥派生/解包/会话密钥 | 生物识别、Keystore 包装、自动锁定生命周期触发 |
| [BIO 生物识别](../implementation/biometric-unlock.md) | 收/交密钥字节 | 全部：`BiometricPrompt` + Keystore/Keychain/HUKS |
| [ENTRY 条目字段](../implementation/entry-field-model.md) | 全部（模型+编解码+CRUD+搜索） | — |
| [GEN 密码生成](../implementation/password-generator.md) | 全部 | 仅展示 |
| [TOTP](../implementation/totp-generation.md) | 全部 | 仅展示倒计时 |
| [IMEX 导出导入](../implementation/encrypted-export-import.md) | 全部（信封/校验/回滚） | 文件选择 IO |
| [CLIP 剪贴板](../implementation/clipboard-fallback.md) | — | 全部（含敏感标记、自动清除） |
| [MIGR 迁移](../implementation/cross-device-migration.md) | 包格式/校验/合并逻辑 | 二维码扫描 UI、本地传输通道 |
| [SYNC 同步](../implementation/cloud-sync.md) | 载荷加密/冲突检测/合并 | 网络上传下载、账号会话 UI |

## 7. 依赖方向与隔离规则

- 平台 → 核心，**单向**，经 FFI；核心**绝不** import 任何 Android/iOS/HarmonyOS API。
- 核心无 UI、无文件 IO 平台依赖、无网络；这些由平台做，核心只处理 bytes 与领域逻辑。
- 秘密尽量留在核心内存（Rust 可清零）；跨 FFI 传递秘密最小化、用后清零（绑定层缓冲也需注意）。
- 核心可做跨平台确定性单元测试，不需要设备。

## 8. 对版本计划的映射

- v0.1：`vault-core` 最小骨架 + Android `keyboard` 原型（经 FFI 调核心的最小子集）。
- v0.2 MVP：核心 VAULT/ENTRY/LOCK/GEN/TOTP/IMEX + Android `app`/`keyboard`/BIO/CLIP。
- v0.3：MIGR。v1.0：发布门禁。v1.1：SYNC。

## 9. 待验证与不在范围

- UniFFI 对复杂类型、回调、错误与可空类型的具体映射，在 L0/L2 落地时确认。
- HarmonyOS 的 Rust 目标工具链可行性 spike（Node-API 路径已确认）。
- 跨 FFI 传递秘密的清零保证（Rust 侧可清零，绑定层缓冲需评估）。
- 不含具体 Rust 代码与 cargo/gradle 配置（属 L0/L2）。
