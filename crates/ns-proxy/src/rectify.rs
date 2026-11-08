//! 整流器：修正某些上游无法处理的请求。
//!
//! - **Thinking 签名**：Anthropic 的 `thinking` 顶层参数与 `content` 内的
//!   `thinking` 块，部分 OpenAI 兼容上游会拒绝 → 剥离。
//! - **图像回退**：Anthropic `image` 块 → OpenAI `image_url`（data URI），
//!   上游不支持图像时降级为纯文本提示（可配置）。

use serde_json::{Value, json};

/// 整流 Anthropic 请求体。
///
/// - `supports_thinking=false`：删除顶层 `thinking` 参数与内容里的 `thinking` 块。
/// - `image_mode`：`Keep`（转 image_url）| `Drop`（删图像块）| `TextPlaceholder`
///   （替换为文本提示）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageMode {
    Keep,
    Drop,
    TextPlaceholder,
}

/// 整流后的请求体（克隆修改，不改变原值）。
pub fn rectify_anthropic(req: &Value, supports_thinking: bool, image_mode: ImageMode) -> Value {
    let mut out = req.clone();
    if !supports_thinking {
        // 顶层 thinking 参数。
        if out.get("thinking").is_some() {
            out.as_object_mut().map(|m| m.remove("thinking"));
        }
        // 内容里的 thinking 块。
        if let Some(messages) = out.get_mut("messages").and_then(Value::as_array_mut) {
            for m in messages.iter_mut() {
                if let Some(content) = m.get_mut("content").and_then(Value::as_array_mut) {
                    content.retain(|b| {
                        b.get("type").and_then(Value::as_str) != Some("thinking")
                    });
                }
            }
        }
    }
    // 图像处理。
    if image_mode != ImageMode::Keep {
        if let Some(messages) = out.get_mut("messages").and_then(Value::as_array_mut) {
            for m in messages.iter_mut() {
                if let Some(content) = m.get_mut("content").and_then(Value::as_array_mut) {
                    let mut i = 0;
                    while i < content.len() {
                        let is_image = content[i].get("type").and_then(Value::as_str) == Some("image");
                        if is_image {
                            match image_mode {
                                ImageMode::Drop => {
                                    content.remove(i);
                                    continue;
                                }
                                ImageMode::TextPlaceholder => {
                                    content[i] = json!({"type": "text", "text": "[图像已省略]"});
                                }
                                ImageMode::Keep => {}
                            }
                        }
                        i += 1;
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_thinking_param_and_blocks() {
        let req = json!({
            "model": "claude",
            "thinking": {"type": "enabled", "budget_tokens": 100},
            "messages": [
                {"role": "user", "content": [
                    {"type": "thinking", "thinking": "思考中"},
                    {"type": "text", "text": "你好"},
                ]},
            ],
        });
        let out = rectify_anthropic(&req, false, ImageMode::Keep);
        assert!(out.get("thinking").is_none());
        let content = out["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
    }

    #[test]
    fn keeps_thinking_when_supported() {
        let req = json!({
            "thinking": {"type": "enabled"},
            "messages": [{"role": "user", "content": [{"type": "thinking", "thinking": "x"}]}],
        });
        let out = rectify_anthropic(&req, true, ImageMode::Keep);
        assert!(out.get("thinking").is_some());
        assert_eq!(out["messages"][0]["content"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn drops_images_when_requested() {
        let req = json!({
            "messages": [{"role": "user", "content": [
                {"type": "text", "text": "看"},
                {"type": "image", "source": {"type": "base64", "data": "x"}},
            ]}],
        });
        let out = rectify_anthropic(&req, true, ImageMode::Drop);
        let content = out["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
    }

    #[test]
    fn replaces_images_with_placeholder() {
        let req = json!({
            "messages": [{"role": "user", "content": [
                {"type": "image", "source": {"type": "base64", "data": "x"}},
            ]}],
        });
        let out = rectify_anthropic(&req, true, ImageMode::TextPlaceholder);
        let content = out["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content[0]["text"], "[图像已省略]");
    }
}
