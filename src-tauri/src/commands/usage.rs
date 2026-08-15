use crate::db::AppState;
use crate::models::{UsageLogItem, UsageOverview, UsagePoint};
use crate::AppResult;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::BufRead;
use std::path::Path;

fn default_pricing() -> Value {
    serde_json::from_str::<Value>(
        r#"{
      "claude-opus":            {"input": 15.0,  "output": 75.0},
      "claude-sonnet":          {"input": 3.0,   "output": 15.0},
      "claude-haiku":           {"input": 0.8,   "output": 4.0},
      "gpt-5":                  {"input": 1.25,  "output": 10.0},
      "gpt-4o":                 {"input": 2.5,   "output": 10.0},
      "gpt-4":                  {"input": 2.5,   "output": 10.0},
      "deepseek":               {"input": 0.27,  "output": 1.1},
      "kimi":                   {"input": 4.0,   "output": 16.0},
      "gemini-2.5-pro":         {"input": 1.25,  "output": 10.0},
      "gemini":                 {"input": 1.25,  "output": 10.0},
      "qwen":                   {"input": 0.15,  "output": 0.6},
      "qwen3":                  {"input": 0.4,   "output": 1.2},
      "grok-3":                 {"input": 3.0,   "output": 15.0}
    }"#,
    )
    .unwrap()
}

fn pricing(conn: &rusqlite::Connection) -> Value {
    let row: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = 'usage_pricing'", [], |r| r.get(0))
        .ok();
    match row {
        Some(s) => serde_json::from_str(&s).unwrap_or_else(|_| default_pricing()),
        None => default_pricing(),
    }
}

fn unit_price(pricing: &Value, model: &str) -> (f64, f64) {
    let lower = model.to_lowercase();
    let Some(table) = pricing.as_object() else { return (0.0, 0.0) };
    let mut best_input = 0.0f64;
    let mut best_output = 0.0f64;
    let mut best_len = 0usize;
    for (key, val) in table {
        if lower.contains(&key.to_lowercase()) && key.len() > best_len {
            if let Some(o) = val.as_object() {
                if let (Some(i), Some(o2)) = (o.get("input").and_then(|x| x.as_f64()), o.get("output").and_then(|x| x.as_f64())) {
                    best_input = i;
                    best_output = o2;
                    best_len = key.len();
                }
            }
        }
    }
    (best_input, best_output)
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

pub fn estimate_cost(pricing: &Value, model: &str, input_tokens: i64, output_tokens: i64) -> f64 {
    let (i, o) = unit_price(pricing, model);
    input_tokens as f64 / 1_000_000.0 * i + output_tokens as f64 / 1_000_000.0 * o
}

/// per-request detail log (append-only, pruned to the latest 1000)
pub fn record_usage_event_to_db(
    db_path: &Path,
    tool_id: &str,
    provider_name: &str,
    model: &str,
    input_tokens: i64,
    output_tokens: i64,
    latency_ms: u64,
    status: u16,
) {
    let Ok(conn) = rusqlite::Connection::open(db_path) else { return };
    let pricing = pricing(&conn);
    let cost = estimate_cost(&pricing, model, input_tokens, output_tokens);
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let _ = conn.execute(
        "INSERT INTO usage_events (ts, tool_id, provider_name, model, input_tokens, output_tokens, cost_usd, latency_ms, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            ts,
            tool_id,
            provider_name,
            if model.is_empty() { "unknown" } else { model },
            input_tokens,
            output_tokens,
            cost,
            latency_ms as i64,
            status as i64
        ],
    );
    // prune to ~1000 rows
    let _ = conn.execute(
        "DELETE FROM usage_events WHERE id NOT IN (SELECT id FROM usage_events ORDER BY id DESC LIMIT 1000)",
        [],
    );
}

