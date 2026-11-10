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
use std::pin::Pin;

use bytes::Bytes;
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::{Body, Frame, Incoming};
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode, Uri};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde_json::Value;
use futures_util::Stream;


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
    /// 故障转移开关（false = 只用第一个上游，不熔断转移）。
    pub failover_enabled: bool,
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
            failover_enabled: true,
        }
    }

    /// 设置故障转移开关（链式）。
    pub fn with_failover(mut self, enabled: bool) -> Self {
        self.failover_enabled = enabled;
        self
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

    /// 设置熔断器参数（链式）。阈值最小 1；冷却秒数最小 1。
    pub fn with_breaker(mut self, threshold: u32, cooldown_secs: u32) -> Self {
        self.breaker_threshold = threshold.max(1);
        self.breaker_cooldown = std::time::Duration::from_secs(u64::from(cooldown_secs.max(1)));
        self
    }
}

type ResBody = http_body_util::combinators::BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>;

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
    failover_enabled: bool,
) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    // 故障转移关闭：只用第一个上游（不熔断转移）。
    if !failover_enabled {
        return order.into_iter().take(1).collect();
    }
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
    // 入站是否请求流式（Anthropic `stream: true`）。
    let mut stream_requested = false;
    if let Ok(parsed) = serde_json::from_slice::<Value>(&body) {
        requested_model = parsed
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string);
        if requested_model.is_none() {
            requested_model = Some("claude".to_string());
        }
        stream_requested = parsed
            .get("stream")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    }

    // 按故障转移队列顺序尝试上游：跳过熔断中的，成功即返回，
    // 失败记录熔断并尝试下一个。聚合模式下按请求模型路由：命中模型 → 该上游优先。
    let n = config.upstreams.len();
    if n == 0 {
        return Ok(err_response(StatusCode::BAD_GATEWAY, "无可用上游"));
    }
    let order = route_order(
        n,
        requested_model.as_deref(),
        config.model_routes.as_deref(),
        config.failover_enabled,
    );
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
                // 流式请求：上游若返回 SSE，逐行转换分块为 Anthropic SSE 帧。
                let is_sse = resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .map(|ct| ct.contains("text/event-stream"))
                    .unwrap_or(false);
                if status.is_success() {
                    record_success(&config, i);
                    if inbound_anthropic && stream_requested && is_sse {
                        // 流式：实时逐块转换上游 SSE → Anthropic SSE 帧并转发。
                        // 不缓冲整个响应体，逐帧读取并增量转换后直接输出，
                        // 保持客户端持续收到数据（SSE 实时性）。
                        let body = resp.into_body();
                        let model = requested_model.as_deref().unwrap_or("claude").to_string();
                        let api_format = upstream.api_format.clone();
                        let stream = SseBodyStream::new(body, api_format, model);
                        record_log(
                            &config,
                            requested_model.as_deref(),
                            status.as_u16(),
                            started.elapsed(),
                        );
                        return Ok(sse_stream_response(status, stream));
                    }
                    let resp_bytes = resp
                        .into_body()
                        .collect()
                        .await
                        .map(|b| b.to_bytes().to_vec())
                        .unwrap_or_default();
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
                let resp_bytes = resp
                    .into_body()
                    .collect()
                    .await
                    .map(|b| b.to_bytes().to_vec())
                    .unwrap_or_default();
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
#[cfg(test)]
// 全缓冲转换器：仅测试用（运行时用 SseStreamConverter 增量转发）。
/// 把上游 SSE 分块流转换为 Anthropic SSE 文本（全缓冲）。
///
/// 输入是上游原始 SSE 字节（`data: {...}\n\n` 分块）。输出是合法的
/// Anthropic Messages SSE：`message_start` 首帧、逐分块 `content_block_delta`、
/// 末尾 `message_stop`。
fn stream_sse_to_anthropic(bytes: &[u8], api_format: &str, model: &str) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut out = String::new();
    let mut started = false;
    let mut got_stop = false;

    for line in text.lines() {
        let line = line.trim();
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data == "[DONE]" {
            continue;
        }
        let Ok(chunk) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        // 首帧前插 message_start。
        if !started {
            push_sse_frame(&mut out, &convert::anthropic_message_start(model));
            started = true;
        }
        // 按上游格式把 chunk 转成 Anthropic 事件帧。
        let frames: Vec<Value> = match api_format {
            "openai_responses" => convert::openai_responses_stream_chunk_to_anthropic(&chunk, model),
            "gemini_native" => convert::gemini_stream_chunk_to_anthropic(&chunk, model),
            "anthropic" => vec![chunk],
            // 默认 OpenAI Chat。
            _ => convert::openai_stream_chunk_to_anthropic(&chunk, model),
        };
        for frame in frames {
            if frame.get("type").and_then(Value::as_str) == Some("message_stop") {
                got_stop = true;
            }
            push_sse_frame(&mut out, &frame);
        }
    }

    if !started {
        push_sse_frame(&mut out, &convert::anthropic_message_start(model));
    }
    if !got_stop {
        push_sse_frame(
            &mut out,
            &serde_json::json!({"type": "message_stop"}),
        );
    }
    out
}

