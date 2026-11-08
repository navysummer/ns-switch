//! 供应商行 → 目标值投影：把 `settings_config` 转成对应工具的补丁。
//!
//! 纯函数：不读写文件。每个工具一个投影器，返回 `JsonPatch` 或 `TomlPatch`。

use serde_json::Value;

use super::floor;
use super::json::{ClearScope, JsonPatch};
use super::patch::KeyPath;
use super::toml::{TomlPatch, item_from_json};
use super::dotenv::EnvPatch;
use super::yaml::{YamlListUpsert, YamlPatch};



/// Claude Code：`~/.claude/settings.json`。
///
/// 地板 = `env` 内 `ANTHROPIC_*`/`AWS_*`/`VERTEX_REGION_*` 前缀 + 命名键 +
/// `CLAUDE_CODE_USE_*` 协议选择器 + `CLAUDE_CODE_SKIP_*_AUTH` + 顶层 `apiKeyHelper`。
pub fn claude_patch(settings_config: &Value) -> JsonPatch {
    let mut patch = JsonPatch {
        clear: vec![ClearScope {
            parent: KeyPath::from("env"),
            is_floor: floor::claude_floor_env,
        }],
        ..Default::default()
    };

    // env 关键字段
    if let Some(env) = settings_config.get("env").and_then(Value::as_object) {
        for (key, value) in env {
            if floor::claude_floor_env(key) {
                patch.set.push((KeyPath::from("env").child(key), value.clone()));
            }
        }
    }

    // 顶层 apiKeyHelper（键名）
    if let Some(helper) = settings_config.get("apiKeyHelper").and_then(Value::as_str) {
        patch.set.push((KeyPath::from("apiKeyHelper"), Value::String(helper.into())));
    }

    patch
}

/// Codex：`~/.codex/config.toml`。
///
/// 地板 = 顶层 `model` + `model_provider` + `model_catalog_json` + `[model_providers.*]` 表。
pub fn codex_patch(settings_config: &Value) -> TomlPatch {
    let mut patch = TomlPatch::default();

    // config 对象内的关键字段
    if let Some(config) = settings_config.get("config").and_then(Value::as_object) {
        for (key, value) in config {
            if codex_floor(key) {
                let item = item_from_json(value);
                match key.as_str() {
                    "model_providers" => {
                        // 整表替换：清掉旧的供应商表，写入目标表
                        patch.remove.push(KeyPath::from("model_providers"));
                        patch.set.push((KeyPath::from("model_providers"), item));
                    }
                    _ => patch.set.push((KeyPath::from(key.as_str()), item)),
                }
            }
        }
    }

    // auth 关键字段
    if let Some(auth) = settings_config.get("auth").and_then(Value::as_object) {
        for (key, value) in auth {
            if codex_auth_floor(key) {
                patch.set.push((KeyPath::from("auth").child(key), item_from_json(value)));
            }
        }
    }

    patch
}

fn codex_floor(key: &str) -> bool {
    matches!(
        key,
        "model" | "model_provider" | "model_catalog_json" | "model_providers"
    )
}

fn codex_auth_floor(key: &str) -> bool {
    matches!(
        key,
        "OPENAI_API_KEY" | "OPENAI_BASE_URL" | "ANTHROPIC_API_KEY" | "ANTHROPIC_BASE_URL"
    )
}

/// Gemini：`~/.gemini/settings.json` + `~/.gemini/.env`。
///
/// 地板 = env 内 `GEMINI_API_KEY`/`GEMINI_BASE_URL`/`GEMINI_MODEL` 等。
pub fn gemini_settings_patch(settings_config: &Value) -> JsonPatch {
    let mut patch = JsonPatch::default();
    if let Some(env) = settings_config.get("env").and_then(Value::as_object) {
        for (key, value) in env {
            if gemini_floor(key) {
                patch.set.push((KeyPath::from("env").child(key), value.clone()));
            }
        }
    }
    patch
}

fn gemini_floor(key: &str) -> bool {
    matches!(
        key,
        "GEMINI_API_KEY"
            | "GEMINI_BASE_URL"
            | "GEMINI_MODEL"
            | "GOOGLE_API_KEY"
            | "GEMINI_SMALL_FAST_MODEL"
    )
}

/// Gemini `.env` 的键（从 settings_config 提取）。
pub fn gemini_env_entries(settings_config: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(env) = settings_config.get("env").and_then(Value::as_object) {
        for (key, value) in env {
            if gemini_floor(key) {
                out.push((key.clone(), super::dotenv::env_value(value)));
            }
        }
    }
    out
}

