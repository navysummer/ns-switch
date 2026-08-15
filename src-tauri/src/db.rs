use rusqlite::Connection;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const DEFAULT_SETTINGS: &str = r#"{"theme":"system","language":"zh","dataDir":""}"#;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub data_dir: PathBuf,
    pub data_dir_override: Mutex<Option<PathBuf>>,
}

impl AppState {
    pub fn open(data_dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
        fs::create_dir_all(data_dir.join("backups")).map_err(|e| e.to_string())?;
        fs::create_dir_all(data_dir.join("skills")).map_err(|e| e.to_string())?;
        let db_path = data_dir.join("ns-switch.db");
        let conn = Connection::open(&db_path).map_err(|e| e.to_string())?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| e.to_string())?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|e| e.to_string())?;
        init_schema(&conn).map_err(|e| e.to_string())?;
        seed_tools(&conn).map_err(|e| e.to_string())?;
        Ok(Self {
            db: Mutex::new(conn),
            data_dir: data_dir.to_path_buf(),
            data_dir_override: Mutex::new(None),
        })
    }

    /// effective data dir (custom config dir overrides the default one)
    pub fn active_data_dir(&self) -> PathBuf {
        match self.data_dir_override.lock() {
            Ok(g) => g.clone().unwrap_or_else(|| self.data_dir.clone()),
            Err(_) => self.data_dir.clone(),
        }
    }
}

pub fn default_data_dir() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        home.join(".ns-switch")
    } else {
        PathBuf::from(".ns-switch")
    }
}

fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS tools (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            visible INTEGER NOT NULL DEFAULT 1
        );
        CREATE TABLE IF NOT EXISTS providers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_id TEXT NOT NULL,
            name TEXT NOT NULL,
            base_url TEXT NOT NULL DEFAULT '',
            api_key TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL DEFAULT '',
            headers TEXT NOT NULL DEFAULT '{}',
            shared_config TEXT NOT NULL DEFAULT '',
            protocol TEXT NOT NULL DEFAULT '',
            is_preset INTEGER NOT NULL DEFAULT 0,
            is_active INTEGER NOT NULL DEFAULT 0,
            is_universal INTEGER NOT NULL DEFAULT 0,
            sort INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS mcp_servers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            command TEXT NOT NULL DEFAULT '',
            args TEXT NOT NULL DEFAULT '[]',
            env TEXT NOT NULL DEFAULT '{}',
            cwd TEXT NOT NULL DEFAULT '',
            transport TEXT NOT NULL DEFAULT 'stdio',
            is_active INTEGER NOT NULL DEFAULT 1,
            tools TEXT NOT NULL DEFAULT '[]'
        );
        CREATE TABLE IF NOT EXISTS skills (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            source TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'installed',
            install_mode TEXT NOT NULL DEFAULT 'symlink',
            installed_at TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS skill_sources (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            url TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS prompts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            content TEXT NOT NULL DEFAULT '',
            file_type TEXT NOT NULL DEFAULT 'CLAUDE_MD',
            is_active INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS usage_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_id TEXT NOT NULL DEFAULT '',
            provider_id INTEGER NOT NULL DEFAULT 0,
            provider_name TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL DEFAULT '',
            day TEXT NOT NULL DEFAULT '',
            requests INTEGER NOT NULL DEFAULT 0,
            input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            cost_usd REAL NOT NULL DEFAULT 0,
            UNIQUE(tool_id, provider_id, model, day)
        );
        CREATE TABLE IF NOT EXISTS usage_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts TEXT NOT NULL,
            tool_id TEXT NOT NULL DEFAULT '',
            provider_name TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL DEFAULT '',
            input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            cost_usd REAL NOT NULL DEFAULT 0,
            latency_ms INTEGER NOT NULL DEFAULT 0,
            status INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )?;
    migrate(&conn)
}