/// 增量 SSE→Anthropic 转换器（实时逐块转发）。
///
/// 按到达的字节块增量处理：维护内部缓冲以跨块拼合完整 SSE 帧
/// （`data: {...}\n\n`），每解析出一个完整帧即转换为 Anthropic 事件输出。
/// `finish()` 在流结束时补发 `message_stop`（若尚未出现）。
pub struct SseStreamConverter {
    /// 内部缓冲（可能含未完成的帧尾部）。
    buf: Vec<u8>,
    /// 是否已发 message_start。
    started: bool,
    /// 上游是否已发 message_stop。
    got_stop: bool,
    /// 上游 API 格式。
    api_format: String,
    /// 目标模型名。
    model: String,
}

impl SseStreamConverter {
    pub fn new(api_format: &str, model: &str) -> Self {
        Self {
            buf: Vec::new(),
            started: false,
            got_stop: false,
            api_format: api_format.to_string(),
            model: model.to_string(),
        }
    }

    /// 喂入一块字节，返回可输出的 Anthropic SSE 字节（可能为空）。
    pub fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        self.buf.extend_from_slice(chunk);
        let mut out = String::new();
        loop {
            let Some(pos) = find_frame_boundary(&self.buf) else {
                break;
            };
            let frame_bytes: Vec<u8> = self.buf.drain(..pos + 2).collect();
            self.emit_frame(&frame_bytes, &mut out);
        }
        out.into_bytes()
    }

    /// 流结束：补发 message_stop（若缺失）。
    pub fn finish(&mut self) -> Vec<u8> {
        let mut out = String::new();
        if !self.buf.is_empty() {
            let frame_bytes: Vec<u8> = std::mem::take(&mut self.buf);
            self.emit_frame(&frame_bytes, &mut out);
        }
        if !self.started {
            let start = convert::anthropic_message_start(&self.model).to_string();
            push_sse_frame_str(&mut out, &start);
            self.started = true;
        }
        if !self.got_stop {
            let stop = serde_json::json!({"type": "message_stop"}).to_string();
            push_sse_frame_str(&mut out, &stop);
        }
        out.into_bytes()
    }

    /// 解析并转换一个完整 SSE 帧（字节）。
    fn emit_frame(&mut self, frame_bytes: &[u8], out: &mut String) {
        let text = String::from_utf8_lossy(frame_bytes);
        let Some(data) = text.lines().find_map(|l| {
            let l = l.trim();
            l.strip_prefix("data:").map(|d| d.trim().to_string())
        }) else {
            return;
        };
        if data == "[DONE]" {
            return;
        }
        let Ok(chunk) = serde_json::from_str::<Value>(&data) else {
            return;
        };
        if !self.started {
            let start = convert::anthropic_message_start(&self.model).to_string();
            push_sse_frame_str(out, &start);
            self.started = true;
        }
        let frames: Vec<Value> = match self.api_format.as_str() {
            "openai_responses" => convert::openai_responses_stream_chunk_to_anthropic(&chunk, &self.model),
            "gemini_native" => convert::gemini_stream_chunk_to_anthropic(&chunk, &self.model),
            "anthropic" => vec![chunk],
            _ => convert::openai_stream_chunk_to_anthropic(&chunk, &self.model),
        };
        for frame in frames {
            if frame.get("type").and_then(Value::as_str) == Some("message_stop") {
                self.got_stop = true;
            }
            push_sse_frame_str(out, &frame.to_string());
        }
    }
}

