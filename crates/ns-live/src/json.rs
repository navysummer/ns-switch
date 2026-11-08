//! JSON 补丁器（Claude Code `settings.json`、Claude Desktop profile 等）。
//!
//! `serde_json` 开 `preserve_order` 保持键序；删除用 `shift_remove`（`indexmap`），
//! 替换原位改值，新增追加到父对象末尾。重新序列化沿用原缩进/换行/BOM/CRLF。
use std::path::{Path, PathBuf};
use std::collections::HashSet;


use serde_json::{Map, Value};

use super::{KeyPath, LivePatch, LiveWriteError, decode_utf8};

/// 按谓词清空的作用域：父对象里所有命中谓词的键（关键字段）。
#[derive(Clone)]
pub struct ClearScope {
    pub parent: KeyPath,
    pub is_floor: fn(&str) -> bool,
}

#[derive(Clone, Default)]
pub struct JsonPatch {
    /// 先清空的关键字段。目标值里也有的键不删，留给 `set` 原位改值。
    pub clear: Vec<ClearScope>,
    /// 按路径点名清掉的键（嵌在用户对象里的关键字段）。`set` 里有同一路径时跳过。
    pub remove: Vec<KeyPath>,
    /// 目标值：已存在就原位改值，不存在就追加到父对象末尾（父对象缺失时一并创建）。
    pub set: Vec<(KeyPath, Value)>,
    /// 缺失时才写入。
    pub seed: Vec<(KeyPath, Value)>,
    /// 当前值等于其中之一才删除（独有字段切走、残留清理）。`set` 里有同一路径时跳过。
    pub remove_if: Vec<(KeyPath, Vec<Value>)>,
}

impl JsonPatch {
    pub fn apply_to(&self, _path: &Path, doc: &mut Value) -> Result<(), LiveWriteError> {
        let targets: HashSet<&KeyPath> = self.set.iter().map(|(k, _)| k).collect();

        for scope in &self.clear {
            let Some(map) = object_at_mut(doc, &scope.parent.0) else {
                continue;
            };
            let doomed: Vec<String> = map
                .keys()
                .filter(|key| (scope.is_floor)(key) && !targets.contains(&scope.parent.child(key)))
                .cloned()
                .collect();
            for key in doomed {
                map.shift_remove(&key);
            }
        }

        for key_path in &self.remove {
            if targets.contains(key_path) {
                continue;
            }
            let (parent, key) = split(key_path);
            if let Some(map) = object_at_mut(doc, parent) {
                map.shift_remove(key);
            }
        }

        for (key_path, value) in &self.set {
            let (parent, key) = split(key_path);
            let map = ensure_object_mut(doc, parent, key_path)?;
            match map.get_mut(key) {
                Some(slot) => *slot = value.clone(),
                None => {
                    map.insert(key.to_string(), value.clone());
                }
            }
        }

        for (key_path, value) in &self.seed {
            let (parent, key) = split(key_path);
            let map = ensure_object_mut(doc, parent, key_path)?;
            if !map.contains_key(key) {
                map.insert(key.to_string(), value.clone());
            }
        }

        for (key_path, values) in &self.remove_if {
            if targets.contains(key_path) {
                continue;
            }
            let (parent, key) = split(key_path);
            let Some(map) = object_at_mut(doc, parent) else {
                continue;
            };
            if map.get(key).is_some_and(|current| values.contains(current)) {
                map.shift_remove(key);
            }
        }

        Ok(())
    }
}

impl LivePatch for JsonPatch {
    fn apply(&self, path: &Path, pre: Option<&[u8]>) -> Result<Vec<u8>, LiveWriteError> {
        crate::jsonc::apply_jsonc(path, self, pre)
    }
}

/// 文件原有的排版习惯，重新序列化时沿用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonStyle {
    pub indent: Vec<u8>,
    pub trailing_newline: bool,
    pub crlf: bool,
    pub bom: bool,
}

impl Default for JsonStyle {
    fn default() -> Self {
        Self {
            indent: b"  ".to_vec(),
            trailing_newline: false,
            crlf: false,
            bom: false,
        }
    }
}

/// 解析写前内容。文件不存在或只有空白时视为空对象（唯一允许的「空底」）；
/// 顶层不是对象时报错，不覆盖。
pub fn parse(path: &Path, pre: Option<&[u8]>) -> Result<(Value, JsonStyle), LiveWriteError> {
    let Some(bytes) = pre else {
        return Ok((Value::Object(Map::new()), JsonStyle::default()));
    };
    let mut text = decode_utf8(path, bytes)?;
    let bom = text.starts_with('\u{feff}');
    if bom {
        text.drain(..'\u{feff}'.len_utf8());
    }
    if text.trim().is_empty() {
        return Ok((
            Value::Object(Map::new()),
            JsonStyle {
                bom,
                ..JsonStyle::default()
            },
        ));
    }

    let doc: Value = serde_json_lenient::from_str(&text).map_err(|err| LiveWriteError::Parse {
        path: path.to_path_buf(),
        line: err.line(),
        column: err.column(),
        message: err.to_string(),
    })?;
    if !doc.is_object() {
        return Err(LiveWriteError::Shape {
            path: path.to_path_buf(),
            key_path: KeyPath::root(),
            expected: "对象",
        });
    }

    let style = JsonStyle {
        indent: detect_indent(&text).unwrap_or_else(|| b"  ".to_vec()),
        trailing_newline: text.ends_with('\n'),
        crlf: text.contains("\r\n"),
        bom,
    };
    Ok((doc, style))
}

fn detect_indent(text: &str) -> Option<Vec<u8>> {
    let mut spaces = None;
    let mut tabs = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed == "{"
            || trimmed == "}"
            || trimmed == "["
            || trimmed == "]"
        {
            continue;
        }
        if line.starts_with('\t') {
            tabs = true;
        } else {
            let leading = line.len() - line.trim_start_matches(' ').len();
            if leading > 0 {
                spaces = Some(leading);
            }
        }
        if spaces.is_some() || tabs {
            break;
        }
    }
    if tabs {
        Some(b"\t".to_vec())
    } else {
        spaces.map(|n| vec![b' '; n.min(8)])
    }
}

fn split(key_path: &KeyPath) -> (&[String], &str) {
    match key_path.0.split_last() {
        Some((last, parent)) => (parent, last.as_str()),
        None => (&[], ""),
    }
}

fn object_at_mut<'a>(
    doc: &'a mut Value,
    parent: &[String],
) -> Option<&'a mut Map<String, Value>> {
    let mut current = doc;
    for segment in parent {
        current = current.get_mut(segment)?;
    }
    current.as_object_mut()
}

fn ensure_object_mut<'a>(
    doc: &'a mut Value,
    parent: &[String],
    key_path: &KeyPath,
) -> Result<&'a mut Map<String, Value>, LiveWriteError> {
    let mut current = doc;
    for segment in parent {
        if current.is_object() {
            if !current.get(segment).is_some_and(|v| v.is_object()) {
                current
                    .as_object_mut()
                    .expect("is_object")
                    .insert(segment.clone(), Value::Object(Map::new()));
            }
        } else {
            return Err(LiveWriteError::NotObject {
                path: PathBuf::new(),
                key_path: key_path.clone(),
            });
        }
        current = current.get_mut(segment).expect("just inserted");
    }
    Ok(current.as_object_mut().expect("parent ensured object"))
}