// ===========================================================================
// 共存式工具（additive）：把供应商 upsert 进工具自身配置，工具内再选。
// 补丁只动该供应商的条目，不删其他供应商、不碰用户自有字段。
// ===========================================================================

/// 从 settings_config 提取 `{baseUrl, apiKey, model}`（json_tool 生成的统一形状）。
fn json_tool_fields(cfg: &Value) -> (String, String, String) {
    let s = |k: &str| {
        cfg.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    (s("baseUrl"), s("apiKey"), s("model"))
}

/// OpenCode：`provider.<id>`（V1 声明）写供应商；顶层 `model` 写 `<id>/<model>`。
pub fn opencode_patch(id: &str, settings_config: &Value) -> JsonPatch {
    let (base_url, api_key, model) = json_tool_fields(settings_config);
    let mut patch = JsonPatch::default();
    // V1 声明：SDK 包 + options + 模型目录。
    let mut models = serde_json::Map::new();
    if !model.is_empty() {
        models.insert(
            model.clone(),
            serde_json::json!({ "name": model, "reasoning": true }),
        );
    }
    patch.set.push((
        KeyPath::from("provider").child(id),
        serde_json::json!({
            "npm": "@ai-sdk/openai-compatible",
            "name": model,
            "options": {
                "baseURL": base_url,
                "apiKey": api_key,
                "setCacheKey": true,
            },
            "models": models,
        }),
    ));
    if !model.is_empty() {
        patch.set.push((
            KeyPath::from("model"),
            Value::String(format!("{id}/{model}")),
        ));
    }
    patch
}

/// OpenClaw：`models.providers.<id>` 写供应商（`models.mode` 保持 merge）。
pub fn openclaw_patch(id: &str, settings_config: &Value) -> JsonPatch {
    let (base_url, api_key, model) = json_tool_fields(settings_config);
    let mut patch = JsonPatch::default();
    patch.set.push((
        KeyPath::from("models").child("providers").child(id),
        serde_json::json!({
            "baseUrl": base_url,
            "apiKey": api_key,
            "api": "openai-completions",
            "models": [{
                "id": model,
                "name": model,
                "contextWindow": 200000,
            }],
        }),
    ));
    patch
}

/// Pi：`models.json` 顶层 `<id>` 写供应商（`name`/`baseUrl`/`api`/`apiKey`/`models`）。
pub fn pi_patch(id: &str, settings_config: &Value) -> JsonPatch {
    let (base_url, api_key, model) = json_tool_fields(settings_config);
    let mut patch = JsonPatch::default();
    patch.set.push((
        KeyPath::from(id),
        serde_json::json!({
            "name": model,
            "baseUrl": base_url,
            "api": "openai-completions",
            "apiKey": api_key,
            "models": [{
                "id": model,
                "name": model,
                "contextWindow": 200000,
            }],
        }),
    ));
    patch
}

/// Hermes：`custom_providers` 列表按 `name` upsert（`base_url`/`api_key`/`model`）。
pub fn hermes_patch(id: &str, settings_config: &Value) -> YamlPatch {
    let (base_url, api_key, model) = json_tool_fields(settings_config);
    let mut patch = YamlPatch::default();
    patch.list_upsert.push(YamlListUpsert {
        parent: KeyPath::from("custom_providers"),
        match_field: "name",
        match_value: id.to_string(),
        entry: serde_yaml::Value::Mapping(serde_yaml::Mapping::from_iter([
            (
                serde_yaml::Value::String("name".into()),
                serde_yaml::Value::String(id.to_string()),
            ),
            (
                serde_yaml::Value::String("base_url".into()),
                serde_yaml::Value::String(base_url),
            ),
            (
                serde_yaml::Value::String("api_key".into()),
                serde_yaml::Value::String(api_key),
            ),
            (
                serde_yaml::Value::String("model".into()),
                serde_yaml::Value::String(model),
            ),
        ])),
    });
    patch
}

/// MCode：`providers` 对象 `<id>` 写供应商（`name`/`options`/`models`）。
pub fn mcode_patch(id: &str, settings_config: &Value) -> YamlPatch {
    let (base_url, api_key, model) = json_tool_fields(settings_config);
    let mut patch = YamlPatch::default();
    patch.set.push((
        KeyPath::from("providers").child(id),
        serde_yaml::Value::Mapping(serde_yaml::Mapping::from_iter([
            (
                serde_yaml::Value::String("name".into()),
                serde_yaml::Value::String(model.clone()),
            ),
            (
                serde_yaml::Value::String("options".into()),
                serde_yaml::Value::Mapping(serde_yaml::Mapping::from_iter([
                    (
                        serde_yaml::Value::String("baseURL".into()),
                        serde_yaml::Value::String(base_url),
                    ),
                    (
                        serde_yaml::Value::String("apiKey".into()),
                        serde_yaml::Value::String(api_key),
                    ),
                    (
                        serde_yaml::Value::String("setCacheKey".into()),
                        serde_yaml::Value::Bool(true),
                    ),
                ])),
            ),
            (
                serde_yaml::Value::String("models".into()),
                serde_yaml::Value::Sequence(vec![serde_yaml::Value::Mapping(
                    serde_yaml::Mapping::from_iter([
                        (
                            serde_yaml::Value::String("id".into()),
                            serde_yaml::Value::String(model.clone()),
                        ),
                        (
                            serde_yaml::Value::String("name".into()),
                            serde_yaml::Value::String(model),
                        ),
                    ]),
                )]),
            ),
        ])),
    ));
    patch
}

/// Kimi：`providers.<id>`（`provider` 内 `base_url`/`api_key`）+ `models.<model>`。
pub fn kimi_patch(id: &str, settings_config: &Value) -> TomlPatch {
    let (base_url, api_key, model) = json_tool_fields(settings_config);
    let mut patch = TomlPatch::default();
    patch.set.push((
        KeyPath::from("providers").child(id),
        item_from_json(&serde_json::json!({
            "provider": {
                "type": "openai",
                "base_url": base_url,
                "api_key": api_key,
            }
        })),
    ));
    if !model.is_empty() {
        patch.set.push((
            KeyPath::from("models").child(&model),
            item_from_json(&serde_json::json!({
                "provider": id,
            })),
        ));
    }
    patch
}

/// OMP：`providers` 对象 `<id>` 写供应商（`name`/`baseUrl`/`api`/`apiKey`/`models`）。
pub fn omp_patch(id: &str, settings_config: &Value) -> YamlPatch {
    let (base_url, api_key, model) = json_tool_fields(settings_config);
    let mut patch = YamlPatch::default();
    patch.set.push((
        KeyPath::from("providers").child(id),
        serde_yaml::Value::Mapping(serde_yaml::Mapping::from_iter([
            (
                serde_yaml::Value::String("name".into()),
                serde_yaml::Value::String(model.clone()),
            ),
            (
                serde_yaml::Value::String("baseUrl".into()),
                serde_yaml::Value::String(base_url),
            ),
            (
                serde_yaml::Value::String("api".into()),
                serde_yaml::Value::String("openai-completions".into()),
            ),
            (
                serde_yaml::Value::String("apiKey".into()),
                serde_yaml::Value::String(api_key),
            ),
            (
                serde_yaml::Value::String("models".into()),
                serde_yaml::Value::Sequence(vec![serde_yaml::Value::Mapping(
                    serde_yaml::Mapping::from_iter([
                        (
                            serde_yaml::Value::String("id".into()),
                            serde_yaml::Value::String(model.clone()),
                        ),
                        (
                            serde_yaml::Value::String("name".into()),
                            serde_yaml::Value::String(model),
                        ),
                    ]),
                )]),
            ),
        ])),
    ));
    patch
}

/// 本地代理监听地址（路由模式写入 live 配置的 base URL）。
pub const ROUTING_HOST: &str = "127.0.0.1";

/// 路由模式写入的占位 key（真实 key 存在 `config.json`，不进 live 文件）。
pub const ROUTING_PLACEHOLDER_KEY: &str = "ns-switch-routing";

/// 路由模式下 Claude Code `settings.json` 补丁：
/// 把 `env.ANTHROPIC_BASE_URL` 指向本地代理，key 用占位值。
pub fn claude_routing_patch(port: u16) -> JsonPatch {
    let base = format!("http://{ROUTING_HOST}:{port}");
    JsonPatch {
        clear: vec![ClearScope {
            parent: KeyPath::from("env"),
            is_floor: floor::claude_floor_env,
        }],
        set: vec![
            (
                KeyPath::from("env").child("ANTHROPIC_BASE_URL"),
                Value::String(base),
            ),
            (
                KeyPath::from("env").child("ANTHROPIC_AUTH_TOKEN"),
                Value::String(ROUTING_PLACEHOLDER_KEY.into()),
            ),
        ],
        ..Default::default()
    }
}

/// 路由模式下 Codex `config.toml` 补丁：
/// `model_providers.ns-switch` 指向本地代理，`model_provider` 选它，
/// `auth.OPENAI_API_KEY` 用占位值。
pub fn codex_routing_patch(port: u16) -> TomlPatch {
    let base = format!("http://{ROUTING_HOST}:{port}");
    let providers = serde_json::json!({
        "ns-switch": {
            "name": "ns-switch 本地路由",
            "base_url": base,
            "wire_api": "chat",
        }
    });
    let mut patch = TomlPatch::default();
    patch.remove.push(KeyPath::from("model_providers"));
    patch.set.push((
        KeyPath::from("model_providers"),
        item_from_json(&providers),
    ));
    patch.set.push((
        KeyPath::from("model_provider"),
        item_from_json(&serde_json::json!("ns-switch")),
    ));
    patch.set.push((
        KeyPath::from("auth").child("OPENAI_API_KEY"),
        item_from_json(&serde_json::json!(ROUTING_PLACEHOLDER_KEY)),
    ));
    patch
}

/// 路由模式下 Gemini `settings.json` 补丁。
pub fn gemini_routing_settings_patch(port: u16) -> JsonPatch {
    let base = format!("http://{ROUTING_HOST}:{port}");
    JsonPatch {
        set: vec![
            (
                KeyPath::from("env").child("GEMINI_BASE_URL"),
                Value::String(base),
            ),
            (
                KeyPath::from("env").child("GEMINI_API_KEY"),
                Value::String(ROUTING_PLACEHOLDER_KEY.into()),
            ),
        ],
        ..Default::default()
    }
}

/// 路由模式下 Gemini `.env` 补丁。
pub fn gemini_routing_env_patch(port: u16) -> EnvPatch {
    let base = format!("http://{ROUTING_HOST}:{port}");
    EnvPatch {
        remove: vec![],
        set: vec![
            ("GEMINI_BASE_URL".into(), base),
            ("GEMINI_API_KEY".into(), ROUTING_PLACEHOLDER_KEY.into()),
        ],
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_routing_points_at_localhost() {
        let patch = claude_routing_patch(8899);
        let mut doc = json!({"env": {"ANTHROPIC_BASE_URL": "https://old", "PATH": "/usr/bin"}});
        patch.apply_to(std::path::Path::new("x"), &mut doc).unwrap();
        assert_eq!(doc["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:8899");
        assert_eq!(doc["env"]["ANTHROPIC_AUTH_TOKEN"], "ns-switch-routing");
        // 非地板键保留
        assert_eq!(doc["env"]["PATH"], "/usr/bin");
    }

    #[test]
    fn codex_routing_selects_ns_switch_provider() {
        let patch = codex_routing_patch(8900);
        let mut doc = crate::toml::parse(
            std::path::Path::new("x"),
            Some(b"model = \"gpt-4o\"\nmodel_provider = \"openai\"\n[auth]\nOPENAI_API_KEY = \"k\"\n"),
        )
        .unwrap()
        .0;
        patch.apply_to(std::path::Path::new("x"), &mut doc).unwrap();
        let text = doc.to_string();
        assert!(text.contains("model_provider = \"ns-switch\""));
        assert!(text.contains("base_url = \"http://127.0.0.1:8900\""));
        assert!(text.contains("OPENAI_API_KEY = \"ns-switch-routing\""));
        assert!(text.contains("[model_providers.ns-switch]"));
    }

    #[test]
    fn gemini_routing_sets_env_and_settings() {
        let patch = gemini_routing_settings_patch(8901);
        let mut doc = json!({"env": {"GEMINI_BASE_URL": "https://old", "GEMINI_API_KEY": "k"}});
        patch.apply_to(std::path::Path::new("x"), &mut doc).unwrap();
        assert_eq!(doc["env"]["GEMINI_BASE_URL"], "http://127.0.0.1:8901");
        assert_eq!(doc["env"]["GEMINI_API_KEY"], "ns-switch-routing");

        let env = gemini_routing_env_patch(8901);
        assert_eq!(env.set[0], ("GEMINI_BASE_URL".into(), "http://127.0.0.1:8901".into()));
        assert_eq!(env.set[1].0, "GEMINI_API_KEY");
        assert_eq!(env.set[1].1, "ns-switch-routing");
    }
}
