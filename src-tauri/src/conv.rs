//! Anthropic <-> OpenAI request/response format conversion for the local proxy.
//! Conversions are pure JSON transforms (no streaming pipelining): the whole
//! upstream body is buffered, converted, and, when the original request asked
//! for streaming, re-wrapped into the target SSE format in one pass.

use serde_json::{json, Map, Value};

/// which protocol a given CLI tool natively speaks
pub fn client_protocol(tool_id: &str) -> &'static str {
    match tool_id {
        "claude_code" | "claude_desktop" | "hermes" => "anthropic",
        "codex" | "opencode" | "openclaw" | "grok" => "openai",
        _ => "other",
    }
}

fn get_str<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(|x| x.as_str())
}

fn text_from_content(content: &Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        let mut out = String::new();
        for item in arr {
            if let Some(t) = item.get("text").and_then(|x| x.as_str()) {
                out.push_str(t);
            }
        }
        return out;
    }
    String::new()
}

// ---------------------------------------------------------------- requests

/// Anthropic Messages request -> OpenAI chat.completions request
pub fn anth_to_oa_request(input: &Value) -> Result<Value, String> {
    if !input.is_object() {
        return Err("anthropic request is not an object".into());
    }
    let mut out = Map::new();
    if let Some(m) = get_str(input, "model") {
        out.insert("model".into(), json!(m));
    }
    let messages_in = input.get("messages").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let mut messages: Vec<Value> = Vec::new();
    let mut sys: Vec<Value> = Vec::new();
    if let Some(s) = input.get("system") {
        match s {
            Value::String(t) => sys.push(json!({"role":"system","content":t})),
            Value::Array(blocks) => {
                for b in blocks {
                    if let Some(t) = b.get("text").and_then(|x| x.as_str()) {
                        sys.push(json!({"role":"system","content":t}));
                    }
                }
            }
            _ => {}
        }
    }
    messages.extend(sys);

    for msg in messages_in {
        let role = get_str(&msg, "role").unwrap_or("user");
        let content = msg.get("content");
        match role {
            "assistant" => {
                let mut text = String::new();
                let mut tool_calls: Vec<Value> = Vec::new();
                if let Some(c) = content {
                    if let Some(arr) = c.as_array() {
                        for block in arr {
                            match block.get("type").and_then(|x| x.as_str()) {
                                Some("text") => {
                                    if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                                        text.push_str(t);
                                    }
                                }
                                Some("tool_use") => {
                                    let id = get_str(block, "id").unwrap_or("").to_string();
                                    let name = get_str(block, "name").unwrap_or("").to_string();
                                    let arguments = block
                                        .get("input")
                                        .cloned()
                                        .map(|i| i.to_string())
                                        .unwrap_or_else(|| "{}".to_string());
                                    tool_calls.push(json!({
                                        "id": id,
                                        "type": "function",
                                        "function": { "name": name, "arguments": arguments }
                                    }));
                                }
                                _ => {}
                            }
                        }
                    } else if let Some(t) = c.as_str() {
                        text = t.to_string();
                    }
                }
                let mut m = Map::new();
                m.insert("role".into(), json!("assistant"));
                m.insert("content".into(), json!(if text.is_empty() { Value::Null } else { Value::String(text) }));
                if !tool_calls.is_empty() {
                    m.insert("tool_calls".into(), Value::Array(tool_calls));
                }
                messages.push(Value::Object(m));
            }
            "user" => {
                if let Some(c) = content {
                    if let Some(arr) = c.as_array() {
                        let mut parts: Vec<Value> = Vec::new();
                        for block in arr {
                            match block.get("type").and_then(|x| x.as_str()) {
                                Some("text") => {
                                    if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                                        parts.push(json!({"type":"text","text":t}));
                                    }
                                }
                                Some("image") => {
                                    if let Some(src) = block.get("source") {
                                        let mt = get_str(src, "media_type").unwrap_or("image/png");
                                        let data = get_str(src, "data").unwrap_or("");
                                        parts.push(json!({
                                            "type": "image_url",
                                            "image_url": { "url": format!("data:{mt};base64,{data}") }
                                        }));
                                    }
                                }
                                Some("tool_result") => {
                                    let tid = get_str(block, "tool_use_id").unwrap_or("").to_string();
                                    let body = text_from_content(block.get("content").unwrap_or(&Value::Null));
                                    messages.push(json!({
                                        "role": "tool",
                                        "tool_call_id": tid,
                                        "content": body
                                    }));
                                }
                                _ => {}
                            }
                        }
                        if !parts.is_empty() {
                            messages.push(Value::Object(Map::from_iter([
                                ("role".into(), json!("user")),
                                ("content".into(), Value::Array(parts)),
                            ])));
                        }
                    } else if let Some(t) = c.as_str() {
                        messages.push(json!({"role":"user","content":t}));
                    }
                }
            }
            _ => {
                messages.push(msg.clone());
            }
        }
    }
    out.insert("messages".into(), Value::Array(messages));

    if let Some(v) = input.get("max_tokens") {
        out.insert("max_completion_tokens".into(), v.clone());
    }
    for (k, oa_k) in [("temperature", "temperature"), ("top_p", "top_p")] {
        if let Some(v) = input.get(k) {
            out.insert(oa_k.into(), v.clone());
        }
    }
    if let Some(arr) = input.get("stop_sequences").and_then(|x| x.as_array()) {
        out.insert("stop".into(), Value::Array(arr.clone()));
    }
    if let Some(tools) = input.get("tools").and_then(|x| x.as_array()) {
        let mut conv: Vec<Value> = Vec::new();
        for t in tools {
            let name = get_str(t, "name").unwrap_or("").to_string();
            let desc = get_str(t, "description").unwrap_or("").to_string();
            let params = t.get("input_schema").cloned().unwrap_or_else(|| json!({"type":"object"}));
            conv.push(json!({"type":"function","function":{"name":name,"description":desc,"parameters":params}}));
        }
        out.insert("tools".into(), Value::Array(conv));
    }
    if let Some(tc) = input.get("tool_choice") {
        let oa = match tc {
            Value::String(s) => match s.as_str() {
                "any" => json!({"type":"required"}),
                "auto" => json!({"type":"auto"}),
                other => json!({"type":"auto","provider-name":other}),
            },
            Value::Object(_) => {
                let ty = get_str(tc, "type").unwrap_or("auto");
                if ty == "tool" {
                    let name = get_str(tc, "name").unwrap_or("").to_string();
                    json!({"type":"function","function":{"name":name}})
                } else {
                    json!({"type":"auto"})
                }
            }
            _ => json!({"type":"auto"}),
        };
        out.insert("tool_choice".into(), oa);
    }
    if let Some(b) = input.get("stream").and_then(|x| x.as_bool()) {
        out.insert("stream".into(), json!(b));
    }
    Ok(Value::Object(out))
}

