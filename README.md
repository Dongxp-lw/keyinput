# Private Input Vault

Private Input Vault 是一个离线优先的私密输入键盘和加密保险库，用于管理密码、2FA 验证码、账号字段以及可复用的私密文本片段。

本产品有意不把系统自动填充作为主要路径。用户需要从安全键盘或主应用中主动选择要输入的内容。默认体验不需要登录，也不需要网络连接。

## 产品原则

- 离线优先：核心功能不依赖账号或网络。
- 用户控制输入：敏感值由用户选择，而不是由应用猜测。
- 本地优先存储：保险库数据在任何备份或同步之前先在本地加密。
- 可选云服务：云服务只保存加密数据，且本地使用永远不依赖云服务。
- 默认不提供键盘词库：键盘默认不内置、不拉取在线词库。
- 清晰的责任边界：应用不承诺识别钓鱼网站或判断目标站点身份。

## 初始范围

- 用于创建和管理加密条目的主保险库应用。
- 用于主动选择并插入已保存字段的安全键盘。
- 本地加密备份与恢复。
- 用于云同步、设备迁移和付费服务的可选账号登录。
- 将 2FA/TOTP 作为高敏感字段类型支持。
- 目标平台：Android、iOS、HarmonyOS。

## 仓库结构

```text
apps/
  android/        Android 原生壳和键盘服务占位
  ios/            iOS 应用和键盘扩展占位
  harmony/        HarmonyOS 应用和输入法占位
core/             共享保险库、加密、同步和领域逻辑占位
docs/
  product/        产品需求、决策记录、整体设计、信息架构、交互设计、平台能力、原型/MVP/迁移/发布/云同步计划、版本权益和路线图
  technical/      架构、安全模型、安全实现设计和数据模型
  testing/        测试策略、测试计划和发布质量门禁
  implementation/ 实现文档层总览与各功能实现文档
tools/            迁移、备份、打包和开发工具占位
```

## 当前状态

这是一个文档优先的项目脚手架。下一步是做平台可行性验证，重点关注 iOS 键盘限制、HarmonyOS 输入法 API、共享加密存储和生物识别解锁行为。

## 关键文档

完整的文档地图、阅读顺序和端到端可追溯链见 [文档地图与串联](docs/README.md)。

- [产品需求](docs/product/product-requirements.md)
- [产品决策记录：主动选择式私密输入](docs/product/product-decision-record.md)
- [产品整体设计](docs/product/product-design.md)
- [信息架构设计](docs/product/information-architecture.md)
- [核心交互设计](docs/product/interaction-design.md)
- [平台能力设计](docs/product/platform-capability-design.md)
- [v0.1 原型计划](docs/product/v0.1-prototype-plan.md)
- [v0.2 离线 MVP 计划](docs/product/v0.2-mvp-plan.md)
- [v0.3 跨设备迁移计划](docs/product/v0.3-migration-plan.md)
- [v1.0 公开发布计划](docs/product/v1.0-release-plan.md)
- [v1.1 云同步计划](docs/product/v1.1-cloud-sync-plan.md)
- [版本与权益计划](docs/product/version-plan.md)
- [技术架构](docs/technical/architecture.md)
- [安全模型](docs/technical/security-model.md)
- [安全实现设计](docs/technical/security-implementation-design.md)
- [数据模型](docs/technical/data-model.md)
- [测试策略](docs/testing/test-strategy.md)
- [测试计划](docs/testing/test-plan.md)
