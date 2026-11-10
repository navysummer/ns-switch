//! Gemini 官方订阅配额查询（Cloud Code RPC）。
//!
//! 两步 API 调用（参考 cc-switch `query_gemini_quota`）：
//! 1. `loadCodeAssist` → 获取 `cloudaicompanionProject`（项目 ID）。
//! 2. `retrieveUserQuota` → 获取按模型分桶的配额数据。
//!
//! 需要 Gemini CLI 的 OAuth 访问令牌（`Bearer`）。本模块用 `curl` 子进程
//! 执行（与 ns-usage 脚本执行风格一致，无 HTTP 客户端依赖）。
//!
//! 配额结果按模型分类（Pro / Flash / Flash Lite），每类取最低
//! `remainingFraction`，转换为已用百分比（utilization）。

use crate::Usage;
use std::path::Path;
use std::process::Command;

const LOAD_CODE_ASSIST: &str = "https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist";
const RETRIEVE_USER_QUOTA: &str =
    "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota";

/// Gemini 配额分类（与 cc-switch 对齐）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GeminiTier {
    Pro,
    Flash,
    FlashLite,
    Other,
}

impl GeminiTier {
    pub fn label(&self) -> &'static str {
        match self {
            GeminiTier::Pro => "Gemini Pro",
            GeminiTier::Flash => "Gemini Flash",
            GeminiTier::FlashLite => "Gemini Flash Lite",
            GeminiTier::Other => "Gemini",
        }
    }
}

/// 将 Gemini 模型 ID 分类为 Pro / Flash / Flash Lite。
pub fn classify_gemini_model(model_id: &str) -> GeminiTier {
    if model_id.contains("flash-lite") {
        GeminiTier::FlashLite
    } else if model_id.contains("flash") {
        GeminiTier::Flash
    } else if model_id.contains("pro") {
        GeminiTier::Pro
    } else {
        GeminiTier::Other
    }
}

/// 用 `curl` 发送一个 JSON POST 请求，返回响应体。
fn curl_post(url: &str, bearer: &str, body: &str) -> anyhow::Result<String> {
    let output = Command::new("curl")
        .args([
            "-sS",
            "-m",
            "15",
            "-X",
            "POST",
            url,
            "-H",
            &format!("Authorization: Bearer {bearer}"),
            "-H",
            "Content-Type: application/json",
            "-d",
            body,
        ])
        .output()?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "curl failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// 从 loadCodeAssist 响应中提取项目 ID。
fn extract_project_id(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(obj) => obj
            .get("id")
            .or_else(|| obj.get("projectId"))
            .and_then(|v| v.as_str())
            .map(String::from),
        _ => None,
    }
}


/// 从 Gemini CLI 的 OAuth 令牌文件读取访问令牌。
///
/// Gemini CLI OAuth 凭据位于 `~/.gemini/oauth_credentials.json`（
/// `access_token` 字段，或 `tokens[].access_token`）。
pub fn read_gemini_access_token(home: &Path) -> Option<String> {
    let candidates = [
        home.join(".gemini").join("oauth_credentials.json"),
        home.join(".gemini").join("credentials.json"),
    ];
    for path in candidates {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if let Some(t) = v.get("access_token").and_then(|x| x.as_str()) {
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
        if let Some(arr) = v.get("tokens").and_then(|x| x.as_array()) {
            for tok in arr {
                if let Some(t) = tok.get("access_token").and_then(|x| x.as_str()) {
                    if !t.is_empty() {
                        return Some(t.to_string());
                    }
                }
            }
        }
    }
    None
}

/// 查询 Gemini 官方订阅配额，返回按模型分类的已用百分比摘要。
///
/// `token` 为 Gemini CLI OAuth 访问令牌。失败返回错误信息（配额不可用）。
pub fn query_gemini_quota(token: &str) -> anyhow::Result<Vec<(GeminiTier, f64, Option<String>)>> {
    // Step 1: loadCodeAssist → 获取项目 ID。
    let load_body = serde_json::json!({
        "metadata": {
            "ideType": "GEMINI_CLI",
            "pluginType": "GEMINI"
        }
    });
    let load_raw = curl_post(LOAD_CODE_ASSIST, token, &load_body.to_string())?;
    let load: serde_json::Value = serde_json::from_str(&load_raw)
        .map_err(|e| anyhow::anyhow!("Failed to parse loadCodeAssist response: {e}"))?;

    let project_id = load
        .get("cloudaicompanionProject")
        .and_then(extract_project_id);

    // Step 2: retrieveUserQuota → 获取配额。
    let mut quota_body = serde_json::json!({});
    if let Some(pid) = project_id {
        quota_body["project"] = serde_json::Value::String(pid);
    }
    let quota_raw = curl_post(RETRIEVE_USER_QUOTA, token, &quota_body.to_string())?;
    let quota: serde_json::Value = serde_json::from_str(&quota_raw)
        .map_err(|e| anyhow::anyhow!("Failed to parse quota response: {e}"))?;

    // 按模型分类，每类取最低 remainingFraction。
    let mut category: std::collections::BTreeMap<String, (f64, Option<String>)> =
        std::collections::BTreeMap::new();
    if let Some(buckets) = quota.get("buckets").and_then(|b| b.as_array()) {
        for bucket in buckets {
            let model_id = bucket
                .get("modelId")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let tier = classify_gemini_model(model_id);
            let remaining = bucket
                .get("remainingFraction")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.0)
                .clamp(0.0, 1.0);
            let reset = bucket
                .get("resetTime")
                .and_then(|v| v.as_str())
                .map(String::from);
            let entry = category
                .entry(tier.label().to_string())
                .or_insert((remaining, reset.clone()));
            if remaining < entry.0 {
                entry.0 = remaining;
                if reset.is_some() {
                    entry.1 = reset;
                }
            }
        }
    }

    if category.is_empty() {
        return Err(anyhow::anyhow!("配额响应为空（buckets 缺失）"));
    }

    // 转换为 (tier, utilization%, reset_at)。
    let sort_order = |name: &str| -> usize {
        match name {
            "Gemini Pro" => 0,
            "Gemini Flash" => 1,
            "Gemini Flash Lite" => 2,
            _ => 3,
        }
    };
    let mut out: Vec<(GeminiTier, f64, Option<String>)> = category
        .into_iter()
        .map(|(name, (remaining, reset))| {
            let tier = match name.as_str() {
                "Gemini Pro" => GeminiTier::Pro,
                "Gemini Flash" => GeminiTier::Flash,
                "Gemini Flash Lite" => GeminiTier::FlashLite,
                _ => GeminiTier::Other,
            };
            (tier, (1.0 - remaining) * 100.0, reset)
        })
        .collect();
    out.sort_by_key(|(t, _, _)| match t {
        GeminiTier::Pro => 0,
        GeminiTier::Flash => 1,
        GeminiTier::FlashLite => 2,
        GeminiTier::Other => 3,
    });
    let _ = sort_order;
    Ok(out)
}

