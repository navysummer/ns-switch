//! YAML 补丁器（Hermes `config.yaml`、MCode `config.yaml`、OMP `models.yml`）。
//!
//! `serde_yaml` 解析出语义值；补丁应用到语义值；再序列化。保序通过
//! `serde_yaml` 的 `Mapping`（IndexMap）实现。列表 upsert 按 `name`/`id` 字段
//! 匹配（Hermes `custom_providers`、OMP 模型列表均如此）。

use std::path::{Path, PathBuf};

use serde_yaml::{Mapping, Value};

use super::{KeyPath, LivePatch, LiveWriteError, decode_utf8};

/// 按谓词清空的作用域：父映射里所有命中谓词的键（关键字段）。
#[derive(Clone)]
pub struct YamlClearScope {
    pub parent: KeyPath,
    pub is_floor: fn(&str) -> bool,
}

/// 列表 upsert：在 `parent` 列表里按 `match_field` 字段匹配 `match_value`，
/// 命中则替换整项，否则追加。Hermes `custom_providers` 用 `name`。
#[derive(Clone)]
pub struct YamlListUpsert {
    pub parent: KeyPath,
    pub match_field: &'static str,
    pub match_value: String,
    pub entry: Value,
}

/// 列表删除：按 `match_field` 匹配删除整项。
#[derive(Clone)]
pub struct YamlListRemove {
    pub parent: KeyPath,
    pub match_field: &'static str,
    pub match_value: String,
}

#[derive(Clone, Default)]
pub struct YamlPatch {
    /// 先清空的关键字段。目标值里也有的键不删，留给 `set` 原位改值。
    pub clear: Vec<YamlClearScope>,
    /// 按路径点名清掉的键。
    pub remove: Vec<KeyPath>,
    /// 目标值：已存在就原位改值，不存在就追加。
    pub set: Vec<(KeyPath, Value)>,
    /// 列表 upsert。
    pub list_upsert: Vec<YamlListUpsert>,
    /// 列表删除。
    pub list_remove: Vec<YamlListRemove>,
}

impl YamlPatch {
    pub fn apply_to(&self, path: &Path, doc: &mut Value) -> Result<(), LiveWriteError> {
        if !doc.is_mapping() {
            *doc = Value::Mapping(Mapping::new());
        }

        let targets: Vec<&KeyPath> = self.set.iter().map(|(k, _)| k).collect();

        for scope in &self.clear {
            let Some(map) = mapping_at_mut(doc, &scope.parent.0) else {
                continue;
            };
            let doomed: Vec<String> = map
                .keys()
                .filter_map(|k| k.as_str().map(str::to_string))
                .filter(|key| (scope.is_floor)(key) && !targets.contains(&&scope.parent.child(key)))
                .collect();
            for key in doomed {
                map.remove(Value::String(key));
            }
        }

        for key_path in &self.remove {
            if targets.contains(&key_path) {
                continue;
            }
            let (parent, key) = split(key_path);
            if let Some(map) = mapping_at_mut(doc, &parent) {
                map.remove(Value::String(key));
            }
        }

        for (key_path, value) in &self.set {
            let (parent, key) = split(key_path);
            let map = ensure_mapping_mut(doc, &parent, path)?;
            map.insert(Value::String(key), value.clone());
        }

        for upsert in &self.list_upsert {
            let (parent, list_key) = split(&upsert.parent);
            let map = ensure_mapping_mut(doc, &parent, path)?;
            let list_key = if list_key.is_empty() {
                upsert.parent.0.last().cloned().unwrap_or_default()
            } else {
                list_key
            };
            let seq = match map.get_mut(&Value::String(list_key.clone())) {
                Some(Value::Sequence(seq)) => seq,
                Some(_) => {
                    return Err(LiveWriteError::Shape {
                        path: path.to_path_buf(),
                        key_path: upsert.parent.clone(),
                        expected: "序列",
                    });
                }
                None => {
                    map.insert(Value::String(list_key.clone()), Value::Sequence(Vec::new()));
                    match map.get_mut(&Value::String(list_key)) {
                        Some(Value::Sequence(seq)) => seq,
                        _ => unreachable!(),
                    }
                }
            };
            let match_field = upsert.match_field;
            if let Some(existing) = seq.iter_mut().find(|item| {
                item.get(match_field).and_then(Value::as_str)
                    == Some(upsert.match_value.as_str())
            }) {
                *existing = upsert.entry.clone();
            } else {
                seq.push(upsert.entry.clone());
            }
        }

        for remove in &self.list_remove {
            let (parent, list_key) = split(&remove.parent);
            let Some(map) = mapping_at_mut(doc, &parent) else {
                continue;
            };
            let list_key = if list_key.is_empty() {
                remove.parent.0.last().cloned().unwrap_or_default()
            } else {
                list_key
            };
            let Some(Value::Sequence(seq)) = map.get_mut(&Value::String(list_key)) else {
                continue;
            };
            let match_field = remove.match_field;
            seq.retain(|item| {
                item.get(match_field).and_then(Value::as_str)
                    != Some(remove.match_value.as_str())
            });
        }

        Ok(())
    }
}

