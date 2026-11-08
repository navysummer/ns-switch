//! HTTP 监听：接收 Anthropic Messages / OpenAI Chat Completions 请求，
//! 经格式转换后转发到上游供应商，响应转回后返回。
//!
//! 监听 `127.0.0.1:<port>`，路径映射：
//! - `/v1/messages` → Anthropic Messages（转 OpenAI Chat Completions 上行）。
//! - `/v1/chat/completions` → OpenAI Chat Completions（原样上行）。
//! - `/health` → 健康探测。

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode, Uri};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde_json::Value;


use crate::convert;
use hyper_util::rt::TokioIo;
/// 上游供应商端点信息（真实供应商的 OpenAI 兼容接口）。
#[derive(Clone, Debug)]
pub struct Upstream {
    /// 上游基地址（如 `https://api.openai.com/v1`）。
    pub base_url: String,
    /// 上游 API key。
    pub api_key: String,
    /// 上游 API 格式：anthropic / openai_chat / openai_responses / gemini_native。
    /// 决定请求体与响应体的格式转换方向（入站 Anthropic → 上游格式）。
    pub api_format: String,
}

impl Upstream {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            api_format: "openai_chat".into(),
        }
    }

    /// 设置 API 格式（链式）。
    pub fn with_format(mut self, api_format: impl Into<String>) -> Self {
        self.api_format = api_format.into();
        self
    }
}

/// 路由配置：上游故障转移队列 + 每上游熔断器。
#[derive(Clone)]
pub struct ProxyConfig {
    /// 按优先级排序的上游供应商（队列顺序）。
    pub upstreams: Vec<Arc<Upstream>>,
    /// 熔断参数。
    pub breaker_threshold: u32,
    pub breaker_cooldown: std::time::Duration,
    /// 每上游熔断状态（index → breaker）。
    pub breakers: Arc<std::sync::Mutex<std::collections::HashMap<usize, crate::failover::CircuitBreaker>>>,
    /// 请求日志（可选；None = 不记录）。
    pub request_log: Option<Arc<crate::log::RequestLog>>,
    /// 上游是否支持 Anthropic thinking 参数（false = 整流剥离）。
    pub supports_thinking: bool,
    /// 图像处理策略。
    pub image_mode: crate::rectify::ImageMode,
    /// 工具 id（日志用）。
    pub tool: String,
    /// 按模型路由：模型名 → upstream 索引（聚合模式）。命中时优先该上游。
    pub model_routes: Option<Arc<std::collections::HashMap<String, usize>>>,
}
impl ProxyConfig {
    pub fn new(upstream: Upstream) -> Self {
        Self::with_queue(vec![upstream])
    }

    pub fn with_queue(upstreams: Vec<Upstream>) -> Self {
        Self {
            upstreams: upstreams.into_iter().map(Arc::new).collect(),
            breaker_threshold: 3,
            breaker_cooldown: std::time::Duration::from_secs(30),
            breakers: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            request_log: None,
            supports_thinking: false,
            image_mode: crate::rectify::ImageMode::Keep,
            tool: "proxy".into(),
            model_routes: None,
        }
    }

    /// 设置请求日志（链式）。
    pub fn with_log(mut self, log: Arc<crate::log::RequestLog>, tool: impl Into<String>) -> Self {
        self.request_log = Some(log);
        self.tool = tool.into();
        self
    }

    /// 设置整流参数（链式）。
    pub fn with_rectify(
        mut self,
        supports_thinking: bool,
        image_mode: crate::rectify::ImageMode,
    ) -> Self {
        self.supports_thinking = supports_thinking;
        self.image_mode = image_mode;
        self
    }

    /// 设置按模型路由（链式）。
    pub fn with_model_routes(
        mut self,
        routes: std::collections::HashMap<String, usize>,
    ) -> Self {
        self.model_routes = Some(Arc::new(routes));
        self
    }
}

type ResBody = Full<Bytes>;

/// 绑定本地监听端口，返回监听器与实际地址（端口 0 时由系统分配）。
pub async fn bind(addr: SocketAddr) -> anyhow::Result<(tokio::net::TcpListener, SocketAddr)> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;
    Ok((listener, local_addr))
}

