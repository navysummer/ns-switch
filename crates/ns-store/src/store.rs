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

use ns_presets::{CurrentSelection, Provider, ProviderCategory, Tool};

/// 每工具路由模式配置（本地代理）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RoutingConfig {
    /// 是否启用本地路由代理。
    pub enabled: bool,
    /// 本地监听端口（0 = 自动分配）。
    pub port: u16,
    /// 监听地址（默认 127.0.0.1）。
    pub listen_addr: String,
    /// 故障转移：上游失败时尝试下一供应商。
    pub failover: bool,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 0,
            listen_addr: "127.0.0.1".into(),
            failover: false,
        }
    }
}


/// 每工具聚合配置。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AggregationConfig {
    /// 是否启用聚合模式。
    pub enabled: bool,
    /// 聚合的供应商 id 列表（保序）。
    pub provider_ids: Vec<String>,
    /// 默认供应商 id（必须 ∈ provider_ids；不可删除）。
    pub default_id: Option<String>,
}

impl AggregationConfig {
    /// 校验：默认必须在列表中；至少保留一个供应商。
    pub fn validate(&self) -> Result<(), String> {
        if self.enabled && self.provider_ids.is_empty() {
            return Err("聚合至少需要一个供应商".into());
        }
        if let Some(def) = &self.default_id {
            if !self.provider_ids.contains(def) {
                return Err(format!("默认供应商 {def} 不在聚合列表中"));
            }
        }
        Ok(())
    }
}

/// 云同步配置（WebDAV / S3）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    /// 提供方：`webdav` / `s3` / 空。
    pub provider: String,
    /// WebDAV 根 URL 或 S3 endpoint。
    pub url: String,
    pub username: String,
    pub password: String,
    /// S3 bucket（WebDAV 忽略）。
    pub bucket: String,
    pub enabled: bool,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            provider: String::new(),
            url: String::new(),
            username: String::new(),
            password: String::new(),
            bucket: String::new(),
            enabled: false,
        }
    }
}
/// 应用设置（子集，后续扩展）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub language: String,
    /// 顶部项目切换器显示（默认隐藏）。
    pub project_switcher: bool,
    pub visible_apps: BTreeMap<String, bool>,
    pub app_order: Vec<String>,
    /// 每工具路由配置（key = 工具 `id_str()`）。

    pub routing: BTreeMap<String, RoutingConfig>,
    /// 云同步配置。
    pub sync: SyncConfig,
}

impl Settings {
    /// 取某工具的路由配置（无记录返回默认）。
    pub fn routing_for(&self, tool: Tool) -> RoutingConfig {
        self.routing
            .get(tool.id_str())
            .cloned()
            .unwrap_or_default()
    }

    /// 写某工具的路由配置（启用与否都记录，便于保留端口）。
    pub fn set_routing(&mut self, tool: Tool, cfg: RoutingConfig) {
        self.routing.insert(tool.id_str().to_string(), cfg);
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            language: "zh-cn".into(),
            project_switcher: false,
            visible_apps: BTreeMap::new(),
            app_order: Vec::new(),
            routing: BTreeMap::new(),
            sync: SyncConfig::default(),
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
    /// 每工具聚合配置（key = 工具 `id_str()`）。

    pub aggregation: BTreeMap<String, AggregationConfig>,
    /// 项目快照（key = 项目名）。

    pub projects: BTreeMap<String, ProjectSnapshot>,
    /// 托管账号（Beta，key = 账号 id）。
    /// 统一供应商（跨应用共享配置，key = 供应商 id）。
    pub universal: BTreeMap<String, ns_presets::UniversalProvider>,
    /// 托管账号（Beta，key = 账号 id）。
    pub accounts: BTreeMap<String, HostedAccount>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            providers: BTreeMap::new(),
            current: CurrentSelection::default(),
            settings: Settings::default(),
            aggregation: BTreeMap::new(),
            projects: BTreeMap::new(),
            universal: BTreeMap::new(),
            accounts: BTreeMap::new(),
        }
    }
}

/// 项目快照：某工具组的当前供应商 + 聚合配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectSnapshot {
    /// 每工具当前供应商 id（key = 工具 `id_str()`）。
    pub selections: BTreeMap<String, String>,
    /// 每工具聚合配置（key = 工具 `id_str()`）。
    pub aggregation: BTreeMap<String, AggregationConfig>,
}

