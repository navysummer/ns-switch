//! M5 本地路由运行时：每工具一个本地代理，管理启动/停止、上游提取。
//!
//! 路由模式开启时：
//! 1. 从当前供应商 `settings_config` 提取真实上游（base_url + key）。
use std::collections::BTreeMap;

use ns_presets::{Provider, Tool};
use ns_proxy::Upstream;

/// 整流图像策略：u32 → `ImageMode`（0=保留 / 1=丢弃 / 2=文本占位）。
pub fn image_mode_from_u32(v: u32) -> ns_proxy::rectify::ImageMode {
    match v {
        1 => ns_proxy::rectify::ImageMode::Drop,
        2 => ns_proxy::rectify::ImageMode::TextPlaceholder,
        _ => ns_proxy::rectify::ImageMode::Keep,
    }
}


/// 运行中的本地代理。
pub struct RunningProxy {
    pub port: u16,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    /// 共享熔断器表（读状态展示；与 ProxyConfig 共享）。
    pub breakers: std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<usize, ns_proxy::CircuitBreaker>>,
    >,
}

/// 每工具代理运行时。
#[derive(Default)]
pub struct RoutingRuntime {
    /// 共享请求日志（所有代理写入，UI 读取）。
    pub request_log: std::sync::Arc<ns_proxy::RequestLog>,
    proxies: BTreeMap<Tool, RunningProxy>,
}

impl RoutingRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_running(&self, tool: Tool) -> bool {
        self.proxies.contains_key(&tool)
    }

    pub fn port_of(&self, tool: Tool) -> Option<u16> {
        self.proxies.get(&tool).map(|p| p.port)
    }

    /// 启动某工具代理（聚合：多上游 + 按模型路由）。
    ///
    /// 返回实际监听端口。调用方需处于 tokio 运行时内。

    pub async fn ensure_running_with(
        &mut self,
        tool: Tool,
        upstreams: Vec<Upstream>,
        model_routes: Option<std::collections::HashMap<String, usize>>,
        listen_addr: &str,
        failover_enabled: bool,
        breaker_threshold: u32,
        breaker_cooldown_secs: u32,
        rectify_enabled: bool,
        rectify_thinking: bool,
        rectify_image: u32,
    ) -> anyhow::Result<u16> {
        if let Some(p) = self.proxies.get_mut(&tool) {
            return Ok(p.port);
        }
        let log = self.request_log.clone();
        let tool_id = tool.id_str().to_string();
        let config = match model_routes {
            Some(routes) => {
                ns_proxy::ProxyConfig::with_queue(upstreams)
                    .with_model_routes(routes)
                    .with_failover(failover_enabled)
                    .with_log(log, tool_id)
                    .with_breaker(breaker_threshold, breaker_cooldown_secs)
            }
            None => {
                ns_proxy::ProxyConfig::with_queue(upstreams)
                    .with_failover(failover_enabled)
                    .with_log(log, tool_id)
                    .with_breaker(breaker_threshold, breaker_cooldown_secs)
            }
        };
        // 整流器：enabled=false 时等效不整流（supports_thinking=true + Keep）。
        let (rect_supports, rect_image) = if rectify_enabled {
            (rectify_thinking, image_mode_from_u32(rectify_image))
        } else {
            (true, ns_proxy::rectify::ImageMode::Keep)
        };
        let config = config.with_rectify(rect_supports, rect_image);
        let breakers = config.breakers.clone();
        let bind_addr = format!("{listen_addr}:0");
        let (listener, addr) = ns_proxy::bind(bind_addr.parse()?).await?;
        let port = addr.port();
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(ns_proxy::serve_listener(listener, config, Some(rx)));
        self.proxies.insert(
            tool,
            RunningProxy {
                port,
                shutdown: Some(tx),
                breakers,
            },
        );
        Ok(port)
    }

    /// 某工具各上游的熔断状态（index → state）。未运行时返回空。
    pub fn breaker_states(&self, tool: Tool) -> Vec<(usize, ns_proxy::CircuitState)> {
        let Some(p) = self.proxies.get(&tool) else {
            return Vec::new();
        };
        let mut states: Vec<(usize, ns_proxy::CircuitState)> = p
            .breakers
            .lock()
            .unwrap()
            .iter()
            .map(|(i, b)| (*i, b.state()))
            .collect();
        states.sort_by_key(|(i, _)| *i);
        states
    }

    /// 停止某工具代理。
    pub fn stop(&mut self, tool: Tool) {
        if let Some(p) = self.proxies.remove(&tool) {
            if let Some(tx) = p.shutdown {
                let _ = tx.send(());
            }
        }
    }

    /// 停止所有代理（退出时调用）。
    pub fn stop_all(&mut self) {
        for tool in self.proxies.keys().copied().collect::<Vec<_>>() {
            self.stop(tool);
        }
    }
}

