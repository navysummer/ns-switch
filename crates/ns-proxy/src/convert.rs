//! API 格式转换：Anthropic Messages ↔ OpenAI Chat Completions。
//!
//! 纯函数，不发起网络请求。目标是让 Claude Code（发 Anthropic Messages 到
//! `/v1/messages`）经由本地代理转到 OpenAI 兼容供应商的 `/v1/chat/completions`，
//! 并把响应转回 Anthropic 格式（含流式 SSE 帧）。

use serde_json::{Value, json};

/// 将 Anthropic Messages 请求体转成 OpenAI Chat Completions 请求体。
///
/// 关键映射：
/// - `system`（字符串或块数组）→ 追加一条 `system` 角色消息。
/// - `messages[].content`（字符串或块数组）→ 展平为文本；图像块丢弃
///   （上游不支持时由整流器/调用方决定）。
/// - `max_tokens` → `max_tokens`；`temperature`/`top_p`/`stream` 透传。
/// - 无对应字段时省略（保留 OpenAI 默认）。
pub fn anthropic_to_openai(req: &Value) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = req.get("system") {
        if let Some(text) = content_to_text(system) {
            if !text.is_empty() {
                messages.push(json!({ "role": "system", "content": text }));
            }
        }
    }
    if let Some(arr) = req.get("messages").and_then(Value::as_array) {
        for m in arr {
            let role = m.get("role").and_then(Value::as_str).unwrap_or("user");
            let content = m.get("content");
            let text = content.and_then(content_to_text).unwrap_or_default();
            messages.push(json!({ "role": role, "content": text }));
        }
    }
    let mut out = json!({ "messages": messages });
    if let Some(v) = req.get("model") {
        out["model"] = v.clone();
    }
    if let Some(v) = req.get("max_tokens") {
        out["max_tokens"] = v.clone();
    }
    for k in ["temperature", "top_p", "stream", "stop"] {
        if let Some(v) = req.get(k) {
            out[k] = v.clone();
        }
    }
    out
}

/// Anthropic 的 `content`（字符串或块数组）→ 纯文本（拼接 text 块）。
pub fn content_to_text(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => Some(s.clone()),
        Value::Array(blocks) => {
            let texts: Vec<&str> = blocks
                .iter()
                .filter_map(|b| {
                    if b.get("type").and_then(Value::as_str) == Some("text") {
                        b.get("text").and_then(Value::as_str)
                    } else {
                        None
                    }
                })
                .collect();
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n"))
            }
        }
        _ => None,
    }
}

/// OpenAI Chat Completions 非流式响应 → Anthropic Messages 响应。
pub fn openai_to_anthropic(resp: &Value, model: &str) -> Value {
    let content = resp
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let finish_reason = resp
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .and_then(|c| c.get("finish_reason"))
        .and_then(Value::as_str)
        .unwrap_or("stop");
    let usage = resp.get("usage");
    let input_tokens = usage
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = usage
        .and_then(|u| u.get("completion_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    json!({
        "id": resp.get("id").and_then(Value::as_str).unwrap_or("chatcmpl-proxy"),
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": content}],
        "model": model,
        "stop_reason": stop_reason_anthropic(finish_reason),
        "usage": {"input_tokens": input_tokens, "output_tokens": output_tokens},
    })
}

/// OpenAI `finish_reason` → Anthropic `stop_reason`。
pub fn stop_reason_anthropic(finish: &str) -> &'static str {
    match finish {
        "length" => "max_tokens",
        "tool_calls" => "tool_use",
        "function_call" => "tool_use",
        _ => "end_turn",
    }
}

/// OpenAI 流式 chunk → Anthropic SSE 数据帧。
///
/// 第一帧带 `message_start`，随后 `content_block_delta`，末尾 `message_delta`
/// + `message_stop`。输入为 OpenAI `data: {...}` 的 JSON（不含 `data:` 前缀）。
pub fn openai_stream_chunk_to_anthropic(chunk: &Value, model: &str) -> Vec<Value> {
    let mut frames = Vec::new();
    let choices = chunk
        .get("choices")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(first) = choices.first() {
        let delta = first.get("delta").cloned().unwrap_or(Value::Null);
        let content = delta.get("content").and_then(Value::as_str).unwrap_or("");
        let finish = first
            .get("finish_reason")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !content.is_empty() {
            frames.push(json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": content},
            }));
        }
        if !finish.is_empty() {
            frames.push(json!({
                "type": "message_delta",
                "delta": {"stop_reason": stop_reason_anthropic(finish)},
                "usage": {"output_tokens": 0},
            }));
            frames.push(json!({"type": "message_stop"}));
        }
    }
    let _ = model;
    frames
}

