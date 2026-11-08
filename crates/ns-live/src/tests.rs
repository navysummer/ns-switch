//! M2 切换引擎测试：地板匹配、投影、补丁保留注释/键序、解析失败即停、.env 保序。

use std::path::Path;

use serde_json::json;

use crate::{
    JsonPatch, KeyPath, LivePatch, LiveWriteError, TomlPatch, claude_patch,
    codex_patch, gemini_settings_patch, json::JsonStyle,
};

#[test]
fn json_patch_preserves_key_order_and_comments() {
    // 模拟 Claude settings.json：用户键 + 注释 + 关键字段
    let before = r#"{
  // 用户注释（保留）
  "apiKeyHelper": "script.sh",
  "env": {
    "ANTHROPIC_BASE_URL": "https://old.example.com",
    "MY_CUSTOM_VAR": "keep-me",
    "ANTHROPIC_MODEL": "claude-old"
  }
}"#;
    let patch = JsonPatch {
        clear: vec![crate::ClearScope {
            parent: KeyPath::from("env"),
            is_floor: crate::claude_floor_env,
        }],
        set: vec![
            (
                KeyPath::from("env").child("ANTHROPIC_BASE_URL"),
                json!("https://new.example.com"),
            ),
            (
                KeyPath::from("env").child("ANTHROPIC_AUTH_TOKEN"),
                json!("sk-new"),
            ),
            (
                KeyPath::from("env").child("ANTHROPIC_MODEL"),
                json!("claude-sonnet-4-5"),
            ),
        ],
        ..Default::default()
    };
    let out = patch.apply(Path::new("settings.json"), Some(before.as_bytes())).unwrap();
    let text = String::from_utf8(out).unwrap();

    // 关键字段被更新
    assert!(text.contains("https://new.example.com"));
    assert!(text.contains("sk-new"));
    assert!(text.contains("claude-sonnet-4-5"));
    // 用户键与注释保留
    assert!(text.contains("MY_CUSTOM_VAR"));
    assert!(text.contains("用户注释"));
    assert!(text.contains("keep-me"));
    // 键序：ANTHROPIC_BASE_URL 在 ANTHROPIC_MODEL 前
    assert!(
        text.find("ANTHROPIC_BASE_URL").unwrap() < text.find("ANTHROPIC_MODEL").unwrap()
    );
    // 顶层 apiKeyHelper 未被清（不在 clear scope）
    assert!(text.contains("apiKeyHelper"));
}

#[test]
fn json_patch_parse_failure_stops() {
    let bad = "{ not valid json";
    let patch = JsonPatch::default();
    let err = patch.apply(Path::new("settings.json"), Some(bad.as_bytes())).unwrap_err();
    assert!(matches!(err, LiveWriteError::Parse { .. }), "应返回 Parse 错误");
}

#[test]
fn json_patch_top_level_not_object_stops() {
    let patch = JsonPatch::default();
    let err = patch.apply(Path::new("settings.json"), Some(b"[1,2,3]")).unwrap_err();
    assert!(matches!(err, LiveWriteError::Shape { .. }), "应返回 Shape 错误");
}

#[test]
fn json_patch_missing_file_creates_object() {
    let patch = JsonPatch {
        set: vec![(KeyPath::from("env").child("ANTHROPIC_MODEL"), json!("m"))],
        ..Default::default()
    };
    let out = patch.apply(Path::new("settings.json"), None).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("ANTHROPIC_MODEL"));
}

#[test]
fn json_style_roundtrip_crlf_and_indent() {
    let before = "{\r\n    \"a\": 1\r\n}\r\n";
    let patch = JsonPatch::default();
    let out = patch.apply(Path::new("f.json"), Some(before.as_bytes())).unwrap();
    assert!(out.windows(2).any(|w| w == b"\r\n"), "保留 CRLF");
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("    \"a\""), "保留 4 空格缩进");
}

#[test]
fn claude_projection_builds_env_patch() {
    let cfg = json!({
        "env": {
            "ANTHROPIC_BASE_URL": "https://api.example.com",
            "ANTHROPIC_AUTH_TOKEN": "sk-abc",
            "ANTHROPIC_MODEL": "claude-sonnet-4-5",
            "ANTHROPIC_SMALL_FAST_MODEL": "claude-haiku-4-5"
        }
    });
    let patch = claude_patch(&cfg);
    assert_eq!(patch.set.len(), 4);
    assert!(!patch.clear.is_empty());
}

