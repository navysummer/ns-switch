//! M4 切换集成：把「供应商行」落到客户端 live 配置文件。
//!
//! 流程（每工具）：拿应用写锁 → `plan`（内存算新内容）→ 首写备份 →
//! 原子写 → 写后重读比对 → 更新 `current`。所有步骤经 `ns-live` 引擎，
//! 与 cc-switch `live/engine.rs` 语义一致。

use std::path::PathBuf;

use ns_live::{
    EnvPatch, JsonPatch, LiveFile, LivePatch, LiveWriteError, claude_patch, codex_patch,
    gemini_env_entries, gemini_settings_patch, lock_app,
};
use ns_presets::{Provider, Tool};

/// 一个工具的 live 写入计划（文件 + 补丁）。
pub struct LivePlan {
    pub file: LiveFile,
    pub patch: Patch,
}


/// 补丁枚举（每个工具格式不同）。
pub enum Patch {
    Json(JsonPatch),
    Toml(ns_live::TomlPatch),
    Env(EnvPatch),
    Yaml(ns_live::YamlPatch),
}

impl LivePatch for Patch {
    fn apply(&self, path: &std::path::Path, pre: Option<&[u8]>) -> Result<Vec<u8>, LiveWriteError> {
        match self {
            Patch::Json(p) => p.apply(path, pre),
            Patch::Toml(p) => p.apply(path, pre),
            Patch::Env(p) => p.apply(path, pre),
            Patch::Yaml(p) => p.apply(path, pre),
        }
    }
}


/// 配置目录覆盖表（key = 工具 `id_str()`，value = 绝对路径）。
pub type ConfigDirs = std::collections::BTreeMap<String, String>;

/// 某工具的配置基础目录（用户覆盖优先，否则 `home` 默认）。
fn tool_base_dir(
    tool: Tool,
    home: &std::path::Path,
    config_dirs: &ConfigDirs,
) -> std::path::PathBuf {
    if let Some(d) = config_dirs
        .get(tool.id_str())
        .filter(|s| !s.trim().is_empty())
    {
        return std::path::PathBuf::from(d);
    }
    match tool {
        Tool::Claude => home.join(".claude"),
        Tool::Codex => home.join(".codex"),
        Tool::Gemini => home.join(".gemini"),
        Tool::GrokBuild => home.join(".grok"),
        Tool::Opencode => home.join(".config").join("opencode"),
        Tool::Openclaw => home.join(".openclaw"),
        Tool::Pi => home.join(".pi").join("agent"),
        Tool::Hermes => home.join(".hermes"),
        Tool::Mcode => home.join(".minimax"),
        Tool::Kimi => home.join(".kimi-code"),
        Tool::Omp => home.join(".omp").join("agent"),
        Tool::Agy => home.join(".agy"),
        _ => home.join(format!(".{}", tool.id_str())),
    }
}

