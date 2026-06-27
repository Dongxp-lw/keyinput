# 技术架构

## 1. 架构概览

系统拆分为平台壳和共享核心。

```text
Android 应用 + 键盘      iOS 应用 + 键盘扩展      HarmonyOS 应用 + 输入法
          |                         |                            |
          +-------------------------+----------------------------+
                                    |
                              共享核心边界
                                    |
                保险库领域、加密、导出/导入、同步协议
```

共享核心负责保险库格式、加密、领域校验、导出/导入和同步冲突逻辑。平台代码负责 UI、键盘集成、安全存储 API、生物识别、打包、权限和平台特定生命周期处理。

## 2. 推荐技术方向

### 共享核心

**已定（2026-06-24）：分层架构 = 一套 Rust 共享核心 + 各端原生 UI。** 安全关键逻辑只实现一次，UI 各端原生。

原因：

- 强内存安全性，适合靠近密码学的领域逻辑。
- 安全核心（保险库格式、加密、序列化、领域/迁移/同步逻辑）**只实现一次**，跨端逐字节一致——这是跨设备迁移与同步正确的前提。
- 适合跨平台确定性测试。

绑定路径（已核实）：

- Android：Rust → `.so` → Kotlin，经 UniFFI（Mozilla，Firefox 移动端生产在用）。
- iOS：Rust → XCFramework → Swift，经 UniFFI。
- HarmonyOS：Rust → `.so` → ArkTS，经 Node-API/NDK（华为官方支持 C/C++ 原生模块）；Rust→HarmonyOS 目标工具链待一次可行性 spike，最坏以薄 C shim 兜底。

核心与原生的分工：

- **Rust 核心（写一次）**：保险库文件格式、KDF+AEAD+信封密钥、序列化、条目/字段领域模型与 CRUD/搜索、TOTP、导出/导入打包、迁移与同步冲突逻辑。
- **各端原生（各写）**：UI、键盘/IME 服务、生物识别提示、安全存储（Keystore/Keychain/HUKS）、剪贴板、生命周期。
- 加密原语改用 Rust crate（RustCrypto 的 argon2 + chacha20poly1305，或 libsodium 经 libsodium-sys），不用 Android 专用的 lazysodium-android。

为什么不是 KMP/C++：KMP 不支持 HarmonyOS 且加密仍按端委托；C/C++ 核心贴近密码学处内存不安全。详见 [安全实现设计](security-implementation-design.md) 第 2.4 节。

### Android

- Kotlin 原生应用。
- 基于 Android 输入法服务实现安全键盘。
- 在合适场景使用 Android Keystore 包装本地保险库密钥。
- 集成 BiometricPrompt 或设备凭据。

### iOS

- Swift 原生应用。
- 用于安全输入的键盘扩展。
- 仅在需要且通过平台可行性验证后使用 App Group 共享容器。
- 使用 Keychain 和 LocalAuthentication 支持本地解锁。

### HarmonyOS

- ArkTS/原生 HarmonyOS 应用。
- 在 API 验证后集成 HarmonyOS 输入法。
- 本地安全存储和原生绑定策略待确认。

### 开源组件选型（Android 优先）

选型原则：复用优先——能用成熟开源库或平台框架实现的，不自行造轮子；只有第三方库和框架都不满足时才手写。密码学是例外：不自行实现密码学原语，只在经过审计的库之间选择（详见 [安全实现设计](security-implementation-design.md) 第 2.4 节）。下表为初步选型，主要库的许可证/版本/维护状态已核实，最终版本在实现评审锁定。

| 能力 | 候选开源/框架 | 许可证 | 复用/手写 | 说明 |
| --- | --- | --- | --- | --- |
| KBD 安全键盘 | Android `InputMethodService`（框架） | — | 框架 + 手写 | 无现成“安全键盘”库，在框架上手写 |
| VAULT / IMEX 加密 | **Rust crate**（RustCrypto argon2+chacha20poly1305，或 libsodium-sys）；曾比较 lazysodium/Tink/BC | — | 复用库（核心层） | 密码学不手写；已定 Rust 核心，见 §2 与安全实现设计第 2.4 节 |
| LOCK 解锁会话 | AndroidX Lifecycle（`ProcessLifecycleOwner`） | Apache-2.0 | 框架 + 手写 | 自动锁定用生命周期；会话逻辑自写 |
| BIO 生物识别 | AndroidX Biometric（`BiometricPrompt` + `CryptoObject`） | Apache-2.0 | 复用官方库 | 稳定版 1.1.0；1.4.0 仍 alpha；支持回溯到 API 23 |
| ENTRY 序列化 | Google protobuf-javalite / Square Wire / kotlinx.serialization（ProtoBuf、CBOR） | BSD-3 / Apache-2.0 / Apache-2.0 | 复用库 | 需保留未知字段（前向兼容）：protobuf-javalite、Wire 明确支持；kotlinx ProtoBuf 是否保留待核实 |
| GEN 密码生成 | `java.security.SecureRandom`（标准库） | — | 薄手写 | 拒绝采样避免取模偏置 |
| TOTP | JCE `javax.crypto.Mac` / HMAC（标准库） | — | 薄手写 | RFC 6238，标准库即可，无需第三方 |
| CLIP 剪贴板 | Android `ClipboardManager` + `EXTRA_IS_SENSITIVE`（框架） | — | 框架 + 手写 | 敏感标记仅渲染提示、非安全控制；自动清除自写 |
| MIGR 二维码 | ZXing core + zxing-android-embedded | Apache-2.0 | 复用库 | v4.3.0（2021），维护偏慢；二维码只承载配对密钥 |
| MIGR 本地传输 | Wi-Fi Direct（框架）/ Nearby Connections | 框架 / 专有 | 复用 + 手写 | Nearby 离线但依赖 Google Play 服务、非开源；非 GMS 设备须回退 |
| SYNC 冲突合并 | 版本向量（自实现）；CRDT 候选 Automerge（Rust 核心）/ Yjs（JS） | — / 多样 | 多为自建 | 无成熟 Kotlin 原生 CRDT 库；v1.1 以版本向量为主，CRDT 留后续 |

