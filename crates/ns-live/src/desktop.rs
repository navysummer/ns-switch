//! Claude Desktop（3P profile）配置投影。
//!
//! Desktop 的第三方供应商不是写在 `claude_desktop_config.json` 里，而是：
//! - `Claude/claude_desktop_config.json` + `Claude-3p/claude_desktop_config.json`
//!   的 `deploymentMode` 切到 `3p`；
//! - `Claude-3p/configLibrary/<PROFILE_ID>.json` 写网关 profile（端点/密钥/模型）；
//! - `Claude-3p/configLibrary/_meta.json` 的 `entries[]` 登记 profile，并设 `appliedId`。
//!
//! 切回官方时反向操作：两个配置文件 `deploymentMode` 回到 `1p`，profile 里
//! 只清关键字段（保留用户在该 profile 里的其它设置），并从 `_meta.json` 摘除条目。
//!
//! 与 cc-switch `claude_desktop_config.rs` 的直连路径语义一致；模型映射（非
//! `claude-*` 上游名）需要本地路由，这里直连只写 Claude 安全模型名或空列表。

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::json::{self as patch_json, ClearScope, JsonPatch};
use super::jsonc;
use super::patch::{KeyPath, LivePatch, LiveWriteError};
use super::switcher::LiveFile;

/// ns-switch 在 Desktop 里登记的 profile id（固定，切回官方时按它摘除）。
pub const PROFILE_ID: &str = "00000000-0000-4000-8000-000000157210";
/// profile 在 Desktop 菜单里的显示名。
pub const PROFILE_NAME: &str = "ns-switch";
/// 配置文件里的 3P 部署模式。
pub const DEPLOYMENT_3P: &str = "3p";
/// 官方部署模式。
pub const DEPLOYMENT_1P: &str = "1p";

/// Desktop 相关文件路径。
#[derive(Debug, Clone)]
pub struct DesktopPaths {
    /// 普通 Claude 数据目录下的 `claude_desktop_config.json`。
    pub normal_config: PathBuf,
    /// 3P 数据目录下的 `claude_desktop_config.json`。
    pub threep_config: PathBuf,
    /// 网关 profile（`configLibrary/<PROFILE_ID>.json`）。
    pub profile: PathBuf,
    /// `configLibrary/_meta.json`。
    pub meta: PathBuf,
}

/// 按「应用数据根目录」构造路径（macOS 为 `~/Library/Application Support`）。
pub fn desktop_paths_in(app_support: &Path) -> DesktopPaths {
    let normal = app_support.join("Claude");
    let threep = app_support.join("Claude-3p");
    let library = threep.join("configLibrary");
    DesktopPaths {
        normal_config: normal.join("claude_desktop_config.json"),
        threep_config: threep.join("claude_desktop_config.json"),
        profile: library.join(format!("{PROFILE_ID}.json")),
        meta: library.join("_meta.json"),
    }
}

/// 平台默认路径（macOS / Windows / Linux）。
pub fn desktop_paths(home: &Path) -> DesktopPaths {
    #[cfg(target_os = "macos")]
    {
        desktop_paths_in(&home.join("Library").join("Application Support"))
    }
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Local"));
        desktop_paths_in(&base)
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        desktop_paths_in(&base)
    }
}

/// 从工具的配置目录（`Claude-3p`）推导路径；父目录即应用数据根。
pub fn desktop_paths_from_tool_dir(tool_dir: &Path) -> DesktopPaths {
    match tool_dir.parent() {
        Some(base) if !base.as_os_str().is_empty() => desktop_paths_in(base),
        _ => desktop_paths_in(tool_dir),
    }
}

/// 桌面 profile 的关键字段（切走/切回官方时清空）。
pub const DESKTOP_PROFILE_FLOOR: &[&str] = &[
    "coworkEgressAllowedHosts",
    "disableDeploymentModeChooser",
    "inferenceGatewayApiKey",
    "inferenceGatewayAuthScheme",
    "inferenceGatewayBaseUrl",
    "inferenceProvider",
    "inferenceModels",
    "modelDiscoveryEnabled",
];