/// 在已绑定监听器上运行 accept 循环（`tokio::spawn` 中调用）。
///
/// `shutdown` 收到信号后退出循环。`None` = 永不停止（进程内常驻）。
pub async fn serve_listener(
    listener: tokio::net::TcpListener,
    config: ProxyConfig,
    mut shutdown: Option<tokio::sync::oneshot::Receiver<()>>,
) -> anyhow::Result<()> {
    let config = Arc::new(config);
    let client = Client::builder(TokioExecutor::new()).build_http();
    loop {
        let accepted = tokio::select! {
            _ = async {
                match &mut shutdown {
                    Some(rx) => {
                        let _ = rx.await;
                    }
                    None => std::future::pending::<()>().await,
                }
            } => {
                return Ok(());
            }
            accepted = listener.accept() => {
                accepted?
            }
        };
        let (stream, _) = accepted;
        let config = config.clone();
        let client = client.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let svc = service_fn(move |req| handle(req, config.clone(), client.clone()));
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, svc)
                .await;
        });
    }
}

/// 便捷入口：绑定 + 启动（返回监听地址）。
pub async fn serve(config: ProxyConfig, addr: SocketAddr) -> anyhow::Result<SocketAddr> {
    let (listener, local_addr) = bind(addr).await?;
    tokio::spawn(serve_listener(listener, config, None));
    Ok(local_addr)
}

/// 计算上游尝试顺序：按模型路由命中时该上游优先，否则原队列顺序。
fn route_order(
    n: usize,
    model: Option<&str>,
    routes: Option<&std::collections::HashMap<String, usize>>,
) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    if let (Some(model), Some(routes)) = (model, routes) {
        if let Some(idx) = routes.get(model).copied() {
            if idx < n {
                order.retain(|&i| i != idx);
                order.insert(0, idx);
            }
        }
    }
    order
}

