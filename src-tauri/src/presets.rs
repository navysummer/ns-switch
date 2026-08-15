use crate::models::Provider;

pub fn presets_for(tool_id: &str) -> Vec<Provider> {
    let official = Provider {
        id: 0,
        tool_id: tool_id.to_string(),
        name: "Official Login".to_string(),
        base_url: match tool_id {
            "claude_code" | "claude_desktop" | "hermes" => "https://api.anthropic.com".into(),
            "codex" | "opencode" | "openclaw" => "https://api.openai.com/v1".into(),
            "gemini" => "https://generativelanguage.googleapis.com".into(),
            "grok" => "https://api.x.ai".into(),
            _ => String::new(),
        },
        api_key: String::new(),
        model: match tool_id {
            "claude_code" | "claude_desktop" => "claude-sonnet-4-5".into(),
            "codex" => "gpt-5-codex".into(),
            "gemini" => String::new(),
            "opencode" => String::new(),
            "grok" => "grok-3".into(),
            _ => String::new(),
        },
        headers: "{}".into(),
        shared_config: String::new(),
        protocol: String::new(),
        is_preset: true,
        is_active: false,
        is_universal: false,
    };

    let mut presets: Vec<Provider> = vec![official];

    if tool_id == "claude_code" || tool_id == "claude_desktop" {
        for (name, base, model) in [
            ("DeepSeek Relay", "https://api.deepseek.com/anthropic", "deepseek-chat"),
            ("SiliconFlow", "https://api.siliconflow.cn/v1", "DeepSeek-V3"),
            ("ApiOne", "https://api.one.onex.one/v1", "claude-sonnet-4-5"),
            ("Volcengine Ark", "https://ark.cn-beijing.volces.com/api/v3", ""),
            ("OpenRouter", "https://openrouter.ai/api/v1", "anthropic/claude-sonnet-4-5"),
            ("Moonshot Kimi", "https://api.moonshot.cn/anthropic", "kimi-k2.5-code"),
            ("ZetaAPI", "https://api.zetaapi.com/v1", "claude-sonnet-4-5"),
            ("APINebula", "https://api.apinebula.ai/v1", "claude-sonnet-4-5"),
            ("AICodeMirror", "https://api.aicodemirror.com/v1", "claude-sonnet-4-5"),
            ("AICoding", "https://api.aicoding.one/v1", "claude-sonnet-4-5"),
            ("RunAPI", "https://api.runapi.com/v1", "claude-sonnet-4-5"),
            ("Groq", "https://api.groq.com/openai/v1", ""),
            ("PackyCode", "https://api.packyapi.com", "claude-sonnet-4-5"),
            ("SBX (AIProxy)", "https://aip.baidu.com/anthropic", ""),
            ("ProxyCN", "https://api.proxycn.ai/anthropic", "claude-sonnet-4-5"),
        ] {
            presets.push(preset(tool_id, name, base, "", model));
        }
    }

    if tool_id == "codex" {
        for (name, base, model) in [
            ("OpenAI Official", "https://api.openai.com/v1", "gpt-5-codex"),
            ("ApiOne", "https://api.one.onex.one/v1", "gpt-5-codex"),
            ("SiliconFlow", "https://api.siliconflow.cn/v1", "Qwen3-Coder"),
            ("Groq", "https://api.groq.com/openai/v1", ""),
            ("OpenRouter", "https://openrouter.ai/api/v1", "openai/gpt-5-codex"),
            ("ZetaAPI", "https://api.zetaapi.com/v1", "gpt-5-codex"),
            ("Fenno.ai", "https://api.fenno.ai/v1", "codex-o1"),
            ("RunAPI", "https://api.runapi.com/v1", "gpt-5-codex"),
            ("Cubence", "https://api.cubence.com/v1", "gpt-5-codex"),
            ("PackyCode", "https://api.packyapi.com/v1", "gpt-5-codex"),
        ] {
            presets.push(preset(tool_id, name, base, "", model));
        }
    }

    if tool_id == "gemini" {
        for (name, base, model) in [
            ("Google Official", "https://generativelanguage.googleapis.com", ""),
            ("OpenRouter", "https://openrouter.ai/api/v1", "google/gemini-2.5-pro"),
            ("Vertex AI", "https://generativelanguage.googleapis.com", "gemini-2.5-pro"),
        ] {
            presets.push(preset(tool_id, name, base, "", model));
        }
    }

    if tool_id == "opencode" {
        for (name, base, model) in [
            ("OpenAI Official", "https://api.openai.com/v1", "gpt-4o"),
            ("Anthropic Official", "https://api.anthropic.com", "claude-sonnet-4-5"),
            ("OpenRouter", "https://openrouter.ai/api/v1", "anthropic/claude-sonnet-4-5"),
            ("Groq", "https://api.groq.com/openai/v1", ""),
            ("SiliconFlow", "https://api.siliconflow.cn/v1", "DeepSeek-V3"),
        ] {
            presets.push(preset(tool_id, name, base, "", model));
        }
    }

    if tool_id == "grok" {
        for (name, base, model) in [
            ("xAI Official", "https://api.x.ai", "grok-3"),
            ("OpenRouter", "https://openrouter.ai/api/v1", "x-ai/grok-3"),
        ] {
            presets.push(preset(tool_id, name, base, "", model));
        }
    }

    presets
}

fn preset(tool_id: &str, name: &str, base: &str, key: &str, model: &str) -> Provider {
    Provider {
        id: 0,
        tool_id: tool_id.to_string(),
        name: name.to_string(),
        base_url: base.to_string(),
        api_key: key.to_string(),
        model: model.to_string(),
        headers: "{}".into(),
        shared_config: String::new(),
        protocol: String::new(),
        is_preset: true,
        is_active: false,
        is_universal: false,
    }
}