/// 指定家目录的 live 计划（测试可用临时目录）。
pub fn plan_for_tool_in(
    tool: Tool,
    provider: &Provider,
    home: &std::path::Path,
    config_dirs: &ConfigDirs,
) -> Vec<LivePlan> {
    let cfg = &provider.settings_config;
    let dir = tool_base_dir(tool, home, config_dirs);
    match tool {
        Tool::Claude => vec![LivePlan {
            file: LiveFile::new("claude", dir.join("settings.json"), true),
            patch: Patch::Json(claude_patch(cfg)),
        }],
        Tool::Codex => vec![LivePlan {
            file: LiveFile::new("codex", dir.join("config.toml"), true),
            patch: Patch::Toml(codex_patch(cfg)),
        }],
        Tool::Gemini => {
            let mut plans = vec![LivePlan {
                file: LiveFile::new("gemini", dir.join("settings.json"), true),
                patch: Patch::Json(gemini_settings_patch(cfg)),
            }];
            let entries = gemini_env_entries(cfg);
            if !entries.is_empty() {
                plans.push(LivePlan {
                    file: LiveFile::new("gemini", dir.join(".env"), true),
                    patch: Patch::Env(EnvPatch {
                        remove: vec![],
                        set: entries,
                    }),
                });
            }
            plans
        }
        Tool::GrokBuild => {
            let id = provider.id.clone();
            vec![LivePlan {
                file: LiveFile::new("grok", dir.join("config.toml"), true),
                patch: Patch::Toml(ns_live::grok_patch(&id, cfg)),
            }]
        }
        // 共存式工具（additive）：把供应商 upsert 进工具自身配置。
        Tool::Opencode => vec![LivePlan {
            file: LiveFile::new("opencode", dir.join("opencode.json"), true),
            patch: Patch::Json(ns_live::opencode_patch(&provider.id, cfg)),
        }],
        Tool::Openclaw => vec![LivePlan {
            file: LiveFile::new("openclaw", dir.join("openclaw.json"), true),
            patch: Patch::Json(ns_live::openclaw_patch(&provider.id, cfg)),
        }],
        Tool::Pi => vec![LivePlan {
            file: LiveFile::new("pi", dir.join("models.json"), true),
            patch: Patch::Json(ns_live::pi_patch(&provider.id, cfg)),
        }],
        Tool::Hermes => vec![LivePlan {
            file: LiveFile::new("hermes", dir.join("config.yaml"), true),
            patch: Patch::Yaml(ns_live::hermes_patch(&provider.id, cfg)),
        }],
        Tool::Mcode => vec![LivePlan {
            file: LiveFile::new("mcode", dir.join("config.yaml"), true),
            patch: Patch::Yaml(ns_live::mcode_patch(&provider.id, cfg)),
        }],
        Tool::Kimi => vec![LivePlan {
            file: LiveFile::new("kimi", dir.join("config.toml"), true),
            patch: Patch::Toml(ns_live::kimi_patch(&provider.id, cfg)),
        }],
        Tool::Omp => vec![LivePlan {
            file: LiveFile::new("omp", dir.join("models.yml"), true),
            patch: Patch::Yaml(ns_live::omp_patch(&provider.id, cfg)),
        }],
        // Agy 卡只开终端，不写 live 文件。
        Tool::Agy => Vec::new(),
        // 其余工具：M5 起逐个接入；当前无 live 文件写入。
        _ => Vec::new(),
    }
}

/// 路由模式下某工具要写的 live 文件计划：把配置指向本地代理
/// (`127.0.0.1:<port>`) + 占位 key。真实 key 留在 `config.json`。
pub fn routing_plans_for_tool_in(
    tool: Tool,
    port: u16,
    home: &std::path::Path,
    config_dirs: &ConfigDirs,
) -> Vec<LivePlan> {
    use ns_live::{
        claude_routing_patch, codex_routing_patch, gemini_routing_env_patch,
        gemini_routing_settings_patch,
    };
    let dir = tool_base_dir(tool, home, config_dirs);
    match tool {
        Tool::Claude => vec![LivePlan {
            file: LiveFile::new("claude", dir.join("settings.json"), true),
            patch: Patch::Json(claude_routing_patch(port)),
        }],
        Tool::Codex => vec![LivePlan {
            file: LiveFile::new("codex", dir.join("config.toml"), true),
            patch: Patch::Toml(codex_routing_patch(port)),
        }],
        Tool::Gemini => vec![
            LivePlan {
                file: LiveFile::new("gemini", dir.join("settings.json"), true),
                patch: Patch::Json(gemini_routing_settings_patch(port)),
            },
            LivePlan {
                file: LiveFile::new("gemini", dir.join(".env"), true),
                patch: Patch::Env(gemini_routing_env_patch(port)),
            },
        ],
        Tool::GrokBuild => vec![LivePlan {
            file: LiveFile::new("grok", dir.join("config.toml"), true),
            patch: Patch::Toml(ns_live::grok_routing_patch("grok-routing", port)),
        }],
        _ => Vec::new(),
    }
}

/// 写入路由模式 live 配置（指向本地代理）。返回写入文件数。
/// 指定家目录 + 配置目录覆盖的路由切换（测试/设置覆盖用）。
pub fn switch_to_routing_in_dirs(
    tool: Tool,
    port: u16,
    backup_dir: &std::path::Path,
    home: &std::path::Path,
    config_dirs: &ConfigDirs,
) -> Result<SwitchOutcome, LiveWriteError> {
    let plans = routing_plans_for_tool_in(tool, port, home, config_dirs);
    if plans.is_empty() {
        return Ok(SwitchOutcome {
            written: 0,
            noop: true,
        });
    }
    let app = tool.id_str();
    let _guard = lock_app(app);
    let mut written = 0;
    let mut noop = true;
    for plan in &plans {
        let planned = ns_live::plan(&plan.file, &plan.patch)?;
        if planned.is_noop() {
            continue;
        }
        ns_live::apply_planned(&planned, backup_dir)?;
        written += 1;
        noop = false;
    }
    Ok(SwitchOutcome {
        written,
        noop,
    })
}
/// 一个冲突告警（写入前检查）。
#[derive(Debug, Clone)]
pub struct Conflict {
    /// 冲突文件路径。
    pub path: PathBuf,
    /// 冲突原因描述。
    pub reason: String,
}

