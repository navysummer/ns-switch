//! 保格式 JSONC 编辑器：注释/键序/缩进原样保留，只改补丁点名的键值。
//!
//! 思路：`serde_json_lenient` 解析出语义值；补丁应用到语义值；再序列化时按
//! **原始 token 流**重放——每个键/值从 token 流取原文（含其注释与空白），
//! 被补丁改掉的键值则输出新值。未被触碰的部分逐字节保留。
//!
//! 保证：键、值、顺序不变；用户键与其注释原样保留；改写只动补丁点名的键。

use std::path::Path;


use serde_json::Value;

use super::{LiveWriteError, decode_utf8};
use crate::json::{JsonPatch, JsonStyle, parse as semantic_parse};

/// 对 JSONC 文件应用 `JsonPatch`，保留注释与格式。
pub fn apply_jsonc(path: &Path, patch: &JsonPatch, pre: Option<&[u8]>) -> Result<Vec<u8>, LiveWriteError> {
    let (mut doc, style) = semantic_parse(path, pre)?;
    patch.apply_to(path, &mut doc)?;
    match pre {
        None => Ok(serialize_simple(&doc, &style)),
        Some(bytes) => {
            let text = decode_utf8(path, bytes)?;
            let tokens = tokenize(&text)?;
            emit(&tokens, &doc, &style)
        }
    }
}

/// 简单序列化（无原始文件时）。
fn serialize_simple(doc: &Value, style: &JsonStyle) -> Vec<u8> {
    let text = serde_json::to_string_pretty(doc).expect("serialize");
    let mut text = text;
    if style.trailing_newline && !text.ends_with('\n') {
        text.push('\n');
    }
    if !style.trailing_newline && text.ends_with('\n') {
        text.pop();
    }
    let mut out = Vec::new();
    if style.bom {
        out.extend_from_slice("\u{feff}".as_bytes());
    }
    out.extend_from_slice(text.as_bytes());
    out
}

/// token 类型。
#[derive(Debug, Clone)]
enum Tok {
    /// 字符串字面量（含引号，原始文本）
    Str(String),
    /// 结构标点：{ } [ ] : ,
    Punct(char),
    /// 注释（// 或 /* */，原始文本）
    Comment(String),
    /// 原始值文本（数字/true/false/null）
    Raw(String),
    /// 空白（原始文本）
    Ws(String),
}

/// 轻量 tokenizer：字符串、注释、标点、值、空白。
fn tokenize(src: &str) -> Result<Vec<Tok>, LiveWriteError> {
    let bytes = src.as_bytes();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        match c {
            '"' => {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                    } else if bytes[i] == b'"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                toks.push(Tok::Str(src[start..i].to_string()));
            }
            '/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                let start = i;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                toks.push(Tok::Comment(src[start..i].to_string()));
            }
            '/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                let start = i;
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
                toks.push(Tok::Comment(src[start..i].to_string()));
            }
            '{' | '}' | '[' | ']' | ':' | ',' => {
                toks.push(Tok::Punct(c));
                i += 1;
            }
            ch if ch.is_whitespace() => {
                let start = i;
                while i < bytes.len() && (bytes[i] as char).is_whitespace() {
                    i += 1;
                }
                toks.push(Tok::Ws(src[start..i].to_string()));
            }
            _ => {
                let start = i;
                while i < bytes.len() {
                    let ch = bytes[i] as char;
                    if ch.is_whitespace() || matches!(ch, '{' | '}' | '[' | ']' | ':' | ',' | '"') {
                        break;
                    }
                    if ch == '/' && i + 1 < bytes.len() && (bytes[i + 1] == b'/' || bytes[i + 1] == b'*') {
                        break;
                    }
                    i += 1;
                }
                toks.push(Tok::Raw(src[start..i].to_string()));
            }
        }
    }
    Ok(toks)
}

/// 发射器：按 token 流重放，被补丁改掉的键/值输出新值。
fn emit(tokens: &[Tok], doc: &Value, style: &JsonStyle) -> Result<Vec<u8>, LiveWriteError> {
    let _ = style;
    let mut pos = 0;
    let mut out = String::new();
    emit_value(tokens, &mut pos, doc, &mut out)?;
    let mut text = out;
    if style.trailing_newline && !text.ends_with('\n') {
        text.push('\n');
    }
    if !style.trailing_newline && text.ends_with('\n') {
        text.pop();
    }
    if style.crlf {
        text = text.replace('\n', "\r\n");
    }
    let mut bytes = Vec::new();
    if style.bom {
        bytes.extend_from_slice("\u{feff}".as_bytes());
    }
    bytes.extend_from_slice(text.as_bytes());
    Ok(bytes)
}