/// 旧版写进 `claude_desktop_config.json` 的 `enterpriseConfig` 网关键。
pub const LEGACY_ENTERPRISE_KEYS: &[&str] = &[
    "disableDeploymentModeChooser",
    "inferenceGatewayApiKey",
    "inferenceGatewayAuthScheme",
    "inferenceGatewayBaseUrl",
    "inferenceProvider",
];

/// profile 键是否是关键字段。
pub fn desktop_profile_floor(key: &str) -> bool {
    DESKTOP_PROFILE_FLOOR.contains(&key)
}

/// `deploymentMode` 补丁（3p 第三方 / 1p 官方）。
///
/// 官方模式额外清掉旧版留在 `enterpriseConfig` 里的网关键。
pub fn deployment_patch(mode: &str) -> JsonPatch {
    let mut patch = JsonPatch {
        set: vec![(KeyPath::from("deploymentMode"), json!(mode))],
        ..Default::default()
    };
    if mode == DEPLOYMENT_1P {
        patch.remove = LEGACY_ENTERPRISE_KEYS
            .iter()
            .map(|key| KeyPath(vec!["enterpriseConfig".to_string(), key.to_string()]))
            .collect();
    }
    patch
}

/// 从供应商 `settings_config` 提取桌面直连凭据（端点 + Bearer key）。
pub fn desktop_credentials(settings_config: &Value) -> Result<(String, String), String> {
    let env = settings_config
        .get("env")
        .and_then(Value::as_object)
        .ok_or_else(|| "Claude Desktop 供应商缺少 env 配置".to_string())?;
    let base_url = env
        .get("ANTHROPIC_BASE_URL")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Claude Desktop 供应商缺少 ANTHROPIC_BASE_URL".to_string())?
        .to_string();
    let key = env
        .get("ANTHROPIC_AUTH_TOKEN")
        .or_else(|| env.get("ANTHROPIC_API_KEY"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Claude Desktop 供应商缺少 ANTHROPIC_AUTH_TOKEN".to_string())?
        .to_string();
    Ok((base_url, key))
}

/// 是否是 Desktop 可直连的 Claude 安全模型名。
pub fn is_claude_safe_model(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    let tail = n
        .strip_prefix("anthropic/claude-")
        .or_else(|| n.strip_prefix("claude-"));
    match tail {
        Some(t) => ["sonnet-", "opus-", "haiku-", "fable-"]
            .iter()
            .any(|p| t.strip_prefix(p).is_some_and(|rest| !rest.trim().is_empty())),
        None => false,
    }
}

/// 网关 profile 补丁：写关键字段 + 可选模型列表。
///
/// `models` 里只有 `claude-*` 安全名会进 `inferenceModels`；其它（Kimi 等非
/// Claude 品牌）留给 Desktop 的模型发现，直连模式不做模型映射。
pub fn profile_patch(settings_config: &Value, models: &[String]) -> Result<JsonPatch, String> {
    let (base_url, api_key) = desktop_credentials(settings_config)?;
    let safe: Vec<String> = models
        .iter()
        .map(|m| m.trim().to_string())
        .filter(|m| is_claude_safe_model(m))
        .collect();
    let mut patch = JsonPatch {
        clear: vec![ClearScope {
            parent: KeyPath::root(),
            is_floor: desktop_profile_floor,
        }],
        ..Default::default()
    };
    patch
        .set
        .push((KeyPath::from("coworkEgressAllowedHosts"), json!(["*"])));
    patch.set.push((
        KeyPath::from("disableDeploymentModeChooser"),
        json!(true),
    ));
    patch.set.push((KeyPath::from("inferenceGatewayApiKey"), json!(api_key)));
    patch.set.push((
        KeyPath::from("inferenceGatewayAuthScheme"),
        json!("bearer"),
    ));
    patch.set.push((KeyPath::from("inferenceGatewayBaseUrl"), json!(base_url)));
    patch.set.push((KeyPath::from("inferenceProvider"), json!("gateway")));
    if !safe.is_empty() {
        patch.set.push((
            KeyPath::from("inferenceModels"),
            Value::Array(safe.into_iter().map(Value::String).collect()),
        ));
    }
    Ok(patch)
}

/// 切回官方：只清 profile 关键字段，保留用户在 profile 里的其它设置。
pub fn official_profile_patch() -> JsonPatch {
    JsonPatch {
        clear: vec![ClearScope {
            parent: KeyPath::root(),
            is_floor: desktop_profile_floor,
        }],
        ..Default::default()
    }
}

/// `_meta.json` 的 entries/appliedId 补丁（数组操作，`JsonPatch` 做不到）。
#[derive(Debug, Clone)]
pub struct DesktopMetaPatch {
    /// true = 登记并应用 ns-switch profile；false = 摘除并回退 appliedId。
    pub applied: bool,
}

impl LivePatch for DesktopMetaPatch {
    fn apply(&self, path: &Path, pre: Option<&[u8]>) -> Result<Vec<u8>, LiveWriteError> {
        let (mut doc, style) = patch_json::parse(path, pre)?;
        let obj = doc.as_object_mut().ok_or_else(|| LiveWriteError::Shape {
            path: path.to_path_buf(),
            key_path: KeyPath::root(),
            expected: "对象",
        })?;
        let is_ours = |entry: &Value| {
            entry.get("id").and_then(Value::as_str) == Some(PROFILE_ID)
        };
        if self.applied {
            let entries = obj
                .entry("entries")
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
                .ok_or_else(|| LiveWriteError::Shape {
                    path: path.to_path_buf(),
                    key_path: KeyPath::from("entries"),
                    expected: "数组",
                })?;
            match entries
                .iter_mut()
                .find(|entry| is_ours(entry))
                .and_then(|entry| entry.as_object_mut())
            {
                Some(entry) => {
                    entry.insert("name".into(), json!(PROFILE_NAME));
                }
                None => entries.push(json!({ "id": PROFILE_ID, "name": PROFILE_NAME })),
            }
            obj.insert("appliedId".into(), json!(PROFILE_ID));
        } else {
            let mut next_id: Option<String> = None;
            if let Some(entries) = obj.get_mut("entries") {
                let entries = entries.as_array_mut().ok_or_else(|| LiveWriteError::Shape {
                    path: path.to_path_buf(),
                    key_path: KeyPath::from("entries"),
                    expected: "数组",
                })?;
                entries.retain(|entry| !is_ours(entry));
                next_id = entries
                    .iter()
                    .find_map(|entry| entry.get("id").and_then(Value::as_str))
                    .map(str::to_string);
            }
            if obj.get("appliedId").and_then(Value::as_str) == Some(PROFILE_ID) {
                match next_id {
                    Some(next) => {
                        obj.insert("appliedId".into(), json!(next));
                    }
                    None => {
                        obj.shift_remove("appliedId");
                    }
                }
            }
        }
        Ok(jsonc::serialize_simple(&doc, &style))
    }
}

/// 构造 Desktop 的 live 文件计划（不含官方判定）：`deployment` × 2 + profile + meta。
pub fn desktop_plan_files(paths: &DesktopPaths) -> [LiveFile; 4] {
    [
        LiveFile::new("claude-desktop", paths.normal_config.clone(), false),
        LiveFile::new("claude-desktop", paths.threep_config.clone(), false),
        LiveFile::new("claude-desktop", paths.profile.clone(), true),
        LiveFile::new("claude-desktop", paths.meta.clone(), false),
    ]
}

/// 判断 `_meta.json` 里当前应用的是不是 ns-switch profile（冲突检测用）。
pub fn meta_applied_id(text: &str) -> Option<String> {
    serde_json_lenient::from_str::<Value>(text)
        .ok()
        .and_then(|v| v.get("appliedId").and_then(Value::as_str).map(str::to_string))
}

/// 判断 profile 是否已含网关配置（冲突检测用）。
pub fn profile_is_gateway(value: &Value) -> bool {
    value.get("inferenceProvider").and_then(Value::as_str) == Some("gateway")
}

/// 读取 profile JSON（不存在/非法返回空对象）。
pub fn read_profile(path: &Path) -> Value {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json_lenient::from_str(&text).unwrap_or_else(|_| Value::Object(Map::new())),
        Err(_) => Value::Object(Map::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ns-live-desktop-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cfg() -> Value {
        json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.example.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-desktop",
            }
        })
    }

    #[test]
    fn profile_patch_writes_gateway_keys() {
        let patch = profile_patch(&cfg(), &[]).unwrap();
        let mut doc = json!({ "existing": 1, "inferenceProvider": "official" });
        patch.apply_to(Path::new("/tmp/x"), &mut doc).unwrap();
        assert_eq!(doc["inferenceProvider"], "gateway");
        assert_eq!(doc["inferenceGatewayBaseUrl"], "https://api.example.com");
        assert_eq!(doc["inferenceGatewayApiKey"], "sk-desktop");
        assert_eq!(doc["existing"], 1, "非关键字段保留");
        assert!(doc.get("inferenceModels").is_none());
    }

    #[test]
    fn profile_patch_filters_unsafe_models() {
        let models = vec![
            "claude-sonnet-5".to_string(),
            "kimi-k2.7-code".to_string(),
            "anthropic/claude-opus-5".to_string(),
        ];
        let patch = profile_patch(&cfg(), &models).unwrap();
        let mut doc = json!({});
        patch.apply_to(Path::new("/tmp/x"), &mut doc).unwrap();
        assert_eq!(doc["inferenceModels"], json!(["claude-sonnet-5", "anthropic/claude-opus-5"]));
    }

    #[test]
    fn profile_patch_rejects_missing_credentials() {
        assert!(profile_patch(&json!({}), &[]).is_err());
        assert!(profile_patch(&json!({ "env": { "ANTHROPIC_BASE_URL": "https://x" } }), &[]).is_err());
    }

    #[test]
    fn safe_model_names() {
        assert!(is_claude_safe_model("claude-sonnet-5"));
        assert!(is_claude_safe_model("anthropic/claude-haiku-4-5"));
        assert!(!is_claude_safe_model("claude-sonnet-"));
        assert!(!is_claude_safe_model("kimi-k2.7-code"));
    }

    #[test]
    fn meta_patch_registers_and_removes_profile() {
        let dir = tmp("meta");
        let path = dir.join("_meta.json");
        fs::write(
            &path,
            br#"{"entries":[{"id":"other","name":"Other"}],"appliedId":"other"}"#,
        )
        .unwrap();
        let apply = DesktopMetaPatch { applied: true };
        let out = apply.apply(&path, Some(&fs::read(&path).unwrap())).unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["appliedId"], PROFILE_ID);
        assert_eq!(v["entries"].as_array().unwrap().len(), 2);

        let remove = DesktopMetaPatch { applied: false };
        let out = remove.apply(&path, Some(&out)).unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["appliedId"], "other");
        assert_eq!(v["entries"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn meta_patch_removes_applied_id_when_empty() {
        let dir = tmp("meta-empty");
        let path = dir.join("_meta.json");
        fs::write(&path, br#"{"entries":[{"id":"other","name":"Other"}]}"#).unwrap();
        let apply = DesktopMetaPatch { applied: true };
        let out = apply.apply(&path, Some(&fs::read(&path).unwrap())).unwrap();
        let remove = DesktopMetaPatch { applied: false };
        let out = remove.apply(&path, Some(&out)).unwrap();
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["appliedId"], "other");
    }

    #[test]
    fn deployment_mode_patch_clears_legacy_enterprise_keys() {
        let mut doc = json!({
            "deploymentMode": "3p",
            "enterpriseConfig": {
                "inferenceGatewayBaseUrl": "https://old",
                "keepMe": true,
            }
        });
        deployment_patch(DEPLOYMENT_1P)
            .apply_to(Path::new("/tmp/x"), &mut doc)
            .unwrap();
        assert_eq!(doc["deploymentMode"], "1p");
        assert!(doc["enterpriseConfig"].get("inferenceGatewayBaseUrl").is_none());
        assert_eq!(doc["enterpriseConfig"]["keepMe"], true);
    }
}
