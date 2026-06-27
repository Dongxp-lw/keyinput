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
| L0-05e | CI：构建 + clippy + detekt + 日志扫描 + 无 INTERNET 校验 | 部分（workflow 已写 + 本地门验证；真实 runner 未验证） | L0-05d | `.github/workflows/ci.yml`（4 job：**rust-core** fmt[建议]/clippy `-D warnings`/test-KAT；**android** assembleDebug+lint[建议]；**security-gates** 日志扫描+无 INTERNET；**supply-chain** cargo-deny）。本地已复现验证：security-gates 全 0、clippy 净、88/88。**未在真实 GitHub runner 跑通**（无 CI 环境）；fmt 因存量代码未格式化暂作建议性（待一次性 `cargo fmt`），detekt/ktlint 待接入 |
| L0-06 | 工程治理规范集（许可/贡献/安全/发布/CHANGELOG/CoC/编码规范/供应链/GitHub 模板） | DONE（D-016/D-017，2026-06-27；占位待替换真实联系方式/团队/URL） | L0-04 | `LICENSE`(Apache-2.0)+`NOTICE`；`CONTRIBUTING.md`(Conventional Commits+DCO+分支/PR)；`SECURITY.md`；`CODE_OF_CONDUCT.md`(Contributor Covenant 2.1)；`CHANGELOG.md`(Keep a Changelog)；`AGENTS.md`；`.editorconfig`；`deny.toml`(cargo-deny)；`.github/`(PR/issue 模板、CODEOWNERS、dependabot)；`docs/technical/release-process.md`+`coding-standards.md`；CI 加 supply-chain job；README 加贡献/安全/许可 |

注：L0 纸面设计已完成（L0-01..04）。L0-05a/b/c/d 已落地并验证（2026-06-24；完整 Rust→UniFFI→Kotlin→Android 链路在 API 36 模拟器跑通 ping）；L0-05e（CI）+ L0-06（治理规范）已落地；CI 真实 runner 待跑。

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
| L2-03 | 序列化（CBOR via `ciborium`，保留未知字段，D-005） | DONE | L2-01 | `codec.rs` 落地；5 序列化测试通过（含跨版本未知字段保留）；`cargo test -p vault-core --lib` 18/18，`clippy -D warnings` 通过，`cargo ndk -t arm64-v8a build -p vault-core --release` 通过（2026-06-25） |
| L2-04 | 错误模型与结果类型（跨 FFI） | DONE | L0-05a、L2-02、L2-03 | `error.rs`：`VaultError`（5 变体）+ `VaultResult`；口令错/篡改合并为不可区分 `WrongPasswordOrTampered`，损坏→`Corrupt`，不兼容→`IncompatibleVersion`；`From<CryptoError/CodecError>`；6 测试通过（24/24），clippy 净，UniFFI 生成 Kotlin `sealed class VaultException`（5 变体 + FfiConverter + ErrorHandler），arm64 交叉编译通过（2026-06-25） |
| L2-05 | CSPRNG 与无偏置采样 | DONE | L2-01 | `rng.rs`：`uniform_index(bound)` 拒绝采样避免取模偏置（bound=0→InvalidInput）；4 测试含 60k 样本分布检验（±10%）（2026-06-25） |
| L2-06 | `VaultCore` FFI 表面（接真实核心实现，非空） | DONE | L2-04、L3-* | `ffi.rs`：`VaultCore`（uniffi::Object，`Mutex<Option<Session>>`）+ DTO（FfiEntry/FfiField/EntrySummary/… uniffi::Record）+ 5 枚举添 uniffi::Enum + 独立函数 generate_password/totp_now/inspect_package。导出 create/unlock/lock/save/change_password、list/get/search/upsert/delete、get_field_value、export/import、touch/is_idle_expired；口令=ByteArray 用后 zeroize；upsert 按 id 保留未知字段；Locked 错误。重生成 Kotlin：`class VaultCore` + DTO data class + 枚举 + 顶层函数（Vec<u8>→ByteArray）验证可调。7 新测试（总 83/83，含跨 FFI 未知字段保留），clippy `-D warnings` 净，arm64 交叉编译通过（2026-06-25） |

