//! 用量定价：内置 models.dev 常见模型价目 + 花费计算。
//!
//! 价格为 USD/百万 token（近似公开价），模型未收录时返回 `None`
//! （调用方显示“未知价”或手动配置）。

/// 一个模型的单价（USD / 1M tokens）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPrice {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    /// 缓存读（cache read）单价；`None` = 按输入价或未知。
    pub cache_read_per_mtok: Option<f64>,
}

impl ModelPrice {
    pub const fn new(input: f64, output: f64, cache_read: Option<f64>) -> Self {
        Self {
            input_per_mtok: input,
            output_per_mtok: output,
            cache_read_per_mtok: cache_read,
        }
    }
}

/// 自定义模型定价（USD / 1M tokens），覆盖内置价格表。
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CustomPrice {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_per_mtok: Option<f64>,
}

impl CustomPrice {
    pub fn new(input_per_mtok: f64, output_per_mtok: f64, cache_read_per_mtok: Option<f64>) -> Self {
        Self {
            input_per_mtok,
            output_per_mtok,
            cache_read_per_mtok,
        }
    }
}

/// 查询模型单价：自定义表优先（精确/前缀匹配），否则内置表。
pub fn model_price_with_custom(
    model: &str,
    custom: &std::collections::BTreeMap<String, CustomPrice>,
) -> Option<ModelPrice> {
    let m = model.to_lowercase();
    // 自定义表：精确匹配优先，再前缀匹配。
    if let Some((_, p)) = custom.iter().find(|(k, _)| k.to_lowercase() == m) {
        return Some(ModelPrice::new(
            p.input_per_mtok,
            p.output_per_mtok,
            p.cache_read_per_mtok,
        ));
    }
    if let Some((_, p)) = custom
        .iter()
        .find(|(k, _)| m.starts_with(&k.to_lowercase()))
    {
        return Some(ModelPrice::new(
            p.input_per_mtok,
            p.output_per_mtok,
            p.cache_read_per_mtok,
        ));
    }
    model_price(model)
}

/// 内置价格表（models.dev 常见模型，USD/1M tokens）。
///
const PRICE_TABLE: &[(&str, ModelPrice)] = &[
    // Anthropic
    ("claude-opus-4-1", ModelPrice::new(15.0, 75.0, Some(1.5))),
    ("claude-opus-4", ModelPrice::new(15.0, 75.0, Some(1.5))),
    ("claude-3-opus", ModelPrice::new(15.0, 75.0, Some(1.5))),
    ("claude-sonnet-4-5", ModelPrice::new(3.0, 15.0, Some(0.3))),
    ("claude-3-7-sonnet", ModelPrice::new(3.0, 15.0, Some(0.3))),
    ("claude-3-5-sonnet", ModelPrice::new(3.0, 15.0, Some(0.3))),
    ("claude-3-5-haiku", ModelPrice::new(0.8, 4.0, Some(0.08))),
    ("claude-haiku", ModelPrice::new(0.8, 4.0, Some(0.08))),
    // OpenAI
    ("gpt-5", ModelPrice::new(1.25, 10.0, None)),
    ("gpt-4.1", ModelPrice::new(2.0, 8.0, None)),
    ("gpt-4o", ModelPrice::new(2.5, 10.0, None)),
    ("gpt-4-turbo", ModelPrice::new(10.0, 30.0, None)),
    ("gpt-4", ModelPrice::new(30.0, 60.0, None)),
    ("o3", ModelPrice::new(2.0, 8.0, None)),
    ("o4-mini", ModelPrice::new(1.1, 4.4, None)),
    // Google
    ("gemini-2.5-pro", ModelPrice::new(1.25, 10.0, Some(0.31))),
    ("gemini-2.5-flash", ModelPrice::new(0.30, 2.50, Some(0.075))),
    ("gemini-2.0-flash", ModelPrice::new(0.10, 0.40, Some(0.025))),
    // Meta / xAI / DeepSeek
    ("llama-3.3", ModelPrice::new(0.25, 0.90, None)),
    ("grok-4", ModelPrice::new(3.0, 15.0, None)),
    ("grok-3", ModelPrice::new(3.0, 15.0, None)),
    ("deepseek-chat", ModelPrice::new(0.27, 1.10, Some(0.07))),
    ("deepseek-reasoner", ModelPrice::new(0.55, 2.19, Some(0.14))),
];