/// 在字节流中查找 SSE 帧边界（`\n\n`），返回帧结束位置（不含边界）。
fn find_frame_boundary(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\n\n")
}

/// 追加一个 Anthropic SSE 帧（`event: <type>\ndata: <json>\n\n`），输入 JSON 字符串。
fn push_sse_frame_str(out: &mut String, frame_json: &str) {
    let event = serde_json::from_str::<Value>(frame_json)
        .ok()
        .and_then(|v| v.get("type").and_then(Value::as_str).map(|s| s.to_string()))
        .unwrap_or_else(|| "message".to_string());
    out.push_str("event: ");
    out.push_str(&event);
    out.push('\n');
    out.push_str("data: ");
    out.push_str(frame_json);
    out.push_str("\n\n");
}

#[cfg(test)]
/// 追加一个 Anthropic SSE 帧（`event: <type>\ndata: <json>\n\n`）。
fn push_sse_frame(out: &mut String, frame: &Value) {
    let event = frame.get("type").and_then(Value::as_str).unwrap_or("message");
    out.push_str("event: ");
    out.push_str(event);
    out.push('\n');
    out.push_str("data: ");
    out.push_str(&frame.to_string());
    out.push_str("\n\n");
}


/// 流式 SSE 响应体：把上游 `Incoming` body 逐帧读取，经 `SseStreamConverter`
/// 增量转换为 Anthropic SSE 帧，作为 `Frame<Bytes>` 流输出。
pub struct SseBodyStream {
    /// 上游响应体（逐帧读取）。
    inner: Incoming,
    /// 增量转换器。
    conv: SseStreamConverter,
    /// 是否已完成（避免重复 finish）。
    done: bool,
}

impl SseBodyStream {
    pub fn new(body: Incoming, api_format: String, model: String) -> Self {
        Self {
            inner: body,
            conv: SseStreamConverter::new(&api_format, &model),
            done: false,
        }
    }
}

impl Stream for SseBodyStream {
    type Item = Result<Frame<Bytes>, std::io::Error>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll;
        // 先尝试从上游读一帧。
        loop {
            match Pin::new(&mut self.inner).poll_frame(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => {
                    if self.done {
                        return Poll::Ready(None);
                    }
                    self.done = true;
                    let tail = self.conv.finish();
                    if tail.is_empty() {
                        return Poll::Ready(None);
                    }
                    return Poll::Ready(Some(Ok(Frame::data(Bytes::from(tail)))));
                }
                Poll::Ready(Some(Ok(frame))) => {
                    let data = frame.into_data().unwrap_or_default();
                    let out = self.conv.push(&data);
                    if out.is_empty() {
                        // 无输出（半帧等），继续读下一帧。
                        continue;
                    }
                    return Poll::Ready(Some(Ok(Frame::data(Bytes::from(out)))));
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(std::io::Error::other(e.to_string()))))
                }
            }
        }
    }
}

