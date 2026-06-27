//! 条目仓库：解锁后内存模型上的 CRUD 与本地搜索（L3-ENTRY，ENTRY-06/07）。
//!
//! 在 [`VaultContent`] 上操作；持久化由 L3-VAULT 负责（序列化 + 加密整个内容）。纯内存、
//! 纯逻辑、可确定性测试。搜索仅在本地进行，**绝不上传查询词**（实现文档 §5.6、§8）。

use crate::entry::{Entry, EntryId, Sensitivity, VaultContent};

/// 条目仓库（包裹一份解锁后的 [`VaultContent`]）。
pub struct EntryRepository {
    content: VaultContent,
}

impl EntryRepository {
    /// 空仓库（当前架构版本）。
    pub fn new() -> Self {
        Self {
            content: VaultContent::new(),
        }
    }

    /// 由既有载荷构造（如 L3-VAULT 解密后交入）。
    pub fn from_content(content: VaultContent) -> Self {
        Self { content }
    }

    /// 借出内部载荷（持久化时由 L3-VAULT 序列化 + 加密）。
    pub fn content(&self) -> &VaultContent {
        &self.content
    }

    /// 借出内部载荷（可变；供锁定时清零敏感值等）。
    pub fn content_mut(&mut self) -> &mut VaultContent {
        &mut self.content
    }

    /// 取回内部载荷（消费仓库）。
    pub fn into_content(self) -> VaultContent {
        self.content
    }

    /// 列出未软删除的条目。
    pub fn list(&self) -> Vec<&Entry> {
        self.content
            .entries
            .iter()
            .filter(|e| !e.is_deleted())
            .collect()
    }

    /// 按 ID 取条目（已软删除的视为不存在）。
    pub fn get(&self, id: &EntryId) -> Option<&Entry> {
        self.content
            .entries
            .iter()
            .find(|e| &e.id == id && !e.is_deleted())
    }

    /// 插入或按 ID 替换条目（增 / 改）。
    pub fn upsert(&mut self, entry: Entry) {
        if let Some(slot) = self.content.entries.iter_mut().find(|e| e.id == entry.id) {
            *slot = entry;
        } else {
            self.content.entries.push(entry);
        }
    }

    /// 软删除：设置 `deletedAt = now`，不物理移除（便于迁移/同步与撤销）。返回是否命中。
    pub fn soft_delete(&mut self, id: &EntryId, now: i64) -> bool {
        match self
            .content
            .entries
            .iter_mut()
            .find(|e| &e.id == id && !e.is_deleted())
        {
            Some(e) => {
                e.deleted_at = Some(now);
                true
            }
            None => false,
        }
    }

    /// 本地搜索（大小写不敏感子串）：匹配标题、标签、字段标签，以及**普通敏感度**字段值。
    /// 刻意不检索敏感 / 高敏感字段值（密码、TOTP 种子、备注、恢复码），避免在搜索结果中
    /// 浮现秘密（实现文档 §8「默认行为保持秘密隐藏」）。已软删除条目不参与。
    pub fn search(&self, query: &str) -> Vec<&Entry> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        self.content
            .entries
            .iter()
            .filter(|e| !e.is_deleted() && entry_matches(e, &q))
            .collect()
    }
}

impl Default for EntryRepository {
    fn default() -> Self {
        Self::new()
    }
}

fn entry_matches(entry: &Entry, query_lower: &str) -> bool {
    if entry.title.to_lowercase().contains(query_lower) {
        return true;
    }
    if entry
        .tags
        .iter()
        .any(|t| t.to_lowercase().contains(query_lower))
    {
        return true;
    }
    entry.fields.iter().any(|f| {
        f.label.to_lowercase().contains(query_lower)
            || (matches!(f.sensitivity, Sensitivity::Normal)
                && f.value.to_lowercase().contains(query_lower))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{EntryType, Field, FieldId, FieldKind};

    fn login(id: &str, title: &str, username: &str, password: &str) -> Entry {
        let mut e = Entry::new(EntryId::new(id), title, EntryType::Login, 1_000);
        e.fields.push(Field::with_defaults(
            FieldId::new(format!("{id}-u")),
            "Username",
            FieldKind::Username,
            username,
        ));
        e.fields.push(Field::with_defaults(
            FieldId::new(format!("{id}-p")),
            "Password",
            FieldKind::Password,
            password,
        ));
        e
    }

    #[test]
    fn upsert_get_list() {
        let mut repo = EntryRepository::new();
        repo.upsert(login("e1", "GitHub", "octocat", "pw1"));
        repo.upsert(login("e2", "GitLab", "tanuki", "pw2"));
        assert_eq!(repo.list().len(), 2);
        assert_eq!(repo.get(&EntryId::new("e1")).unwrap().title, "GitHub");
        assert!(repo.get(&EntryId::new("missing")).is_none());
    }

    #[test]
    fn upsert_replaces_existing_by_id() {
        let mut repo = EntryRepository::new();
        repo.upsert(login("e1", "GitHub", "octocat", "pw1"));
        repo.upsert(login("e1", "GitHub (renamed)", "octocat", "pw1"));
        assert_eq!(repo.list().len(), 1);
        assert_eq!(repo.get(&EntryId::new("e1")).unwrap().title, "GitHub (renamed)");
    }

    #[test]
    fn soft_delete_hides_from_list_and_get() {
        let mut repo = EntryRepository::new();
        repo.upsert(login("e1", "GitHub", "octocat", "pw1"));
        assert!(repo.soft_delete(&EntryId::new("e1"), 2_000));
        assert!(repo.list().is_empty());
        assert!(repo.get(&EntryId::new("e1")).is_none());
        // 物理记录仍在（便于同步/迁移），但带 deletedAt。
        assert_eq!(repo.content().entries.len(), 1);
        assert_eq!(repo.content().entries[0].deleted_at, Some(2_000));
        // 重复删除不命中。
        assert!(!repo.soft_delete(&EntryId::new("e1"), 3_000));
    }

    #[test]
    fn search_matches_title_tag_and_field_label() {
        let mut repo = EntryRepository::new();
        let mut e = login("e1", "GitHub", "octocat", "pw1");
        e.tags.push("dev-tools".into());
        repo.upsert(e);
        assert_eq!(repo.search("hub").len(), 1); // 标题
        assert_eq!(repo.search("DEV-TOOLS").len(), 1); // 标签，大小写不敏感
        assert_eq!(repo.search("username").len(), 1); // 字段标签
    }

    #[test]
    fn search_matches_normal_value_but_not_secret_value() {
        let mut repo = EntryRepository::new();
        repo.upsert(login("e1", "GitHub", "octocat", "super-secret-pw"));
        // 普通敏感度的用户名值可被搜到。
        assert_eq!(repo.search("octocat").len(), 1);
        // 高敏感的密码值不进入可搜索文本。
        assert!(repo.search("super-secret-pw").is_empty());
    }

    #[test]
    fn search_excludes_deleted_and_empty_query() {
        let mut repo = EntryRepository::new();
        repo.upsert(login("e1", "GitHub", "octocat", "pw1"));
        repo.soft_delete(&EntryId::new("e1"), 2_000);
        assert!(repo.search("hub").is_empty());
        assert!(repo.search("   ").is_empty());
    }
}