/// 写入前冲突检测：目标工具 live 文件当前内容是否被「非当前激活供应商」占用
/// 或存在手动改动。
///
/// 规则：
/// - 文件不存在 → 无冲突（将新建）。
/// - 当前无激活供应商 → 文件非空视为手动占用冲突。
/// - 有激活供应商 B（≠ 目标 A）→ 磁盘内容与 B 的预期写入一致 ⇒ 正常切换；
///   不一致 ⇒ 手动改动/被其他进程占用 ⇒ 冲突。
/// 指定家目录 + 配置目录覆盖的冲突检测。
pub fn check_conflicts_dirs(
    tool: Tool,
    target: &Provider,
    current_provider: Option<&Provider>,
    home: &std::path::Path,
    config_dirs: &ConfigDirs,
) -> Vec<Conflict> {
    let mut conflicts = Vec::new();
    let target_plans = plan_for_tool_in(tool, target, home, config_dirs);
    // 当前激活供应商的预期内容（用于「磁盘是否等于 B 的写入」判断）。
    let current_plans = current_provider.map(|p| plan_for_tool_in(tool, p, home, config_dirs));
    for plan in &target_plans {
        let Some(pre) = ns_live::read_current(&plan.file.path).ok().flatten() else {
            continue; // 文件不存在：无冲突
        };
        // 若磁盘内容与当前激活供应商预期一致 ⇒ 干净切换。
        if let Some(current_plans) = &current_plans {
            let matches_current = current_plans.iter().any(|cp| {
                cp.patch
                    .apply(&cp.file.path, Some(&pre))
                    .map(|bytes| bytes == pre)
                    .unwrap_or(false)
            });
            if matches_current {
                continue;
            }
        } else if pre.is_empty() {
            continue;
        }
        conflicts.push(Conflict {
            path: plan.file.path.clone(),
            reason: "已被修改或由其他供应商占用".to_string(),
        });
    }
    conflicts
}

