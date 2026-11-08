//! 端到端：启动 mock 上游（OpenAI 兼容）+ 本地代理，发送 Anthropic
//! Messages 请求，验证代理把上行转成 OpenAI、下行转回 Anthropic。

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use serde_json::{Value, json};

/// 启动一个返回固定 OpenAI 响应的 mock 上游。
/// `captured` 记录收到的上行请求体（供测试断言）。
async fn start_mock_upstream(
    captured: Arc<Mutex<Value>>,
) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let captured = captured.clone();
            tokio::spawn(async move {
                let io = TokioIo::new(stream);
                let svc = service_fn(move |req: Request<Incoming>| {
                    let captured = captured.clone();
                    async move {
                        let path = req.uri().path().to_string();
                        // 读请求体（验证上行是 OpenAI 格式）。
                        let body = req
                            .into_body()
                            .collect()
                            .await
                            .map(|b| b.to_bytes().to_vec())
                            .unwrap_or_default();
                        let parsed: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                        *captured.lock().unwrap() = parsed.clone();
                        // mock 上游返回固定 OpenAI 响应。
                        let resp_body = if path == "/v1/chat/completions" {
                            json!({
                                "id": "chatcmpl-mock",
                                "object": "chat.completion",
                                "choices": [{
                                    "index": 0,
                                    "message": {"role": "assistant", "content": "来自上游"},
                                    "finish_reason": "stop",
                                }],
                                "usage": {"prompt_tokens": 3, "completion_tokens": 4},
                            })
                        } else {
                            json!({"error": "not found"})
                        };
                        let body = Full::new(Bytes::from(resp_body.to_string()));
                        Ok::<_, Infallible>(
                            Response::builder()
                                .status(StatusCode::OK)
                                .body(body)
                                .unwrap(),
                        )
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(io, svc)
                    .await;
            });
        }
    });
    addr
}

/// 向本地代理发一个 Anthropic Messages 请求，返回 (状态码, 响应体)。
async fn post_anthropic(addr: SocketAddr) -> (StatusCode, String) {
    let client = hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build_http();
    let uri = format!("http://{addr}/v1/messages");
    let req = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(
            json!({
                "model": "claude-sonnet-4-5",
                "max_tokens": 64,
                "system": "系统提示",
                "messages": [{"role": "user", "content": "你好"}],
            })
            .to_string(),
        )))
        .unwrap();
    let resp = client.request(req).await.unwrap();
    let status = resp.status();
    let body = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, String::from_utf8_lossy(&body).to_string())
}

    #[tokio::test]
    async fn proxy_converts_anthropic_to_openai_and_back() {
        let captured = Arc::new(Mutex::new(Value::Null));
        let upstream_addr = start_mock_upstream(captured.clone()).await;
        let upstream = ns_proxy::Upstream::new(format!("http://{upstream_addr}/v1"), "sk-mock");
        let config = ns_proxy::ProxyConfig::new(upstream);
        let (listener, proxy_addr) = ns_proxy::bind("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        tokio::spawn(ns_proxy::serve_listener(listener, config, None));

        let (status, body) = post_anthropic(proxy_addr).await;
        assert_eq!(status, StatusCode::OK);
        let parsed: Value = serde_json::from_str(&body).unwrap();
        // 下行是 Anthropic 格式。
        assert_eq!(parsed["type"], "message");
        assert_eq!(parsed["content"][0]["text"], "来自上游");
        assert_eq!(parsed["model"], "claude-sonnet-4-5");
        // usage 映射：prompt_tokens→input_tokens, completion_tokens→output_tokens。
        assert_eq!(parsed["usage"]["input_tokens"], 3);
        assert_eq!(parsed["usage"]["output_tokens"], 4);
        // mock 上游收到的上行是 OpenAI 格式（system 折叠为首条消息）。
        let upstream_req = captured.lock().unwrap().clone();
        assert_eq!(upstream_req["model"], "claude-sonnet-4-5");
        assert_eq!(upstream_req["messages"][0]["role"], "system");
        assert_eq!(upstream_req["messages"][0]["content"], "系统提示");
        assert_eq!(upstream_req["messages"][1]["content"], "你好");
    }

    #[tokio::test]
    async fn proxy_fails_over_to_second_upstream() {
        // 第一个上游不可达（死端口），第二个是正常 mock。
        let captured = Arc::new(Mutex::new(Value::Null));
        let upstream_addr = start_mock_upstream(captured.clone()).await;
        let dead = ns_proxy::Upstream::new("http://127.0.0.1:9", "sk");
        let healthy =
            ns_proxy::Upstream::new(format!("http://{upstream_addr}/v1"), "sk-mock");
        let config = ns_proxy::ProxyConfig::with_queue(vec![dead, healthy]);
        let (listener, proxy_addr) = ns_proxy::bind("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        tokio::spawn(ns_proxy::serve_listener(listener, config, None));

        let (status, body) = post_anthropic(proxy_addr).await;
        assert_eq!(status, StatusCode::OK);
        let parsed: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["content"][0]["text"], "来自上游");
        // 成功请求最终落到第二个上游。
        assert_eq!(captured.lock().unwrap()["model"], "claude-sonnet-4-5");
    }

    #[tokio::test]
    async fn proxy_rectifies_thinking_and_logs() {
        let captured = Arc::new(Mutex::new(Value::Null));
        let upstream_addr = start_mock_upstream(captured.clone()).await;
        let upstream =
            ns_proxy::Upstream::new(format!("http://{upstream_addr}/v1"), "sk-mock");
        let log = Arc::new(ns_proxy::RequestLog::new(100));
        let config = ns_proxy::ProxyConfig::with_queue(vec![upstream])
            .with_log(log.clone(), "claude")
            .with_rectify(false, ns_proxy::ImageMode::Drop);
        let (listener, proxy_addr) = ns_proxy::bind("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        tokio::spawn(ns_proxy::serve_listener(listener, config, None));

        // 带 thinking 参数 + 图像块的 Anthropic 请求。
        let client = hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build_http();
        let uri = format!("http://{proxy_addr}/v1/messages");
        let req = hyper::Request::builder()
            .method(hyper::Method::POST)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(
                json!({
                    "model": "claude-sonnet-4-5",
                    "max_tokens": 64,
                    "thinking": {"type": "enabled", "budget_tokens": 100},
                    "messages": [{"role": "user", "content": [
                        {"type": "thinking", "thinking": "思考"},
                        {"type": "text", "text": "hi"},
                        {"type": "image", "source": {"type": "base64", "data": "x"}},
                    ]}],
                })
                .to_string(),
            )))
            .unwrap();
        let resp = client.request(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // 上游收到的上行已整流：无 thinking 顶层参数、无 thinking 块、无图像块。
        let upstream_req = captured.lock().unwrap().clone();
        assert!(upstream_req.get("thinking").is_none());
        let content = upstream_req["messages"][0]["content"].as_str().unwrap();
        assert_eq!(content, "hi");
        // 请求日志记录了这条。
        let snap = log.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].tool, "claude");
        assert_eq!(snap[0].requested_model, "claude-sonnet-4-5");
        assert!(snap[0].ok);
    }