/// 处理单个 HTTP 请求（转换 + 转发 + 回写）。
pub async fn handle(
    req: Request<Incoming>,
    config: Arc<ProxyConfig>,
    client: Client<hyper_util::client::legacy::connect::HttpConnector, Full<Bytes>>,
) -> Result<Response<ResBody>, Infallible> {
    let path = req.uri().path().to_string();
    let method = req.method().clone();
    let body = read_body(req).await;
    let body = match body {
        Ok(b) => b,
        Err(e) => return Ok(err_response(StatusCode::INTERNAL_SERVER_ERROR, &e)),
    };

    if path == "/health" {
        return Ok(json_response(StatusCode::OK, r#"{"ok":true}"#));
    }

    // 记录请求方指定的模型，用于下行响应的 model 字段（上游响应模型不同）。
    let started = crate::log::start();
    let mut requested_model: Option<String> = None;
    // 入站是否为 Anthropic Messages（需按上游格式转换）。
    let inbound_anthropic = path == "/v1/messages";
    if let Ok(parsed) = serde_json::from_slice::<Value>(&body) {
        requested_model = parsed
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string);
        if requested_model.is_none() {
            requested_model = Some("claude".to_string());
        }
    }

    // 按故障转移队列顺序尝试上游：跳过熔断中的，成功即返回，
    // 失败记录熔断并尝试下一个。聚合模式下按请求模型路由：命中模型 → 该上游优先。
    let n = config.upstreams.len();
    if n == 0 {
        return Ok(err_response(StatusCode::BAD_GATEWAY, "无可用上游"));
    }
    let order = route_order(n, requested_model.as_deref(), config.model_routes.as_deref());
    for i in order {
        let allow = {
            let breakers = config.breakers.lock().unwrap();
            breakers
                .get(&i)
                .map(|b| b.allow())
                .unwrap_or(true)
        };
        if !allow {
            continue;
        }
        let upstream = config.upstreams[i].clone();
        // 上行：Anthropic 入站按上游格式转换；否则原样。
        let outbound = if inbound_anthropic {
            let parsed: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            let rectified = crate::rectify::rectify_anthropic(
                &parsed,
                config.supports_thinking,
                config.image_mode,
            );
            convert_inbound(&rectified, &upstream.api_format, &body)
        } else {
            body.clone()
        };
        let upstream_url = build_upstream_url(&upstream.base_url, &path, &upstream.api_format);
        match forward(
            &client,
            &upstream_url,
            &upstream.api_key,
            outbound,
            method.clone(),
        )
        .await
        {
            Ok(resp) => {
                let status = resp.status();
                let resp_bytes = resp
                    .into_body()
                    .collect()
                    .await
                    .map(|b| b.to_bytes().to_vec())
                    .unwrap_or_default();
                if status.is_success() {
                    record_success(&config, i);
                    let final_body = if inbound_anthropic {
                        let parsed: Value =
                            serde_json::from_slice(&resp_bytes).unwrap_or(Value::Null);
                        let model = requested_model.as_deref().unwrap_or("claude");
                        convert_outbound(&parsed, &upstream.api_format, model, &resp_bytes)
                    } else {
                        resp_bytes
                    };
                    record_log(&config, requested_model.as_deref(), status.as_u16(), started.elapsed());
                    return Ok(json_response(status, &String::from_utf8_lossy(&final_body)));
                }
                // 上游返回非 2xx：不算熔断（业务错误），直接回传。
                record_log(&config, requested_model.as_deref(), status.as_u16(), started.elapsed());
                return Ok(json_response(status, &String::from_utf8_lossy(&resp_bytes)));
            }
            Err(e) => {
                record_failure(&config, i);
                // 尝试下一个上游。
                let _ = e;
            }
        }
    }
    record_log(&config, requested_model.as_deref(), 502, started.elapsed());
    Ok(err_response(
        StatusCode::BAD_GATEWAY,
        "所有上游均不可用或熔断",
    ))
}

/// 按上游格式转换 Anthropic 入站请求体（非流式请求）。
fn convert_inbound(rectified: &Value, api_format: &str, original: &[u8]) -> Vec<u8> {
    match api_format {
        "openai_responses" => serde_json::to_vec(&convert::anthropic_to_openai_responses(rectified))
            .unwrap_or_else(|_| original.to_vec()),
        "gemini_native" => serde_json::to_vec(&convert::anthropic_to_gemini(rectified))
            .unwrap_or_else(|_| original.to_vec()),
        "anthropic" => serde_json::to_vec(rectified).unwrap_or_else(|_| original.to_vec()),
        // 默认 OpenAI Chat。
        _ => serde_json::to_vec(&convert::anthropic_to_openai(rectified))
            .unwrap_or_else(|_| original.to_vec()),
    }
}

/// 按上游格式转换上游响应 → Anthropic 响应体。
fn convert_outbound(parsed: &Value, api_format: &str, model: &str, original: &[u8]) -> Vec<u8> {
    let converted = match api_format {
        "openai_responses" => convert::openai_responses_to_anthropic(parsed, model),
        "gemini_native" => convert::gemini_to_anthropic(parsed, model),
        "anthropic" => parsed.clone(),
        _ => convert::openai_to_anthropic(parsed, model),
    };
    serde_json::to_vec(&converted).unwrap_or_else(|_| original.to_vec())
}

fn record_log(config: &ProxyConfig, model: Option<&str>, status: u16, elapsed: std::time::Duration) {
    if let Some(log) = &config.request_log {
        let actual = model.unwrap_or("");
        log.record(&config.tool, actual, actual, status, elapsed);
    }
}


fn record_success(config: &ProxyConfig, i: usize) {
    if let Ok(mut breakers) = config.breakers.lock() {
        let b = breakers
            .entry(i)
            .or_insert_with(|| crate::failover::CircuitBreaker::new(
                config.breaker_threshold,
                config.breaker_cooldown,
            ));
        b.record_success();
    }
}

fn record_failure(config: &ProxyConfig, i: usize) {
    if let Ok(mut breakers) = config.breakers.lock() {
        let b = breakers
            .entry(i)
            .or_insert_with(|| crate::failover::CircuitBreaker::new(
                config.breaker_threshold,
                config.breaker_cooldown,
            ));
        b.record_failure();
    }
}

/// 读完整请求体。
async fn read_body(req: Request<Incoming>) -> Result<Vec<u8>, String> {
    req.into_body()
        .collect()
        .await
        .map(|b| b.to_bytes().to_vec())
        .map_err(|e| e.to_string())
}

fn build_upstream_url(base: &str, path: &str, api_format: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    let resource = path.strip_prefix("v1/").unwrap_or(&path);
    // Anthropic `/v1/messages` 按上游格式路由到对应端点。
    let resource = if resource == "messages" {
        match api_format {
            "openai_responses" => "responses",
            "gemini_native" => "gemini/generateContent",
            "anthropic" => "messages",
            _ => "chat/completions",
        }
    } else {
        resource
    };
    format!("{base}/{resource}")
}

/// 转发请求到上游（返回响应）。
async fn forward(
    client: &Client<hyper_util::client::legacy::connect::HttpConnector, Full<Bytes>>,
    url: &str,
    api_key: &str,
    body: Vec<u8>,
    method: Method,
) -> anyhow::Result<Response<Incoming>> {
    let uri: Uri = url.parse().map_err(|e| anyhow::anyhow!("无效 URL: {e}"))?;
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {api_key}"))
        .body(Full::new(Bytes::from(body)))
        .map_err(|e| anyhow::anyhow!("构造请求失败: {e}"))?;
    let resp = client
        .request(req)
        .await
        .map_err(|e| anyhow::anyhow!("上游请求失败: {e}"))?;
    Ok(resp)
}

/// 文本 JSON 响应。
fn json_response(status: StatusCode, body: &str) -> Response<ResBody> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body.to_string())))
        .unwrap()
}