/// OpenAI chat.completions request -> Anthropic Messages request
pub fn oa_to_anth_request(input: &Value) -> Result<Value, String> {
    if !input.is_object() {
        return Err("openai request is not an object".into());
    }
    let mut out = Map::new();
    if let Some(m) = get_str(input, "model") {
        out.insert("model".into(), json!(m));
    }
    let mut messages_in = input.get("messages").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let mut sys: Vec<String> = Vec::new();
    let mut messages: Vec<Value> = Vec::new();
    for msg in messages_in.drain(..) {
        let role = get_str(&msg, "role").unwrap_or("user").to_string();
        match role.as_str() {
            "system" => {
                if let Some(s) = msg.get("content").and_then(|x| x.as_str()) {
                    sys.push(s.to_string());
                }
            }
            "user" => {
                let content = msg.get("content");
                let mut parts: Vec<Value> = Vec::new();
                if let Some(arr) = content.and_then(|x| x.as_array()) {
                    for part in arr {
                        match part.get("type").and_then(|x| x.as_str()) {
                            Some("text") => {
                                if let Some(t) = part.get("text").and_then(|x| x.as_str()) {
                                    parts.push(json!({"type":"text","text":t}));
                                }
                            }
                            Some("image_url") => {
                                let url = part
                                    .get("image_url")
                                    .and_then(|u| u.get("url"))
                                    .and_then(|x| x.as_str())
                                    .unwrap_or("");
                                if let Some(rest) = url.strip_prefix("data:") {
                                    if let Some((meta, data)) = rest.split_once(";base64,") {
                                        parts.push(json!({
                                            "type":"image",
                                            "source":{"type":"base64","media_type":meta,"data":data}
                                        }));
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                } else if let Some(t) = content.and_then(|x| x.as_str()) {
                    parts.push(json!({"type":"text","text":t}));
                }
                if !parts.is_empty() {
                    messages.push(json!({"role":"user","content":Value::Array(parts)}));
                }
            }
            "assistant" => {
                let mut blocks: Vec<Value> = Vec::new();
                if let Some(content) = msg.get("content").and_then(|x| x.as_str()) {
                    if !content.is_empty() {
                        blocks.push(json!({"type":"text","text":content}));
                    }
                }
                if let Some(calls) = msg.get("tool_calls").and_then(|x| x.as_array()) {
                    for c in calls {
                        let id = get_str(c, "id").unwrap_or("").to_string();
                        let name = c
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string();
                        let args = c
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("{}");
                        let input_val: Value = serde_json::from_str(args).unwrap_or_else(|_| json!({"raw": args}));
                        blocks.push(json!({"type":"tool_use","id":id,"name":name,"input":input_val}));
                    }
                }
                if blocks.is_empty() {
                    blocks.push(json!({"type":"text","text":""}));
                }
                messages.push(json!({"role":"assistant","content":Value::Array(blocks)}));
            }
            "tool" => {
                let tid = get_str(&msg, "tool_call_id").unwrap_or("").to_string();
                let content = msg.get("content").and_then(|x| x.as_str()).unwrap_or("");
                let cval: Value =
                    serde_json::from_str(content).unwrap_or_else(|_| json!(content));
                messages.push(json!({
                    "role":"user",
                    "content": [ { "type":"tool_result", "tool_use_id": tid, "content": cval } ]
                }));
            }
            _ => {}
        }
    }
    if !sys.is_empty() {
        let system: Vec<Value> = sys.into_iter().map(|s| json!({"type":"text","text":s})).collect();
        out.insert("system".into(), Value::Array(system));
    }
    out.insert("messages".into(), Value::Array(messages));

    out.insert("max_tokens".into(), input.get("max_completion_tokens").or_else(|| input.get("max_tokens")).cloned().unwrap_or(json!(4096)));
    for k in ["temperature", "top_p"] {
        if let Some(v) = input.get(k) {
            out.insert(k.into(), v.clone());
        }
    }
    if let Some(stop) = input.get("stop") {
        let arr: Vec<Value> = match stop {
            Value::String(s) => vec![json!(s)],
            Value::Array(a) => a.clone(),
            _ => Vec::new(),
        };
        if !arr.is_empty() {
            out.insert("stop_sequences".into(), Value::Array(arr));
        }
    }
    if let Some(tools) = input.get("tools").and_then(|x| x.as_array()) {
        let mut conv: Vec<Value> = Vec::new();
        for t in tools {
            let f = t.get("function").cloned().unwrap_or_default();
            let name = get_str(&f, "name").unwrap_or("").to_string();
            let desc = get_str(&f, "description").unwrap_or("").to_string();
            let params = f.get("parameters").cloned().unwrap_or_else(|| json!({"type":"object"}));
            conv.push(json!({
                "name": name,
                "description": desc,
                "input_schema": params,
                "type": "custom"
            }));
        }
        out.insert("tools".into(), Value::Array(conv));
    }
    if let Some(tc) = input.get("tool_choice") {
        let anth = match tc {
            Value::String(s) => match s.as_str() {
                "required" => json!({"type":"any"}),
                _ => json!({"type":"auto"}),
            },
            Value::Object(_) if get_str(tc, "type") == Some("function") => {
                let name = tc.get("function").and_then(|f| f.get("name")).and_then(|x| x.as_str()).unwrap_or("").to_string();
                json!({"type":"tool","name":name})
            }
            _ => json!({"type":"auto"}),
        };
        out.insert("tool_choice".into(), anth);
    }
    if let Some(b) = input.get("stream").and_then(|x| x.as_bool()) {
        out.insert("stream".into(), json!(b));
    }
    Ok(Value::Object(out))
}

// ---------------------------------------------------------------- responses

/// OpenAI chat.completions response -> Anthropic Messages response
pub fn oa_to_anth_response(input: &Value) -> Result<Value, String> {
    if !input.is_object() {
        return Err("openai response is not an object".into());
    }
    let id = get_str(input, "id").unwrap_or("").to_string();
    let model = get_str(input, "model").unwrap_or("").to_string();
    let usage = input.get("usage").cloned().unwrap_or_else(|| json!({}));
    let input_tokens = usage.get("prompt_tokens").and_then(|x| x.as_i64()).unwrap_or(0);
    let output_tokens = usage.get("completion_tokens").and_then(|x| x.as_i64()).unwrap_or(0);

    let mut content: Vec<Value> = Vec::new();
    let mut stop_reason: Value = Value::Null;

    if let Some(choices) = input.get("choices").and_then(|x| x.as_array()) {
        if let Some(first) = choices.first() {
            if let Some(msg) = first.get("message") {
                if let Some(text) = msg.get("content").and_then(|x| x.as_str()) {
                    if !text.is_empty() {
                        content.push(json!({"type":"text","text":text}));
                    }
                }
                if let Some(calls) = msg.get("tool_calls").and_then(|x| x.as_array()) {
                    for c in calls {
                        let tid = get_str(c, "id").unwrap_or("").to_string();
                        let name = c.get("function").and_then(|f| f.get("name")).and_then(|x| x.as_str()).unwrap_or("").to_string();
                        let args = c.get("function").and_then(|f| f.get("arguments")).and_then(|x| x.as_str()).unwrap_or("{}");
                        let input_val: Value = serde_json::from_str(args).unwrap_or_else(|_| json!({"raw": args}));
                        content.push(json!({"type":"tool_use","id":tid,"name":name,"input":input_val}));
                    }
                }
            }
            if let Some(fr) = first.get("finish_reason").and_then(|x| x.as_str()) {
                stop_reason = json!(match fr {
                    "stop" => "end_turn",
                    "tool_calls" => "tool_use",
                    "length" => "max_tokens",
                    "content_filter" => "max_tokens",
                    other => other,
                });
            }
        }
    }
    if content.is_empty() {
        content.push(json!({"type":"text","text":""}));
    }

    Ok(json!({
        "id": if id.starts_with("msg_") { id } else { format!("msg_{id}") },
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": stop_reason,
        "stop_sequence": Value::Null,
        "usage": { "input_tokens": input_tokens, "output_tokens": output_tokens }
    }))
}

/// Anthropic Messages response -> OpenAI chat.completions response
pub fn anth_to_oa_response(input: &Value) -> Result<Value, String> {
    if !input.is_object() {
        return Err("anthropic response is not an object".into());
    }
    let id = get_str(input, "id").unwrap_or("").to_string();
    let model = get_str(input, "model").unwrap_or("").to_string();
    let usage = input.get("usage").cloned().unwrap_or_else(|| json!({}));
    let input_tokens = usage.get("input_tokens").and_then(|x| x.as_i64()).unwrap_or(0);
    let output_tokens = usage.get("output_tokens").and_then(|x| x.as_i64()).unwrap_or(0);

    let mut text: String = String::new();
    let mut tool_calls: Vec<Value> = Vec::new();
    if let Some(content) = input.get("content").and_then(|x| x.as_array()) {
        for block in content {
            match block.get("type").and_then(|x| x.as_str()) {
                Some("text") => {
                    if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                        text.push_str(t);
                    }
                }
                Some("tool_use") => {
                    let tid = get_str(block, "id").unwrap_or("").to_string();
                    let name = get_str(block, "name").unwrap_or("").to_string();
                    let args = block.get("input").cloned().map(|i| i.to_string()).unwrap_or_else(|| "{}".to_string());
                    tool_calls.push(json!({
                        "id": tid,
                        "type": "function",
                        "function": { "name": name, "arguments": args }
                    }));
                }
                _ => {}
            }
        }
    }
    let stop_reason = input.get("stop_reason").and_then(|x| x.as_str()).unwrap_or("");
    let finish = match stop_reason {
        "end_turn" | "stop_sequence" => "stop",
        "tool_use" => "tool_calls",
        "max_tokens" => "length",
        other => other,
    };
    let mut message = Map::new();
    message.insert("role".into(), json!("assistant"));
    message.insert("content".into(), json!(if text.is_empty() { Value::Null } else { Value::String(text) }));
    if !tool_calls.is_empty() {
        message.insert("tool_calls".into(), Value::Array(tool_calls));
    }
    message.insert("refusal".into(), Value::Null);
    let created = chrono::Utc::now().timestamp();
    Ok(json!({
        "id": id,
        "object": "chat.completion",
        "created": created,
        "model": model,
        "choices": [{
            "index": 0,
            "message": Value::Object(message),
            "finish_reason": json!(finish),
            "logprobs": Value::Null
        }],
        "usage": { "prompt_tokens": input_tokens, "completion_tokens": output_tokens, "total_tokens": input_tokens + output_tokens }
    }))
}

// ---------------------------------------------------------------- streaming

/// wrap a full converted response body into an OpenAI SSE stream (single chunk)
pub fn to_openai_sse(body: &Value) -> String {
    let id = body.get("id").and_then(|x| x.as_str()).unwrap_or("chatcmpl-ns").to_string();
    let model = body.get("model").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let created = chrono::Utc::now().timestamp();
    let choice = body.get("choices").and_then(|x| x.as_array()).and_then(|a| a.first()).cloned().unwrap_or_else(|| json!({}));
    let message = choice.get("message").cloned().unwrap_or_else(|| json!({}));
    let content = message.get("content").and_then(|x| x.as_str()).unwrap_or("");
    let tool_calls = message.get("tool_calls").cloned().unwrap_or_else(|| json!([]));
    let finish = choice.get("finish_reason").and_then(|x| x.as_str()).unwrap_or("");

    let mut delta = Map::new();
    delta.insert("role".into(), json!("assistant"));
    delta.insert("content".into(), json!(content));
    if let Some(arr) = tool_calls.as_array() {
        if !arr.is_empty() {
            let indexed: Vec<Value> = arr
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let mut x = c.clone();
                    if let Value::Object(m) = &mut x {
                        m.insert("index".into(), json!(i));
                    }
                    x
                })
                .collect();
            delta.insert("tool_calls".into(), Value::Array(indexed));
        }
    }
    let chunk = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model,
        "choices": [{ "index": 0, "delta": delta, "finish_reason": json!(if finish.is_empty() { Value::Null } else { Value::String(finish.to_string()) }) }]
    });
    format!("data: {chunk}\n\ndata: [DONE]\n\n")
}