/// 切换结果（用于状态栏/通知）。
#[derive(Debug, Clone)]
pub struct SwitchOutcome {
    /// 实际写入的文件数（noop 不算）。
    pub written: usize,
    /// 是否为 noop（内容与现有一致）。
    pub noop: bool,
}
/// 指定家目录的切换（测试用临时目录）。
/// 指定家目录 + 配置目录覆盖的切换。
pub fn switch_to_in_dirs(
    tool: Tool,
    provider: &Provider,
    backup_dir: &std::path::Path,
    home: &std::path::Path,
    config_dirs: &ConfigDirs,
) -> Result<SwitchOutcome, LiveWriteError> {
    let plans = plan_for_tool_in(tool, provider, home, config_dirs);
    if plans.is_empty() {
        return Ok(SwitchOutcome {
            written: 0,
            noop: true,
        });
    }
    let app = tool.id_str();
    let _guard = lock_app(app);
    let mut written = 0;
    let mut noop = true;
    for plan in &plans {
        let planned = ns_live::plan(&plan.file, &plan.patch)?;
        if planned.is_noop() {
            continue;
        }
        ns_live::apply_planned(&planned, backup_dir)?;
        written += 1;
        noop = false;
    }
    Ok(SwitchOutcome {
        written,
        noop,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude_provider() -> Provider {
        let mut p = Provider::with_id(
            "claude-88api-test".into(),
            "88api".into(),
            serde_json::json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.88api.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-test",
                    "ANTHROPIC_MODEL": "claude-sonnet-4-5",
                }
            }),
        );
        p.meta = Some(ns_presets::ProviderMeta {
            template_type: Some("claude".into()),
            ..Default::default()
        });
        p
    }

    #[test]
    fn claude_plan_writes_settings_json() {
        let home = std::env::temp_dir().join(format!("ns-switch-plan-{}", std::process::id()));
        let plans = plan_for_tool_in(Tool::Claude, &claude_provider(), &home, &ConfigDirs::new());
        assert_eq!(plans.len(), 1);
        assert!(plans[0].file.path.ends_with("settings.json"));
        assert_eq!(plans[0].file.app, "claude");
    }

    #[test]
    fn config_dir_override_moves_plan_path() {
        let home = std::env::temp_dir().join("ns-switch-plan-override");
        let mut dirs = ConfigDirs::new();
        dirs.insert("claude".into(), "/custom/claude".into());
        let plans = plan_for_tool_in(Tool::Claude, &claude_provider(), &home, &dirs);
        assert_eq!(plans.len(), 1);
        assert!(plans[0].file.path.starts_with("/custom/claude"), "路径应为覆盖目录: {}", plans[0].file.path.display());
        assert!(plans[0].file.path.ends_with("settings.json"));
        // 路由计划同样尊重覆盖。
        let rplans = routing_plans_for_tool_in(Tool::Claude, 8899, &home, &dirs);
        assert!(rplans[0].file.path.starts_with("/custom/claude"));
    }

    #[test]
    fn gemini_plan_writes_settings_and_env() {
        let home = std::env::temp_dir().join("ns-switch-plan-gemini");
        let provider = Provider::with_id(
            "gemini-ppio-test".into(),
            "PPIO".into(),
            serde_json::json!({
                "env": {
                    "GEMINI_API_KEY": "sk-g",
                    "GEMINI_BASE_URL": "https://ppio.com",
                    "GEMINI_MODEL": "gemini-2.5-pro",
                }
            }),
        );
        let plans = plan_for_tool_in(Tool::Gemini, &provider, &home, &ConfigDirs::new());
        assert_eq!(plans.len(), 2, "settings.json + .env");
        assert!(plans[0].file.path.ends_with("settings.json"));
        assert!(plans[1].file.path.ends_with(".env"));
    }

    #[test]
    fn switch_to_writes_claude_settings() {
        let base = std::env::temp_dir().join(format!("ns-switch-switch-{}", std::process::id()));
        let home = base.join("home");
        let backup = base.join("backups");
        let outcome = switch_to_in_dirs(Tool::Claude, &claude_provider(), &backup, &home, &ConfigDirs::new()).unwrap();
        assert_eq!(outcome.written, 1);
        assert!(!outcome.noop);
        let path = home.join(".claude").join("settings.json");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("api.88api.com"));
        assert!(text.contains("sk-test"));
        assert!(text.contains("claude-sonnet-4-5"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn e2e_switch_and_verify_live_file() {
        let base = std::env::temp_dir().join(format!("ns-switch-e2e-{}", std::process::id()));
        let home = base.join("home");
        let backup = base.join("backups");
        let path = home.join(".claude").join("settings.json");
        // 预置一份原始 live 文件，验证首写备份会保留它。
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://original.example.com"}}"#,
        )
        .unwrap();

        // 供应商 B（88api）→ 写入并核对。
        let b = claude_provider();
        let o1 = switch_to_in_dirs(Tool::Claude, &b, &backup, &home, &ConfigDirs::new()).unwrap();
        assert_eq!(o1.written, 1);
        let text1 = std::fs::read_to_string(&path).unwrap();
        assert!(text1.contains("api.88api.com"));
        assert!(text1.contains("sk-test"));

        // 切到供应商 A（other）→ 内容更新。
        let a = Provider::with_id(
            "claude-other-e2e".into(),
            "Other".into(),
            serde_json::json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://other.example.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-other",
                    "ANTHROPIC_MODEL": "claude-opus-4-1",
                }
            }),
        );
        let o2 = switch_to_in_dirs(Tool::Claude, &a, &backup, &home, &ConfigDirs::new()).unwrap();
        assert_eq!(o2.written, 1);
        let text2 = std::fs::read_to_string(&path).unwrap();
        assert!(text2.contains("other.example.com"));
        assert!(text2.contains("sk-other"));
        assert!(!text2.contains("api.88api.com"), "旧供应商地址应被清掉");

        // 重复切到 A → noop（不重写）。
        let o3 = switch_to_in_dirs(Tool::Claude, &a, &backup, &home, &ConfigDirs::new()).unwrap();
        assert!(o3.noop);
        assert_eq!(o3.written, 0);

        // 首写备份应保留 B 的原始内容。
        let backups: Vec<_> = std::fs::read_dir(&backup).unwrap().collect();
        assert!(backups.iter().any(|e| {
            e.as_ref()
                .unwrap()
                .path()
                .extension()
                .map(|s| s != "source")
                .unwrap_or(false)
        }));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn clean_switch_has_no_conflict() {
        let base = std::env::temp_dir().join(format!("ns-switch-conflict-{}", std::process::id()));
        let home = base.join("home");
        let backup = base.join("backups");
        // 当前供应商 B 已写入磁盘。
        let b = claude_provider();
        switch_to_in_dirs(Tool::Claude, &b, &backup, &home, &ConfigDirs::new()).unwrap();
        // 切到 A，且 current = B：磁盘与 B 一致 ⇒ 无冲突。
        let a = Provider::with_id(
            "claude-other".into(),
            "Other".into(),
            serde_json::json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://other.example.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-other",
                    "ANTHROPIC_MODEL": "claude-opus-4",
                }
            }),
        );
        let conflicts = check_conflicts_dirs(Tool::Claude, &a, Some(&b), &home, &ConfigDirs::new());
        assert!(conflicts.is_empty(), "干净切换不应有冲突: {:?}", conflicts);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn manual_edit_detected_as_conflict() {
        let base = std::env::temp_dir().join(format!("ns-switch-conflict2-{}", std::process::id()));
        let home = base.join("home");
        let backup = base.join("backups");
        let b = claude_provider();
        switch_to_in_dirs(Tool::Claude, &b, &backup, &home, &ConfigDirs::new()).unwrap();
        // 手动改掉磁盘（模拟外部修改）。
        let settings = home.join(".claude").join("settings.json");
        std::fs::write(&settings, r#"{"env":{"ANTHROPIC_BASE_URL":"https://manual.example.com"}}"#)
            .unwrap();
        let a = claude_provider();
        let conflicts = check_conflicts_dirs(Tool::Claude, &a, Some(&b), &home, &ConfigDirs::new());
        assert_eq!(conflicts.len(), 1, "手动改动应报冲突");
        assert!(conflicts[0].path.ends_with("settings.json"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn no_write_tools_have_no_plans() {
        let home = std::env::temp_dir().join("ns-switch-plan-none");
        let provider = claude_provider();
        // Agy 卡只开终端，不写 live 文件。
        assert!(plan_for_tool_in(Tool::Agy, &provider, &home, &ConfigDirs::new()).is_empty());
    }

    #[test]
    fn opencode_plan_writes_provider() {
        let home = std::env::temp_dir().join("ns-switch-plan-opencode");
        let provider = Provider::with_id(
            "opencode-88api-test".into(),
            "88API".into(),
            serde_json::json!({
                "baseUrl": "https://api.88api.com",
                "apiKey": "sk-test",
                "model": "claude-sonnet-4-5",
            }),
        );
        let plans = plan_for_tool_in(Tool::Opencode, &provider, &home, &ConfigDirs::new());
        assert_eq!(plans.len(), 1);
        assert!(plans[0].file.path.ends_with("opencode.json"));
        assert_eq!(plans[0].file.app, "opencode");
    }

    #[test]
    fn coexist_tools_write_provider_files() {
        let base = std::env::temp_dir().join(format!("ns-switch-coexist-{}", std::process::id()));
        let home = base.join("home");
        let backup = base.join("backups");

        // OpenCode: JSON 写 provider.<id>。
        let oc = Provider::with_id(
            "opencode-88api-test".into(),
            "88API".into(),
            serde_json::json!({
                "baseUrl": "https://api.88api.com",
                "apiKey": "sk-test",
                "model": "claude-sonnet-4-5",
            }),
        );
        let o = switch_to_in_dirs(Tool::Opencode, &oc, &backup, &home, &ConfigDirs::new()).unwrap();
        assert_eq!(o.written, 1);
        let text = std::fs::read_to_string(
            home.join(".config").join("opencode").join("opencode.json"),
        )
        .unwrap();
        assert!(text.contains("opencode-88api-test"));
        assert!(text.contains("api.88api.com"));
        assert!(text.contains("sk-test"));

        // OpenClaw: JSON5 写 models.providers.<id>。
        let ocw = Provider::with_id(
            "openclaw-88api-test".into(),
            "88API".into(),
            serde_json::json!({
                "baseUrl": "https://api.88api.com",
                "apiKey": "sk-test",
                "model": "claude-sonnet-4-5",
            }),
        );
        let o = switch_to_in_dirs(Tool::Openclaw, &ocw, &backup, &home, &ConfigDirs::new()).unwrap();
        assert_eq!(o.written, 1);
        let text = std::fs::read_to_string(home.join(".openclaw").join("openclaw.json")).unwrap();
        assert!(text.contains("openclaw-88api-test"));
        assert!(text.contains("api.88api.com"));

        // Hermes: YAML 写 custom_providers 列表。
        let h = Provider::with_id(
            "hermes-88api-test".into(),
            "88API".into(),
            serde_json::json!({
                "baseUrl": "https://api.88api.com",
                "apiKey": "sk-test",
                "model": "claude-sonnet-4-5",
            }),
        );
        let o = switch_to_in_dirs(Tool::Hermes, &h, &backup, &home, &ConfigDirs::new()).unwrap();
        assert_eq!(o.written, 1);
        let text = std::fs::read_to_string(home.join(".hermes").join("config.yaml")).unwrap();
        assert!(text.contains("hermes-88api-test"));
        assert!(text.contains("base_url"));

        // Kimi: TOML 写 providers.<id>。
        let k = Provider::with_id(
            "kimi-88api-test".into(),
            "88API".into(),
            serde_json::json!({
                "baseUrl": "https://api.88api.com",
                "apiKey": "sk-test",
                "model": "claude-sonnet-4-5",
            }),
        );
        let o = switch_to_in_dirs(Tool::Kimi, &k, &backup, &home, &ConfigDirs::new()).unwrap();
        assert_eq!(o.written, 1);
        let text = std::fs::read_to_string(home.join(".kimi-code").join("config.toml")).unwrap();
        assert!(text.contains("kimi-88api-test"));
        assert!(text.contains("base_url"));

        // Omp: YAML 写 providers.<id>。
        let om = Provider::with_id(
            "omp-88api-test".into(),
            "88API".into(),
            serde_json::json!({
                "baseUrl": "https://api.88api.com",
                "apiKey": "sk-test",
                "model": "claude-sonnet-4-5",
            }),
        );
        let o = switch_to_in_dirs(Tool::Omp, &om, &backup, &home, &ConfigDirs::new()).unwrap();
        assert_eq!(o.written, 1);
        let text = std::fs::read_to_string(home.join(".omp").join("agent").join("models.yml")).unwrap();
        assert!(text.contains("omp-88api-test"));

        // Pi: JSON 写顶层 <id>。
        let pi = Provider::with_id(
            "pi-88api-test".into(),
            "88API".into(),
            serde_json::json!({
                "baseUrl": "https://api.88api.com",
                "apiKey": "sk-test",
                "model": "claude-sonnet-4-5",
            }),
        );
        let o = switch_to_in_dirs(Tool::Pi, &pi, &backup, &home, &ConfigDirs::new()).unwrap();
        assert_eq!(o.written, 1);
        let text = std::fs::read_to_string(home.join(".pi").join("agent").join("models.json")).unwrap();
        assert!(text.contains("pi-88api-test"));
        assert!(text.contains("api.88api.com"));

        let _ = std::fs::remove_dir_all(&base);
    }


    #[test]
    fn mcode_writes_providers_object() {
        let base = std::env::temp_dir().join(format!("ns-switch-mcode-{}", std::process::id()));
        let home = base.join("home");
        let backup = base.join("backups");
        let m = Provider::with_id(
            "mcode-88api-test".into(),
            "88API".into(),
            serde_json::json!({
                "baseUrl": "https://api.88api.com",
                "apiKey": "sk-test",
                "model": "claude-sonnet-4-5",
            }),
        );
        let o = switch_to_in_dirs(Tool::Mcode, &m, &backup, &home, &ConfigDirs::new()).unwrap();
        assert_eq!(o.written, 1);
        let text = std::fs::read_to_string(home.join(".minimax").join("config.yaml")).unwrap();
        assert!(text.contains("mcode-88api-test"));
        assert!(text.contains("baseURL"));
        assert!(text.contains("sk-test"));
        let _ = std::fs::remove_dir_all(&base);
    }
    #[test]
    fn routing_settings_roundtrip() {
        let base = std::env::temp_dir().join(format!("ns-switch-rs-{}", std::process::id()));
        let home = base.join("home");
        let backup = base.join("backups");
        let _ = switch_to_routing_in_dirs(Tool::Claude, 8899, &backup, &home, &ConfigDirs::new()).unwrap();
        // settings 里的 routing 配置经 Store 持久化。
        let dir = base.join("store");
        {
            let mut store = ns_store::Store::open_at(dir.clone()).unwrap();
            let mut rc = store.config().settings.routing_for(Tool::Claude);
            rc.enabled = true;
            rc.port = 8899;
            store.set_routing(Tool::Claude, rc).unwrap();
        }
        let store = ns_store::Store::open_at(dir.clone()).unwrap();
        let rc = store.config().settings.routing_for(Tool::Claude);
        assert!(rc.enabled);
        assert_eq!(rc.port, 8899);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn routing_switch_writes_localhost_placeholder() {
        let base = std::env::temp_dir().join(format!("ns-switch-routing-{}", std::process::id()));
        let home = base.join("home");
        let backup = base.join("backups");
        let outcome = switch_to_routing_in_dirs(Tool::Claude, 8899, &backup, &home, &ConfigDirs::new()).unwrap();
        assert_eq!(outcome.written, 1);
        let text =
            std::fs::read_to_string(home.join(".claude").join("settings.json")).unwrap();
        assert!(text.contains("http://127.0.0.1:8899"));
        assert!(text.contains("ns-switch-routing"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn routing_gemini_writes_settings_and_env() {
        let base = std::env::temp_dir().join(format!("ns-switch-rg-{}", std::process::id()));
        let home = base.join("home");
        let backup = base.join("backups");
        let outcome = switch_to_routing_in_dirs(Tool::Gemini, 8900, &backup, &home, &ConfigDirs::new()).unwrap();
        assert_eq!(outcome.written, 2, "settings.json + .env");
        let settings = std::fs::read_to_string(home.join(".gemini").join("settings.json")).unwrap();
        assert!(settings.contains("http://127.0.0.1:8900"));
        let env = std::fs::read_to_string(home.join(".gemini").join(".env")).unwrap();
        assert!(env.contains("GEMINI_BASE_URL=http://127.0.0.1:8900"));
        assert!(env.contains("GEMINI_API_KEY=ns-switch-routing"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn grok_switch_writes_config_toml() {
        let base = std::env::temp_dir().join(format!("ns-switch-grok-{}", std::process::id()));
        let home = base.join("home");
        let backup = base.join("backups");
        let p = Provider::with_id(
            "grok-88api-test".into(),
            "88API".into(),
            serde_json::json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.88api.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-grok",
                    "ANTHROPIC_MODEL": "grok-4.5",
                }
            }),
        );
        let o = switch_to_in_dirs(Tool::GrokBuild, &p, &backup, &home, &ConfigDirs::new()).unwrap();
        assert_eq!(o.written, 1);
        let text = std::fs::read_to_string(home.join(".grok").join("config.toml")).unwrap();
        assert!(text.contains("[model.grok-88api-test]"), "写入模型段: {text}");
        assert!(text.contains("https://api.88api.com"));
        assert!(text.contains("sk-grok"));
        assert!(text.contains("default = \"grok-88api-test\""));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn grok_routing_switch_writes_localhost() {
        let base = std::env::temp_dir().join(format!("ns-switch-grok-r-{}", std::process::id()));
        let home = base.join("home");
        let backup = base.join("backups");
        let o = switch_to_routing_in_dirs(Tool::GrokBuild, 8903, &backup, &home, &ConfigDirs::new()).unwrap();
        assert_eq!(o.written, 1);
        let text = std::fs::read_to_string(home.join(".grok").join("config.toml")).unwrap();
        assert!(text.contains("http://127.0.0.1:8903"));
        assert!(text.contains("ns-switch-routing"));
        let _ = std::fs::remove_dir_all(&base);
    }
    /// 产品级功能验证：真实预设表（706 条）→ build → 全链路 switch 引擎 →
    /// 临时 HOME 的 live 文件，逐工具核对写入、幂等（noop）与冲突检测。
    /// 只写临时目录，不碰本机真实 live 配置。
    #[test]
    fn every_preset_switches_through_engine() {
        use ns_presets::presets_for;

        let base = std::env::temp_dir().join(format!("ns-switch-all-presets-{}", std::process::id()));
        let home = base.join("home");
        let backup = base.join("backups");
        let dirs = ConfigDirs::new();
        let mut total = 0usize;
        let mut written_files = 0usize;

        // 每个工具取非官方预设（官方无 endpoint/key 不参与 live 写入语义）。
        for tool in Tool::ALL {
            let presets: Vec<_> = presets_for(tool)
                .filter(|p| p.category != ns_presets::ProviderCategory::Official)
                .collect();
            if presets.is_empty() {
                continue;
            }
            // 取该工具第一条预设：构建供应商并真实切换一次。
            let p = presets[0];
            let provider = Provider::with_id(
                format!("{}-fncheck", p.id),
                p.name.to_string(),
                (p.build)("https://api.example.com", "sk-fncheck", "model-fncheck"),
            );
            // ClaudeDesktop 尚无 live 文件约定、Agy 卡只开终端：跳过（设计上不写）。
            let plans = plan_for_tool_in(tool, &provider, &home, &dirs);
            if plans.is_empty() {
                continue;
            }
            // 取该工具第一条预设：构建供应商并真实切换一次。
            let p = presets[0];
            let provider = Provider::with_id(
                format!("{}-fncheck", p.id),
                p.name.to_string(),
                (p.build)("https://api.example.com", "sk-fncheck", "model-fncheck"),
            );
            let outcome = switch_to_in_dirs(tool, &provider, &backup, &home, &dirs)
                .unwrap_or_else(|e| panic!("{} 切换失败: {e}", tool.label()));
            assert!(outcome.written >= 1, "{} 预设 {} 应写入 live 文件", tool.label(), p.name);
            written_files += outcome.written;
            total += 1;

            // 幂等：重复切换 → noop。
            let again = switch_to_in_dirs(tool, &provider, &backup, &home, &dirs).unwrap();
            assert!(again.noop, "{} 重复切换应 noop", tool.label());

            // 切换目标与当前一致 → 无冲突。
            let conflicts = check_conflicts_dirs(tool, &provider, Some(&provider), &home, &dirs);
            assert!(conflicts.is_empty(), "{} 干净切换不应冲突: {:?}", tool.label(), conflicts);
        }

        assert!(total >= 11, "应覆盖所有写 live 的工具（ClaudeDesktop/Agy 除外）: {total}");
        assert!(written_files >= 11, "应写入 ≥11 个 live 文件: {written_files}");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// 产品级功能验证：4 个切换类工具的真实预设逐一写入（Claude/Codex/Gemini/Grok），
    /// 验证关键字段落盘（端点/密钥/模型）且首写备份保留原始文件。
    #[test]
    fn real_presets_switch_tools_write_key_fields() {
        use ns_presets::presets_for;

        let base = std::env::temp_dir().join(format!("ns-switch-real-kf-{}", std::process::id()));
        let home = base.join("home");
        let backup = base.join("backups");
        let dirs = ConfigDirs::new();

        // 预置各工具一份「原始」live 文件，验证首写备份保留。
        for (tool, content) in [
            (Tool::Claude, r#"{"env":{"ANTHROPIC_BASE_URL":"https://orig.example.com"}}"#),
            (Tool::Codex, "model_provider = \"custom\"\nmodel = \"orig\"\n"),
            (Tool::Gemini, r#"{"security":{"auth":{"selectedType":"gemini-api-key"}}}"#),
            (Tool::GrokBuild, "[cli]\ninstaller = \"npm\"\n"),
        ] {
            let path = tool_base_dir(tool, &home, &dirs).join(match tool {
                Tool::Claude | Tool::Gemini => "settings.json",
                Tool::Codex | Tool::GrokBuild => "config.toml",
                _ => unreachable!(),
            });
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, content).unwrap();
        }

        // 每工具取第一条非官方预设切换，核对关键字段。
        for tool in [Tool::Claude, Tool::Codex, Tool::Gemini, Tool::GrokBuild] {
            let p = presets_for(tool)
                .find(|p| p.category != ns_presets::ProviderCategory::Official)
                .expect("非官方预设");
            let provider = Provider::with_id(
                format!("{}-kf", p.id),
                p.name.to_string(),
                (p.build)("https://api.example.com", "sk-kf", "model-kf"),
            );
            let outcome = switch_to_in_dirs(tool, &provider, &backup, &home, &dirs).unwrap();
            // Gemini 写 settings.json + .env 两个文件；其余写 1 个。
            let expected = if tool == Tool::Gemini { 2 } else { 1 };
            assert_eq!(outcome.written, expected, "{} 应写 {expected} 个文件", tool.label());
            let dir = tool_base_dir(tool, &home, &dirs);
            let text = if tool == Tool::Gemini {
                std::fs::read_to_string(dir.join("settings.json")).unwrap()
            } else {
                let f = match tool {
                    Tool::Claude => "settings.json",
                    _ => "config.toml",
                };
                std::fs::read_to_string(dir.join(f)).unwrap()
            };
            // 关键字段必须落盘。
            assert!(text.contains("https://api.example.com"), "{} 端点未落盘:\n{}", tool.label(), text);
            assert!(text.contains("sk-kf"), "{} 密钥未落盘", tool.label());
        }

        // 首写备份保留原始文件。
        let backups: Vec<_> = std::fs::read_dir(&backup).unwrap().collect();
        assert!(!backups.is_empty(), "应有首写备份");
        let _ = std::fs::remove_dir_all(&base);
    }
}
