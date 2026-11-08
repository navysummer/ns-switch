//! 提示词库存储（`prompts.json`）。
//!
//! 库是 `config.json` 之外独立的自管文件（`<data_dir>/prompts.json`），
//! 与 ns-mcp/ns-skills 的独立存储模式一致。库键 = 工具 + 条目名；
//! 每次写回提示文件前，先把文件既有内容读回库（先回存，不丢失）。
//!
//! 文件格式：
//! ```json
//! { "tools": { "claude": { "项目规范": { "body": "...", "enabled": true } } } }
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use ns_presets::Tool;

use crate::model::PromptEntry;

/// 库文件根对象。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PromptLibrary {
    /// tool id_str → (条目名 → 条目)。
    pub tools: BTreeMap<String, BTreeMap<String, PromptEntry>>,
}

impl PromptLibrary {
    /// 某工具的条目（按名排序）。
    pub fn entries_for(&self, tool: Tool) -> Vec<PromptEntry> {
        self.tools
            .get(tool.id_str())
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }

    /// 取某工具某条目。
    pub fn get(&self, tool: Tool, name: &str) -> Option<&PromptEntry> {
        self.tools
            .get(tool.id_str())
            .and_then(|m| m.get(name))
    }

    /// 插入（新增或更新）；返回是否为新条目。
    pub fn put(&mut self, tool: Tool, entry: PromptEntry) -> bool {
        let map = self.tools.entry(tool.id_str().to_string()).or_default();
        let is_new = !map.contains_key(&entry.name);
        map.insert(entry.name.clone(), entry);
        is_new
    }

    /// 删除；返回是否真的删除了。
    pub fn remove(&mut self, tool: Tool, name: &str) -> bool {
        let Some(map) = self.tools.get_mut(tool.id_str()) else {
            return false;
        };
        let removed = map.remove(name).is_some();
        if map.is_empty() {
            self.tools.remove(tool.id_str());
        }
        removed
    }
}

/// 库存储句柄。
pub struct PromptStore {
    path: PathBuf,
    lib: PromptLibrary,
}

impl PromptStore {
    /// 打开（或初始化）默认数据目录下的提示词库。
    pub fn open() -> Result<Self, String> {
        Self::open_at(default_data_dir())
    }

    /// 在指定目录下打开提示词库。
    pub fn open_at(dir: PathBuf) -> Result<Self, String> {
        let path = dir.join("prompts.json");
        let lib = if path.exists() {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("读取提示词库失败: {e}"))?;
            serde_json::from_str(&text).unwrap_or_default()
        } else {
            PromptLibrary::default()
        };
        Ok(Self { path, lib })
    }

    /// 当前库内容（克隆）。
    pub fn library(&self) -> PromptLibrary {
        self.lib.clone()
    }

    /// 某工具的条目。
    pub fn entries_for(&self, tool: Tool) -> Vec<PromptEntry> {
        self.lib.entries_for(tool)
    }

    /// 插入条目并保存。
    pub fn save_entry(&mut self, tool: Tool, entry: PromptEntry) -> Result<bool, String> {
        let is_new = self.lib.put(tool, entry);
        self.persist()?;
        Ok(is_new)
    }

    /// 删除条目并保存。
    pub fn remove_entry(&mut self, tool: Tool, name: &str) -> Result<bool, String> {
        let removed = self.lib.remove(tool, name);
        if removed {
            self.persist()?;
        }
        Ok(removed)
    }

    /// 原子写库文件（临时文件 + rename）。
    fn persist(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(&self.lib)
            .map_err(|e| format!("序列化提示词库失败: {e}"))?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| format!("写提示词库失败: {e}"))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| format!("落盘提示词库失败: {e}"))?;
        Ok(())
    }
}

/// 默认数据目录（与 ns-store 一致；`NS_SWITCH_HOME` 可覆盖）。
pub fn default_data_dir() -> PathBuf {
    if let Some(env) = std::env::var_os("NS_SWITCH_HOME") {
        return PathBuf::from(env);
    }
    directories::BaseDirs::new()
        .map(|d| d.config_dir().join("ns-switch"))
        .unwrap_or_else(|| PathBuf::from(".ns-switch"))
}

/// 读文件内容（不存在 = 空字符串；其他错误上抛）。
pub fn read_file_or_empty(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("读取 {} 失败: {e}", path.display())),
    }
}

/// 写文件内容（父目录自动创建）。
pub fn write_file(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, content).map_err(|e| format!("写 {} 失败: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());
    static DIR_SEQ: AtomicU32 = AtomicU32::new(0);

    fn temp_dir() -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "ns-prompts-test-{}-{}",
            std::process::id(),
            DIR_SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn put_get_remove_roundtrip() {
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = temp_dir();
        let mut store = PromptStore::open_at(dir.clone()).unwrap();
        assert!(store.entries_for(Tool::Claude).is_empty());

        let entry = PromptEntry {
            name: "项目规范".into(),
            body: "# 规范\n\n- 提交信息\n".into(),
            enabled: true,
            updated_at: 0,
        };
        assert!(store.save_entry(Tool::Claude, entry).unwrap());
        // 再存同名 → 更新非新增。
        let entry = PromptEntry {
            name: "项目规范".into(),
            body: "# 规范 v2\n".into(),
            ..Default::default()
        };
        assert!(!store.save_entry(Tool::Claude, entry).unwrap());

        let got = store.entries_for(Tool::Claude);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].body, "# 规范 v2\n");

        // 落盘后重新打开。
        drop(store);
        let mut store = PromptStore::open_at(dir.clone()).unwrap();
        assert_eq!(store.entries_for(Tool::Claude).len(), 1);
        assert!(store.remove_entry(Tool::Claude, "项目规范").unwrap());
        assert!(store.entries_for(Tool::Claude).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_write_file_helpers() {
        let _guard = TEST_LOCK.lock().unwrap();
        let dir = temp_dir();
        let path = dir.join("sub").join("x.md");
        assert_eq!(read_file_or_empty(&path).unwrap(), "");
        write_file(&path, "hello").unwrap();
        assert_eq!(read_file_or_empty(&path).unwrap(), "hello");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