/// 从供应商配置提取真实上游（base_url + key + api_format）。
pub fn provider_upstream(tool: Tool, provider: &Provider) -> Option<Upstream> {
    let cfg = &provider.settings_config;
    let fmt = provider
        .meta
        .as_ref()
        .and_then(|m| m.api_format.clone())
        .unwrap_or_else(|| "anthropic".to_string());
    match tool {
        Tool::Claude => {
            let env = cfg.get("env")?;
            let base = env.get("ANTHROPIC_BASE_URL")?.as_str()?;
            let key = env.get("ANTHROPIC_AUTH_TOKEN").and_then(|v| v.as_str()).unwrap_or("");
            Some(Upstream::new(base.to_string(), key.to_string()).with_format(fmt))
        }
        Tool::Codex => {
            let config = cfg.get("config")?;
            let providers = config.get("model_providers")?.as_object()?;
            // 取第一个供应商的 base_url。
            let (_, v) = providers.iter().next()?;
            let base = v.get("base_url")?.as_str()?;
            let key = cfg
                .get("auth")
                .and_then(|a| a.get("OPENAI_API_KEY"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            Some(Upstream::new(base.to_string(), key.to_string()).with_format(fmt))
        }
        Tool::Gemini => {
            let env = cfg.get("env")?;
            let base = env.get("GEMINI_BASE_URL")?.as_str()?;
            let key = env.get("GEMINI_API_KEY").and_then(|v| v.as_str()).unwrap_or("");
            Some(Upstream::new(base.to_string(), key.to_string()).with_format(fmt))
        }
        Tool::GrokBuild => {
            let env = cfg.get("env")?;
            let base = env.get("ANTHROPIC_BASE_URL")?.as_str()?;
            let key = env.get("ANTHROPIC_AUTH_TOKEN").and_then(|v| v.as_str()).unwrap_or("");
            Some(Upstream::new(base.to_string(), key.to_string()).with_format(fmt))
        }
        _ => None,
    }
}

/// 聚合：提取多个供应商的上游（每供应商一个）。
pub fn provider_upstreams(tool: Tool, providers: &[&Provider]) -> Vec<Upstream> {
    providers
        .iter()
        .filter_map(|p| provider_upstream(tool, p))
        .collect()
}

/// 聚合：模型 → upstream 索引映射（每供应商的可见模型指向其索引）。
pub fn aggregation_model_routes(providers: &[&Provider]) -> std::collections::HashMap<String, usize> {
    let mut routes = std::collections::HashMap::new();
    for (i, p) in providers.iter().enumerate() {
        for model in ns_presets::provider_models(p) {
            routes.entry(model).or_insert(i);
        }
    }
    routes
}



#[cfg(test)]
mod tests {
    use super::*;

    fn claude_provider() -> Provider {
        Provider::with_id(
            "claude-88api-test".into(),
            "88api".into(),
            serde_json::json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.88api.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-test",
                }
            }),
        )
    }

    #[test]
    fn claude_upstream_extraction() {
        let u = provider_upstream(Tool::Claude, &claude_provider()).unwrap();
        assert_eq!(u.base_url, "https://api.88api.com");
        assert_eq!(u.api_key, "sk-test");
    }

    #[test]
    fn codex_upstream_extraction() {
        let p = Provider::with_id(
            "codex-test".into(),
            "custom".into(),
            serde_json::json!({
                "auth": {"OPENAI_API_KEY": "sk-codex"},
                "config": {
                    "model_provider": "custom",
                    "model_providers": {
                        "custom": {"base_url": "https://codex.example.com", "wire_api": "chat"}
                    }
                }
            }),
        );
        let u = provider_upstream(Tool::Codex, &p).unwrap();
        assert_eq!(u.base_url, "https://codex.example.com");
        assert_eq!(u.api_key, "sk-codex");
    }

    #[test]
    fn gemini_upstream_extraction() {
        let p = Provider::with_id(
            "gemini-test".into(),
            "ppio".into(),
            serde_json::json!({
                "env": {"GEMINI_BASE_URL": "https://ppio.example.com", "GEMINI_API_KEY": "sk-g"}
            }),
        );
        let u = provider_upstream(Tool::Gemini, &p).unwrap();
        assert_eq!(u.base_url, "https://ppio.example.com");
        assert_eq!(u.api_key, "sk-g");
    }

    #[test]
    fn grok_upstream_extraction() {
        let p = Provider::with_id(
            "grok-test".into(),
            "88API".into(),
            serde_json::json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.88api.com",
                    "ANTHROPIC_AUTH_TOKEN": "sk-grok",
                }
            }),
        );
        let u = provider_upstream(Tool::GrokBuild, &p).unwrap();
        assert_eq!(u.base_url, "https://api.88api.com");
        assert_eq!(u.api_key, "sk-grok");
    }

    #[tokio::test]
    async fn runtime_starts_stops_proxy() {
        let mut rt = RoutingRuntime::new();
        let upstream = Upstream::new("http://127.0.0.1:9", "sk"); // 无需可达，仅生命周期。
        let port = rt
            .ensure_running_with(Tool::Claude, vec![upstream], None, "127.0.0.1", true, 3, 30, true, false, 0)
            .await
            .unwrap();
        assert!(port > 0);
        assert!(rt.is_running(Tool::Claude));
        assert_eq!(rt.port_of(Tool::Claude), Some(port));
        // 重复 ensure 返回同一端口（不重复启动）。
        let port2 = rt
            .ensure_running_with(
                Tool::Claude,
                vec![Upstream::new("http://127.0.0.1:9", "sk")],
                None,
                "127.0.0.1",
                true,
                3,
                30,
                true,
                false,
                0,
            )
            .await
            .unwrap();
        assert_eq!(port2, port);
        // 停止后不再运行。
        rt.stop(Tool::Claude);
        assert!(!rt.is_running(Tool::Claude));
        assert_eq!(rt.port_of(Tool::Claude), None);
        // 不支持的 GrokBuild 也能启停（当前无写文件，但代理可运行）。
        let port3 = rt
            .ensure_running_with(
                Tool::GrokBuild,
                vec![Upstream::new("http://127.0.0.1:9", "sk")],
                None,
                "127.0.0.1",
                true,
                3,
                30,
                true,
                false,
                0,
            )
            .await
            .unwrap();
        assert!(port3 > 0);
        rt.stop_all();
        assert!(!rt.is_running(Tool::GrokBuild));
    }

    #[test]
    fn aggregation_routes_map_models_to_indices() {
        let a = Provider::with_id(
            "a".into(),
            "Alpha".into(),
            serde_json::json!({ "env": { "ANTHROPIC_MODEL": "claude-sonnet-4-5" } }),
        );
        let b = Provider::with_id(
            "b".into(),
            "Beta".into(),
            serde_json::json!({ "config": { "model": "gpt-6-sol" } }),
        );
        let routes = aggregation_model_routes(&[&a, &b]);
        assert_eq!(routes.get("claude-sonnet-4-5"), Some(&0));
        assert_eq!(routes.get("gpt-6-sol"), Some(&1));
    }

    #[test]
    fn image_mode_mapping() {
        use ns_proxy::rectify::ImageMode;
        assert_eq!(image_mode_from_u32(0), ImageMode::Keep);
        assert_eq!(image_mode_from_u32(1), ImageMode::Drop);
        assert_eq!(image_mode_from_u32(2), ImageMode::TextPlaceholder);
        assert_eq!(image_mode_from_u32(99), ImageMode::Keep);
    }
}