impl Default for ProjectSnapshot {
    fn default() -> Self {
        Self {
            selections: BTreeMap::new(),
            aggregation: BTreeMap::new(),
        }
    }
}

/// 托管账号（Beta：Copilot / ChatGPT / xAI 订阅作为供应商）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HostedAccount {
    /// 账号 id（copilot / chatgpt / xai）。
    pub id: String,
    /// 生成后的托管供应商 id（空 = 未生成）。
    pub provider_id: String,
    /// 显示名。
    pub name: String,
    /// 是否已登录（Beta：登录标记，真实 OAuth 走浏览器回调）。
    pub logged_in: bool,
}

impl Default for HostedAccount {
    fn default() -> Self {
        Self {
            id: String::new(),
            provider_id: String::new(),
            name: String::new(),
            logged_in: false,
        }
    }
}

impl Config {
    /// 列出某工具的供应商（按 meta 记录的工具归属）。
    pub fn providers_for(&self, tool: Tool) -> Vec<&Provider> {
        let mut list: Vec<_> = self
            .providers
            .values()
            .filter(|p| provider_tool_of(p) == Some(tool))
            .collect();
        // 稳定排序：有 sort_index 的在前升序，无的按 id 殿后。
        list.sort_by_key(|p| (p.sort_index.is_none(), p.sort_index.unwrap_or(usize::MAX), p.id.clone()));
        list
    }

    /// 取某工具的聚合配置（无记录返回默认）。
    pub fn aggregation_for(&self, tool: Tool) -> AggregationConfig {
        self.aggregation
            .get(tool.id_str())
            .cloned()
            .unwrap_or_default()
    }

    /// 写某工具的聚合配置。
    pub fn set_aggregation(&mut self, tool: Tool, cfg: AggregationConfig) {
        self.aggregation.insert(tool.id_str().to_string(), cfg);
    }

    /// 某工具在聚合中的供应商（保序，跳过不存在/不属该工具的）。
    pub fn aggregation_providers(&self, tool: Tool) -> Vec<&Provider> {
        let cfg = self.aggregation_for(tool);
        cfg.provider_ids
            .iter()
            .filter_map(|id| self.providers.get(id))
            .filter(|p| provider_tool_of(p) == Some(tool))
            .collect()
    }
}

/// 一个 live 首写备份条目。
#[derive(Debug, Clone, serde::Serialize)]
pub struct BackupInfo {
    /// 备份文件路径。
    pub path: String,
    /// 原文件路径（`.source` 标记）。
    pub source: String,
    /// 名称（备份文件名）。
    pub name: String,
    /// 大小（字节）。
    pub size: u64,
    /// 备份时间（Unix 秒）。
    pub mtime: i64,
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

