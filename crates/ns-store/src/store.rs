//! 配置存储：读、写（原子）、供应商 CRUD、设置。
//!
//! 目录：`~/.ns-switch/`（Windows: `%APPDATA%`，经 `directories`）。
//! 结构：
//! - `config.json` — SSOT 状态（供应商、当前选择、设置）
//! - `backups/live-first-write/` — 首次改写每个 live 配置文件前的备份

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use ns_presets::{CurrentSelection, Provider, Tool};

/// 应用设置（子集，后续扩展）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub visible_apps: BTreeMap<String, bool>,
    pub app_order: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            visible_apps: BTreeMap::new(),
            app_order: Vec::new(),
        }
    }
}

/// SSOT 配置根对象。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub providers: BTreeMap<String, Provider>,
    pub current: CurrentSelection,
    pub settings: Settings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            providers: BTreeMap::new(),
            current: CurrentSelection::default(),
            settings: Settings::default(),
        }
    }
}

impl Config {
    /// 列出某工具的供应商（按 meta 记录的工具归属）。
    pub fn providers_for(&self, tool: Tool) -> Vec<&Provider> {
        self.providers
            .values()
            .filter(|p| provider_tool_of(p) == Some(tool))
            .collect()
    }
}

/// 存储句柄。
pub struct Store {
    dir: PathBuf,
    config_path: PathBuf,
    config: Config,
}

impl Store {
    /// 打开（或初始化）默认配置目录下的存储。
    pub fn open() -> Result<Self> {
        let dir = default_data_dir();
        Self::open_at(dir)
    }

    /// 打开指定目录下的存储（测试用）。
    pub fn open_at(dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&dir)
            .with_context(|| format!("创建配置目录失败: {}", dir.display()))?;
        let config_path = dir.join("config.json");
        let config = load_config(&config_path)?;
        Ok(Self {
            dir,
            config_path,
            config,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// 备份目录（live 配置文件首写备份）。
    pub fn backups_dir(&self) -> PathBuf {
        self.dir.join("backups").join("live-first-write")
    }

    /// 保存 config.json（原子写）。
    pub fn save(&self) -> Result<()> {
        atomic_write_json(&self.config_path, &self.config)
    }

    /// 添加供应商并保存。
    pub fn add_provider(&mut self, provider: Provider) -> Result<()> {
        self.config.providers.insert(provider.id.clone(), provider);
        self.save()
    }

    /// 更新供应商并保存。
    pub fn update_provider(&mut self, provider: Provider) -> Result<()> {
        if !self.config.providers.contains_key(&provider.id) {
            bail!("供应商不存在: {}", provider.id);
        }
        self.config.providers.insert(provider.id.clone(), provider);
        self.save()
    }

    /// 删除供应商并保存。切换类工具当前在用不可删（调用方先校验）。
    pub fn remove_provider(&mut self, id: &str) -> Result<()> {
        if self.config.providers.remove(id).is_none() {
            bail!("供应商不存在: {id}");
        }
        // 从 current 中清理
        for tool in Tool::ALL {
            if self.config.current.get(tool) == Some(&id.to_string()) {
                self.config.current.remove(tool);
            }
        }
        self.save()
    }

    /// 设置当前选择并保存。
    pub fn set_current(&mut self, tool: Tool, id: String) -> Result<()> {
        self.config.current.set(tool, id);
        self.save()
    }
}

/// 默认数据目录。
pub fn default_data_dir() -> PathBuf {
    if let Some(env) = std::env::var_os("NS_SWITCH_HOME") {
        return PathBuf::from(env);
    }
    directories::BaseDirs::new()
        .map(|d| d.config_dir().join("ns-switch"))
        .unwrap_or_else(|| PathBuf::from(".ns-switch"))
}

/// 读取配置；文件不存在返回默认；损坏时备份为 `.corrupt-<ts>` 并回退默认。
fn load_config(path: &Path) -> Result<Config> {
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = fs::read_to_string(path)
        .with_context(|| format!("读取配置失败: {}", path.display()))?;
    match serde_json::from_str::<Config>(&text) {
        Ok(cfg) => Ok(cfg),
        Err(e) => {
            let backup = path.with_extension(format!("corrupt-{}.json", now_ts()));
            let _ = fs::rename(path, &backup);
            eprintln!("配置损坏（{}），已备份到 {}，回退默认", e, backup.display());
            Ok(Config::default())
        }
    }
}

/// 原子写 JSON：写临时文件 + rename。
fn atomic_write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("路径无父目录: {}", path.display()))?;
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(".{}.tmp-{}", file_name(path), now_ts()));
    let text = serde_json::to_string_pretty(value)?;
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "config".into())
}

fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 供应商 → 归属工具（由 meta 的 template_type 携带工具 id，kebab-case）。
pub fn provider_tool_of(p: &Provider) -> Option<Tool> {
    let tool_str = p.meta.as_ref()?.template_type.as_deref()?;
    serde_json::from_value::<Tool>(Value::String(tool_str.to_string())).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ns_presets::ProviderCategory;

    fn test_dir() -> PathBuf {
        // 并行测试必须唯一：用单调计数器，避免同毫秒时间戳互相删目录。
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("ns-switch-test-{n}-{}", now_ts()))
    }

    fn sample_provider(id: &str) -> Provider {
        let mut p = Provider::with_id(
            id.into(),
            format!("供应商 {id}"),
            serde_json::json!({ "env": { "ANTHROPIC_BASE_URL": "https://example.com" } }),
        );
        p.category = Some(ProviderCategory::Aggregator);
        p
    }

    #[test]
    fn fresh_store_defaults() {
        let dir = test_dir();
        let store = Store::open_at(dir.clone()).unwrap();
        assert!(store.config().providers.is_empty());
        assert_eq!(store.config().settings.theme, "dark");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn add_and_roundtrip() {
        let dir = test_dir();
        {
            let mut store = Store::open_at(dir.clone()).unwrap();
            store.add_provider(sample_provider("p1")).unwrap();
            store.set_current(Tool::Claude, "p1".into()).unwrap();
        }
        {
            let store = Store::open_at(dir.clone()).unwrap();
            assert!(store.config().providers.contains_key("p1"));
            assert_eq!(store.config().current.get(Tool::Claude).map(String::as_str), Some("p1"));
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_config_backs_up_and_defaults() {
        let dir = test_dir();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.json"), "{ not json").unwrap();
        let store = Store::open_at(dir.clone()).unwrap();
        assert!(store.config().providers.is_empty());
        let backups: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("corrupt"))
            .collect();
        assert_eq!(backups.len(), 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn remove_clears_current() {
        let dir = test_dir();
        {
            let mut store = Store::open_at(dir.clone()).unwrap();
            store.add_provider(sample_provider("p1")).unwrap();
            store.set_current(Tool::Claude, "p1".into()).unwrap();
            store.remove_provider("p1").unwrap();
        }
        let store = Store::open_at(dir.clone()).unwrap();
        assert!(store.config().providers.is_empty());
        assert_eq!(store.config().current.get(Tool::Claude), None);
        let _ = fs::remove_dir_all(dir);
    }
}
