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

/// 客户端 live 配置文件目录（`~`）。
fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// 指定家目录的 live 计划（测试可用临时目录）。
pub fn plan_for_tool_in(tool: Tool, provider: &Provider, home: &std::path::Path) -> Vec<LivePlan> {
    let cfg = &provider.settings_config;
    match tool {
        Tool::Claude => vec![LivePlan {
            file: LiveFile::new("claude", home.join(".claude").join("settings.json"), true),
            patch: Patch::Json(claude_patch(cfg)),
        }],
        Tool::Codex => vec![LivePlan {
            file: LiveFile::new("codex", home.join(".codex").join("config.toml"), true),
            patch: Patch::Toml(codex_patch(cfg)),
        }],
        Tool::Gemini => {
            let mut plans = vec![LivePlan {
                file: LiveFile::new("gemini", home.join(".gemini").join("settings.json"), true),
                patch: Patch::Json(gemini_settings_patch(cfg)),
            }];
            let entries = gemini_env_entries(cfg);
            if !entries.is_empty() {
                plans.push(LivePlan {
                    file: LiveFile::new("gemini", home.join(".gemini").join(".env"), true),
                    patch: Patch::Env(EnvPatch {
                        remove: vec![],
                        set: entries,
                    }),
                });
            }
            plans
        }
        // 共存式工具（additive）：把供应商 upsert 进工具自身配置。
        Tool::Opencode => vec![LivePlan {
            file: LiveFile::new("opencode", home.join(".config").join("opencode").join("opencode.json"), true),
            patch: Patch::Json(ns_live::opencode_patch(&provider.id, cfg)),
        }],
        Tool::Openclaw => vec![LivePlan {
            file: LiveFile::new("openclaw", home.join(".openclaw").join("openclaw.json"), true),
            patch: Patch::Json(ns_live::openclaw_patch(&provider.id, cfg)),
        }],
        Tool::Pi => vec![LivePlan {
            file: LiveFile::new("pi", home.join(".pi").join("agent").join("models.json"), true),
            patch: Patch::Json(ns_live::pi_patch(&provider.id, cfg)),
        }],
        Tool::Hermes => vec![LivePlan {
            file: LiveFile::new("hermes", home.join(".hermes").join("config.yaml"), true),
            patch: Patch::Yaml(ns_live::hermes_patch(&provider.id, cfg)),
        }],
        Tool::Mcode => vec![LivePlan {
            file: LiveFile::new("mcode", home.join(".minimax").join("config.yaml"), true),
            patch: Patch::Yaml(ns_live::mcode_patch(&provider.id, cfg)),
        }],
        Tool::Kimi => vec![LivePlan {
            file: LiveFile::new("kimi", home.join(".kimi-code").join("config.toml"), true),
            patch: Patch::Toml(ns_live::kimi_patch(&provider.id, cfg)),
        }],
        Tool::Omp => vec![LivePlan {
            file: LiveFile::new("omp", home.join(".omp").join("agent").join("models.yml"), true),
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
) -> Vec<LivePlan> {
    use ns_live::{
        claude_routing_patch, codex_routing_patch, gemini_routing_env_patch,
        gemini_routing_settings_patch,
    };
    match tool {
        Tool::Claude => vec![LivePlan {
            file: LiveFile::new("claude", home.join(".claude").join("settings.json"), true),
            patch: Patch::Json(claude_routing_patch(port)),
        }],
        Tool::Codex => vec![LivePlan {
            file: LiveFile::new("codex", home.join(".codex").join("config.toml"), true),
            patch: Patch::Toml(codex_routing_patch(port)),
        }],
        Tool::Gemini => vec![
            LivePlan {
                file: LiveFile::new("gemini", home.join(".gemini").join("settings.json"), true),
                patch: Patch::Json(gemini_routing_settings_patch(port)),
            },
            LivePlan {
                file: LiveFile::new("gemini", home.join(".gemini").join(".env"), true),
                patch: Patch::Env(gemini_routing_env_patch(port)),
            },
        ],
        // GrokBuild 路由：M5 后续子任务接入（当前仅记录开关）。
        Tool::GrokBuild => Vec::new(),
        _ => Vec::new(),
    }
}

/// 写入路由模式 live 配置（指向本地代理）。返回写入文件数。
pub fn switch_to_routing_in(
    tool: Tool,
    port: u16,
    backup_dir: &std::path::Path,
    home: &std::path::Path,
) -> Result<SwitchOutcome, LiveWriteError> {
    let plans = routing_plans_for_tool_in(tool, port, home);
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
pub fn check_conflicts(
    tool: Tool,
    target: &Provider,
    current_provider: Option<&Provider>,
    home: &std::path::Path,
) -> Vec<Conflict> {
    let mut conflicts = Vec::new();
    let target_plans = plan_for_tool_in(tool, target, home);
    // 当前激活供应商的预期内容（用于「磁盘是否等于 B 的写入」判断）。
    let current_plans = current_provider.map(|p| plan_for_tool_in(tool, p, home));
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
pub fn switch_to(
    tool: Tool,
    provider: &Provider,
    backup_dir: &std::path::Path,
) -> Result<SwitchOutcome, LiveWriteError> {
    switch_to_in(tool, provider, backup_dir, &home())
}

/// 指定家目录的切换（测试用临时目录）。
pub fn switch_to_in(
    tool: Tool,
    provider: &Provider,
    backup_dir: &std::path::Path,
    home: &std::path::Path,
) -> Result<SwitchOutcome, LiveWriteError> {
    let plans = plan_for_tool_in(tool, provider, home);
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
        let plans = plan_for_tool_in(Tool::Claude, &claude_provider(), &home);
        assert_eq!(plans.len(), 1);
        assert!(plans[0].file.path.ends_with("settings.json"));
        assert_eq!(plans[0].file.app, "claude");
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
        let plans = plan_for_tool_in(Tool::Gemini, &provider, &home);
        assert_eq!(plans.len(), 2, "settings.json + .env");
        assert!(plans[0].file.path.ends_with("settings.json"));
        assert!(plans[1].file.path.ends_with(".env"));
    }

    #[test]
    fn switch_to_writes_claude_settings() {
        let base = std::env::temp_dir().join(format!("ns-switch-switch-{}", std::process::id()));
        let home = base.join("home");
        let backup = base.join("backups");
        let outcome = switch_to_in(Tool::Claude, &claude_provider(), &backup, &home).unwrap();
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
        let o1 = switch_to_in(Tool::Claude, &b, &backup, &home).unwrap();
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
        let o2 = switch_to_in(Tool::Claude, &a, &backup, &home).unwrap();
        assert_eq!(o2.written, 1);
        let text2 = std::fs::read_to_string(&path).unwrap();
        assert!(text2.contains("other.example.com"));
        assert!(text2.contains("sk-other"));
        assert!(!text2.contains("api.88api.com"), "旧供应商地址应被清掉");

        // 重复切到 A → noop（不重写）。
        let o3 = switch_to_in(Tool::Claude, &a, &backup, &home).unwrap();
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
        switch_to_in(Tool::Claude, &b, &backup, &home).unwrap();
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
        let conflicts = check_conflicts(Tool::Claude, &a, Some(&b), &home);
        assert!(conflicts.is_empty(), "干净切换不应有冲突: {:?}", conflicts);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn manual_edit_detected_as_conflict() {
        let base = std::env::temp_dir().join(format!("ns-switch-conflict2-{}", std::process::id()));
        let home = base.join("home");
        let backup = base.join("backups");
        let b = claude_provider();
        switch_to_in(Tool::Claude, &b, &backup, &home).unwrap();
        // 手动改掉磁盘（模拟外部修改）。
        let settings = home.join(".claude").join("settings.json");
        std::fs::write(&settings, r#"{"env":{"ANTHROPIC_BASE_URL":"https://manual.example.com"}}"#)
            .unwrap();
        let a = claude_provider();
        let conflicts = check_conflicts(Tool::Claude, &a, Some(&b), &home);
        assert_eq!(conflicts.len(), 1, "手动改动应报冲突");
        assert!(conflicts[0].path.ends_with("settings.json"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn no_write_tools_have_no_plans() {
        let home = std::env::temp_dir().join("ns-switch-plan-none");
        let provider = claude_provider();
        // Agy 卡只开终端，不写 live 文件。
        assert!(plan_for_tool_in(Tool::Agy, &provider, &home).is_empty());
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
        let plans = plan_for_tool_in(Tool::Opencode, &provider, &home);
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
        let o = switch_to_in(Tool::Opencode, &oc, &backup, &home).unwrap();
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
        let o = switch_to_in(Tool::Openclaw, &ocw, &backup, &home).unwrap();
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
        let o = switch_to_in(Tool::Hermes, &h, &backup, &home).unwrap();
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
        let o = switch_to_in(Tool::Kimi, &k, &backup, &home).unwrap();
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
        let o = switch_to_in(Tool::Omp, &om, &backup, &home).unwrap();
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
        let o = switch_to_in(Tool::Pi, &pi, &backup, &home).unwrap();
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
        let o = switch_to_in(Tool::Mcode, &m, &backup, &home).unwrap();
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
        let _ = switch_to_routing_in(Tool::Claude, 8899, &backup, &home).unwrap();
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
        let outcome = switch_to_routing_in(Tool::Claude, 8899, &backup, &home).unwrap();
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
        let outcome = switch_to_routing_in(Tool::Gemini, 8900, &backup, &home).unwrap();
        assert_eq!(outcome.written, 2, "settings.json + .env");
        let settings = std::fs::read_to_string(home.join(".gemini").join("settings.json")).unwrap();
        assert!(settings.contains("http://127.0.0.1:8900"));
        let env = std::fs::read_to_string(home.join(".gemini").join(".env")).unwrap();
        assert!(env.contains("GEMINI_BASE_URL=http://127.0.0.1:8900"));
        assert!(env.contains("GEMINI_API_KEY=ns-switch-routing"));
        let _ = std::fs::remove_dir_all(&base);
    }
}