    /// 列出 live 首写备份（按 mtime 新→旧）。
    pub fn list_backups(&self) -> Vec<BackupInfo> {
        let dir = self.backups_dir();
        let Ok(entries) = fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for e in entries.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(".source") {
                continue;
            }
            let source = fs::read_to_string(format!("{}.source", path.display()))
                .unwrap_or_default();
            let meta = e.metadata().ok();
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let mtime = meta
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            out.push(BackupInfo {
                path: path.to_string_lossy().into_owned(),
                source,
                name,
                size,
                mtime,
            });
        }
        out.sort_by_key(|b| std::cmp::Reverse(b.mtime));
        out
    }

    /// 从备份恢复原文件（复制备份内容到 `.source` 标记的路径）。
    pub fn restore_backup(&self, backup_path: &str) -> Result<()> {
        let backup = Path::new(backup_path);
        let source = fs::read_to_string(format!("{}.source", backup.display()))
            .map_err(|e| anyhow::anyhow!("备份无来源标记: {e}"))?;
        let bytes = fs::read(backup)
            .map_err(|e| anyhow::anyhow!("读备份失败: {e}"))?;
        if let Some(parent) = Path::new(&source).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&source, bytes).map_err(|e| anyhow::anyhow!("恢复失败: {e}"))?;
        Ok(())
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
        // 从聚合列表中清理；若删除的是默认，先切到列表首项。
        for cfg in self.config.aggregation.values_mut() {
            let was_default = cfg.default_id.as_deref() == Some(id);
            cfg.provider_ids.retain(|p| p != id);
            if was_default {
                cfg.default_id = cfg.provider_ids.first().cloned();
            }
        }
        self.save()
    }

    /// 保存统一供应商（新增或更新）并同步子供应商到各应用。
    /// 返回 (claude, codex, gemini) 各应用是否启用（用于 UI 提示）。
    pub fn save_universal(&mut self, u: ns_presets::UniversalProvider) -> Result<()> {
        let id = u.id.clone();
        self.config.universal.insert(id.clone(), u);
        self.sync_universal_children(&id)?;
        self.save()
    }

    /// 从统一供应商投影子供应商（universal-claude-<id> 等）写入 providers。
    /// 关闭的应用删除其子供应商。
    pub fn sync_universal_children(&mut self, id: &str) -> Result<()> {
        let Some(u) = self.config.universal.get(id).cloned() else {
            bail!("统一供应商不存在: {id}");
        };
        for (suffix, child) in [
            ("claude", u.to_claude_provider()),
            ("codex", u.to_codex_provider()),
            ("gemini", u.to_gemini_provider()),
        ] {
            let child_id = format!("universal-{suffix}-{id}");
            match child {
                Some(p) => {
                    // 保留已有子供应商的应用专属配置（created_at/sort_index）。
                    if let Some(existing) = self.config.providers.get(&child_id).cloned() {
                        let mut p = p;
                        p.created_at = existing.created_at.or(p.created_at);
                        p.sort_index = existing.sort_index;
                        p.notes = existing.notes;
                        p.meta = existing.meta.or(p.meta);
                        self.config.providers.insert(child_id, p);
                    } else {
                        self.config.providers.insert(child_id, p);
                    }
                }
                None => {
                    self.config.providers.remove(&child_id);
                }
            }
        }
        Ok(())
    }

    /// 删除统一供应商及其子供应商。
    pub fn remove_universal(&mut self, id: &str) -> Result<()> {
        if self.config.universal.remove(id).is_none() {
            bail!("统一供应商不存在: {id}");
        }
        for suffix in ["claude", "codex", "gemini"] {
            let child_id = format!("universal-{suffix}-{id}");
            // 子供应商若正被某应用使用，先清 current。
            for tool in Tool::ALL {
                if self.config.current.get(tool) == Some(&child_id) {
                    self.config.current.remove(tool);
                }
            }
            self.config.providers.remove(&child_id);
        }
        self.save()
    }

    /// 写某工具的聚合配置并保存。
    pub fn set_aggregation(&mut self, tool: Tool, cfg: AggregationConfig) -> Result<()> {
        cfg.validate().map_err(anyhow::Error::msg)?;
        self.config.set_aggregation(tool, cfg);
        self.save()
    }

    /// 按给定 id 顺序重排（写入 sort_index 0..n；未列出的清空）。
    pub fn reorder_providers(&mut self, tool: Tool, ids: &[String]) -> Result<()> {
        for (idx, id) in ids.iter().enumerate() {
            if let Some(p) = self.config.providers.get_mut(id) {
                p.sort_index = Some(idx);
            }
        }
        // 未在列表中的同工具供应商清空 sort_index，避免残留排序。
        let listed: std::collections::HashSet<&String> = ids.iter().collect();
        let tool_ids: Vec<String> = self
            .config
            .providers_for(tool)
            .into_iter()
            .map(|p| p.id.clone())
            .collect();
        for id in tool_ids {
            if !listed.contains(&id) {
                if let Some(p) = self.config.providers.get_mut(&id) {
                    p.sort_index = None;
                }
            }
        }
        self.save()
    }

    /// 导出某工具全部供应商为 JSON 文本（可读格式）。
    pub fn export_providers(&self, tool: Tool) -> Result<String> {
        let list = self.config.providers_for(tool);
        let pretty = serde_json::to_string_pretty(&list)?;
        Ok(pretty)
    }

    /// 从 JSON 文本导入供应商（整段替换该工具现有列表；保留 id/created_at）。
    /// 返回导入数量。仅接受合法 Provider 数组。
    pub fn import_providers(&mut self, tool: Tool, json: &str) -> Result<usize> {
        let list: Vec<Provider> = serde_json::from_str(json)
            .map_err(|e| anyhow::anyhow!("JSON 解析失败: {e}"))?;
        // 先移除该工具现有全部。
        let tool_ids: Vec<String> = self
            .config
            .providers_for(tool)
            .into_iter()
            .map(|p| p.id.clone())
            .collect();
        for id in &tool_ids {
            self.config.providers.remove(id);
        }
        for (idx, mut p) in list.into_iter().enumerate() {
            // 补齐工具归属元数据：无 template_type 或不属于当前工具的，
            // 强制归入当前工具，保证导出→导入语义一致。
            let meta = p.meta.get_or_insert_with(Default::default);
            if meta.template_type.as_deref() != Some(tool.id_str()) {
                meta.template_type = Some(tool.id_str().to_string());
            }
            p.sort_index = Some(idx);
            self.config.providers.insert(p.id.clone(), p);
        }
        let count = self.config.providers_for(tool).len();
        self.save()?;
        Ok(count)
    }
    pub fn set_current(&mut self, tool: Tool, id: String) -> Result<()> {
        self.config.current.set(tool, id);
        self.save()
    }

    /// 列出项目快照名（排序）。
    pub fn list_projects(&self) -> Vec<String> {
        self.config.projects.keys().cloned().collect()
    }

    /// 保存当前状态为项目快照（current + aggregation）。
    pub fn save_project(&mut self, name: &str) -> Result<()> {
        let mut snap = ProjectSnapshot::default();
        for tool in Tool::ALL {
            let key = tool.id_str().to_string();
            if let Some(id) = self.config.current.get(tool) {
                snap.selections.insert(key.clone(), id.clone());
            }
            if let Some(cfg) = self.config.aggregation.get(&key) {
                snap.aggregation.insert(key, cfg.clone());
            }
        }
        self.config.projects.insert(name.to_string(), snap);
        self.save()
    }

    /// 应用项目快照到 current + aggregation。
    pub fn apply_project(&mut self, name: &str) -> Result<()> {
        let Some(snap) = self.config.projects.get(name).cloned() else {
            bail!("项目不存在: {name}");
        };
        for (key, id) in &snap.selections {
            if let Some(tool) = Tool::ALL.iter().find(|t| t.id_str() == key) {
                if self.config.providers.contains_key(id) {
                    self.config.current.set(*tool, id.clone());
                }
            }
        }
        for (key, cfg) in &snap.aggregation {
            self.config.aggregation.insert(key.clone(), cfg.clone());
        }
        self.save()
    }

    /// 删除项目快照。
    pub fn delete_project(&mut self, name: &str) -> Result<()> {
        if self.config.projects.remove(name).is_none() {
            bail!("项目不存在: {name}");
        }
        self.save()
    }

    /// 登录托管账号：标记登录并生成托管供应商（路由模式）。
    pub fn login_account(&mut self, id: &str, name: &str) -> Result<()> {
        let pid = format!("hosted-{id}");
        if !self.config.providers.contains_key(&pid) {
            let mut p = Provider::with_id(
                pid.clone(),
                format!("{name} 订阅"),
                serde_json::json!({
                    "env": {
                        "ANTHROPIC_BASE_URL": "http://127.0.0.1:8787",
                        "ANTHROPIC_AUTH_TOKEN": format!("hosted-{id}-token"),
                    },
                }),
            );
            p.category = Some(ProviderCategory::ThirdParty);
            self.config.providers.insert(pid.clone(), p);
        }
        self.config.accounts.insert(
            id.to_string(),
            HostedAccount {
                id: id.into(),
                provider_id: pid,
                name: name.into(),
                logged_in: true,
            },
        );
        self.save()
    }

    /// 登出托管账号（保留供应商，仅标记）。
    pub fn logout_account(&mut self, id: &str) -> Result<()> {
        if let Some(a) = self.config.accounts.get_mut(id) {
            a.logged_in = false;
        }
        self.save()
    }

    /// 写某工具的路由配置并保存。
    pub fn set_routing(&mut self, tool: Tool, cfg: RoutingConfig) -> Result<()> {
        self.config.settings.set_routing(tool, cfg);
        self.save()
    }

    /// 写云同步配置并保存。
    pub fn set_sync_config(&mut self, cfg: SyncConfig) -> Result<()> {
        self.config.settings.sync = cfg;
        self.save()
    }

    /// 写主题模式（dark / light / system）并保存。
    pub fn set_theme(&mut self, theme: &str) -> Result<()> {
        self.config.settings.theme = theme.to_string();
        self.save()
    }

    /// 写语言（zh-cn / zh-tw / en / ja）并保存。
    pub fn set_language(&mut self, lang: &str) -> Result<()> {
        self.config.settings.language = lang.to_string();
        self.save()
    }

    /// 切换顶部项目切换器显示（保存）。
    pub fn set_project_switcher(&mut self, visible: bool) -> Result<()> {
        self.config.settings.project_switcher = visible;
        self.save()
    }

    /// 推送 config.json 到 WebDAV（`PUT {url}/config.json`）。
    pub fn sync_push(&self) -> Result<()> {
        let cfg = &self.config.settings.sync;
        if cfg.provider != "webdav" || cfg.url.is_empty() {
            bail!("仅支持 WebDAV 推送（请在设置配置 URL）");
        }
        let url = format!("{}/config.json", cfg.url.trim_end_matches('/'));
        let out = std::process::Command::new("curl")
            .arg("-sS")
            .arg("-o")
            .arg("/dev/null")
            .arg("-w")
            .arg("%{http_code}")
            .arg("-u")
            .arg(format!("{}:{}", cfg.username, cfg.password))
            .arg("-T")
            .arg(self.config_path.to_string_lossy().into_owned())
            .arg(&url)
            .output()
            .map_err(|e| anyhow::anyhow!("curl 执行失败: {e}"))?;
        let code = String::from_utf8_lossy(&out.stdout);
        if code.starts_with('2') {
            Ok(())
        } else {
            bail!("WebDAV 推送失败: HTTP {code}")
        }
    }

    /// 从 WebDAV 拉取 config.json 并应用。
    pub fn sync_pull(&mut self) -> Result<()> {
        let cfg = self.config.settings.sync.clone();
        if cfg.provider != "webdav" || cfg.url.is_empty() {
            bail!("仅支持 WebDAV 拉取（请在设置配置 URL）");
        }
        let url = format!("{}/config.json", cfg.url.trim_end_matches('/'));
        let out = std::process::Command::new("curl")
            .arg("-sS")
            .arg("-u")
            .arg(format!("{}:{}", cfg.username, cfg.password))
            .arg(&url)
            .output()
            .map_err(|e| anyhow::anyhow!("curl 执行失败: {e}"))?;
        if !out.status.success() {
            bail!("WebDAV 拉取失败: HTTP {}", out.status);
        }
        let remote: Config = serde_json::from_slice(&out.stdout)
            .map_err(|e| anyhow::anyhow!("远端 config 解析失败: {e}"))?;
        self.config = remote;
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
    use ns_presets::{ProviderCategory, ProviderMeta};

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

    fn provider_for_tool(id: &str, tool: Tool) -> Provider {
        let mut p = sample_provider(id);
        p.meta = Some(ProviderMeta {
            template_type: Some(tool.id_str().to_string()),
            ..Default::default()
        });
        p
    }

    #[test]
    fn providers_for_sorted_by_sort_index() {
        let dir = test_dir();
        {
            let mut store = Store::open_at(dir.clone()).unwrap();
            store.add_provider(provider_for_tool("b", Tool::Claude)).unwrap();
            store.add_provider(provider_for_tool("a", Tool::Claude)).unwrap();
            store
                .reorder_providers(Tool::Claude, &["a".into(), "b".into()])
                .unwrap();
        }
        let store = Store::open_at(dir.clone()).unwrap();
        let ids: Vec<_> = store
            .config()
            .providers_for(Tool::Claude)
            .into_iter()
            .map(|p| p.id.clone())
            .collect();
        assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn export_import_roundtrip() {
        let dir = test_dir();
        {
            let mut store = Store::open_at(dir.clone()).unwrap();
            store.add_provider(provider_for_tool("p1", Tool::Claude)).unwrap();
            store.add_provider(provider_for_tool("p2", Tool::Claude)).unwrap();
            let json = store.export_providers(Tool::Claude).unwrap();
            assert!(json.contains("p1"));
        }
        {
            let mut store = Store::open_at(dir.clone()).unwrap();
            let json = store.export_providers(Tool::Claude).unwrap();
            let n = store.import_providers(Tool::Claude, &json).unwrap();
            assert_eq!(n, 2);
            let ids: Vec<_> = store
                .config()
                .providers_for(Tool::Claude)
                .into_iter()
                .map(|p| p.id.clone())
                .collect();
            assert!(ids.contains(&"p1".to_string()));
            assert!(ids.contains(&"p2".to_string()));
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn import_rejects_bad_json() {
        let dir = test_dir();
        let mut store = Store::open_at(dir.clone()).unwrap();
        assert!(store.import_providers(Tool::Claude, "{ not valid").is_err());
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn aggregation_crud_and_default_guard() {
        let dir = test_dir();
        {
            let mut store = Store::open_at(dir.clone()).unwrap();
            store.add_provider(provider_for_tool("a", Tool::Claude)).unwrap();
            store.add_provider(provider_for_tool("b", Tool::Claude)).unwrap();
            store.add_provider(provider_for_tool("c", Tool::Codex)).unwrap();

            // 空列表启用 → 校验失败。
            let cfg = AggregationConfig {
                enabled: true,
                provider_ids: vec![],
                default_id: None,
            };
            assert!(store.set_aggregation(Tool::Claude, cfg).is_err());

            // 默认不在列表 → 校验失败。
            let cfg = AggregationConfig {
                enabled: true,
                provider_ids: vec!["a".into(), "b".into()],
                default_id: Some("c".into()),
            };
            assert!(store.set_aggregation(Tool::Claude, cfg).is_err());

            // 合法配置。
            let cfg = AggregationConfig {
                enabled: true,
                provider_ids: vec!["a".into(), "b".into()],
                default_id: Some("a".into()),
            };
            store.set_aggregation(Tool::Claude, cfg).unwrap();

            // 聚合供应商保序。
            let ids: Vec<_> = store
                .config()
                .aggregation_providers(Tool::Claude)
                .into_iter()
                .map(|p| p.id.clone())
                .collect();
            assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);

            // 删除默认 → 自动切到列表首项。
            store.remove_provider("a").unwrap();
            let cfg = store.config().aggregation_for(Tool::Claude);
            assert_eq!(cfg.provider_ids, vec!["b".to_string()]);
            assert_eq!(cfg.default_id.as_deref(), Some("b"));
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn project_snapshot_roundtrip() {
        let dir = test_dir();
        {
            let mut store = Store::open_at(dir.clone()).unwrap();
            store.add_provider(sample_provider("p1")).unwrap();
            store.add_provider(sample_provider("p2")).unwrap();
            store.set_current(Tool::Claude, "p1".into()).unwrap();
            store.save_project("工作").unwrap();
            store.set_current(Tool::Claude, "p2".into()).unwrap();
            assert_eq!(
                store.config().current.get(Tool::Claude).map(String::as_str),
                Some("p2")
            );
            store.apply_project("工作").unwrap();
            assert_eq!(
                store.config().current.get(Tool::Claude).map(String::as_str),
                Some("p1")
            );
            assert_eq!(store.list_projects(), vec!["工作".to_string()]);
            store.delete_project("工作").unwrap();
            assert!(store.list_projects().is_empty());
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn hosted_account_creates_provider() {
        let dir = test_dir();
        {
            let mut store = Store::open_at(dir.clone()).unwrap();
            store.login_account("copilot", "GitHub Copilot").unwrap();
            assert!(store.config().accounts.contains_key("copilot"));
            assert!(store.config().providers.contains_key("hosted-copilot"));
            assert!(store.config().accounts["copilot"].logged_in);
            store.logout_account("copilot").unwrap();
            assert!(!store.config().accounts["copilot"].logged_in);
            // 供应商保留。
            assert!(store.config().providers.contains_key("hosted-copilot"));
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn backup_list_and_restore() {
        let dir = test_dir();
        {
            let store = Store::open_at(dir.clone()).unwrap();
            let bdir = store.backups_dir();
            fs::create_dir_all(&bdir).unwrap();
            let src = dir.join("env.actual");
            fs::write(&src, "NEW").unwrap();
            fs::write(bdir.join("abc-env"), "OLD").unwrap();
            fs::write(
                bdir.join("abc-env.source"),
                src.to_string_lossy().into_owned(),
            )
            .unwrap();
            let list = store.list_backups();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].name, "abc-env");
            assert!(list[0].size > 0);
            store.restore_backup(&list[0].path).unwrap();
            assert_eq!(fs::read_to_string(&src).unwrap(), "OLD");
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn sync_config_roundtrip() {
        let dir = test_dir();
        {
            let mut store = Store::open_at(dir.clone()).unwrap();
            store
                .set_sync_config(SyncConfig {
                    provider: "webdav".into(),
                    url: "https://dav.example.com/ns".into(),
                    username: "u".into(),
                    password: "p".into(),
                    bucket: String::new(),
                    enabled: true,
                })
                .unwrap();
            assert_eq!(store.config().settings.sync.provider, "webdav");
            assert!(store.config().settings.sync.enabled);
            // 非 WebDAV 推送应报错（不发网络）。
            let mut bad = store.config().settings.sync.clone();
            bad.provider = "s3".into();
            assert!(store.set_sync_config(bad).is_ok());
            assert!(store.sync_push().is_err());
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn universal_sync_creates_children() {
        let dir = test_dir();
        {
            let mut store = Store::open_at(dir.clone()).unwrap();
            let u = ns_presets::UniversalProvider {
                id: "newapi".into(),
                name: "NewAPI".into(),
                provider_type: "newapi".into(),
                apps: ns_presets::UniversalApps::default(),
                base_url: "https://api.newapi.pro".into(),
                api_key: "sk-test".into(),
                models: ns_presets::UniversalModels::default(),
                website_url: Some("https://www.newapi.pro".into()),
                icon: Some("newapi".into()),
                icon_color: Some("#00A67E".into()),
                created_at: Some(1),
            };
            store.save_universal(u).unwrap();
        }
        let store = Store::open_at(dir.clone()).unwrap();
        assert!(store.config().universal.contains_key("newapi"));
        // 三个子供应商都存在且 meta 标记 universal。
        for suffix in ["claude", "codex", "gemini"] {
            let id = format!("universal-{suffix}-newapi");
            let p = store.config().providers.get(&id).expect(&id);
            assert_eq!(
                p.meta.as_ref().and_then(|m| m.template_type.as_deref()),
                Some("universal")
            );
        }
        // Claude 子供应商含 ANTHROPIC_BASE_URL。
        let c = store.config().providers.get("universal-claude-newapi").unwrap();
        assert_eq!(
            c.settings_config["env"]["ANTHROPIC_BASE_URL"],
            "https://api.newapi.pro"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn universal_disable_app_removes_child() {
        let dir = test_dir();
        {
            let mut store = Store::open_at(dir.clone()).unwrap();
            let u = ns_presets::UniversalProvider {
                id: "gw".into(),
                name: "网关".into(),
                provider_type: "custom_gateway".into(),
                apps: ns_presets::UniversalApps {
                    claude: false,
                    codex: true,
                    gemini: true,
                },
                base_url: "https://gw.example.com".into(),
                api_key: "sk".into(),
                models: ns_presets::UniversalModels::default(),
                website_url: None,
                icon: None,
                icon_color: None,
                created_at: None,
            };
            store.save_universal(u).unwrap();
        }
        let store = Store::open_at(dir.clone()).unwrap();
        assert!(!store.config().providers.contains_key("universal-claude-gw"));
        assert!(store.config().providers.contains_key("universal-codex-gw"));
        assert!(store.config().providers.contains_key("universal-gemini-gw"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn universal_remove_deletes_children() {
        let dir = test_dir();
        {
            let mut store = Store::open_at(dir.clone()).unwrap();
            let u = ns_presets::UniversalProvider {
                id: "gw2".into(),
                name: "网关2".into(),
                provider_type: "custom_gateway".into(),
                apps: ns_presets::UniversalApps::default(),
                base_url: "https://gw2.example.com".into(),
                api_key: "sk".into(),
                models: ns_presets::UniversalModels::default(),
                website_url: None,
                icon: None,
                icon_color: None,
                created_at: None,
            };
            store.save_universal(u).unwrap();
            store.remove_universal("gw2").unwrap();
        }
        let store = Store::open_at(dir.clone()).unwrap();
        assert!(!store.config().universal.contains_key("gw2"));
        for suffix in ["claude", "codex", "gemini"] {
            assert!(!store.config().providers.contains_key(&format!("universal-{suffix}-gw2")));
        }
        let _ = fs::remove_dir_all(dir);
    }
}