关键提醒：

- Nearby Connections 虽是离线 P2P，但依赖 Google Play 服务且非开源，在无 GMS 的设备（含部分 HarmonyOS/AOSP）不可用，须以 Wi-Fi Direct 或 v0.2 文件迁移包作回退。
- zxing-android-embedded 最近发布于 2021 年，维护活跃度较低；如需更高识别率可评估 ML Kit（依赖 GMS、非开源），二者按是否接受 GMS 取舍。
- AndroidX Biometric 1.4.0 仍处 alpha（认证 API 正在重构），稳定落地用 1.1.0，1.4.0 待转稳定后再评估。
- ENTRY 的前向兼容要求保留未知字段；若为硬约束，倾向 Google protobuf-javalite 或 Square Wire（明确支持），kotlinx.serialization 的 ProtoBuf 是否保留未知字段需先核实。
- **已定 Rust 共享核心**：核心层（加密、序列化、TOTP、领域、迁移/同步逻辑）改走 Rust 生态（RustCrypto、prost/serde_cbor 等）；本表对**原生层**（键盘、生物识别、剪贴板、二维码扫描）的复用判断仍适用，对核心层以 Rust crate 替代（见 §2 共享核心）。

## 3. 主要组件

### 保险库核心

- 条目模型。
- 字段模型。
- 加密和解密。
- 保险库文件读写。
- 保险库架构迁移。
- TOTP 生成。
- 导出/导入包校验。

### 主应用

- 保险库设置和解锁。
- 条目管理。
- 搜索、标签、收藏和模板。
- 备份、恢复和迁移。
- 可用时提供云账号和订阅 UI。

### 安全键盘

- 默认锁定状态。
- 解锁交接。
- 条目搜索和选择。
- 字段插入。
- 最近使用/收藏条目。
- 对敏感内容执行严格的不记录日志行为。

### 可选云服务

- 账号身份。
- 加密对象存储。
- 设备注册。
- 同步元数据。
- 订阅权益。

云服务不得接收保险库明文、主密码、明文 TOTP 种子或派生的内容加密密钥。

模块结构、核心与原生的层间契约（核心 API 与平台端口）、依赖方向规则见 [模块架构与层间契约](module-architecture.md)（L1）。

## 4. 数据流：离线输入

```text
用户打开键盘
  -> 键盘请求解锁状态
  -> 平台解锁，或请求主应用/核心解锁
  -> 用户搜索条目
  -> 用户选择具体字段
  -> 核心返回已授权的字段值
  -> 键盘插入被选中的值
  -> 敏感值在可行时尽快从内存中清除
```

## 5. 数据流：加密导出

```text
用户请求导出
  -> 核心序列化保险库载荷
  -> 核心加密并认证导出包
  -> 平台将导出包写入用户选择的位置
  -> 导入流程在替换或合并保险库前校验导出包
```

## 6. 数据流：云同步

```text
本地保险库发生变更
  -> 核心创建加密同步载荷
  -> 平台上传密文和元数据
  -> 云端存储带版本的加密对象
  -> 另一台设备下载密文
  -> 核心在本地校验、解密并合并
```

## 7. 平台风险清单

| 风险 | 影响 | 缓解方式 |
| --- | --- | --- |
| iOS 在敏感字段中阻止第三方键盘。 | 密码插入可能无法覆盖所有场景。 | 提供应用内复制备选方案，并记录限制。 |
| iOS “完全访问”权限可能让用户担忧。 | 降低信任和采用率。 | 尽量减少依赖，清楚解释，并保留本地路径。 |
| HarmonyOS 输入法 API 可能随版本或分发渠道变化。 | 平台支持延迟。 | 在承诺发布日期前完成可行性验证。 |
| 键盘生命周期可能清除已解密状态。 | 用户可能需要频繁解锁。 | 使用短生命周期会话令牌，并按平台调优体验。 |
| 云同步冲突如果设计不当可能损坏数据。 | 存在数据丢失风险。 | 使用追加式版本、认证载荷、导入回滚和冲突测试。 |

## 8. 初始实现顺序

1. Android 键盘可行性原型。
2. 共享保险库文件和加密原型。
3. 离线应用 MVP。
4. 导出/导入包。
5. iOS 和 HarmonyOS 可行性验证。
6. 可选云同步设计和服务实现。

构建系统、工具链与 SDK 基线、工程规范见 [工程基础](engineering-foundation.md)（L0）。