#[tauri::command]
pub fn usage_events(state: tauri::State<AppState>, limit: Option<i64>) -> AppResult<Vec<Value>> {
    let limit = limit.unwrap_or(100).max(1).min(1000);
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut stmt = conn.prepare(
        "SELECT id, ts, tool_id, provider_name, model, input_tokens, output_tokens, cost_usd, latency_ms, status
         FROM usage_events ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(rusqlite::params![limit], |r| {
        Ok(json!({
            "id": r.get::<_, i64>(0)?,
            "ts": r.get::<_, String>(1)?,
            "toolId": r.get::<_, String>(2)?,
            "providerName": r.get::<_, String>(3)?,
            "model": r.get::<_, String>(4)?,
            "inputTokens": r.get::<_, i64>(5)?,
            "outputTokens": r.get::<_, i64>(6)?,
            "costUsd": r.get::<_, f64>(7)?,
            "latencyMs": r.get::<_, i64>(8)?,
            "status": r.get::<_, i64>(9)?,
        }))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// upsert an aggregated usage record (provider/model/day). db_path points at the
/// sqlite file so it can be called from the proxy worker thread.
pub fn record_usage_to_db(
    db_path: &Path,
    tool_id: &str,
    provider_id: i64,
    provider_name: &str,
    model: &str,
    input_tokens: i64,
    output_tokens: i64,
) {
    let Ok(conn) = rusqlite::Connection::open(db_path) else { return };
    let pricing = pricing(&conn);
    let cost = estimate_cost(&pricing, model, input_tokens, output_tokens);
    let day = today();
    let _ = conn.execute(
        "INSERT INTO usage_logs (tool_id, provider_id, provider_name, model, day, requests, input_tokens, output_tokens, cost_usd)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, ?8)
         ON CONFLICT(tool_id, provider_id, model, day) DO UPDATE SET
           requests = requests + 1,
           input_tokens = input_tokens + excluded.input_tokens,
           output_tokens = output_tokens + excluded.output_tokens,
           cost_usd = cost_usd + excluded.cost_usd,
           provider_name = excluded.provider_name",
        rusqlite::params![
            tool_id,
            provider_id,
            provider_name,
            if model.is_empty() { "unknown" } else { model },
            day,
            input_tokens,
            output_tokens,
            cost
        ],
    );
}

fn row_to_log(row: &rusqlite::Row) -> rusqlite::Result<UsageLogItem> {
    Ok(UsageLogItem {
        id: row.get(0)?,
        tool_id: row.get(1)?,
        provider_id: row.get(2)?,
        provider_name: row.get(3)?,
        model: row.get(4)?,
        day: row.get(5)?,
        requests: row.get(6)?,
        input_tokens: row.get(7)?,
        output_tokens: row.get(8)?,
        cost_usd: row.get(9)?,
    })
}

#[tauri::command]
pub fn usage_overview(state: tauri::State<AppState>) -> AppResult<UsageOverview> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let today = today();
    Ok(UsageOverview {
        total_cost: conn
            .query_row("SELECT COALESCE(SUM(cost_usd),0) FROM usage_logs", [], |r| r.get(0))
            .unwrap_or(0.0),
        today_cost: conn
            .query_row(
                "SELECT COALESCE(SUM(cost_usd),0) FROM usage_logs WHERE day = ?1",
                rusqlite::params![today],
                |r| r.get(0),
            )
            .unwrap_or(0.0),
        today_requests: conn
            .query_row(
                "SELECT COALESCE(SUM(requests),0) FROM usage_logs WHERE day = ?1",
                rusqlite::params![today],
                |r| r.get(0),
            )
            .unwrap_or(0),
        total_requests: conn
            .query_row("SELECT COALESCE(SUM(requests),0) FROM usage_logs", [], |r| r.get(0))
            .unwrap_or(0),
        models: conn
            .query_row("SELECT COUNT(DISTINCT model) FROM usage_logs", [], |r| r.get(0))
            .unwrap_or(0),
    })
}

#[tauri::command]
pub fn usage_by_day(state: tauri::State<AppState>, days: Option<i64>) -> AppResult<Vec<UsagePoint>> {
    let days = days.unwrap_or(30).max(1).min(365);
    let start = (chrono::Local::now() - chrono::Duration::days(days - 1))
        .format("%Y-%m-%d")
        .to_string();
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut stmt = conn.prepare(
        "SELECT day, COALESCE(SUM(cost_usd),0), COALESCE(SUM(requests),0),
                COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0)
         FROM usage_logs WHERE day >= ?1 GROUP BY day ORDER BY day",
    )?;
    let rows = stmt.query_map(rusqlite::params![start], |r| {
        Ok(UsagePoint {
            day: r.get(0)?,
            cost: r.get(1)?,
            requests: r.get(2)?,
            input_tokens: r.get(3)?,
            output_tokens: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[tauri::command]
pub fn usage_logs(state: tauri::State<AppState>, limit: Option<i64>) -> AppResult<Vec<UsageLogItem>> {
    let limit = limit.unwrap_or(100).max(1);
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut stmt = conn.prepare(
        "SELECT id, tool_id, provider_id, provider_name, model, day, requests, input_tokens, output_tokens, cost_usd
         FROM usage_logs ORDER BY day DESC, id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(rusqlite::params![limit], row_to_log)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[tauri::command]
pub fn usage_pricing_get(state: tauri::State<AppState>) -> AppResult<String> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    Ok(serde_json::to_string_pretty(&pricing(&conn)).map_err(|e| e.to_string())?)
}

#[tauri::command]
pub fn usage_pricing_set(state: tauri::State<AppState>, data: String) -> AppResult<()> {
    let v: Value = serde_json::from_str(&data).map_err(|_| "invalid pricing JSON")?;
    if !v.is_object() {
        return Err("pricing must be an object of {model: {input, output}}".into());
    }
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('usage_pricing', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![v.to_string()],
    )?;
    Ok(())
}

#[tauri::command]
pub fn usage_reset(state: tauri::State<AppState>) -> AppResult<()> {
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    conn.execute("DELETE FROM usage_logs", [])?;
    Ok(())
}

#[tauri::command]
pub fn usage_scan_sessions(state: tauri::State<AppState>) -> AppResult<u32> {
    let db_path = state.active_data_dir().join("ns-switch.db");
    let mut total = 0u32;
    total += scan_claude(&db_path);
    total += scan_codex(&db_path);
    Ok(total)
}

fn usage_tokens(v: &Value) -> (i64, i64) {
    let u = v.get("usage").or_else(|| v.get("message").and_then(|m| m.get("usage")));
    let Some(u) = u else { return (0, 0) };
    let inp = u
        .get("input_tokens")
        .or_else(|| u.get("prompt_tokens"))
        .and_then(|x| x.as_i64())
        .unwrap_or(0);
    let out = u
        .get("output_tokens")
        .or_else(|| u.get("completion_tokens"))
        .and_then(|x| x.as_i64())
        .unwrap_or(0);
    (inp, out)
}

fn scan_claude(db_path: &std::path::Path) -> u32 {
    let dir = match dirs::home_dir() {
        Some(h) => h.join(".claude").join("projects"),
        None => return 0,
    };
    let mut processed = 0u32;
    if !dir.is_dir() {
        return 0;
    }
    let Ok(proj_dirs) = std::fs::read_dir(&dir) else { return 0 };
    for proj in proj_dirs.flatten() {
        if !proj.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let files: Vec<_> = std::fs::read_dir(proj.path())
            .map(|it| {
                it.flatten()
                    .filter(|e| e.path().extension().map(|x| x == "jsonl").unwrap_or(false))
                    .map(|e| e.path())
                    .collect()
            })
            .unwrap_or_default();
        for file in files {
            if let Ok(f) = std::fs::File::open(&file) {
                let reader = std::io::BufReader::new(f);
                let mut last_input = 0i64;
                let mut last_output = 0i64;
                for line in reader.lines().flatten() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                    let (i, o) = usage_tokens(&v);
                    if i > 0 || o > 0 {
                        last_input = i;
                        last_output = o;
                    }
                    if v.get("costUSD").and_then(|c| c.as_f64()).is_some() {
                        record_usage_to_db(
                            db_path,
                            "claude_code",
                            0,
                            "imported",
                            v.get("model").and_then(|m| m.as_str()).unwrap_or("unknown"),
                            last_input,
                            last_output,
                        );
                        processed += 1;
                    }
                }
            }
        }
    }
    processed
}

fn scan_codex(db_path: &std::path::Path) -> u32 {
    let Some(home) = dirs::home_dir() else { return 0 };
    let dir = home.join(".codex").join("sessions");
    if !dir.is_dir() {
        return 0;
    }
    let mut processed = 0u32;
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().map(|x| x == "jsonl").unwrap_or(false) {
            if let Ok(f) = std::fs::File::open(&path) {
                let reader = std::io::BufReader::new(f);
                for line in reader.lines().flatten() {
                    if let Ok(v) = serde_json::from_str::<Value>(&line) {
                        let (i, o) = usage_tokens(&v);
                        if i > 0 || o > 0 {
                            record_usage_to_db(
                                db_path,
                                "codex",
                                0,
                                "imported",
                                v.get("model").and_then(|m| m.as_str()).unwrap_or("unknown"),
                                i,
                                o,
                            );
                            processed += 1;
                        }
                    }
                }
            }
        }
    }
    processed
}

/// per-model breakdown for the dashboard
#[tauri::command]
pub fn usage_by_model(state: tauri::State<AppState>, limit: Option<i64>) -> AppResult<Vec<Value>> {
    let limit = limit.unwrap_or(20).max(1);
    let conn = state.db.lock().map_err(|_| "db lock poisoned")?;
    let mut stmt = conn.prepare(
        "SELECT COALESCE(model,'unknown') as m,
                COALESCE(SUM(requests),0) as req,
                COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0),
                COALESCE(SUM(cost_usd),0)
         FROM usage_logs GROUP BY m ORDER BY cost_usd DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(rusqlite::params![limit], |r| {
        Ok(json!({
            "model": r.get::<_, String>(0)?,
            "requests": r.get::<_, i64>(1)?,
            "inputTokens": r.get::<_, i64>(2)?,
            "outputTokens": r.get::<_, i64>(3)?,
            "cost": r.get::<_, f64>(4)?,
        }))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn merge_pricing_defaults(_h: &mut HashMap<String, String>) {}

#[allow(unused)]
fn _unused_import(_: &Map<String, Value>) {}