impl LivePatch for YamlPatch {
    fn apply(&self, path: &Path, pre: Option<&[u8]>) -> Result<Vec<u8>, LiveWriteError> {
        let mut doc: Value = match pre {
            None => Value::Mapping(Mapping::new()),
            Some(bytes) => {
                let text = decode_utf8(path, bytes)?;
                if text.trim().is_empty() {
                    Value::Mapping(Mapping::new())
                } else {
                    serde_yaml::from_str(&text).map_err(|err| LiveWriteError::Parse {
                        path: path.to_path_buf(),
                        line: err.location().map(|l| l.line()).unwrap_or(1),
                        column: err.location().map(|l| l.column()).unwrap_or(1),
                        message: err.to_string(),
                    })?
                }
            }
        };
        self.apply_to(path, &mut doc)?;
        let mut out = serde_yaml::to_string(&doc).map_err(|err| LiveWriteError::Parse {
            path: path.to_path_buf(),
            line: 1,
            column: 1,
            message: err.to_string(),
        })?;
        if pre.is_none() || pre.is_some_and(|b| !b.is_empty()) {
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
        Ok(out.into_bytes())
    }
}

/// 把 `KeyPath` 分成父路径和末段。
fn split(key_path: &KeyPath) -> (Vec<String>, String) {
    let mut p = key_path.0.clone();
    let key = p.pop().unwrap_or_default();
    (p, key)
}

fn mapping_at_mut<'a>(doc: &'a mut Value, path: &[String]) -> Option<&'a mut Mapping> {
    let mut current = doc;
    for seg in path {
        let map = current.as_mapping_mut()?;
        let next = map.get_mut(Value::String(seg.clone()))?;
        current = next;
    }
    current.as_mapping_mut()
}

fn ensure_mapping_mut<'a>(
    doc: &'a mut Value,
    path: &[String],
    path_buf: &Path,
) -> Result<&'a mut Mapping, LiveWriteError> {
    let mut current = doc;
    for seg in path {
        let map = current.as_mapping_mut().ok_or_else(|| {
            LiveWriteError::NotObject {
                path: path_buf.to_path_buf(),
                key_path: KeyPath(path.to_vec()),
            }
        })?;
        if !map.get(Value::String(seg.clone())).is_some_and(Value::is_mapping) {
            map.insert(Value::String(seg.clone()), Value::Mapping(Mapping::new()));
        }
        current = map.get_mut(Value::String(seg.clone())).expect("just inserted");
    }
    current.as_mapping_mut().ok_or_else(|| LiveWriteError::NotObject {
        path: PathBuf::from("yaml"),
        key_path: KeyPath(path.to_vec()),
    })
}