/// 生成 Anthropic `message_start` 帧（流式首帧）。
pub fn anthropic_message_start(model: &str) -> Value {
    json!({
        "type": "message_start",
        "message": {
            "id": "msg_proxy_start",
            "type": "message",
            "role": "assistant",
            "content": [],
            "model": model,
            "stop_reason": null,
            "usage": {"input_tokens": 0, "output_tokens": 0},
        },
    })
}

/// 将 Anthropic Messages 请求体转成 OpenAI Responses 请求体（`/v1/responses`）。
///
/// 映射：`system` → `instructions`；`messages[]` → `input[]`（role/content 文本）；
/// `max_tokens`/`model`/`stream` 透传。
pub fn anthropic_to_openai_responses(req: &Value) -> Value {
    let mut input = Vec::new();
    if let Some(system) = req.get("system") {
        if let Some(text) = content_to_text(system) {
            if !text.is_empty() {
                input.push(json!({ "role": "system", "content": text }));
            }
        }
    }
    if let Some(arr) = req.get("messages").and_then(Value::as_array) {
        for m in arr {
            let role = m.get("role").and_then(Value::as_str).unwrap_or("user");
            let content = m.get("content");
            let text = content.and_then(content_to_text).unwrap_or_default();
            input.push(json!({ "role": role, "content": text }));
        }
    }
    let mut out = json!({ "input": input });
    if let Some(v) = req.get("model") {
        out["model"] = v.clone();
    }
    if let Some(v) = req.get("max_tokens") {
        out["max_output_tokens"] = v.clone();
    }
    if let Some(v) = req.get("stream") {
        out["stream"] = v.clone();
    }
    out
}

