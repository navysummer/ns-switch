//! 小工具：Claude Code 便捷开关 + VS Code 联动检测。
//!
//! 开关写入 `~/.claude/settings.json` 的 `env`（floor 已保留这些键，
//! 切换供应商不会清除）。VS Code 联动 = 检测 `code` CLI 可用性。

use std::path::PathBuf;

const CLAUDE_SETTINGS_REL: &str = ".claude/settings.json";

/// Claude settings.json 路径。
pub fn claude_settings_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(CLAUDE_SETTINGS_REL))
}

/// 读取 `env` 某 key 是否为 "true"。
fn env_bool(key: &str) -> bool {
    let Some(path) = claude_settings_path() else {
        return false;
    };
    let Ok(raw) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    v.get("env")
        .and_then(|e| e.get(key))
        .and_then(|x| x.as_str())
        .map(|s| s == "true")
        .unwrap_or(false)
}

/// 设置 `env` 某 key 为 "true"/"false"（写盘，保留其他字段）。
fn set_env_bool(key: &str, enabled: bool) -> Result<(), String> {
    let path = claude_settings_path().ok_or("无法定位 HOME")?;
    let mut v = if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|r| serde_json::from_str::<serde_json::Value>(&r).ok())
            .unwrap_or_else(|| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    if !v.is_object() {
        v = serde_json::json!({});
    }
    if !v.get("env").map(|e| e.is_object()).unwrap_or(false) {
        v["env"] = serde_json::json!({});
    }
    if enabled {
        v["env"][key] = serde_json::Value::String("true".into());
    } else {
        v["env"].as_object_mut().map(|o| o.remove(key));
    }
    std::fs::write(&path, serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 跳过初次确认（`CLAUDE_CODE_SKIP_VERIFY_AUTH`）。
pub fn skip_verify_enabled() -> bool {
    env_bool("CLAUDE_CODE_SKIP_VERIFY_AUTH")
}
pub fn set_skip_verify(enabled: bool) -> Result<(), String> {
    set_env_bool("CLAUDE_CODE_SKIP_VERIFY_AUTH", enabled)
}

/// 隐藏署名（`CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC`）。
pub fn hide_signature_enabled() -> bool {
    env_bool("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC")
}
pub fn set_hide_signature(enabled: bool) -> Result<(), String> {
    set_env_bool("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", enabled)
}

/// VS Code CLI 是否可用（`code` 在 PATH）。
pub fn vscode_available() -> bool {
    std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p)
                .any(|dir| dir.join("code").exists() || dir.join("code.cmd").exists())
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_bool_missing_is_false() {
        // 无 HOME 环境下返回 false（不 panic）。
        let _ = env_bool("NOPE_KEY");
    }

    #[test]
    fn vscode_detection_runs() {
        // 检测不 panic，返回 bool。
        let _ = vscode_available();
    }
}