/// 错误 JSON 响应。
fn err_response(status: StatusCode, msg: &str) -> Response<ResBody> {
    let body = serde_json::json!({ "error": msg }).to_string();
    json_response(status, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_url_joins_base_and_path() {
        // OpenAI 路径原样（base 含 /v1，resource 追加）。
        assert_eq!(
            build_upstream_url("https://api.openai.com/v1", "/v1/chat/completions", "openai_chat"),
            "https://api.openai.com/v1/chat/completions"
        );
        // Anthropic `/v1/messages` → OpenAI `chat/completions`。
        assert_eq!(
            build_upstream_url("https://x.com/v1", "v1/messages", "openai_chat"),
            "https://x.com/v1/chat/completions"
        );
        // openai_responses → `/v1/responses`。
        assert_eq!(
            build_upstream_url("https://x.com/v1", "v1/messages", "openai_responses"),
            "https://x.com/v1/responses"
        );
        // anthropic → 原样 `messages`。
        assert_eq!(
            build_upstream_url("https://x.com/v1", "v1/messages", "anthropic"),
            "https://x.com/v1/messages"
        );
        // gemini_native → `gemini/generateContent`。
        assert_eq!(
            build_upstream_url("https://x.com/v1beta", "v1/messages", "gemini_native"),
            "https://x.com/v1beta/gemini/generateContent"
        );
    }

    #[test]
    fn convert_inbound_dispatches_by_format() {
        let rectified = serde_json::json!({
            "model": "m",
            "messages": [{"role": "user", "content": "hi"}],
        });
        let orig = serde_json::to_vec(&rectified).unwrap();
        // openai_responses → input[].
        let r = convert_inbound(&rectified, "openai_responses", &orig);
        let v: serde_json::Value = serde_json::from_slice(&r).unwrap();
        assert!(v.get("input").is_some());
        assert!(v.get("messages").is_none());
        // gemini_native → contents[].
        let g = convert_inbound(&rectified, "gemini_native", &orig);
        let v: serde_json::Value = serde_json::from_slice(&g).unwrap();
        assert!(v.get("contents").is_some());
        // anthropic → 原样。
        let a = convert_inbound(&rectified, "anthropic", &orig);
        let v: serde_json::Value = serde_json::from_slice(&a).unwrap();
        assert!(v.get("messages").is_some());
        // 默认 → openai chat messages.
        let c = convert_inbound(&rectified, "", &orig);
        let v: serde_json::Value = serde_json::from_slice(&c).unwrap();
        assert!(v.get("messages").is_some());
        assert!(v.get("input").is_none());
    }

    #[test]
    fn convert_outbound_dispatches_by_format() {
        let openai_chat = serde_json::json!({
            "choices": [{"message": {"content": "hi"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1},
        });
        let orig = serde_json::to_vec(&openai_chat).unwrap();
        let a = convert_outbound(&openai_chat, "openai_chat", "claude", &orig);
        let v: serde_json::Value = serde_json::from_slice(&a).unwrap();
        assert_eq!(v["type"], "message");
        assert_eq!(v["content"][0]["text"], "hi");

        let responses = serde_json::json!({
            "output": [{"type": "message", "content": [{"type": "output_text", "text": "hi"}]}],
        });
        let orig = serde_json::to_vec(&responses).unwrap();
        let a = convert_outbound(&responses, "openai_responses", "claude", &orig);
        let v: serde_json::Value = serde_json::from_slice(&a).unwrap();
        assert_eq!(v["type"], "message");
        assert_eq!(v["content"][0]["text"], "hi");

        let gemini = serde_json::json!({
            "candidates": [{"content": {"parts": [{"text": "hi"}]}, "finishReason": "STOP"}],
        });
        let orig = serde_json::to_vec(&gemini).unwrap();
        let a = convert_outbound(&gemini, "gemini_native", "claude", &orig);
        let v: serde_json::Value = serde_json::from_slice(&a).unwrap();
        assert_eq!(v["type"], "message");
        assert_eq!(v["content"][0]["text"], "hi");
    }

    #[test]
    fn route_order_prefers_matching_model() {
        let mut routes = std::collections::HashMap::new();
        routes.insert("gpt-6-sol".to_string(), 2usize);
        let order = route_order(4, Some("gpt-6-sol"), Some(&routes));
        assert_eq!(order[0], 2);
        assert_eq!(order.len(), 4);
    }

    #[tokio::test]
    async fn handle_roundtrips_responses_upstream() {
        // Mock 上游：返回 OpenAI Responses 格式。
        let mock = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mock_addr = mock.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = mock.accept().await.unwrap();
            let io = hyper_util::rt::TokioIo::new(stream);
            let svc = service_fn(|_req| async {
                Ok::<_, std::convert::Infallible>(Response::builder()
                    .status(200)
                    .header("content-type", "application/json")
                    .body(Full::new(Bytes::from(
                        serde_json::json!({
                            "id": "resp_1",
                            "output": [{"type": "message", "content": [{"type": "output_text", "text": "hello-from-upstream"}]}],
                        })
                        .to_string(),
                    )))
                    .unwrap())
            });
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, svc)
                .await;
        });

        // 代理指向 mock 上游（openai_responses 格式）。
        let upstream = Upstream::new(
            format!("http://{}", mock_addr),
            "sk-test",
        ).with_format("openai_responses");
        let config = ProxyConfig::new(upstream);
        let addr = serve(config, "127.0.0.1:0".parse().unwrap()).await.unwrap();

        // Anthropic 请求 → 代理。
        let client = Client::builder(TokioExecutor::new()).build_http();
        let req = Request::builder()
            .method("POST")
            .uri(format!("http://{}/v1/messages", addr))
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(
                serde_json::json!({
                    "model": "claude-sonnet-4-5",
                    "max_tokens": 100,
                    "messages": [{"role": "user", "content": "ping"}],
                })
                .to_string(),
            )))
            .unwrap();
        let resp = client.request(req).await.unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["type"], "message");
        assert_eq!(v["content"][0]["text"], "hello-from-upstream");
        assert_eq!(v["model"], "claude-sonnet-4-5");
    }

    #[test]
    fn route_order_falls_back_to_queue() {
        let mut routes = std::collections::HashMap::new();
        routes.insert("gpt-6-sol".to_string(), 2usize);
        let order = route_order(4, Some("unknown-model"), Some(&routes));
        assert_eq!(order, vec![0, 1, 2, 3]);
    }
}