/// 构造流式 SSE 响应。
fn sse_stream_response(
    status: StatusCode,
    stream: SseBodyStream,
) -> Response<ResBody> {
    let body = StreamBody::new(stream);
    Response::builder()
        .status(status)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .body(
            http_body_util::BodyExt::map_err(
                body,
                |e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) },
            )
            .boxed(),
        )
        .unwrap()
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
        .body(
            Full::new(Bytes::from(body.to_string()))
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })
                .boxed(),
        )
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
        let order = route_order(4, Some("gpt-6-sol"), Some(&routes), true);
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
        let order = route_order(4, Some("unknown-model"), Some(&routes), true);
        assert_eq!(order, vec![0, 1, 2, 3]);
    }

    #[test]
    fn stream_sse_openai_to_anthropic_frames() {
        // OpenAI Chat SSE：两个 content 分块 + finish。
        let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"},\"finish_reason\":null}]}\n\n\
                   data: {\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":null}]}\n\n\
                   data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
                   data: [DONE]\n\n";
        let out = stream_sse_to_anthropic(sse.as_bytes(), "openai_chat", "claude-x");
        // 首帧 message_start、中间 content_block_delta、尾帧 message_stop。
        assert!(out.contains("event: message_start"));
        assert!(out.contains("\"text\":\"Hel\""));
        assert!(out.contains("\"text\":\"lo\""));
        assert!(out.contains("event: message_stop"));
    }

    #[test]
    fn stream_sse_anthropic_passthrough() {
        // Anthropic 上游已是 anthropic SSE：透传但补 message_start 头。
        let sse = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n";
        let out = stream_sse_to_anthropic(sse.as_bytes(), "anthropic", "claude-x");
        assert!(out.contains("event: message_start"));
        assert!(out.contains("\"text\":\"hi\""));
        assert!(out.contains("event: message_stop"));
    }

    #[test]
    fn sse_stream_converter_incremental_chunks() {
        // 模拟逐块到达：第一块含半个帧，第二块含剩余 + 第二个完整帧。
        let mut conv = SseStreamConverter::new("openai_chat", "claude-x");
        let part1 = b"data: {\"choices\":[{\"delta\":{\"content\":\"He";
        let out1 = conv.push(part1);
        assert!(out1.is_empty(), "半帧不应有输出");
        let part2 = b"l\"},\"finish_reason\":null}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":null}]}\n\n";
        let out2 = conv.push(part2);
        let s2 = String::from_utf8_lossy(&out2);
        assert!(s2.contains("event: message_start"), "首帧应补 message_start");
        assert!(s2.contains("\"text\":\"Hel\""));
        assert!(s2.contains("\"text\":\"lo\""), "完整帧2应在 out2 输出");
        let out3 = conv.push(b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n");
        let s3 = String::from_utf8_lossy(&out3);
        let done = conv.finish();
        let s4 = String::from_utf8_lossy(&done);
        assert!(
            s3.contains("event: message_stop") || s4.contains("event: message_stop"),
            "message_stop 应在 stop 帧或 finish 补发中出现"
        );
    }

    #[test]
    fn sse_stream_converter_single_chunk() {
        let mut conv = SseStreamConverter::new("openai_chat", "claude-x");
        let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n\ndata: [DONE]\n\n";
        let out = conv.push(sse.as_bytes());
        let s = String::from_utf8_lossy(&out);
        assert!(s.contains("event: message_start"));
        assert!(s.contains("\"text\":\"Hi\""));
        let done = conv.finish();
        let s2 = String::from_utf8_lossy(&done);
        assert!(s2.contains("event: message_stop"));
    }

    #[test]
    fn route_order_disabled_failover_uses_first() {
        let mut routes = std::collections::HashMap::new();
        routes.insert("m".to_string(), 2usize);
        // 即使模型路由命中 idx=2，failover 关闭时只用第一个。
        let order = route_order(4, Some("m"), Some(&routes), false);
        assert_eq!(order, vec![0]);
    }
}