/// 便捷：把 Gemini 配额转换为 `Usage`（取 Pro/Flash 中最高已用）。
pub fn gemini_quota_to_usage(
    tiers: &[(GeminiTier, f64, Option<String>)],
) -> Usage {
    let max_util = tiers
        .iter()
        .map(|(_, u, _)| *u)
        .fold(0.0f64, f64::max);
    let reset = tiers
        .iter()
        .find(|(_, u, _)| *u == max_util)
        .and_then(|(_, _, r)| r.clone());
    Usage {
        used: Some(max_util),
        remaining: Some((100.0 - max_util).max(0.0)),
        total: Some(100.0),
        reset_at: reset.as_deref().and_then(parse_rfc3339_secs),
        unit: "%".into(),
        currency: None,
        source: "Gemini 官方配额（Cloud Code RPC）".into(),
    }
}

/// 解析 RFC 3339 时间戳为 Unix 秒。
fn parse_rfc3339_secs(s: &str) -> Option<i64> {
    // 简单解析 YYYY-MM-DDTHH:MM:SS[.frac]Z。
    let s = s.trim_end_matches('Z');
    let mut it = s.split(|c| c == '-' || c == ':' || c == 'T' || c == '.');
    let year: i64 = it.next()?.parse().ok()?;
    let month: i64 = it.next()?.parse().ok()?;
    let day: i64 = it.next()?.parse().ok()?;
    let hour: i64 = it.next()?.parse().ok()?;
    let min: i64 = it.next()?.parse().ok()?;
    let sec: i64 = it.next()?.parse().ok()?;
    // 用 days-from-civil 算法（Howard Hinnant）。
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hour * 3600 + min * 60 + sec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_gemini_models() {
        assert_eq!(classify_gemini_model("gemini-2.5-flash-lite"), GeminiTier::FlashLite);
        assert_eq!(classify_gemini_model("gemini-2.5-flash"), GeminiTier::Flash);
        assert_eq!(classify_gemini_model("gemini-2.5-pro"), GeminiTier::Pro);
        assert_eq!(classify_gemini_model("agnes-3.0"), GeminiTier::Other);
    }

    #[test]
    fn parses_rfc3339() {
        let t = parse_rfc3339_secs("2026-11-10T12:00:00Z").unwrap();
        assert!(t > 0);
        // 2026-11-10 12:00 UTC 应约为 1797... 秒；仅断言为正且接近。
        let expect = 1769_000_000i64;
        assert!((t - expect).abs() < 50_000_000, "t={t}");
    }

    #[test]
    fn quota_to_usage_takes_max() {
        let tiers = vec![
            (GeminiTier::Pro, 30.0, Some("2026-11-10T12:00:00Z".into())),
            (GeminiTier::Flash, 60.0, None),
        ];
        let u = gemini_quota_to_usage(&tiers);
        assert_eq!(u.used, Some(60.0));
        assert_eq!(u.remaining, Some(40.0));
        assert_eq!(u.unit, "%");
        assert!(u.reset_at.is_none());
    }
}