/// migrations for existing databases (add columns/ tables added after 0.0.x)
fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let cols = |table: &str| -> Vec<String> {
        let mut v = Vec::new();
        if let Ok(mut stmt) = conn.prepare(&format!("PRAGMA table_info({table})")) {
            let rows = stmt.query_map([], |r| r.get::<_, String>(1));
            if let Ok(rows) = rows {
                for r in rows.flatten() {
                    v.push(r);
                }
            }
        }
        v
    };
    let add = |table: &str, column: &str, ddl: &str| -> rusqlite::Result<()> {
        if !cols(table).iter().any(|c| c == column) {
            conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {ddl};"))?;
        }
        Ok(())
    };

    // Handle legacy 'tool' column -> rename to 'tool_id'
    let provider_cols = cols("providers");
    if provider_cols.iter().any(|c| c == "tool") {
        conn.execute_batch(
            "CREATE TABLE providers_new (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tool_id TEXT NOT NULL DEFAULT 'claude_code',
                name TEXT NOT NULL DEFAULT '',
                base_url TEXT NOT NULL DEFAULT '',
                api_key TEXT NOT NULL DEFAULT '',
                model TEXT NOT NULL DEFAULT '',
                headers TEXT NOT NULL DEFAULT '{}',
                shared_config TEXT NOT NULL DEFAULT '',
                protocol TEXT NOT NULL DEFAULT '',
                is_preset INTEGER NOT NULL DEFAULT 0,
                is_active INTEGER NOT NULL DEFAULT 0,
                is_universal INTEGER NOT NULL DEFAULT 0,
                sort INTEGER NOT NULL DEFAULT 0
            );
            INSERT INTO providers_new (id, tool_id, name, base_url, api_key, model, headers, shared_config, protocol, is_preset, is_active, is_universal, sort)
            SELECT id, COALESCE(tool, tool_id, 'claude_code'), name, base_url, api_key, model, headers, shared_config, protocol, is_preset, is_active, is_universal, sort FROM providers;
            DROP TABLE providers;
            ALTER TABLE providers_new RENAME TO providers;
            ",
        )?;
    } else {
        add("providers", "tool_id", "tool_id TEXT NOT NULL DEFAULT 'claude_code'")?;
    }

    add("providers", "name", "name TEXT NOT NULL DEFAULT ''")?;
    add("providers", "base_url", "base_url TEXT NOT NULL DEFAULT ''")?;
    add("providers", "api_key", "api_key TEXT NOT NULL DEFAULT ''")?;
    add("providers", "model", "model TEXT NOT NULL DEFAULT ''")?;
    add("providers", "headers", "headers TEXT NOT NULL DEFAULT '{}'")?;
    add("providers", "shared_config", "shared_config TEXT NOT NULL DEFAULT ''")?;
    add("providers", "protocol", "protocol TEXT NOT NULL DEFAULT ''")?;
    add("providers", "is_preset", "is_preset INTEGER NOT NULL DEFAULT 0")?;
    add("providers", "is_active", "is_active INTEGER NOT NULL DEFAULT 0")?;
    add("providers", "is_universal", "is_universal INTEGER NOT NULL DEFAULT 0")?;
    add("providers", "sort", "sort INTEGER NOT NULL DEFAULT 0")?;
    add("skills", "install_mode", "install_mode TEXT NOT NULL DEFAULT 'symlink'")?;
    // mcp_servers table columns
    add("mcp_servers", "cwd", "cwd TEXT NOT NULL DEFAULT ''")?;
    add("mcp_servers", "transport", "transport TEXT NOT NULL DEFAULT 'stdio'")?;
    add("mcp_servers", "is_active", "is_active INTEGER NOT NULL DEFAULT 1")?;
    add("mcp_servers", "tools", "tools TEXT NOT NULL DEFAULT '[]'")?;
    // skills table columns
    add("skills", "description", "description TEXT NOT NULL DEFAULT ''")?;
    add("skills", "source", "source TEXT NOT NULL DEFAULT ''")?;
    add("skills", "status", "status TEXT NOT NULL DEFAULT 'installed'")?;
    add("skills", "installed_at", "installed_at TEXT NOT NULL DEFAULT ''")?;
    // prompts table columns
    add("prompts", "content", "content TEXT NOT NULL DEFAULT ''")?;
    add("prompts", "file_type", "file_type TEXT NOT NULL DEFAULT 'CLAUDE_MD'")?;
    add("prompts", "is_active", "is_active INTEGER NOT NULL DEFAULT 0")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS skill_sources (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            url TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS usage_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_id TEXT NOT NULL DEFAULT '',
            provider_id INTEGER NOT NULL DEFAULT 0,
            provider_name TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL DEFAULT '',
            day TEXT NOT NULL DEFAULT '',
            requests INTEGER NOT NULL DEFAULT 0,
            input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            cost_usd REAL NOT NULL DEFAULT 0,
            UNIQUE(tool_id, provider_id, model, day)
        );
        CREATE TABLE IF NOT EXISTS usage_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts TEXT NOT NULL,
            tool_id TEXT NOT NULL DEFAULT '',
            provider_name TEXT NOT NULL DEFAULT '',
            model TEXT NOT NULL DEFAULT '',
            input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            cost_usd REAL NOT NULL DEFAULT 0,
            latency_ms INTEGER NOT NULL DEFAULT 0,
            status INTEGER NOT NULL DEFAULT 0
        );
        ",
    )?;
    Ok(())
}

