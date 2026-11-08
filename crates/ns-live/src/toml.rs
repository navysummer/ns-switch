//! TOML 补丁器（Codex、Grok Build 的 `config.toml`），基于 `toml_edit`：注释、空行、
//! 键的写法和表的位置都原样保留，只改补丁点名的键。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, Table, Value};

use super::{KeyPath, LivePatch, LiveWriteError, decode_utf8};

/// 按谓词清空的作用域：父表里所有命中谓词的键。
#[derive(Clone)]
pub struct TomlClearScope {
    pub parent: KeyPath,
    pub is_floor: fn(&str) -> bool,
}

#[derive(Clone, Default)]
pub struct TomlPatch {
    /// 先清空的关键字段。目标值里也有的键不删，留给 `set` 原位改值。
    pub clear: Vec<TomlClearScope>,
    /// 按路径点名清掉的键或整张表（嵌在用户表里的模型名、上次写入的供应商表）。
    pub remove: Vec<KeyPath>,
    /// 目标值：已存在就原位替换（保留原来的位置和空白），不存在就追加。
    pub set: Vec<(KeyPath, Item)>,
    /// 缺失时才写入。
    pub seed: Vec<(KeyPath, Item)>,
    /// 当前值等于其中之一才删除（不看空白和注释）。`set` 里有同一路径时跳过。
    pub remove_if: Vec<(KeyPath, Vec<Value>)>,
}

impl TomlPatch {
    pub fn apply_to(&self, path: &Path, doc: &mut DocumentMut) -> Result<(), LiveWriteError> {
        let targets: HashSet<&KeyPath> = self.set.iter().map(|(k, _)| k).collect();
        let root = doc.as_table_mut();

        for scope in &self.clear {
            let Some(table) = table_at_mut(root, &scope.parent.0) else {
                continue;
            };
            let doomed: Vec<String> = table
                .iter()
                .map(|(key, _)| key.to_string())
                .filter(|key| (scope.is_floor)(key) && !targets.contains(&scope.parent.child(key)))
                .collect();
            for key in doomed {
                table.remove(&key);
            }
        }

        for key_path in &self.remove {
            if targets.contains(key_path) {
                continue;
            }
            let (parent, key) = split(key_path);
            if let Some(table) = table_at_mut(root, parent) {
                table.remove(key);
            }
        }

        for (key_path, item) in &self.set {
            let (parent, key) = split(key_path);
            let table = ensure_table_mut(root, parent, key_path)?;
            match table.get_mut(key) {
                Some(slot) => {
                    // 保留旧值的注释（prefix/suffix decor），只换值
                    let decor = slot
                        .as_value()
                        .map(|v| v.decor().clone())
                        .or_else(|| slot.as_table().map(|t| t.decor().clone()));
                    *slot = item.clone();
                    if let Some(d) = decor {
                        if let Some(v) = slot.as_value_mut() {
                            *v.decor_mut() = d;
                        } else if let Some(t) = slot.as_table_mut() {
                            *t.decor_mut() = d;
                        }
                    }
                }
                None => {
                    table.insert(key, item.clone());
                }
            }
        }

        for (key_path, item) in &self.seed {
            let (parent, key) = split(key_path);
            let table = ensure_table_mut(root, parent, key_path)?;
            if !table.contains_key(key) {
                table.insert(key, item.clone());
            }
        }

        for (key_path, values) in &self.remove_if {
            if targets.contains(key_path) {
                continue;
            }
            let (parent, key) = split(key_path);
            let Some(table) = table_at_mut(root, parent) else {
                continue;
            };
            let matches = table.get(key).is_some_and(|current| {
                let Some(current) = current.as_value() else {
                    return false;
                };
                values.iter().any(|v| current.to_string() == v.to_string())
            });
            if matches {
                table.remove(key);
            }
        }

        let _ = path;
        Ok(())
    }
}

