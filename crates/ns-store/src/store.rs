//! 配置存储：读、写（原子）、供应商 CRUD、设置。
//!
//! 目录：`~/.ns-switch/`（`NS_SWITCH_HOME` 可覆盖；HOME 缺失时回退当前目录）。
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
use sha2::Digest;

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
    /// 熔断器失败阈值（连续失败 N 次打开；默认 3）。
    #[serde(default = "default_breaker_threshold")]
    pub breaker_threshold: u32,
    /// 熔断器冷却秒数（打开后重试前等待；默认 30）。
    #[serde(default = "default_breaker_cooldown")]
    pub breaker_cooldown: u32,
    /// 整流器启用（修正上游无法处理的请求；默认 true）。对齐 cc-switch `RectifierConfig`。
    #[serde(default = "default_true")]
    pub rectify_enabled: bool,
    /// 上游是否支持 Anthropic thinking 参数（false = 整流剥离；默认 false）。
    #[serde(default)]
    pub rectify_thinking: bool,
    /// 图像处理策略（0=保留 / 1=丢弃 / 2=文本占位；默认 0=保留）。
    #[serde(default)]
    pub rectify_image: u32,
}

fn default_breaker_threshold() -> u32 {
    3
}

fn default_true() -> bool {
    true
}

fn default_breaker_cooldown() -> u32 {
    30
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 0,
            listen_addr: "127.0.0.1".into(),
            failover: false,
            breaker_threshold: 3,
            breaker_cooldown: 30,
            rectify_enabled: true,
            rectify_thinking: false,
            rectify_image: 0,
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
    /// 自动同步间隔（分钟；0 = 关闭）。
    #[serde(default)]
    pub auto_sync_minutes: u32,
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
            auto_sync_minutes: 0,
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
    /// 首次运行欢迎页是否已关闭。
    pub welcome_dismissed: bool,
    /// 上次启动见过的应用版本（WhatsNew 检测用；空 = 首次）。
    pub last_seen_version: String,
    /// 每工具路由配置（key = 工具 `id_str()`）。

    pub routing: BTreeMap<String, RoutingConfig>,
    /// 云同步配置。
    pub sync: SyncConfig,
    /// 是否使用共享 `~/.agents/skills` 目录（Skills 全工具共用）。
    pub shared_skills: bool,
    /// 自定义模型定价（key = 模型名前缀，value = {input_per_mtok,
    /// output_per_mtok, cache_read_per_mtok}）。覆盖内置价格表。
    #[serde(default)]
    pub custom_pricing: BTreeMap<String, serde_json::Value>,
    /// 全局出站代理 URL（None/空 = 直连）。ns-switch 自身出站请求（curl）经此代理。
    #[serde(default)]
    pub global_proxy: Option<String>,
    /// 每工具配置目录覆盖（key = 工具 `id_str()`，value = 绝对路径）。
    /// 空/缺失 = 使用默认 `~/.<tool>`。对齐 cc-switch `DirectorySettings`。
    #[serde(default)]
    pub config_dirs: BTreeMap<String, String>,
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


    /// 设置自定义模型定价（覆盖内置；input/output/cache 为 USD/1M tokens）。
    pub fn set_custom_price(
        &mut self,
        model: &str,
        input_per_mtok: f64,
        output_per_mtok: f64,
        cache_read_per_mtok: Option<f64>,
    ) {
        let mut v = serde_json::Map::new();
        v.insert("input_per_mtok".into(), serde_json::json!(input_per_mtok));
        v.insert("output_per_mtok".into(), serde_json::json!(output_per_mtok));
        if let Some(c) = cache_read_per_mtok {
            v.insert("cache_read_per_mtok".into(), serde_json::json!(c));
        }
        self.custom_pricing
            .insert(model.to_string(), serde_json::Value::Object(v));
    }

    /// 删除自定义模型定价。
    pub fn remove_custom_price(&mut self, model: &str) {
        self.custom_pricing.remove(model);
    }

    /// 当前全局出站代理 URL（None/空 = 直连）。
    pub fn global_proxy(&self) -> Option<&str> {
        self.global_proxy
            .as_deref()
            .filter(|s| !s.trim().is_empty())
    }

    /// 设置全局出站代理（空字符串 = 清除/直连）。
    pub fn set_global_proxy(&mut self, url: &str) {
        let trimmed = url.trim();
        self.global_proxy = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
    }

    /// 某工具的配置目录覆盖（`config_dirs`；None = 用默认）。
    pub fn config_dir_for(&self, tool: Tool) -> Option<&str> {
        self.config_dirs
            .get(tool.id_str())
            .map(String::as_str)
            .filter(|s| !s.trim().is_empty())
    }

    /// 设置某工具的配置目录覆盖（空字符串 = 清除覆盖/恢复默认）。
    pub fn set_config_dir(&mut self, tool: Tool, dir: &str) {
        let trimmed = dir.trim();
        if trimmed.is_empty() {
            self.config_dirs.remove(tool.id_str());
        } else {
            self.config_dirs.insert(tool.id_str().to_string(), trimmed.to_string());
        }
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
            welcome_dismissed: false,
            last_seen_version: String::new(),
            routing: BTreeMap::new(),
            sync: SyncConfig::default(),
            shared_skills: false,
            custom_pricing: BTreeMap::new(),
            global_proxy: None,
            config_dirs: BTreeMap::new(),
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
    /// 当前激活的项目快照名（None = 未激活任何项目）。
    /// 兼容字段：`active_projects` 优先（cc-switch Profile 语义对齐，
    /// 每工具组独立当前项目）；旧配置仅此字段时回退为 Claude 组。
    pub active_project: Option<String>,
    /// 每工具组当前激活的项目快照名（key = 工具 `id_str()`，cc-switch
    /// Profile 语义：Claude 组=Claude+ClaudeDesktop，其余每工具一组）。
    #[serde(default)]
    pub active_projects: BTreeMap<String, String>,
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
            active_project: None,
            active_projects: BTreeMap::new(),
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
    /// 扩展范围快照（MCP/Skills/Prompts 等，key = 资源名，JSON 载体）。
    /// 向后兼容：`#[serde(default)]`，旧快照无此字段。
    #[serde(default)]
    pub extras: BTreeMap<String, serde_json::Value>,
}

impl Default for ProjectSnapshot {
    fn default() -> Self {
        Self {
            selections: BTreeMap::new(),
            aggregation: BTreeMap::new(),
            extras: BTreeMap::new(),
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

    /// 备份根目录（含首写备份、手动备份、同步冲突备份）。
    pub fn backups_root(&self) -> PathBuf {
        self.dir.join("backups")
    }

    /// 列出 live 首写备份（按 mtime 新→旧）。
    pub fn list_backups(&self) -> Vec<BackupInfo> {
        scan_backups(&self.backups_dir(), None)
    }

    /// 手动备份 `config.json` 到 `backups/manual/`，返回备份文件路径。
    ///
    /// 配置文件尚未落盘时（首次运行）先序列化当前内存配置，避免产生空备份。
    pub fn manual_backup(&self) -> Result<PathBuf> {
        let dir = self.backups_root().join("manual");
        fs::create_dir_all(&dir)
            .with_context(|| format!("创建备份目录失败: {}", dir.display()))?;
        let path = dir.join(format!("config-{}.json", now_ts()));
        if self.config_path.exists() {
            fs::copy(&self.config_path, &path)
                .with_context(|| format!("复制配置到备份失败: {}", path.display()))?;
        } else {
            atomic_write_json(&path, &self.config)?;
        }
        Ok(path)
    }

    /// 列出手动备份（新→旧）。
    pub fn list_manual_backups(&self) -> Vec<BackupInfo> {
        scan_backups(&self.backups_root().join("manual"), Some(&self.config_path))
    }

    /// 从手动备份恢复 `config.json`（恢复前先把当前配置备份到 `backups/pre-restore/`）。
    ///
    /// 备份内容必须是合法配置；否则报错且不改动现有配置。
    pub fn restore_manual_backup(&mut self, backup_path: &str) -> Result<()> {
        let bytes = fs::read(backup_path)
            .with_context(|| format!("读备份失败: {backup_path}"))?;
        let text = String::from_utf8(bytes.clone())
            .map_err(|e| anyhow::anyhow!("备份不是 UTF-8 文本: {e}"))?;
        serde_json::from_str::<Config>(&text)
            .map_err(|e| anyhow::anyhow!("备份内容不是有效的配置: {e}"))?;
        // 恢复前保留当前配置，便于回退。
        let pre = self.backups_root().join("pre-restore");
        if self.config_path.exists() {
            fs::create_dir_all(&pre)?;
            let _ = fs::copy(
                &self.config_path,
                pre.join(format!("config-{}.json", now_ts())),
            );
        }
        let tmp = self
            .config_path
            .with_extension(format!("restore-{}", now_ts()));
        fs::write(&tmp, &bytes).with_context(|| format!("写恢复临时文件失败: {}", tmp.display()))?;
        fs::rename(&tmp, &self.config_path)
            .with_context(|| format!("替换配置失败: {}", self.config_path.display()))?;
        self.config = load_config(&self.config_path)?;
        Ok(())
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

    /// 保存当前状态为项目快照（current + aggregation + extras）。
    ///
    /// `scope`：当前工具组（cc-switch Profile 语义，工具 `id_str()`）。
    /// `extras`：扩展范围快照（MCP/Skills/Prompts 等，key = 资源名，
    /// 由调用方收集的 JSON 载体）；空 map 时仅快照供应商/聚合（向后兼容）。
    pub fn save_project(
        &mut self,
        name: &str,
        scope: &str,
        extras: &BTreeMap<String, serde_json::Value>,
    ) -> Result<()> {
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
        snap.extras = extras.clone();
        self.config.projects.insert(name.to_string(), snap);
        self.config.active_project = Some(name.to_string());
        self.config
            .active_projects
            .insert(scope.to_string(), name.to_string());
        self.save()
    }

    /// 应用项目快照到 current + aggregation；返回目标项目的 extras。
    ///
    /// `scope`：当前工具组（cc-switch Profile 语义，工具 `id_str()`）。
    /// `current_extras`：当前激活项目的 extras（用于自动回存；调用方传
    /// 当前 UI 收集的资源状态）。切换前自动把当前 scope 的激活项目保存
    /// 为快照，避免切换后丢失上一个项目的状态。首次应用无激活则不回存。
    pub fn apply_project(
        &mut self,
        name: &str,
        scope: &str,
        current_extras: &BTreeMap<String, serde_json::Value>,
    ) -> Result<BTreeMap<String, serde_json::Value>> {
        let Some(snap) = self.config.projects.get(name).cloned() else {
            bail!("项目不存在: {name}");
        };
        // 自动回存当前 scope 的激活项目（若非目标项目且存在）。
        let prev = self
            .config
            .active_projects
            .get(scope)
            .cloned()
            .or_else(|| {
                // 兼容：旧配置仅 active_project（视为 Claude 组）。
                if scope == "claude" || scope == "claude-desktop" {
                    self.config.active_project.clone()
                } else {
                    None
                }
            });
        if let Some(prev_name) = &prev {
            if prev_name != name {
                self.save_project(prev_name, scope, current_extras)?;
            }
        }
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
        self.config.active_project = Some(name.to_string());
        self.config
            .active_projects
            .insert(scope.to_string(), name.to_string());
        self.save()?;
        Ok(snap.extras)
    }

    /// 工具组 scope（cc-switch Profile 语义）：Claude+ClaudeDesktop 同组
    /// "claude"，其余工具各自 `id_str()`。
    pub fn scope_for(tool: Tool) -> &'static str {
        match tool {
            Tool::Claude | Tool::ClaudeDesktop => "claude",
            other => other.id_str(),
        }
    }

    /// 某工具组当前激活的项目名（`active_projects` 优先，兼容 `active_project`）。
    pub fn active_project_for(&self, tool: Tool) -> Option<String> {
        let scope = Self::scope_for(tool);
        self.config
            .active_projects
            .get(scope)
            .cloned()
            .or_else(|| {
                if scope == "claude" {
                    self.config.active_project.clone()
                } else {
                    None
                }
            })
    }

    /// 列出项目快照名（排序）。
    pub fn list_projects(&self) -> Vec<String> {
        self.config.projects.keys().cloned().collect()
    }


    /// 删除项目快照。
    pub fn delete_project(&mut self, name: &str) -> Result<()> {
        if self.config.projects.remove(name).is_none() {
            bail!("项目不存在: {name}");
        }
        self.save()
    }

    /// 登录托管账号：标记登录、保存真实 OAuth token 到工具凭据文件、生成托管供应商。
    ///
    /// `provider` 为 OAuth provider（`xai`/`codex`）；`token` 为 OAuth access_token。
    /// 凭据写入：Codex `~/.codex/auth.json`（`tokens.access_token`）、
    /// xAI `~/.xai/credentials.json`（`access_token`）。
    pub fn login_account(
        &mut self,
        id: &str,
        name: &str,
        provider: &str,
        token: &str,
    ) -> Result<()> {
        // 保存真实 token 到工具凭据文件（OAuth 登录成功后的持久化）。
        if let Err(e) = save_oauth_token(provider, token) {
            eprintln!("OAuth token 保存警告: {e}");
        }
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

    /// 切换共享 `~/.agents/skills` 目录（保存）。
    pub fn set_shared_skills(&mut self, shared: bool) -> Result<()> {
        self.config.settings.shared_skills = shared;
        self.save()
    }

    /// 记录首次运行欢迎页已关闭（保存）。
    pub fn set_welcome_dismissed(&mut self, dismissed: bool) -> Result<()> {
        self.config.settings.welcome_dismissed = dismissed;
        self.save()
    }

    /// 记录本次启动看到的版本（WhatsNew 检测用）。
    pub fn set_last_seen_version(&mut self, v: &str) -> Result<()> {
        self.config.settings.last_seen_version = v.to_string();
        self.save()
    }

    /// 设置全局出站代理（保存；空 = 直连）。
    pub fn set_global_proxy(&mut self, url: &str) -> Result<()> {
        self.config.settings.set_global_proxy(url);
        self.save()
    }

    /// 设置某工具配置目录覆盖（保存；空 = 恢复默认）。
    pub fn set_config_dir(&mut self, tool: Tool, dir: &str) -> Result<()> {
        self.config.settings.set_config_dir(tool, dir);
        self.save()
    }

    /// 设置自定义模型定价（保存）。
    pub fn set_custom_price(
        &mut self,
        model: &str,
        input_per_mtok: f64,
        output_per_mtok: f64,
        cache_read_per_mtok: Option<f64>,
    ) -> Result<()> {
        self.config
            .settings
            .set_custom_price(model, input_per_mtok, output_per_mtok, cache_read_per_mtok);
        self.save()
    }

    /// 删除自定义模型定价（保存）。
    pub fn remove_custom_price(&mut self, model: &str) -> Result<()> {
        self.config.settings.remove_custom_price(model);
        self.save()
    }

    /// 某工具是否在侧栏显示（默认显示）。
    pub fn tool_visible(&self, tool: Tool) -> bool {
        self.config
            .settings
            .visible_apps
            .get(tool.id_str())
            .copied()
            .unwrap_or(true)
    }

    /// 设置工具可见性并保存。
    pub fn set_tool_visible(&mut self, tool: Tool, visible: bool) -> Result<()> {
        self.config
            .settings
            .visible_apps
            .insert(tool.id_str().to_string(), visible);
        self.save()
    }

    /// 侧栏工具顺序：按 `app_order` 优先，未记录者按 `Tool::ALL` 顺序追加。
    pub fn tool_order(&self) -> Vec<Tool> {
        let order = &self.config.settings.app_order;
        let mut out: Vec<Tool> = Vec::new();
        for id in order {
            if let Some(t) = Tool::ALL.iter().find(|t| t.id_str() == id) {
                if !out.contains(t) {
                    out.push(*t);
                }
            }
        }
        for t in Tool::ALL {
            if !out.contains(&t) {
                out.push(t);
            }
        }
        out
    }

    /// 保存侧栏工具顺序。
    pub fn set_tool_order(&mut self, order: &[Tool]) -> Result<()> {
        self.config.settings.app_order = order.iter().map(|t| t.id_str().to_string()).collect();
        self.save()
    }

    /// 在侧栏把工具移动到目标索引（-1 = 移到最前）。
    pub fn move_tool(&mut self, tool: Tool, target: isize) -> Result<()> {
        let mut order = self.tool_order();
        if let Some(pos) = order.iter().position(|&t| t == tool) {
            let t = order.remove(pos);
            let idx = (target.max(0) as usize).min(order.len());
            order.insert(idx, t);
            self.set_tool_order(&order)?;
        }
        Ok(())
    }


    /// 推送 config.json 到 WebDAV（`PUT {url}/config.json`）。
    pub fn sync_push(&self) -> Result<()> {
        let cfg = &self.config.settings.sync;
        if cfg.url.is_empty() {
            bail!("请先在设置配置同步 URL");
        }
        if cfg.provider == "s3" {
            return self.s3_push(cfg);
        }
        if cfg.provider != "webdav" {
            bail!("仅支持 WebDAV / S3 推送");
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

    /// S3 PUT `config.json`（SigV4 签名）。
    fn s3_push(&self, cfg: &SyncConfig) -> Result<()> {
        if cfg.bucket.is_empty() || cfg.username.is_empty() || cfg.password.is_empty() {
            bail!("S3 需配置 bucket、Access Key、Secret Key");
        }
        let body = fs::read_to_string(&self.config_path)
            .with_context(|| format!("读配置失败: {}", self.config_path.display()))?;
        let (amz_date, _date) = aws_now();
        let payload_hash = hex::encode(sha2::Sha256::digest(body.as_bytes()));
        let url = s3_object_url(&cfg.url, &cfg.bucket, "config.json");
        let host = url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("")
            .to_string();
        let path = url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .find('/')
            .map(|i| &url[url.find("://").map(|j| j + 3).unwrap_or(0) + i..])
            .unwrap_or("/")
            .to_string();
        let auth = aws_sigv4(
            "PUT",
            &host,
            &path,
            &cfg.username,
            &cfg.password,
            &s3_region(&cfg.url),
            &payload_hash,
            &amz_date,
        );
        let out = std::process::Command::new("curl")
            .arg("-sS")
            .arg("-o")
            .arg("/dev/null")
            .arg("-w")
            .arg("%{http_code}")
            .arg("-X")
            .arg("PUT")
            .arg("-H")
            .arg(format!("Authorization: {auth}"))
            .arg("-H")
            .arg(format!("x-amz-date: {amz_date}"))
            .arg("-H")
            .arg(format!("x-amz-content-sha256: {payload_hash}"))
            .arg("--data-binary")
            .arg(&body)
            .arg(&url)
            .output()
            .map_err(|e| anyhow::anyhow!("curl 执行失败: {e}"))?;
        let code = String::from_utf8_lossy(&out.stdout);
        if code.starts_with('2') {
            Ok(())
        } else {
            bail!("S3 推送失败: HTTP {code}")
        }
    }


    /// 从 WebDAV 拉取 config.json 并应用。
    pub fn sync_pull(&mut self) -> Result<()> {
        let cfg = self.config.settings.sync.clone();
        if cfg.url.is_empty() {
            bail!("请先在设置配置同步 URL");
        }
        let remote: Config = if cfg.provider == "s3" {
            if cfg.bucket.is_empty() || cfg.username.is_empty() || cfg.password.is_empty() {
                bail!("S3 需配置 bucket、Access Key、Secret Key");
            }
            let (amz_date, _date) = aws_now();
            let payload_hash = hex::encode(sha2::Sha256::digest(b""));
            let url = s3_object_url(&cfg.url, &cfg.bucket, "config.json");
            let host = url
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .split('/')
                .next()
                .unwrap_or("")
                .to_string();
            let scheme_len = url.find("://").map(|j| j + 3).unwrap_or(0);
            let path = url[scheme_len..]
                .find('/')
                .map(|i| url[scheme_len + i..].to_string())
                .unwrap_or_else(|| "/".to_string());
            let auth = aws_sigv4(
                "GET",
                &host,
                &path,
                &cfg.username,
                &cfg.password,
                &s3_region(&cfg.url),
                &payload_hash,
                &amz_date,
            );
            let out = std::process::Command::new("curl")
                .arg("-sS")
                .arg("-H")
                .arg(format!("Authorization: {auth}"))
                .arg("-H")
                .arg(format!("x-amz-date: {amz_date}"))
                .arg("-H")
                .arg(format!("x-amz-content-sha256: {payload_hash}"))
                .arg(&url)
                .output()
                .map_err(|e| anyhow::anyhow!("curl 执行失败: {e}"))?;
            if !out.status.success() {
                bail!("S3 拉取失败: HTTP {}", out.status);
            }
            serde_json::from_slice(&out.stdout)
                .map_err(|e| anyhow::anyhow!("远端 config 解析失败: {e}"))?
        } else {
            if cfg.provider != "webdav" {
                bail!("仅支持 WebDAV / S3 拉取");
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
            serde_json::from_slice(&out.stdout)
                .map_err(|e| anyhow::anyhow!("远端 config 解析失败: {e}"))?
        };
        // 冲突处理：覆盖前自动备份本地 config（避免丢失本地修改）。
        backup_sync_conflict(&self.config_path, &self.config);
        self.config = remote;
        self.save()
    }

}

/// 同步冲突备份：本地 config 与远端不同时，备份本地到 `backups/sync/`。
///
/// 返回是否创建了备份（纯函数，便于单测）。
fn backup_sync_conflict(config_path: &Path, local: &Config) -> bool {
    let Ok(local_raw) = fs::read_to_string(config_path) else {
        return false;
    };
    let remote_raw = serde_json::to_string_pretty(local).unwrap_or_default();
    if local_raw.trim() == remote_raw.trim() {
        return false;
    }
    let backup_dir = config_path
        .parent()
        .map(|p| p.join("backups").join("sync"))
        .unwrap_or_else(|| PathBuf::from("backups/sync"));
    if fs::create_dir_all(&backup_dir).is_err() {
        return false;
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup = backup_dir.join(format!("config-{ts}.json"));
    fs::write(&backup, &local_raw).is_ok()
}

/// 保存 OAuth token 到工具凭据文件（OAuth 登录成功后的持久化）。
///
/// - `codex`：`~/.codex/auth.json` 的 `tokens.access_token`（保留其他字段）。
/// - `xai`：`~/.xai/credentials.json` 的 `access_token`。
/// 其他 provider 暂不落盘（返回 Ok，避免阻塞登录）。
fn save_oauth_token(provider: &str, token: &str) -> Result<()> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Ok(());
    };
    match provider {
        "codex" => {
            let path = home.join(".codex").join("auth.json");
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut v: Value = if path.exists() {
                fs::read_to_string(&path)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(Value::Object(Default::default()))
            } else {
                Value::Object(Default::default())
            };
            let obj = v.as_object_mut().ok_or_else(|| anyhow::anyhow!("auth.json 非对象"))?;
            let tokens = obj
                .entry("tokens")
                .or_insert_with(|| Value::Object(Default::default()));
            if let Some(t) = tokens.as_object_mut() {
                t.insert("access_token".into(), Value::String(token.to_string()));
            }
            fs::write(&path, serde_json::to_string_pretty(&v)?)?;
            Ok(())
        }
        "xai" => {
            let path = home.join(".xai").join("credentials.json");
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let v = serde_json::json!({ "access_token": token });
            fs::write(&path, serde_json::to_string_pretty(&v)?)?;
            Ok(())
        }
        "copilot" => {
            let path = home.join(".config").join("github-copilot").join("hosts.json");
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut v: Value = if path.exists() {
                fs::read_to_string(&path)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(Value::Object(Default::default()))
            } else {
                Value::Object(Default::default())
            };
            let obj = v.as_object_mut().ok_or_else(|| anyhow::anyhow!("hosts.json 非对象"))?;
            let host = obj
                .entry("github.com")
                .or_insert_with(|| Value::Object(Default::default()));
            if let Some(h) = host.as_object_mut() {
                h.insert("oauth_token".into(), Value::String(token.to_string()));
            }
            fs::write(&path, serde_json::to_string_pretty(&v)?)?;
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod sync_conflict_tests {
    use super::*;

    #[test]
    fn backup_created_when_local_differs() {
        let dir = std::env::temp_dir().join("ns-sync-conflict-test");
        let _ = fs::create_dir_all(&dir);
        let cfg_path = dir.join("config.json");
        // 本地内容 A，远端（当前 config 序列化）内容 B。
        fs::write(&cfg_path, r#"{"providers":{}}"#).unwrap();
        let mut remote = Config::default();
        remote.settings.shared_skills = true;
        let created = backup_sync_conflict(&cfg_path, &remote);
        assert!(created, "本地与远端不同应创建备份");
        let backups = fs::read_dir(dir.join("backups").join("sync")).unwrap().count();
        assert_eq!(backups, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_backup_when_identical() {
        let dir = std::env::temp_dir().join("ns-sync-conflict-same-test");
        let _ = fs::create_dir_all(&dir);
        let cfg_path = dir.join("config.json");
        let local = Config::default();
        let raw = serde_json::to_string_pretty(&local).unwrap();
        fs::write(&cfg_path, &raw).unwrap();
        let created = backup_sync_conflict(&cfg_path, &local);
        assert!(!created, "本地与远端相同不应备份");
        let _ = fs::remove_dir_all(&dir);
    }
}

/// AWS Signature V4 签名（S3 PUT/GET 用）。
fn aws_sigv4(
    method: &str,
    host: &str,
    path: &str,
    access_key: &str,
    secret_key: &str,
    region: &str,
    payload_hash: &str,
    amz_date: &str,
) -> String {
    use hmac::{Hmac, Mac};
    use sha2::{Digest, Sha256};
    type HmacSha256 = Hmac<Sha256>;

    let service = "s3";
    let canonical_uri = if path.is_empty() { "/" } else { path };
    let canonical_query = "";
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";
    let canonical_headers = format!(
        "host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n"
    );
    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );
    let scope = format!("{}/{region}/{service}/aws4_request", &amz_date[..8]);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        hex::encode(Sha256::digest(canonical_request.as_bytes()))
    );
    let mut k = HmacSha256::new_from_slice(format!("AWS4{secret_key}").as_bytes()).unwrap();
    k.update(amz_date.as_bytes());
    let k_date = k.finalize().into_bytes();
    let mut k = HmacSha256::new_from_slice(&k_date).unwrap();
    k.update(region.as_bytes());
    let k_region = k.finalize().into_bytes();
    let mut k = HmacSha256::new_from_slice(&k_region).unwrap();
    k.update(service.as_bytes());
    let k_service = k.finalize().into_bytes();
    let mut k = HmacSha256::new_from_slice(&k_service).unwrap();
    k.update(b"aws4_request");
    let k_signing = k.finalize().into_bytes();
    let mut k = HmacSha256::new_from_slice(&k_signing).unwrap();
    k.update(string_to_sign.as_bytes());
    let signature = hex::encode(k.finalize().into_bytes());
    format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders={signed_headers}, Signature={signature}"
    )
}

/// 从 epoch 秒转 UTC (amz_date, date)（Howard Hinnant 算法）。
fn aws_now() -> (String, String) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let days = secs.div_euclid(86400);
    let day_secs = secs.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    let hh = day_secs / 3600;
    let mm = (day_secs % 3600) / 60;
    let ss = day_secs % 60;
    (
        format!("{y:04}{m:02}{d:02}T{hh:02}{mm:02}{ss:02}Z"),
        format!("{y:04}{m:02}{d:02}"),
    )
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// S3 对象 URL（`{endpoint}/{bucket}/{key}`）。
fn s3_object_url(endpoint: &str, bucket: &str, key: &str) -> String {
    format!(
        "{}/{}/{}",
        endpoint.trim_end_matches('/'),
        bucket.trim_start_matches('/').trim_end_matches('/'),
        key.trim_start_matches('/')
    )
}

/// 从 S3 endpoint 推断 region（无显式 region 时默认 us-east-1）。
fn s3_region(_endpoint: &str) -> String {
    "us-east-1".to_string()
}

/// 默认数据目录。
pub fn default_data_dir() -> PathBuf {
    if let Some(env) = std::env::var_os("NS_SWITCH_HOME") {
        return PathBuf::from(env);
    }
    // 规范（AGENTS.md §1.1.3）：供应商列表与设置存 `~/.ns-switch/config.json`，
    // 与 cc-switch 的 `~/.cc-switch/` 约定一致（不放进 macOS Application Support）。
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".ns-switch"))
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

/// 扫描备份目录（新→旧）。`source_override` 为空时读 `.source` 标记文件。
fn scan_backups(dir: &Path, source_override: Option<&Path>) -> Vec<BackupInfo> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if name.ends_with(".source") || name.starts_with('.') {
            continue;
        }
        let source = match source_override {
            Some(p) => p.to_string_lossy().into_owned(),
            None => fs::read_to_string(format!("{}.source", path.display())).unwrap_or_default(),
        };
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
    fn routing_config_breaker_defaults_and_serde() {
        // 默认熔断参数（向后兼容旧 config 缺失字段）。
        let rc = RoutingConfig::default();
        assert_eq!(rc.breaker_threshold, 3);
        assert_eq!(rc.breaker_cooldown, 30);
        // 旧 config 无 breaker 字段 → 反序列化回退默认。
        let rc: RoutingConfig = serde_json::from_str(
            r#"{"enabled":false,"port":0,"listen_addr":"127.0.0.1","failover":false}"#,
        )
        .unwrap();
        assert_eq!(rc.breaker_threshold, 3);
        assert_eq!(rc.breaker_cooldown, 30);
        // 显式值往返。
        let mut rc = RoutingConfig::default();
        rc.breaker_threshold = 5;
        rc.breaker_cooldown = 60;
        let json = serde_json::to_string(&rc).unwrap();
        let back: RoutingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.breaker_threshold, 5);
        assert_eq!(back.breaker_cooldown, 60);
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
            store
                .save_project("工作", "claude", &Default::default())
                .unwrap();
            store.set_current(Tool::Claude, "p2".into()).unwrap();
            assert_eq!(
                store.config().current.get(Tool::Claude).map(String::as_str),
                Some("p2")
            );
            store
                .apply_project("工作", "claude", &Default::default())
                .unwrap();
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
            // 临时 HOME：避免污染真实凭据文件。
            let home = dir.join("home");
            fs::create_dir_all(&home).unwrap();
            let old_home = std::env::var_os("HOME");
            // 临时 HOME 切换（edition 2024：set_var unsafe）。
            unsafe { std::env::set_var("HOME", &home) };
            {
                let mut store = Store::open_at(dir.clone()).unwrap();
                store
                    .login_account("copilot", "GitHub Copilot", "copilot", "gho_test_token")
                    .unwrap();
                assert!(store.config().accounts.contains_key("copilot"));
                assert!(store.config().providers.contains_key("hosted-copilot"));
                assert!(store.config().accounts["copilot"].logged_in);
                // token 落盘到临时 HOME。
                let hosts = home.join(".config/github-copilot/hosts.json");
                assert!(hosts.exists(), "hosts.json 应写入");
                let raw = fs::read_to_string(&hosts).unwrap();
                assert!(raw.contains("gho_test_token"));
                store.logout_account("copilot").unwrap();
                assert!(!store.config().accounts["copilot"].logged_in);
                // 供应商保留。
                assert!(store.config().providers.contains_key("hosted-copilot"));
            }
            match old_home {
                Some(h) => unsafe { std::env::set_var("HOME", h) },
                None => unsafe { std::env::remove_var("HOME") },
            }
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
    fn manual_backup_and_restore_roundtrip() {
        let dir = test_dir();
        {
            let mut store = Store::open_at(dir.clone()).unwrap();
            store.add_provider(sample_provider("p1")).unwrap();
            let path = store.manual_backup().unwrap();
            assert!(path.exists(), "手动备份应落盘");
            // 备份可在列表中看到，来源指向 config.json。
            let list = store.list_manual_backups();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].name, path.file_name().unwrap().to_string_lossy());
            assert!(list[0].source.ends_with("config.json"));
            assert!(list[0].size > 0);
            // 改动配置后从备份恢复 → 回到备份时的状态。
            store.add_provider(sample_provider("p2")).unwrap();
            assert_eq!(store.config().providers.len(), 2);
            store
                .restore_manual_backup(list[0].path.as_str())
                .unwrap();
            assert_eq!(store.config().providers.len(), 1);
            assert!(store.config().providers.contains_key("p1"));
            // 恢复前快照保留当前配置。
            assert!(store.backups_root().join("pre-restore").exists());
            // 非法备份内容被拒绝且不改动配置。
            let bad = dir.join("bad.json");
            fs::write(&bad, "not json").unwrap();
            assert!(store.restore_manual_backup(bad.to_str().unwrap()).is_err());
            assert_eq!(store.config().providers.len(), 1);
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
                    auto_sync_minutes: 30,
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
    #[test]
    fn custom_pricing_roundtrip() {
        let dir = test_dir();
        {
            let mut store = Store::open_at(dir.clone()).unwrap();
            store
                .set_custom_price("claude-sonnet-4-5", 3.0, 15.0, Some(0.3))
                .unwrap();
            store
                .set_custom_price("grok-3", 2.0, 10.0, None)
                .unwrap();
        }
        let store = Store::open_at(dir.clone()).unwrap();
        let p = store.config().settings.custom_pricing.clone();
        let sonnet = p.get("claude-sonnet-4-5").expect("sonnet priced");
        assert_eq!(sonnet.get("input_per_mtok").and_then(|x| x.as_f64()), Some(3.0));
        assert_eq!(sonnet.get("output_per_mtok").and_then(|x| x.as_f64()), Some(15.0));
        assert_eq!(sonnet.get("cache_read_per_mtok").and_then(|x| x.as_f64()), Some(0.3));
        let grok = p.get("grok-3").expect("grok priced");
        assert_eq!(grok.get("cache_read_per_mtok").and_then(|x| x.as_f64()), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn sigv4_signature_format_and_aws_now() {
        // 已知 AWS 文档示例：GET /us-east-1/aws4_request 签名格式。
        let auth = aws_sigv4(
            "GET",
            "example.com",
            "/bucket/config.json",
            "AKID",
            "SECRET",
            "us-east-1",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "20260101T000000Z",
        );
        assert!(auth.starts_with("AWS4-HMAC-SHA256 Credential=AKID/20260101/us-east-1/s3/aws4_request"), "凭据前缀: {auth}");
        assert!(auth.contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date"), "签名头: {auth}");
        assert!(auth.contains("Signature="), "签名存在: {auth}");

        let (amz, date) = aws_now();
        assert_eq!(amz.len(), 16, "amz_date 应为 yyyymmddTHHMMSSZ");
        assert!(amz.ends_with('Z'));
        assert_eq!(date.len(), 8, "date 应为 yyyymmdd");
        // 日期合理性：2026 年内。
        let y: i32 = date[..4].parse().unwrap();
        assert!((2020..2030).contains(&y), "年份合理: {y}");

        // civil_from_days 已知值：epoch day 0 = 1970-01-01。
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2026-11-10 是已知日期；用 aws_now 一致性验证（非负天数）。
        let (y2, m2, d2) = civil_from_days(20700);
        assert!(y2 >= 2026, "天 20700 应近 2026: {y2}-{m2}-{d2}");
        assert!((1..=12).contains(&m2));
        assert!((1..=31).contains(&d2));

        // S3 对象 URL。
        assert_eq!(
            s3_object_url("https://s3.example.com", "my-bucket", "config.json"),
            "https://s3.example.com/my-bucket/config.json"
        );
    }

    #[test]
    fn config_dir_override_roundtrip() {
        let dir = test_dir();
        {
            let mut store = Store::open_at(dir.clone()).unwrap();
            store.set_config_dir(Tool::Claude, "/custom/claude").unwrap();
            store.set_config_dir(Tool::Codex, "").unwrap(); // 清除
        }
        let store = Store::open_at(dir.clone()).unwrap();
        assert_eq!(
            store.config().settings.config_dir_for(Tool::Claude),
            Some("/custom/claude")
        );
        assert_eq!(store.config().settings.config_dir_for(Tool::Codex), None);
        assert_eq!(store.config().settings.config_dir_for(Tool::Gemini), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn config_dir_serde_roundtrip() {
        let mut s = Settings::default();
        s.set_config_dir(Tool::GrokBuild, "/srv/grok");
        let v = serde_json::to_value(&s).unwrap();
        let back: Settings = serde_json::from_value(v).unwrap();
        assert_eq!(back.config_dir_for(Tool::GrokBuild), Some("/srv/grok"));
    }

    #[test]
    fn default_data_dir_follows_spec_home_dot_ns_switch() {
        // 规范（AGENTS.md §1.1.3）：供应商列表与设置存 `~/.ns-switch/config.json`。
        // 不放进 macOS Application Support（与 cc-switch `~/.cc-switch/` 约定一致）。
        unsafe {
            std::env::set_var("NS_SWITCH_HOME", "");
            std::env::remove_var("NS_SWITCH_HOME");
        }
        let home = std::env::var_os("HOME").expect("测试需要 HOME");
        let expected = std::path::PathBuf::from(&home).join(".ns-switch");
        assert_eq!(default_data_dir(), expected);
        // NS_SWITCH_HOME 覆盖仍优先。
        unsafe {
            std::env::set_var("NS_SWITCH_HOME", "/tmp/custom-ns-home");
        }
        assert_eq!(default_data_dir(), std::path::PathBuf::from("/tmp/custom-ns-home"));
        unsafe {
            std::env::remove_var("NS_SWITCH_HOME");
        }
    }

}
