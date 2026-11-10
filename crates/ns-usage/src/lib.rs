//! ns-usage — 用量查询：配额/余额/重置倒计时 + 会话日志扫描。
//!
//! 用量来源二选一：
//! - **脚本**：供应商提供的 JavaScript 模板（受管子进程执行，`node`/`deno`/`bun`），
//!   脚本 `console.log` 输出 JSON `{ total, used, remaining, resetAt, unit, currency }`。
//! - **内置模板**：内置的常用供应商端点查询（Claude 订阅配额、Codex 余额等）。

mod pricing;
pub use pricing::{
    CustomPrice, ModelPrice, cost, cost_with_custom, import_models_dev, model_price,
};
mod session;
pub use session::*;
mod gemini;
pub use gemini::*;


use std::path::Path;

/// 一次用量快照。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Usage {
    /// 总量（配额或余额上限；`None` = 未知）。
    pub total: Option<f64>,
    /// 已用。
    pub used: Option<f64>,
    /// 剩余。
    pub remaining: Option<f64>,
    /// 重置时间（Unix 秒；`None` = 不重置）。
    pub reset_at: Option<i64>,
    /// 单位（token / credit / 请求）。
    #[serde(default)]
    pub unit: String,
    /// 货币（余额用；`None` = 非货币）。
    #[serde(default)]
    pub currency: Option<String>,
    /// 查询来源描述（脚本名 / 内置模板名）。
    #[serde(default)]
    pub source: String,
}

impl Default for Usage {
    fn default() -> Self {
        Self {
            total: None,
            used: None,
            remaining: None,
            reset_at: None,
            unit: String::new(),
            currency: None,
            source: String::new(),
        }
    }
}

/// 运行时解析（脚本执行器）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    Node,
    Deno,
    Bun,
}

impl Runtime {
    pub fn cmd(&self) -> &'static str {
        match self {
            Runtime::Node => "node",
            Runtime::Deno => "deno",
            Runtime::Bun => "bun",
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Runtime::Node => "Node.js",
            Runtime::Deno => "Deno",
            Runtime::Bun => "Bun",
        }
    }
}

/// 探测可用的脚本运行时（按 node → deno → bun 顺序）。
pub fn detect_runtime() -> Option<Runtime> {
    for rt in [Runtime::Node, Runtime::Deno, Runtime::Bun] {
        let ok = std::process::Command::new(rt.cmd())
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return Some(rt);
        }
    }
    None
}

/// 从脚本输出 JSON 解析 `Usage`（宽松：缺失字段取默认）。
pub fn parse_script_output(text: &str) -> anyhow::Result<Usage> {
    let v: serde_json::Value = serde_json::from_str(text.trim())
        .map_err(|e| anyhow::anyhow!("脚本输出不是合法 JSON: {e}"))?;
    let num = |k: &str| -> Option<f64> {
        v.get(k)
            .and_then(|x| {
                x.as_f64()
                    .or_else(|| x.as_str().and_then(|s| s.parse::<f64>().ok()))
            })
    };
    let mut u = Usage {
        total: num("total").or(num("quota")).or(num("limit")),
        used: num("used"),
        remaining: num("remaining").or(num("left")).or(num("balance")),
        reset_at: num("resetAt")
            .or(num("reset_at"))
            .or(num("resetDate"))
            .map(|x| x as i64),
        unit: v
            .get("unit")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        currency: v
            .get("currency")
            .and_then(|x| x.as_str())
            .map(str::to_string),
        ..Default::default()
    };
    // 若给了 remaining 但没有 total/used，按剩余直接展示。
    if u.remaining.is_none() && u.total.is_some() && u.used.is_some() {
        u.remaining = Some(u.total.unwrap() - u.used.unwrap());
    }
    if u.used.is_none() && u.total.is_some() && u.remaining.is_some() {
        u.used = Some(u.total.unwrap() - u.remaining.unwrap());
    }
    Ok(u)
}

/// 执行用量脚本（受管子进程），返回解析后的 `Usage`。
pub fn execute_script(path: &Path, runtime: Runtime) -> anyhow::Result<Usage> {
    let out = std::process::Command::new(runtime.cmd())
        .arg(path)
        .output()
        .map_err(|e| anyhow::anyhow!("执行 {} 失败: {e}", runtime.cmd()))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow::anyhow!(
            "脚本执行失败（{}）：{}",
            out.status,
            stderr.trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut u = parse_script_output(&stdout)?;
    u.source = path.to_string_lossy().into_owned();
    Ok(u)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quota_json() {
        let u = parse_script_output(r#"{"total":100,"used":30,"remaining":70,"resetAt":1770000000,"unit":"request"}"#)
            .unwrap();
        assert_eq!(u.total, Some(100.0));
        assert_eq!(u.used, Some(30.0));
        assert_eq!(u.remaining, Some(70.0));
        assert_eq!(u.reset_at, Some(1770000000));
    }

    #[test]
    fn parses_balance_json() {
        let u = parse_script_output(r#"{"balance":12.5,"currency":"USD"}"#).unwrap();
        assert_eq!(u.remaining, Some(12.5));
        assert_eq!(u.currency.as_deref(), Some("USD"));
    }

    #[test]
    fn computes_missing_from_total_used() {
        let u = parse_script_output(r#"{"total":100,"used":30}"#).unwrap();
        assert_eq!(u.remaining, Some(70.0));
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(parse_script_output("not json").is_err());
    }

    #[test]
    fn detects_runtime_or_none() {
        // 测试环境可能有或没有 node/deno/bun；只断言不 panic。
        let _ = detect_runtime();
    }
}