## L3 领域核心（Rust，纯逻辑可测）

排期 ENTRY → VAULT → LOCK → GEN → TOTP → IMEX；功能细分见各实现文档第 9 节（用 Rust 实现，非 Kotlin）。

| ID | 任务 | 状态 | 依赖 | 实现文档（细分任务） | 验收证据 |
| --- | --- | --- | --- | --- | --- |
| L3-ENTRY | 条目/字段模型 + 编解码 + CRUD + 本地搜索 | DONE | L2-03、L2-04 | [条目与字段模型](../docs/implementation/entry-field-model.md) §9（ENTRY-01..07） | `entry.rs`（Entry/Field/TotpField/枚举 + 默认策略 §6 + VaultContent 编解码）+ `repository.rs`（CRUD + 本地搜索）；往返一致、未知字段保留（嵌套 flatten 注入测试）、搜索不检索高敏值；13 新测试（总 37/37），clippy `-D warnings` 净，arm64 交叉编译通过（2026-06-25） |
| L3-VAULT | 保险库格式 + KDF + 信封 + AEAD + 篡改检测 | DONE | L2-02、L3-ENTRY | [保险库加密核心](../docs/implementation/vault-crypto-core.md) §9（VAULT-01..08） | `vault.rs`：VaultFile/VaultHeader/CryptoProfile + `Vault::create/create_with_params/open/save/change_password`；头部作 AAD、DEK→HKDF 内容子密钥、错口令与篡改不可区分、版本/算法门禁不改动入参、明文用后 zeroize、遮蔽 Debug；11 新测试（总 48/48），clippy `-D warnings` 净，arm64 交叉编译通过。标准 KAT 向量留 L6-01（2026-06-25） |
| L3-LOCK | 解锁会话密钥逻辑（核心侧） | DONE | L3-VAULT | [主密码与解锁会话](../docs/implementation/master-password-unlock.md) §9 | `lock.rs`：`Session::unlock`（用 `Vault::open`，错口令不可区分）+ 最小持有（仅 DEK，无 KEK/主密码）+ `touch`/`is_idle_expired`（纯空闲判定供平台自锁）+ Drop/`lock` 清零字段明文与 TOTP 种子（`VaultContent::zeroize_secrets`）+ 遮蔽 Debug；自锁计时/生命周期/键盘授权/生物识别/退避 UI 在平台（L4）。6 新测试（总 54/54），clippy `-D warnings` 净，arm64 交叉编译通过（2026-06-25） |
| L3-GEN | 密码生成器 | DONE | L2-05 | [密码生成器](../docs/implementation/password-generator.md) §9（GEN-01..04） | `generator.rs`：`PasswordPolicy` + `generate` 返回 `Zeroizing<String>`（Drop 清零）；字符集组装/排除易混淆、拒绝采样无偏置、至少一类/长度下限校验；6 新测试含分布无偏置（总 64/64），clippy `-D warnings` 净，arm64 交叉编译通过（2026-06-25） |
| L3-TOTP | TOTP 生成 | DONE | L2-02 | [基础 TOTP](../docs/implementation/totp-generation.md) §9（TOTP-01..05） | `totp.rs`：HOTP 动态截断 + `generate`（HMAC-SHA1/256/512 via `hmac 0.13`+`sha1 0.11`+`sha2 0.11`，8 字节大端计数器，时间注入，floorDiv）；RFC 6238 附录 B 全 12 向量（SHA1/256/512×4 时间，含 2603 年 64 位时间步）通过；digits/period/种子长校验；中间缓冲 zeroize。4 新测试（总 68/68），clippy `-D warnings` 净，arm64 交叉编译通过（2026-06-25） |
| L3-IMEX | 加密导出/导入包 | DONE | L3-VAULT、L3-ENTRY | [加密导出与导入](../docs/implementation/encrypted-export-import.md) §9（IMEX-01..06） | `imex.rs`：`TransferPackage`（独立 salt/DEK/口令，复用 VAULT `CryptoProfile`）+ `export/export_with_params/inspect/import`；迁移子密钥用 `SUBKEY_MIGRATION` 与本地内容密钥域分离；导出不可明文读、错口令=篡改不可区分、版本门禁、`import` 无副作用（失败不动现有库，原子写入归平台）；明文用后 zeroize。8 新测试（总 76/76），clippy `-D warnings` 净，arm64 交叉编译通过（2026-06-25） |

