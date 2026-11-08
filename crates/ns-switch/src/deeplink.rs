//! 深链解析：`nsswitch://` 导入供应商 / MCP / Skills / Prompts。
//!
//! 格式：`nsswitch://<action>?k=v&k2=v2`（URL 编码值）。
//! 动作：`import-provider`、`import-mcp`、`import-skill`、`import-prompt`、`switch-provider`。

/// 深链动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeepLinkAction {
    ImportProvider {
        preset_id: String,
        endpoint: String,
        key: String,
        model: String,
    },
    ImportMcp {
        name: String,
        command: String,
    },
    ImportSkill {
        name: String,
        source: String,
    },
    ImportPrompt {
        name: String,
        content: String,
    },
    SwitchProvider {
        tool: String,
        provider_id: String,
    },
    /// 导入仓库 URL（MCP / Skills / Prompts 仓库）。
    ImportRepo {
        kind: String,
        url: String,
    },
}

/// 解析深链 URL（无匹配返回 `None`）。
pub fn parse(url: &str) -> Option<DeepLinkAction> {
    let rest = url.strip_prefix("nsswitch://")?;
    let (action, query) = match rest.split_once('?') {
        Some((a, q)) => (a, q),
        None => (rest, ""),
    };
    let params = parse_query(query);
    match action {
        "import-provider" => Some(DeepLinkAction::ImportProvider {
            preset_id: params.get("preset").cloned().unwrap_or_default(),
            endpoint: params.get("endpoint").cloned().unwrap_or_default(),
            key: params.get("key").cloned().unwrap_or_default(),
            model: params.get("model").cloned().unwrap_or_default(),
        }),
        "import-mcp" => Some(DeepLinkAction::ImportMcp {
            name: params.get("name").cloned().unwrap_or_default(),
            command: params.get("command").cloned().unwrap_or_default(),
        }),
        "import-skill" => Some(DeepLinkAction::ImportSkill {
            name: params.get("name").cloned().unwrap_or_default(),
            source: params.get("source").cloned().unwrap_or_default(),
        }),
        "import-prompt" => Some(DeepLinkAction::ImportPrompt {
            name: params.get("name").cloned().unwrap_or_default(),
            content: params.get("content").cloned().unwrap_or_default(),
        }),
        "switch-provider" => Some(DeepLinkAction::SwitchProvider {
            tool: params.get("tool").cloned().unwrap_or_default(),
            provider_id: params.get("provider").cloned().unwrap_or_default(),
        }),
        "import-repo" => Some(DeepLinkAction::ImportRepo {
            kind: params.get("kind").cloned().unwrap_or_default(),
            url: params.get("url").cloned().unwrap_or_default(),
        }),
        _ => None,
    }
}

/// 解析 `k=v&k2=v2`（URL 解码）。
fn parse_query(query: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for pair in query.split('&').filter(|s| !s.is_empty()) {
        if let Some((k, v)) = pair.split_once('=') {
            map.insert(url_decode(k), url_decode(v));
        }
    }
    map
}

/// 简易 URL 解码（`%XX`）。
fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_import_provider() {
        let url = "nsswitch://import-provider?preset=claude-88api&endpoint=https%3A%2F%2Fapi.88api.com&key=sk-test";
        let a = parse(url).unwrap();
        assert_eq!(
            a,
            DeepLinkAction::ImportProvider {
                preset_id: "claude-88api".into(),
                endpoint: "https://api.88api.com".into(),
                key: "sk-test".into(),
                model: String::new(),
            }
        );
    }

    #[test]
    fn parses_switch_provider() {
        let url = "nsswitch://switch-provider?tool=claude&provider=hosted-copilot";
        assert_eq!(
            parse(url).unwrap(),
            DeepLinkAction::SwitchProvider {
                tool: "claude".into(),
                provider_id: "hosted-copilot".into(),
            }
        );
    }

    #[test]
    fn parses_import_repo() {
        let url = "nsswitch://import-repo?kind=skill&url=https%3A%2F%2Fgithub.com%2Fuser%2Fskills-repo";
        assert_eq!(
            parse(url).unwrap(),
            DeepLinkAction::ImportRepo {
                kind: "skill".into(),
                url: "https://github.com/user/skills-repo".into(),
            }
        );
    }
    #[test]
    fn rejects_unknown_action() {
        assert!(parse("nsswitch://bogus").is_none());
        assert!(parse("https://example.com").is_none());
    }
}