/// OpenAI Responses 非流式响应 → Anthropic Messages 响应。
pub fn openai_responses_to_anthropic(resp: &Value, model: &str) -> Value {
    let text = resp
        .get("output")
        .and_then(Value::as_array)
        .and_then(|arr| arr.iter().find(|o| o.get("type").and_then(Value::as_str) == Some("message")))
        .and_then(|msg| msg.get("content"))
        .and_then(Value::as_array)
        .and_then(|arr| arr.iter().find(|c| c.get("type").and_then(Value::as_str) == Some("output_text")))
        .and_then(|c| c.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let usage = resp.get("usage");
    let input_tokens = usage
        .and_then(|u| u.get("input_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = usage
        .and_then(|u| u.get("output_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    json!({
        "id": resp.get("id").and_then(Value::as_str).unwrap_or("resp_proxy"),
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": text}],
        "model": model,
        "stop_reason": "end_turn",
        "usage": {"input_tokens": input_tokens, "output_tokens": output_tokens},
    })
}

/// OpenAI Responses 流式 chunk → Anthropic SSE 数据帧。
pub fn openai_responses_stream_chunk_to_anthropic(chunk: &Value, model: &str) -> Vec<Value> {
    let mut frames = Vec::new();
    let rtype = chunk.get("type").and_then(Value::as_str).unwrap_or("");
    match rtype {
        "response.output_text.delta" => {
            let delta = chunk.get("delta").and_then(Value::as_str).unwrap_or("");
            if !delta.is_empty() {
                frames.push(json!({
                    "type": "content_block_delta",
                    "index": 0,
                    "delta": {"type": "text_delta", "text": delta},
                }));
            }
        }
        "response.completed" => {
            frames.push(json!({
                "type": "message_delta",
                "delta": {"stop_reason": "end_turn"},
                "usage": {"output_tokens": 0},
            }));
            frames.push(json!({"type": "message_stop"}));
        }
        _ => {}
    }
    let _ = model;
    frames
}

/// 将 Anthropic Messages 请求体转成 Gemini Native 请求体（`generateContent`）。
///
/// 映射：`system` → 首条 system 角色；`messages[]` → `contents[]`（role/user·model）；
/// `max_tokens` → `generationConfig.maxOutputTokens`；`model` → `model`。
pub fn anthropic_to_gemini(req: &Value) -> Value {
    let mut contents = Vec::new();
    if let Some(system) = req.get("system") {
        if let Some(text) = content_to_text(system) {
            if !text.is_empty() {
                contents.push(json!({ "role": "user", "parts": [{"text": text}] }));
            }
        }
    }
    if let Some(arr) = req.get("messages").and_then(Value::as_array) {
        for m in arr {
            let role = match m.get("role").and_then(Value::as_str).unwrap_or("user") {
                "assistant" => "model",
                _ => "user",
            };
            let content = m.get("content");
            let text = content.and_then(content_to_text).unwrap_or_default();
            contents.push(json!({ "role": role, "parts": [{"text": text}] }));
        }
    }
    let mut out = json!({ "contents": contents });
    if let Some(v) = req.get("model") {
        out["model"] = v.clone();
    }
    if let Some(v) = req.get("max_tokens") {
        out["generationConfig"] = json!({ "maxOutputTokens": v.clone() });
    }
    if let Some(v) = req.get("stream") {
        if v.as_bool() == Some(true) {
            out["stream"] = json!(true);
        }
    }
    out
}

/// Gemini Native 非流式响应 → Anthropic Messages 响应。
pub fn gemini_to_anthropic(resp: &Value, model: &str) -> Value {
    let text = resp
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(Value::as_array)
        .and_then(|parts| parts.iter().find_map(|p| p.get("text").and_then(Value::as_str)))
        .unwrap_or("");
    let usage = resp.get("usageMetadata");
    let input_tokens = usage
        .and_then(|u| u.get("promptTokenCount"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let output_tokens = usage
        .and_then(|u| u.get("candidatesTokenCount"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let finish = resp
        .get("candidates")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .and_then(|c| c.get("finishReason"))
        .and_then(Value::as_str)
        .unwrap_or("STOP");
    let stop_reason = match finish {
        "MAX_TOKENS" => "max_tokens",
        _ => "end_turn",
    };
    json!({
        "id": resp.get("responseId").and_then(Value::as_str).unwrap_or("gem_proxy"),
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": text}],
        "model": model,
        "stop_reason": stop_reason,
        "usage": {"input_tokens": input_tokens, "output_tokens": output_tokens},
    })
}

/// Gemini Native 流式 chunk → Anthropic SSE 数据帧。
pub fn gemini_stream_chunk_to_anthropic(chunk: &Value, model: &str) -> Vec<Value> {
    let mut frames = Vec::new();
    if let Some(candidates) = chunk.get("candidates").and_then(Value::as_array) {
        if let Some(first) = candidates.first() {
            let text = first
                .get("content")
                .and_then(|c| c.get("parts"))
                .and_then(Value::as_array)
                .and_then(|parts| parts.iter().find_map(|p| p.get("text").and_then(Value::as_str)))
                .unwrap_or("");
            if !text.is_empty() {
                frames.push(json!({
                    "type": "content_block_delta",
                    "index": 0,
                    "delta": {"type": "text_delta", "text": text},
                }));
            }
            let finish = first.get("finishReason").and_then(Value::as_str).unwrap_or("");
            if !finish.is_empty() {
                let stop = if finish == "MAX_TOKENS" { "max_tokens" } else { "end_turn" };
                frames.push(json!({
                    "type": "message_delta",
                    "delta": {"stop_reason": stop},
                    "usage": {"output_tokens": 0},
                }));
                frames.push(json!({"type": "message_stop"}));
            }
        }
    }
    let _ = model;
    frames
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_to_openai_maps_system_and_messages() {
        let req = json!({
            "model": "claude-sonnet-4-5",
            "max_tokens": 512,
            "system": "你是一个助手",
            "messages": [
                {"role": "user", "content": "你好"},
                {"role": "assistant", "content": [{"type": "text", "text": "在的"}]},
            ],
        });
        let openai = anthropic_to_openai(&req);
        let msgs = openai["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "你是一个助手");
        assert_eq!(msgs[1]["content"], "你好");
        assert_eq!(msgs[2]["content"], "在的");
        assert_eq!(openai["max_tokens"], 512);
    }

    #[test]
    fn anthropic_content_block_to_text() {
        let content = json!([
            {"type": "text", "text": "a"},
            {"type": "image", "source": {"type": "base64", "data": "x"}},
            {"type": "text", "text": "b"},
        ]);
        assert_eq!(content_to_text(&content).as_deref(), Some("a\nb"));
        // 纯图像 → None
        let img = json!([{"type": "image", "source": {"type": "base64", "data": "x"}}]);
        assert_eq!(content_to_text(&img), None);
    }

    #[test]
    fn openai_response_to_anthropic() {
        let resp = json!({
            "id": "chatcmpl-1",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hi"},
                "finish_reason": "stop",
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5},
        });
        let anthropic = openai_to_anthropic(&resp, "claude-sonnet-4-5");
        assert_eq!(anthropic["type"], "message");
        assert_eq!(anthropic["content"][0]["text"], "Hi");
        assert_eq!(anthropic["stop_reason"], "end_turn");
        assert_eq!(anthropic["usage"]["input_tokens"], 10);
        assert_eq!(anthropic["usage"]["output_tokens"], 5);
    }

    #[test]
    fn openai_stream_chunk_maps_deltas() {
        let chunk = json!({
            "choices": [{
                "delta": {"role": "assistant", "content": "Hello"},
                "finish_reason": null,
            }],
        });
        let frames = openai_stream_chunk_to_anthropic(&chunk, "m");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["type"], "content_block_delta");
        assert_eq!(frames[0]["delta"]["text"], "Hello");
    }

    #[test]
    fn stop_reason_maps() {
        assert_eq!(stop_reason_anthropic("length"), "max_tokens");
        assert_eq!(stop_reason_anthropic("tool_calls"), "tool_use");
        assert_eq!(stop_reason_anthropic("stop"), "end_turn");
    }

    #[test]
    fn anthropic_to_openai_responses_maps() {
        let req = json!({
            "model": "gpt-5",
            "max_tokens": 100,
            "system": "sys",
            "messages": [{"role": "user", "content": "hi"}],
        });
        let r = anthropic_to_openai_responses(&req);
        let input = r["input"].as_array().unwrap();
        assert_eq!(input.len(), 2);
        assert_eq!(input[0]["role"], "system");
        assert_eq!(input[0]["content"], "sys");
        assert_eq!(input[1]["content"], "hi");
        assert_eq!(r["max_output_tokens"], 100);
        assert_eq!(r["model"], "gpt-5");
    }

    #[test]
    fn openai_responses_to_anthropic_maps() {
        let resp = json!({
            "id": "resp_1",
            "output": [{
                "type": "message",
                "content": [{"type": "output_text", "text": "Hello"}],
            }],
            "usage": {"input_tokens": 3, "output_tokens": 2},
        });
        let a = openai_responses_to_anthropic(&resp, "gpt-5");
        assert_eq!(a["type"], "message");
        assert_eq!(a["content"][0]["text"], "Hello");
        assert_eq!(a["usage"]["input_tokens"], 3);
        assert_eq!(a["usage"]["output_tokens"], 2);
    }

    #[test]
    fn openai_responses_stream_maps() {
        let delta = json!({"type": "response.output_text.delta", "delta": "Hi"});
        let frames = openai_responses_stream_chunk_to_anthropic(&delta, "gpt-5");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["delta"]["text"], "Hi");
        let done = json!({"type": "response.completed"});
        let done_frames = openai_responses_stream_chunk_to_anthropic(&done, "gpt-5");
        assert_eq!(done_frames.last().unwrap()["type"], "message_stop");
    }

    #[test]
    fn anthropic_to_gemini_maps() {
        let req = json!({
            "model": "gemini-2.5-pro",
            "max_tokens": 64,
            "system": "sys",
            "messages": [
                {"role": "user", "content": "q"},
                {"role": "assistant", "content": "a"},
            ],
        });
        let g = anthropic_to_gemini(&req);
        let contents = g["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 3);
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[0]["parts"][0]["text"], "sys");
        assert_eq!(contents[1]["parts"][0]["text"], "q");
        // assistant → model
        assert_eq!(contents[2]["role"], "model");
        assert_eq!(contents[2]["parts"][0]["text"], "a");
        assert_eq!(g["generationConfig"]["maxOutputTokens"], 64);
    }

    #[test]
    fn gemini_to_anthropic_maps() {
        let resp = json!({
            "responseId": "gem_1",
            "candidates": [{
                "content": {"parts": [{"text": "Hi"}]},
                "finishReason": "STOP",
            }],
            "usageMetadata": {"promptTokenCount": 4, "candidatesTokenCount": 2},
        });
        let a = gemini_to_anthropic(&resp, "gemini-2.5-pro");
        assert_eq!(a["type"], "message");
        assert_eq!(a["content"][0]["text"], "Hi");
        assert_eq!(a["stop_reason"], "end_turn");
        assert_eq!(a["usage"]["input_tokens"], 4);
        assert_eq!(a["usage"]["output_tokens"], 2);
    }

    #[test]
    fn gemini_stream_maps_deltas() {
        let chunk = json!({
            "candidates": [{
                "content": {"parts": [{"text": "He"}]},
                "finishReason": "",
            }],
        });
        let frames = gemini_stream_chunk_to_anthropic(&chunk, "g");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["delta"]["text"], "He");
        let fin = json!({
            "candidates": [{"content": {"parts": [{"text": ""}]}, "finishReason": "STOP"}],
        });
        let fin_frames = gemini_stream_chunk_to_anthropic(&fin, "g");
        assert_eq!(fin_frames.last().unwrap()["type"], "message_stop");
    }
}