## L4 平台集成（Android 原生）

| ID | 任务 | 状态 | 依赖 | 实现文档 | 验收证据 |
| --- | --- | --- | --- | --- | --- |
| L4-FFI | core-bindings 消费 + 会话桥接 | DONE（编译验证；运行期待验证） | L0-05d、L3-VAULT | [模块架构](../docs/technical/module-architecture.md) §4-5 | `:vault-data` 的 `VaultManager` 调全 `VaultCore` 表面 + `VaultStore` 持久化；`assembleDebug` 绿 |
| L4-KBD | 安全键盘 IME | DONE（运行期已验证，API 36） | L4-FFI | [安全键盘输入法](../docs/implementation/secure-keyboard-ime.md) §9 | `VaultImeService`（独立会话+内置解锁键盘+`commitText` 直填，不过剪贴板）；打包清单含 IME service + BIND_INPUT_METHOD + InputMethod intent-filter。**运行期端到端验证（API 36，2026-06-26）**：自绘键盘渲染→独立 Argon2id 解锁（错口令正确拒绝、正确口令进条目列表）→选条目/字段→`commitText` 直填第三方输入框，TOTP 直填码 `581456` 与独立 HMAC-SHA1（counter=59418329）逐位一致 |
| L4-BIO | 生物识别解锁 | DONE（运行期已验证，API 36） | L4-FFI | [生物识别解锁](../docs/implementation/biometric-unlock.md) §9 | `BiometricGate`（Keystore AES-GCM、用户认证必需、新注册失效→降级主密码、CryptoObject）；解锁界面 + 设置启用/关闭已接线。**运行期验证（API 36，2026-06-27）**：解锁屏「用生物识别解锁」→系统 BiometricPrompt（含 Fingerprint sensor + 「用主密码」兑底）→`adb emu finger touch 1`→解锁回列表（两次复现）；设置页显「已启用」 |
| L4-CLIP | 剪贴板兜底 | DONE（编译验证；运行期部分：遮蔽/敏感标记已验证，30s 清除不可观测） | L4-FFI | [剪贴板兜底](../docs/implementation/clipboard-fallback.md) §9（CLIP-01..04） | `ClipboardHelper`：API33+ 敏感标记 + 约 30s 自动清除；详情页复制已接线。**运行期（API 36）**：复制预览遮蔽为 `••••••`（EXTRA_IS_SENSITIVE 生效）+「约30s自清」提示已验证；**30s 实际清除无法无人值守观测**（API 36 无 `cmd clipboard get-text`、`dumpsys clipboard` 空、shell 不能读前台剪贴板）——由代码路径+遮蔽+提示佐证，若需铁证需写 instrumentation 测试 |
| L4-APP | 主应用 UI + 持久化接线 | DONE（编译验证；运行期部分；导出/导入缺陷已修复验证） | L4-FFI | 信息架构/核心交互 | Compose：引导/解锁/列表+搜索/详情(揭示·复制·TOTP 倒计时)/编辑+生成器/设置(改密+导出导入)；后台自动锁定；`assembleDebug` 绿。**缺陷已修（2026-06-27）**：原 `VaultApplication.onStop` 无条件 `lock()` 与导出/导入的 SAF 选择器（DocumentsUI，跨进程）冲突→选择器返回后会话已锁→导出写 0 字节。修法：`VaultApplication` 加一次性 `suppressNextBackgroundLock`，`VaultViewModel.armSafPicker()` 在开 `CreateDocument`/`OpenDocument` 前置位，`onStop` 放行一次即复位（QR/BiometricPrompt 本进程不触发 onStop，不受影响）。重装实测：导出 1094B 加密包、导入整库还原、App 不被锁 |

## L5 跨设备与云