/// wrap a full converted response body into an Anthropic SSE event stream
pub fn to_anthropic_sse(body: &Value) -> String {
    let id = body.get("id").and_then(|x| x.as_str()).unwrap_or("msg_ns").to_string();
    let model = body.get("model").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let usage = body.get("usage").cloned().unwrap_or_else(|| json!({}));
    let input_tokens = usage.get("input_tokens").and_then(|x| x.as_i64()).unwrap_or(0);
    let output_tokens = usage.get("output_tokens").and_then(|x| x.as_i64()).unwrap_or(0);
    let stop_reason = body.get("stop_reason").and_then(|x| x.as_str()).unwrap_or("end_turn");

    let mut out = String::new();
    out.push_str("event: message_start\ndata: ");
    out.push_str(&json!({
        "type":"message_start",
        "message":{"id":id,"type":"message","role":"assistant","content":[],"model":model,"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":input_tokens,"output_tokens":0}}
    }).to_string());
    out.push_str("\n\n");

    let mut index = 0usize;
    if let Some(content) = body.get("content").and_then(|x| x.as_array()) {
        let mut text = String::new();
        let mut tools: Vec<(&str, &str, &Value)> = Vec::new();
        for block in content {
            match block.get("type").and_then(|x| x.as_str()) {
                Some("text") => {
                    if let Some(t) = block.get("text").and_then(|x| x.as_str()) {
                        text.push_str(t);
                    }
                }
                Some("tool_use") => {
                    tools.push((
                        block.get("id").and_then(|x| x.as_str()).unwrap_or(""),
                        block.get("name").and_then(|x| x.as_str()).unwrap_or(""),
                        block.get("input").unwrap_or(&Value::Null),
                    ));
                }
                _ => {}
            }
        }
        if !text.is_empty() {
            out.push_str("event: content_block_start\ndata: ");
            out.push_str(&json!({"type":"content_block_start","index":index,"content_block":{"type":"text","text":""}}).to_string());
            out.push_str("\n\n");
            for chunk in text.as_bytes().chunks(1800) {
                let part = String::from_utf8_lossy(chunk).to_string();
                out.push_str("event: content_block_delta\ndata: ");
                out.push_str(&json!({"type":"content_block_delta","index":index,"delta":{"type":"text_delta","text":part}}).to_string());
                out.push_str("\n\n");
            }
            out.push_str("event: content_block_stop\ndata: ");
            out.push_str(&json!({"type":"content_block_stop","index":index}).to_string());
            out.push_str("\n\n");
            index += 1;
        }
        for (tid, name, input_val) in tools {
            out.push_str("event: content_block_start\ndata: ");
            out.push_str(&json!({"type":"content_block_start","index":index,"content_block":{"type":"tool_use","id":tid,"name":name,"input":{}}}).to_string());
            out.push_str("\n\n");
            out.push_str("event: content_block_delta\ndata: ");
            out.push_str(&json!({"type":"content_block_delta","index":index,"delta":{"type":"input_json_delta","partial_json":input_val.to_string()}}).to_string());
            out.push_str("\n\n");
            out.push_str("event: content_block_stop\ndata: ");
            out.push_str(&json!({"type":"content_block_stop","index":index}).to_string());
            out.push_str("\n\n");
            index += 1;
        }
    }
    out.push_str("event: message_delta\ndata: ");
    out.push_str(&json!({
        "type":"message_delta",
        "delta":{"stop_reason":stop_reason,"stop_sequence":null},
        "usage":{"output_tokens":output_tokens}
    }).to_string());
    out.push_str("\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n");
    out
}