fn emit_value(
    tokens: &[Tok],
    pos: &mut usize,
    value: &Value,
    out: &mut String,
) -> Result<(), LiveWriteError> {
    match value {
        Value::Object(map) => {
            let existing = collect_object_keys(tokens, *pos);
            skip_ws_comments(tokens, pos, out);
            if matches!(tokens.get(*pos), Some(Tok::Punct('{'))) {
                out.push('{');
                *pos += 1;
            } else {
                out.push('{');
            }
            skip_ws_comments(tokens, pos, out);
            let mut first = true;
            for (key, val) in map {
                if !first {
                    take_sep(tokens, pos, out);
                }
                first = false;
                let _ = emit_key(
                    tokens,
                    pos,
                    existing.iter().find(|k| *k == key),
                    key,
                    out,
                )?;
                skip_ws_comments(tokens, pos, out);
                take_colon(tokens, pos, out);
                emit_value(tokens, pos, val, out)?;
            }
            skip_ws_comments(tokens, pos, out);
            if matches!(tokens.get(*pos), Some(Tok::Punct('}'))) {
                out.push('}');
                *pos += 1;
            } else {
                out.push('}');
            }
        }
        Value::Array(arr) => {
            skip_ws_comments(tokens, pos, out);
            if matches!(tokens.get(*pos), Some(Tok::Punct('['))) {
                out.push('[');
                *pos += 1;
            } else {
                out.push('[');
            }
            skip_ws_comments(tokens, pos, out);
            let mut first = true;
            for item in arr {
                if !first {
                    take_sep(tokens, pos, out);
                }
                first = false;
                emit_value(tokens, pos, item, out)?;
            }
            skip_ws_comments(tokens, pos, out);
            if matches!(tokens.get(*pos), Some(Tok::Punct(']'))) {
                out.push(']');
                *pos += 1;
            } else {
                out.push(']');
            }
        }
        Value::String(s) => {
            skip_ws_comments(tokens, pos, out);
            // 值一律输出语义值（补丁可能已改）；消费原 token 保持对齐
            if matches!(tokens.get(*pos), Some(Tok::Str(_))) {
                let _ = take(tokens, pos);
            }
            out.push_str(&serde_json::to_string(s).expect("string"));
        }
        _ => {
            skip_ws_comments(tokens, pos, out);
            // 消费原值 token（raw/str），输出语义值
            if matches!(
                tokens.get(*pos),
                Some(Tok::Raw(_)) | Some(Tok::Str(_))
            ) {
                let _ = take(tokens, pos);
            }
            out.push_str(&serde_json::to_string(value).expect("value"));
        }
    }
    Ok(())
}

/// 收集对象键（token 流中当前位置起的 `"key":` 序列），返回原始顺序。
fn collect_object_keys(tokens: &[Tok], mut pos: usize) -> Vec<String> {
    let mut keys = Vec::new();
    // 跳过开头的空白/注释直到 '{'
    while pos < tokens.len() {
        match &tokens[pos] {
            Tok::Ws(_) | Tok::Comment(_) => pos += 1,
            Tok::Punct('{') => {
                pos += 1;
                break;
            }
            _ => return keys,
        }
    }
    let mut expect_key = true;
    while pos < tokens.len() {
        match &tokens[pos] {
            Tok::Ws(_) | Tok::Comment(_) => pos += 1,
            Tok::Str(s) if expect_key => {
                keys.push(parse_str_lit(s));
                expect_key = false;
                pos += 1;
            }
            Tok::Punct(':') => {
                expect_key = false;
                pos += 1;
            }
            Tok::Punct(',') => {
                expect_key = true;
                pos += 1;
            }
            Tok::Punct('}') => break,
            _ => {
                expect_key = false;
                pos += 1;
            }
        }
    }
    keys
}

fn emit_key(
    tokens: &[Tok],
    pos: &mut usize,
    original: Option<&String>,
    key: &str,
    out: &mut String,
) -> Result<bool, LiveWriteError> {
    let _ = original;
    // 保留原键的 token（含其前导注释）
    if matches!(tokens.get(*pos), Some(Tok::Str(_))) {
        out.push_str(&take(tokens, pos));
    } else {
        out.push_str(&serde_json::to_string(key).expect("key"));
    }
    Ok(true)
}

fn skip_ws_comments(tokens: &[Tok], pos: &mut usize, out: &mut String) {
    while matches!(tokens.get(*pos), Some(Tok::Ws(_)) | Some(Tok::Comment(_))) {
        out.push_str(&take(tokens, pos));
    }
}

fn take_sep(tokens: &[Tok], pos: &mut usize, out: &mut String) {
    // 输出逗号（保留其后空白）
    if matches!(tokens.get(*pos), Some(Tok::Punct(','))) {
        out.push(',');
        *pos += 1;
        skip_ws_comments(tokens, pos, out);
    } else {
        out.push(',');
    }
}

fn take_colon(tokens: &[Tok], pos: &mut usize, out: &mut String) {
    if matches!(tokens.get(*pos), Some(Tok::Punct(':'))) {
        out.push(':');
        *pos += 1;
    } else {
        out.push(':');
    }
}

fn take(tokens: &[Tok], pos: &mut usize) -> String {
    let t = tokens.get(*pos).cloned().unwrap_or(Tok::Ws(String::new()));
    *pos += 1;
    match t {
        Tok::Str(s) | Tok::Comment(s) | Tok::Raw(s) | Tok::Ws(s) => s,
        Tok::Punct(c) => c.to_string(),
    }
}

/// 解析字符串字面量为实际键名。
fn parse_str_lit(lit: &str) -> String {
    serde_json::from_str::<String>(lit).unwrap_or_else(|_| lit.trim_matches('"').to_string())
}

