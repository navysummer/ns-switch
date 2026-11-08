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
    let p = model_price(model)?;
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
    }
}