#[test]
fn codex_projection_builds_toml_patch() {
    let cfg = json!({
        "auth": { "OPENAI_API_KEY": "sk-codex" },
        "config": {
            "model_provider": "custom",
            "model": "gpt-5",
            "model_providers": { "custom": { "base_url": "https://api.example.com", "wire_api": "chat" } }
        }
    });
    let patch = codex_patch(&cfg);
    // set 包含 auth.OPENAI_API_KEY、顶层 model/model_provider/model_providers
    assert!(patch.set.iter().any(|(k, _)| k.to_string() == "auth.OPENAI_API_KEY"));
    assert!(patch.set.iter().any(|(k, _)| k.to_string() == "model"));
    assert!(patch.set.iter().any(|(k, _)| k.to_string() == "model_provider"));
    assert!(patch.set.iter().any(|(k, _)| k.to_string() == "model_providers"));
    // 旧的 model_providers 表先移除再写
    assert!(patch.remove.iter().any(|k| k.to_string() == "model_providers"));
}

#[test]
fn codex_toml_patch_preserves_comments_and_other_tables() {
    let before = r#"# Codex 配置
model = "gpt-4o"          # 用户模型注释
model_provider = "openai"
[model_providers.openai]
name = "OpenAI"
base_url = "https://api.openai.com/v1"

[model_providers.custom]
name = "自定义"
base_url = "https://old.example.com"

# 用户自己的段
[my_plugin]
enabled = true
"#;
    let patch = TomlPatch {
        remove: vec![KeyPath::from("model_providers")],
        set: vec![
            (KeyPath::from("model"), crate::toml::item_from_json(&json!("gpt-5"))),
            (
                KeyPath::from("model_providers").child("custom"),
                crate::toml::item_from_json(&json!({
                    "name": "自定义", "base_url": "https://new.example.com", "wire_api": "chat"
                })),
            ),
        ],
        ..Default::default()
    };
    let out = patch.apply(Path::new("config.toml"), Some(before.as_bytes())).unwrap();
    let text = String::from_utf8(out).unwrap();

    // 用户注释与段保留
    assert!(text.contains("# Codex 配置"));
    assert!(text.contains("用户模型注释"));
    assert!(text.contains("[my_plugin]"));
    assert!(text.contains("enabled = true"));
    // 目标值写入
    assert!(text.contains("gpt-5"));
    assert!(text.contains("https://new.example.com"));
    // 旧的 openai 供应商表被清（整个 model_providers 移除后重写）
    assert!(!text.contains("api.openai.com"));
}

#[test]
fn gemini_projection() {
    let cfg = json!({
        "env": {
            "GEMINI_API_KEY": "gk-1",
            "GEMINI_BASE_URL": "https://generativelanguage.googleapis.com",
            "GEMINI_MODEL": "gemini-2.0-flash",
            "CUSTOM_VAR": "keep"
        }
    });
    let patch = gemini_settings_patch(&cfg);
    assert_eq!(patch.set.len(), 3);
    // 自定义变量不入地板
    assert!(!patch.set.iter().any(|(k, _)| k.to_string().contains("CUSTOM_VAR")));
}

#[test]
fn env_patch_preserves_order_and_comments() {
    let before = "# 注释\nGEMINI_API_KEY=old\n\nGEMINI_MODEL=gemini-old\nUSER_VAR=1\n";
    let patch = crate::EnvPatch {
        remove: vec!["GEMINI_API_KEY".into()],
        set: vec![
            ("GEMINI_API_KEY".into(), "gk-new".into()),
            ("GEMINI_MODEL".into(), "gemini-2.0-flash".into()),
        ],
    };
    let out = patch.apply(Path::new(".env"), Some(before.as_bytes())).unwrap();
    let text = String::from_utf8(out).unwrap();
    // 注释与用户键保留
    assert!(text.contains("# 注释"));
    assert!(text.contains("USER_VAR=1"));
    // 原位替换
    assert!(text.contains("GEMINI_API_KEY=gk-new"));
    assert!(text.contains("GEMINI_MODEL=gemini-2.0-flash"));
    // 键序保持（API_KEY 在 MODEL 前）
    assert!(text.find("GEMINI_API_KEY").unwrap() < text.find("GEMINI_MODEL").unwrap());
}

#[test]
fn toml_parse_failure_stops() {
    let bad = "not = = valid toml";
    let patch = TomlPatch::default();
    let err = patch.apply(Path::new("config.toml"), Some(bad.as_bytes())).unwrap_err();
    assert!(matches!(err, LiveWriteError::Parse { .. }));
}

#[test]
fn json_style_default_matches() {
    let style = JsonStyle::default();
    assert_eq!(style.indent, b"  ");
    assert!(!style.trailing_newline);
}

#[test]
fn claude_floor_keeps_user_keys() {
    assert!(crate::claude_floor_env("ANTHROPIC_BASE_URL"));
    assert!(crate::claude_floor_env("CLAUDE_CODE_USE_BEDROCK"));
    assert!(!crate::claude_floor_env("MY_CUSTOM_VAR"));
    assert!(!crate::claude_floor_env("PATH"));
}


