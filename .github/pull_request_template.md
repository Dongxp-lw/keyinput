<!-- 感谢贡献！请在提交前填写下面各项，并删除不适用的部分。 -->

## 变更说明
<!-- 这个 PR 做了什么、为什么。简明扼要。 -->

## 关联 issue
<!-- 例如：Closes #123 / Refs #456 -->

## 变更类型
<!-- 勾选适用项 -->
- [ ] feat：新功能
- [ ] fix：缺陷修复
- [ ] docs：仅文档
- [ ] refactor：重构（无功能变化）
- [ ] perf：性能
- [ ] test：测试
- [ ] build / ci：构建或流水线
- [ ] chore：杂项

## 自检清单
- [ ] 提交信息遵循 **Conventional Commits**（`type(scope): subject`）
- [ ] 已在本地通过：`cargo test` + `cargo clippy -- -D warnings`（涉及 Rust 时）
- [ ] 已在本地通过：`:app:assembleDebug`（涉及 Android 时）
- [ ] **未引入 `INTERNET` 权限**（除非该功能确需联网，并已按 D-015 告知用户）
- [ ] **未记录任何秘密**（主密码/密钥/字段值/TOTP 种子或码不进日志、不进崩溃报告）
- [ ] **未提交任何真实密钥、口令、keystore 或测试用真实秘密**
- [ ] 已更新 `CHANGELOG.md` 的 `[Unreleased]`（用户可见变更时）
- [ ] 已更新相关文档（`docs/` 或实现文档第 9 节）
- [ ] CI 全绿

## 测试与验证
<!-- 你怎么验证的？单元/KAT/instrumented/运行期实测，给出关键证据。 -->

## 安全影响
<!-- 是否触及加密、密钥、锁定、权限、网络等安全相关面？如有，说明评估。 -->

## 截图 / 录屏（如涉及 UI）