pub const ALL_TOOLS: &[(&str, &str)] = &[
    ("claude_code", "Claude Code"),
    ("claude_desktop", "Claude Desktop"),
    ("codex", "Codex"),
    ("gemini", "Gemini CLI"),
    ("grok", "Grok Build"),
    ("opencode", "OpenCode"),
    ("openclaw", "OpenClaw"),
    ("hermes", "Hermes Agent"),
];

fn seed_tools(conn: &Connection) -> rusqlite::Result<()> {
    for (id, name) in ALL_TOOLS {
        conn.execute(
            "INSERT OR IGNORE INTO tools (id, name) VALUES (?1, ?2)",
            rusqlite::params![id, name],
        )?;
    }
    Ok(())
}

pub fn get_settings_value(conn: &Connection) -> Value {
    let row: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = 'app'", [], |r| r.get(0))
        .ok();
    match row {
        Some(s) => serde_json::from_str(&s).unwrap_or_else(|_| serde_json::from_str(DEFAULT_SETTINGS).unwrap()),
        None => serde_json::from_str(DEFAULT_SETTINGS).unwrap(),
    }
}

pub fn set_settings_value(conn: &Connection, v: &Value) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('app', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![v.to_string()],
    )?;
    Ok(())
}

pub fn ensure_autobackup(state: &AppState) {
    let now = chrono::Local::now();
    let stamp = now.format("%Y%m%d-%H%M%S").to_string();
    let last = state.active_data_dir().join("backups").join(".last");
    let mut do_backup = true;
    if let Ok(s) = fs::read_to_string(&last) {
        if let Ok(t) = chrono::DateTime::parse_from_str(&s.trim(), "%Y%m%d-%H%M%S") {
            let age = (now - t.with_timezone(&chrono::Local)).num_hours();
            do_backup = age >= 24;
        }
    }
    if !do_backup {
        return;
    }
    let _ = fs::write(&last, &stamp);
    let backup = state.active_data_dir().join("backups").join(format!("ns-switch-{stamp}.db"));
    let src = state.active_data_dir().join("ns-switch.db");
    if src.exists() {
        let _ = fs::copy(&src, &backup);
        rotate_backups(state);
    }
}

pub(crate) fn rotate_backups(state: &AppState) {
    let dir = state.active_data_dir().join("backups");
    if let Ok(entries) = fs::read_dir(&dir) {
        let mut files: Vec<(String, PathBuf)> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "db").unwrap_or(false))
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.strip_prefix("ns-switch-")
                    .map(|_| (name.clone(), e.path()))
            })
            .collect();
        files.sort();
        if files.len() > 10 {
            for (_, p) in files.drain(..files.len() - 10) {
                let _ = fs::remove_file(p);
            }
        }
    }
}