// ---------------------------------------------------------------- dispatch

pub fn convert_request(
    source_proto: &str,
    provider_proto: &str,
    body: &mut Value,
) -> Result<(), String> {
    if source_proto == "anthropic" && provider_proto == "openai" {
        let v = anth_to_oa_request(body)?;
        *body = v;
        Ok(())
    } else if source_proto == "openai" && provider_proto == "anthropic" {
        let v = oa_to_anth_request(body)?;
        *body = v;
        Ok(())
    } else {
        Ok(())
    }
}

pub fn convert_response(
    source_proto: &str,
    provider_proto: &str,
    body: &mut Value,
) -> Result<(), String> {
    if source_proto == "anthropic" && provider_proto == "openai" {
        let v = oa_to_anth_response(body)?;
        *body = v;
        Ok(())
    } else if source_proto == "openai" && provider_proto == "anthropic" {
        let v = anth_to_oa_response(body)?;
        *body = v;
        Ok(())
    } else {
        Ok(())
    }
}

/// whether a source request is asking for a streamed response
pub fn is_stream_request(body: &Value, headers: &axum::http::HeaderMap) -> bool {
    if let Some(b) = body.get("stream").and_then(|x| x.as_bool()) {
        if b {
            return true;
        }
    }
    headers
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|a| a.contains("text/event-stream"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anth_req() -> Value {
        serde_json::json!({
            "model": "claude-sonnet-4-5",
            "system": "You are helpful",
            "messages": [
                {"role":"user","content":"hi"},
                {"role":"assistant","content":[{"type":"text","text":"hello"},{"type":"tool_use","id":"t1","name":"get_weather","input":{"city":"SF"}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"sunny"}]}
            ],
            "max_tokens": 1000,
            "tools":[{"name":"get_weather","description":"weather","input_schema":{"type":"object"}}],
            "tool_choice":{"type":"auto"},
            "stream": true
        })
    }

    #[test]
    fn anth_to_oa_roundtrip_request() {
        let a = anth_req();
        let oa = anth_to_oa_request(&a).unwrap();
        assert_eq!(oa["model"], "claude-sonnet-4-5");
        assert_eq!(oa["stream"], true);
        let msgs = oa["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 4, "{msgs:?}"); // system + user + assistant + tool
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[2]["role"], "assistant");
        assert!(msgs[2]["tool_calls"][0]["function"]["name"] == "get_weather");
        assert!(msgs[3]["role"] == "tool");
        assert!(msgs[3]["tool_call_id"] == "t1");
        assert_eq!(oa["max_completion_tokens"], 1000);
        assert_eq!(oa["tools"][0]["function"]["name"], "get_weather");

        let back = oa_to_anth_request(&oa).unwrap();
        assert_eq!(back["system"][0]["text"], "You are helpful");
        let bmsgs = back["messages"].as_array().unwrap();
        assert_eq!(bmsgs.len(), 3);
        assert!(bmsgs[1]["content"].as_array().unwrap().iter().any(|b| b["type"] == "tool_use"));
        assert!(bmsgs[2]["content"].as_array().unwrap().iter().any(|b| b["type"] == "tool_result"));
        assert_eq!(back["max_tokens"], 1000);
    }

    fn oa_req() -> Value {
        serde_json::json!({
            "model":"gpt-4o",
            "messages":[
                {"role":"system","content":"be strict"},
                {"role":"user","content":"hi"},
                {"role":"assistant","content":"ok","tool_calls":[{"id":"c1","type":"function","function":{"name":"calc","arguments":"{\"x\":2}"}}]},
                {"role":"tool","tool_call_id":"c1","content":"4"}
            ],
            "max_completion_tokens":500,
            "stream":true,
            "tools":[{"type":"function","function":{"name":"calc","description":"x","parameters":{"type":"object"}}}]
        })
    }

    #[test]
    fn oa_to_anth_roundtrip_request() {
        let a = oa_req();
        let anth = oa_to_anth_request(&a).unwrap();
        assert_eq!(anth["system"][0]["text"], "be strict");
        let msgs = anth["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        assert!(msgs[1]["content"].as_array().unwrap().iter().any(|b| b["type"] == "tool_use"));
        assert!(msgs[2]["content"].as_array().unwrap().iter().any(|b| b["type"] == "tool_result"));
        assert_eq!(anth["max_tokens"], 500);
        assert_eq!(anth["tools"][0]["name"], "calc");

        let back = anth_to_oa_request(&anth).unwrap();
        let bmsgs = back["messages"].as_array().unwrap();
        assert!(bmsgs.iter().any(|m| m["role"] == "tool"));
        assert!(bmsgs.iter().any(|m| m.get("tool_calls").is_some()));
        assert_eq!(back["max_completion_tokens"], 500);
    }

    #[test]
    fn response_conversion_both_ways() {
        let oa = serde_json::json!({
            "id":"chatcmpl-1","model":"gpt-4o","created":1,
            "choices":[{"index":0,"message":{"role":"assistant","content":"42","tool_calls":[{"id":"c1","type":"function","function":{"name":"calc","arguments":"{\"x\":2}"}}]},"finish_reason":"tool_calls"}],
            "usage":{"prompt_tokens":10,"completion_tokens":5}
        });
        let anth = oa_to_anth_response(&oa).unwrap();
        assert_eq!(anth["type"], "message");
        assert_eq!(anth["stop_reason"], "tool_use");
        assert_eq!(anth["usage"]["input_tokens"], 10);
        assert!(anth["content"].as_array().unwrap().iter().any(|b| b["type"] == "tool_use"));

        let back = anth_to_oa_response(&anth).unwrap();
        assert_eq!(back["object"], "chat.completion");
        assert_eq!(back["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(back["usage"]["prompt_tokens"], 10);
        assert_eq!(back["choices"][0]["message"]["tool_calls"][0]["function"]["name"], "calc");
    }

    #[test]
    fn sse_wrappers_render_events() {
        let anth = serde_json::json!({
            "id":"msg_1","type":"message","role":"assistant","model":"m",
            "content":[{"type":"text","text":"hello world"},{"type":"tool_use","id":"t1","name":"f","input":{"a":1}}],
            "stop_reason":"end_turn","stop_sequence":null,
            "usage":{"input_tokens":1,"output_tokens":2}
        });
        let sse = to_anthropic_sse(&anth);
        assert!(sse.contains("message_start"));
        assert!(sse.contains("\"hello world\""));
        assert!(sse.contains("input_json_delta"));
        assert!(sse.contains("message_stop"));

        let oa = serde_json::json!({
            "id":"x","object":"chat.completion","model":"m",
            "choices":[{"index":0,"message":{"role":"assistant","content":"hello world"},"finish_reason":"stop","logprobs":null}],
            "usage":{"prompt_tokens":1,"completion_tokens":2}
        });
        let sse2 = to_openai_sse(&oa);
        assert!(sse2.contains("chat.completion.chunk"));
        assert!(sse2.contains("[DONE]"));
        assert!(sse2.contains("\"hello world\""));
    }
}