# 数据模型

本文档描述初始逻辑模型。物理存储和序列化细节将在核心实现阶段最终确定。

## 1. 保险库

```text
Vault
- id
- schemaVersion
- createdAt
- updatedAt
- cryptoProfile
- entries[]
- settings
```

保险库内容应加密为单个带认证的载荷，或加密为多条带认证的记录。初始实现可以先选择更简单的模型；如果同步性能需要记录级加密，再逐步演进。

## 2. 条目

```text
Entry
- id
- title
- type: login | secure-note | identity | payment | template | custom
- fields[]
- tags[]
- favorite
- archived
- createdAt
- updatedAt
- deletedAt?
```

## 3. 字段

```text
Field
- id
- label
- kind: username | password | email | phone | totp | text | multiline | url | address | secret | note
- value
- sensitivity: normal | sensitive | high
- inputBehavior: insert | copy | reveal-only
- requireReauth
- createdAt
- updatedAt
```

敏感字段包括密码、TOTP 种子、私密备注、恢复码，以及用户标记为秘密的字段。

## 4. TOTP 字段

```text
TotpField
- issuer
- accountName
- secret
- algorithm: SHA1 | SHA256 | SHA512
- digits
- periodSeconds
```

secret 必须加密存储。生成的验证码是临时值，不得写入日志或同步元数据。

## 5. 迁移包

```text
TransferPackage
- packageVersion
- createdAt
- sourceDeviceLabel?
- vaultId
- encryptedPayload
- authenticationTag
- cryptoProfile
```

迁移包用于导出/导入、本地迁移，以及用户自主控制的跨设备移动。迁移包必须独立于本地保险库文件进行加密和认证保护。

## 6. 同步对象

```text
SyncObject
- objectId
- vaultId
- deviceId
- baseVersion
- version
- createdAt
- encryptedPayload
- authenticationTag
```

云同步应只处理密文。冲突元数据可能对服务端可见，但秘密内容必须保持加密。

## 7. 设备记录

```text
DeviceRecord
- deviceId
- displayName
- platform
- registeredAt
- lastSyncAt
- publicSyncKey?
```

设备记录用于可选云功能和迁移流程。本地单机使用不需要设备记录。

## 8. 架构迁移规则

- 每个保险库文件都必须包含架构版本。
- 迁移必须具备确定性，并且可以测试。
- 迁移失败不得覆盖上一份有效保险库。
- 导入和同步在应用变更前必须校验架构兼容性。
- 应尽可能保留未知字段类型，避免降级或跨版本迁移时丢失数据。