| ID | 任务 | 状态 | 依赖 | 实现文档 | 验收证据 |
| --- | --- | --- | --- | --- | --- |
| L5-MIGR | 跨设备迁移（v0.3） | TODO | L3-IMEX、L4-APP | [跨设备迁移](../docs/implementation/cross-device-migration.md) §9（MIGR-01..07） | TP-301..308 |
| L5-SYNC | 云同步（v1.1） | TODO | L5-MIGR | [云同步](../docs/implementation/cloud-sync.md) §9（SYNC-01..08） | TP-201..206 |

## L6 测试与发布门禁

| ID | 任务 | 状态 | 依赖 | 验收证据 |
| --- | --- | --- | --- | --- |
| L6-01 | 核心已知答案测试套件（KDF/AEAD/TOTP） | DONE | L2-02、L3-TOTP | 权威向量逐字节通过：Argon2id=RFC 9106 §5.3；XChaCha20-Poly1305=draft-arciszewski-xchacha-03 §A.1（经我们的 `open` 验证）；HKDF-SHA256=RFC 5869 TC1/TC3；TOTP=RFC 6238 附录 B（12 向量，L3-TOTP 已含）。另加 `derive_kek`/`derive_subkey` 接线检验。`cargo test -p vault-core --lib` 88/88，clippy `-D warnings` 净（KAT 为 `#[cfg(test)]`，不入 cdylib/arm64）。2026-06-26 |
| L6-02 | 离线与隐私验证（无 INTERNET、抓包、日志扫描、StrictMode） | 部分（静态完成 + 运行期大部分；仅抓包待做） | L4-APP | 静态已验证：源码与打包清单均**无 `INTERNET`**（仅 CAMERA+biometric）；**代码零日志**；`StrictMode`（`buildConfig=true`+`detectAll`+`penaltyLog`）；`usesCleartextTraffic=false`+`network_security_config`。**运行期已验证（API 36 模拟器冒烟 2026-06-26）**：主密码 `Passw0rd1234` 在 5827 行 logcat **0 次出现**（含我方 PID）；`StrictMode` 实际触发（捕获 `ViewModel.init` 主线程磁盘读，penaltyLog）。**仅剩**：真机抓包零流量（需代理） |
| L6-03 | v0.2 MVP 退出标准回归 | 部分（核心流程+键盘直填+改/删条目+生物识别+导出导入+扫码集成 均已验证；剪贴板30s实际清除/二维码解码/拓包 受限不可无人值守测） | L4-* | API 36 模拟器冒烟（2026-06-26/27）已验证：①离线创建保险库（Argon2id→`vault.pivault` 343B 落盘）；④主密码解锁（证加解密往返）；②口令与 TOTP 码不入日志（0 次）；③核心测试 88/88+KAT；⑤**条目 CRUD+持久化**（新建+【改标题 GitHub2→GitHubEdited，锁定→生物识别解锁后仍在，证落盘】+【**删除**→软删除确认→列表空「还没有条目」，锁定/生物识别解锁后仍空，证删除落盘】）；**TOTP 实时码 `610492`/`581456`/`727660` 与独立 HMAC-SHA1 逐位一致**；**复制**（遮蔽 `••••••`+「约30s自清」）；**搜索**（zzz→无、Git→GitHub）；**安全键盘直填**（自绘键盘→独立解锁→`commitText` 不过剪贴板，以截图为准）；**生物识别解锁（L4-BIO）**（BiometricPrompt→`finger touch 1`→解锁）；**导出/导入**（缺陷已修：导出 1094B 加密包无明文、导入还原 GitHubEdited+TOTP 种子逐位、全程不被锁）；**二维码扫码集成**（扫码按钮→CAMERA 运行时授权→zxing `CaptureActivity` 相机预览启动「对准 2FA 二维码」，本进程不误锁；取消回编辑页仍解锁）；⑥TOTP RFC 向量；⑦导入失败不破坏库（核心测试）。**受限不可无人值守测**：剪贴板 30s 实际清除（API 36 adb 读不了剪贴板：无 `cmd clipboard`、`dumpsys clipboard` 空）、二维码实际解码（无法给虚拟相机喂码）、真机抓包（但无 INTERNET 权限已是更强保证） | 
| L6-04 | 发布门禁（MASVS 对齐） | TODO | L6-* | 见 [v1.0 发布计划](../docs/product/v1.0-release-plan.md)，门禁通过 |