/// 从 models.dev API 导入模型价目，返回 `BTreeMap<model_id, CustomPrice>`。
///
/// 用 `curl` 下载 `https://models.dev/api.json`（约 6MB），解析每个提供商的
/// `models[].cost`（USD/1M tokens）。失败返回错误信息。
pub fn import_models_dev() -> anyhow::Result<std::collections::BTreeMap<String, CustomPrice>> {
    let out = std::process::Command::new("curl")
        .args(["-sS", "-m", "30", "https://models.dev/api.json"])
        .output()
        .map_err(|e| anyhow::anyhow!("curl failed: {e}"))?;
    if !out.status.success() {
        return Err(anyhow::anyhow!(
            "models.dev 下载失败 ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let root: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| anyhow::anyhow!("models.dev 响应解析失败: {e}"))?;
    let mut map = std::collections::BTreeMap::new();
    let obj = root
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("models.dev 响应非对象"))?;
    for provider in obj.values() {
        let Some(models) = provider.get("models").and_then(|m| m.as_object()) else {
            continue;
        };
        for (mid, model) in models {
            let Some(cost) = model.get("cost") else {
                continue;
            };
            let Some(input) = cost.get("input").and_then(|v| v.as_f64()) else {
                continue;
            };
            let output = cost.get("output").and_then(|v| v.as_f64()).unwrap_or(input);
            let cache_read = cost.get("cache_read").and_then(|v| v.as_f64());
            map.insert(
                mid.to_lowercase(),
                CustomPrice::new(input, output, cache_read),
            );
        }
    }
    Ok(map)
}

/// 查询模型单价（前缀匹配，未收录返回 None）。
pub fn model_price(model: &str) -> Option<ModelPrice> {
    let m = model.to_lowercase();
    PRICE_TABLE
        .iter()
        .find(|(key, _)| m.starts_with(*key))
        .map(|(_, p)| *p)
}

/// 计算某模型 token 花费（USD）。无价格返回 None。
///
/// - `input_tokens` / `output_tokens`：常规。
/// - `cache_read_tokens`：缓存读（若单价未知则按输入价计）。
pub fn cost(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
) -> Option<f64> {
    cost_with_custom(model, input_tokens, output_tokens, cache_read_tokens, &Default::default())
}

/// 计算某模型 token 花费（USD），自定义定价表优先。
pub fn cost_with_custom(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    custom: &std::collections::BTreeMap<String, CustomPrice>,
) -> Option<f64> {
    let p = model_price_with_custom(model, custom)?;
    let cache = cache_read_tokens as f64
        / 1_000_000.0
        * p.cache_read_per_mtok.unwrap_or(p.input_per_mtok);
    let input = input_tokens as f64 / 1_000_000.0 * p.input_per_mtok;
    let output = output_tokens as f64 / 1_000_000.0 * p.output_per_mtok;
    Some(input + output + cache)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_lookup_prefix_matches() {
        assert!(model_price("claude-sonnet-4-5").is_some());
        // 带日期后缀命中前缀。
        assert!(model_price("claude-sonnet-4-5-20250929").is_some());
        assert!(model_price("gpt-5").is_some());
        assert!(model_price("unknown-model").is_none());
    }

    #[test]
    fn cost_computes_usd() {
        // claude-sonnet-4-5: $3/1M in, $15/1M out, $0.3/1M cache.
        let c = cost("claude-sonnet-4-5", 1_000_000, 1_000_000, 1_000_000).unwrap();
        assert!((c - 18.3).abs() < 1e-9, "got {c}");
    }

    #[test]
    fn cost_unknown_model_none() {
        assert_eq!(cost("nope-model", 1, 1, 1), None);
    }

    #[test]
    fn cost_zero_tokens_zero() {
        let c = cost("gpt-5", 0, 0, 0).unwrap();
        assert_eq!(c, 0.0);
        assert_eq!(c, 0.0);
    }

    #[test]
    fn import_models_dev_fetches_prices() {
        // 网络可用时下载 models.dev 并解析出价目（模型数 > 100）。
        // 网络不可用时跳过（curl 失败返回 Err）。
        match import_models_dev() {
            Ok(map) => {
                assert!(map.len() > 100, "应解析出大量模型价目，got {}", map.len());
                // 常见模型应存在。
                let has_claude = map.keys().any(|k| k.contains("claude-sonnet"));
                let has_gpt = map.keys().any(|k| k.contains("gpt"));
                assert!(has_claude || has_gpt, "应含 Claude 或 GPT 模型");
            }
            Err(_) => {
                // 网络受限：测试跳过（不失败）。
                eprintln!("models.dev 网络不可用，跳过");
            }
        }
    }
}