impl LivePatch for TomlPatch {
    fn apply(&self, path: &Path, pre: Option<&[u8]>) -> Result<Vec<u8>, LiveWriteError> {
        let (mut doc, style) = parse(path, pre)?;
        self.apply_to(path, &mut doc)?;
        serialize(&doc, &style)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TomlStyle {
    pub trailing_newline: bool,
    pub crlf: bool,
}

impl Default for TomlStyle {
    fn default() -> Self {
        Self {
            trailing_newline: false,
            crlf: false,
        }
    }
}

pub fn parse(path: &Path, pre: Option<&[u8]>) -> Result<(DocumentMut, TomlStyle), LiveWriteError> {
    let Some(bytes) = pre else {
        return Ok((DocumentMut::new(), TomlStyle::default()));
    };
    let text = decode_utf8(path, bytes)?;
    if text.trim().is_empty() {
        return Ok((DocumentMut::new(), TomlStyle::default()));
    }
    let doc: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| {
        let (line, column) = (e.span().map(|s| s.start).unwrap_or(0), 0);
        LiveWriteError::Parse {
            path: path.to_path_buf(),
            line,
            column,
            message: e.to_string(),
        }
    })?;
    let style = TomlStyle {
        trailing_newline: text.ends_with('\n'),
        crlf: text.contains("\r\n"),
    };
    Ok((doc, style))
}

fn serialize(doc: &DocumentMut, style: &TomlStyle) -> Result<Vec<u8>, LiveWriteError> {
    let mut text = doc.to_string();
    if !text.ends_with('\n') && style.trailing_newline {
        text.push('\n');
    }
    if text.ends_with('\n') && !style.trailing_newline {
        text.pop();
    }
    if style.crlf {
        text = text.replace('\n', "\r\n");
    }
    Ok(text.into_bytes())
}

fn split(key_path: &KeyPath) -> (&[String], &str) {
    match key_path.0.split_last() {
        Some((last, parent)) => (parent, last.as_str()),
        None => (&[], ""),
    }
}

fn table_at_mut<'a>(root: &'a mut Table, parent: &[String]) -> Option<&'a mut Table> {
    let mut current: &mut Table = root;
    for segment in parent {
        match current.get_mut(segment)? {
            Item::Table(t) => current = t,
            _ => return None,
        }
    }
    Some(current)
}

fn ensure_table_mut<'a>(
    root: &'a mut Table,
    parent: &[String],
    key_path: &KeyPath,
) -> Result<&'a mut Table, LiveWriteError> {
    let mut current = root;
    for segment in parent {
        if !current.contains_key(segment) {
            current.insert(segment, Item::Table(Table::new()));
        }
        current = match current.get_mut(segment) {
            Some(Item::Table(t)) => t,
            _ => {
                return Err(LiveWriteError::NotObject {
                    path: PathBuf::new(),
                    key_path: key_path.clone(),
                })
            }
        };
    }
    Ok(current)
}

/// 从 JSON 值构造 toml_edit Item（用于 `set`）。
pub fn item_from_json(value: &serde_json::Value) -> Item {
    match value {
        serde_json::Value::String(s) => Item::Value(Value::from(s.clone())),
        serde_json::Value::Bool(b) => Item::Value(Value::from(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Item::Value(Value::from(i))
            } else if let Some(f) = n.as_f64() {
                Item::Value(Value::from(f))
            } else {
                Item::Value(Value::from(0))
            }
        }
        serde_json::Value::Null => Item::Value(Value::from("")),
        serde_json::Value::Array(arr) => {
            let mut v = toml_edit::Array::new();
            for e in arr {
                let item = item_from_json(e);
                if let Some(val) = item.into_value().ok() {
                    v.push(val);
                }
            }
            Item::Value(Value::Array(v))
        }
        serde_json::Value::Object(obj) => {
            let mut t = Table::new();
            for (k, v) in obj {
                t.insert(k, item_from_json(v));
            }
            Item::Table(t)
        }
    }
}

/// 从 JSON 值构造 toml_edit 值（用于 `remove_if` 比较）。
pub fn value_from_json(value: &serde_json::Value) -> Value {
    item_from_json(value)
        .into_value()
        .unwrap_or_else(|_| Value::from(""))
}


