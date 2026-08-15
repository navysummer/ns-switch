use app_lib::{adapters, commands, config, db, models::Provider};
use std::fs;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct TestHome {
    dir: std::path::PathBuf,
}

impl TestHome {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("ns-switch-test-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&dir).unwrap();
        std::env::set_var("HOME", &dir);
        std::env::set_var("XDG_CONFIG_HOME", dir.join(".config"));
        TestHome { dir }
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.dir).ok();
    }
}

fn run_test<F: FnOnce(&std::path::Path)>(f: F) {
    let _guard = ENV_LOCK.lock().unwrap();
    let home = TestHome::new();
    f(&home.dir);
}

#[test]
fn seed_and_list_presets() {
    run_test(|home| {
        let data = home.join(".ns-switch");
        let state = db::AppState::open(&data).unwrap();

        commands::providers::seed_presets(&state, "claude_code").unwrap();
        let providers = commands::providers::load_providers(&state, "claude_code").unwrap();
        assert!(providers.len() >= 6, "expected presets seeded, got {}", providers.len());

        // seeding twice must not duplicate
        commands::providers::seed_presets(&state, "claude_code").unwrap();
        let providers2 = commands::providers::load_providers(&state, "claude_code").unwrap();
        assert_eq!(providers.len(), providers2.len());
    });
}

#[test]
fn switch_writes_claude_settings() {
    run_test(|home| {
        let data = home.join(".ns-switch");
        let state = db::AppState::open(&data).unwrap();

        let provider = Provider {
            id: 1,
            tool_id: "claude_code".into(),
            name: "Test Relay".into(),
            base_url: "https://relay.example.com".into(),
            api_key: "sk-test-123".into(),
            model: "claude-sonnet-4-5".into(),
            headers: "{}".into(),
            shared_config: r#"{"toolTip": "hi"}"#.into(),
            protocol: String::new(),
            is_preset: false,
            is_active: true,
            is_universal: false,
        };
        config::write_active(&state, &provider).unwrap();

        let cfg = fs::read_to_string(home.join(".claude/settings.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&cfg).unwrap();
        let env = &v["env"];
        assert_eq!(env["ANTHROPIC_BASE_URL"], "https://relay.example.com");
        assert_eq!(env["ANTHROPIC_AUTH_TOKEN"], "sk-test-123");
        assert_eq!(env["ANTHROPIC_MODEL"], "claude-sonnet-4-5");
        assert_eq!(v["toolTip"], "hi");
    });
}

#[test]
fn mcp_sync_writes_everywhere() {
    run_test(|home| {
        let data = home.join(".ns-switch");
        let state = db::AppState::open(&data).unwrap();

        {
            let conn = state.db.lock().unwrap();
            conn.execute(
                "INSERT INTO mcp_servers (name, command, args, env, cwd, transport, is_active, tools)
                 VALUES ('fs-tools', 'npx', '[\"-y\",\"@modelcontextprotocol/server-filesystem\"]', '{}', '/workspace', 'stdio', 1, '[\"claude_code\",\"opencode\"]')",
                [],
            )
            .unwrap();
        }
        config::sync_mcp_for_tool(&state, "claude_code");
        let cfg = fs::read_to_string(home.join(".claude/settings.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&cfg).unwrap();
        assert!(v["mcpServers"]["fs-tools"].is_object());

        config::sync_mcp_for_tool(&state, "opencode");
        let cfg = fs::read_to_string(home.join(".config/opencode/opencode.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&cfg).unwrap();
        assert!(v["mcpServers"]["fs-tools"].is_object());
        fs::remove_dir_all(home).ok();
    });
}

#[test]
fn read_existing_claude_config() {
    run_test(|home| {
        let cfg_dir = home.join(".claude");
        fs::create_dir_all(&cfg_dir).unwrap();
        fs::write(
            cfg_dir.join("settings.json"),
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://x.example.com","ANTHROPIC_AUTH_TOKEN":"sk-old"}}"#,
        )
        .unwrap();

        let (base, key, _) = adapters::read_existing_config("claude_code");
        assert_eq!(base, "https://x.example.com");
        assert_eq!(key, "sk-old");
    });
}

#[test]
fn universal_writes_three_configs() {
    run_test(|home| {
        let data = home.join(".ns-switch");
        let state = db::AppState::open(&data).unwrap();
        let provider = Provider {
            id: 1,
            tool_id: "universal".into(),
            name: "Universal Relay".into(),
            base_url: "https://relay.example.com".into(),
            api_key: "sk-univ".into(),
            model: "claude-sonnet-4-5".into(),
            headers: "{}".into(),
            shared_config: String::new(),
            protocol: String::new(),
            is_preset: false,
            is_active: true,
            is_universal: true,
        };
        config::write_active(&state, &provider).unwrap();

        let claude = fs::read_to_string(home.join(".claude/settings.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&claude).unwrap();
        assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "https://relay.example.com");

        let codex = fs::read_to_string(home.join(".config/.codex/config.toml")).unwrap();
        assert!(codex.contains("model_provider"), "codex toml: {codex}");

        let gemini = fs::read_to_string(home.join(".gemini/settings.json")).unwrap();
        let g: serde_json::Value = serde_json::from_str(&gemini).unwrap();
        assert_eq!(g["env"]["GOOGLE_GENERATIVE_AI_BASE_URL"], "https://relay.example.com");
    });
}

#[test]
fn usage_record_and_overview() {
    run_test(|home| {
        let data = home.join(".ns-switch");
        let state = db::AppState::open(&data).unwrap();
        let db_path = data.join("ns-switch.db");
        app_lib::commands::usage::record_usage_to_db(
            &db_path,
            "claude_code",
            5,
            "Test Relay",
            "claude-sonnet-4-5",
            1000,
            200,
        );
        app_lib::commands::usage::record_usage_to_db(
            &db_path,
            "claude_code",
            5,
            "Test Relay",
            "claude-sonnet-4-5",
            1000,
            200,
        );
        let conn = state.db.lock().unwrap();
        let requests: i64 = conn
            .query_row("SELECT requests FROM usage_logs WHERE provider_id = 5", [], |r| r.get(0))
            .unwrap();
        assert_eq!(requests, 2, "same day should be aggregated");
        let cost: f64 = conn
            .query_row("SELECT COALESCE(SUM(cost_usd),0) FROM usage_logs", [], |r| r.get(0))
            .unwrap();
        assert!(cost > 0.0, "cost should be estimated from pricing");
    });
}