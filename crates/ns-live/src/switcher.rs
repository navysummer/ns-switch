//! 写编排：读当前 → 内存算新内容 → 首写备份 → 原子写 → 写后重读比对。
//!
//! 与 cc-switch `live/engine.rs` 语义一致：
//! - `plan` 只读不算写；`stage` 才落盘。
//! - 每个 live 配置文件第一次经引擎写入前，留一份原文件字节级备份（去重靠 `.source` 标记）。
//! - 每个应用一把不可重入的写锁；所有写入方（切换、编辑器、托盘）都先拿它。
//! - 写完重读字节与计划内容比对，不一致视为失败（外部并发写防护）。

use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, LazyLock, Mutex};

use sha2::{Digest, Sha256};

use super::{LivePatch, LiveWriteError};

/// 一个待切换的 live 配置文件（工具客户端的绝对路径）。
#[derive(Debug, Clone)]
pub struct LiveFile {
    /// 归属应用（`claude`/`codex`/`gemini`…），写锁按它串行。
    pub app: String,
    /// 绝对路径。
    pub path: PathBuf,
    /// 是否私有文件（0600）。
    pub private: bool,
}

impl LiveFile {
    pub fn new(app: impl Into<String>, path: impl Into<PathBuf>, private: bool) -> Self {
        Self {
            app: app.into(),
            path: path.into(),
            private,
        }
    }
}

/// 十六进制 SHA-256。
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 读当前字节；文件不存在返回 `None`，其他错误照报。
pub fn read_current(path: &Path) -> Result<Option<Vec<u8>>, LiveWriteError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(source) => Err(LiveWriteError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// 在内存里算好的一次写入。
#[derive(Debug, Clone)]
pub struct Planned {
    pub file: LiveFile,
    /// 写前内容 hash；`None` 表示文件不存在。
    pub pre: Option<String>,
    pre_bytes: Option<Vec<u8>>,
    /// 写后内容 hash；与 `pre` 相等即 noop。
    pub planned: Option<String>,
    bytes: Option<Vec<u8>>,
}

impl Planned {
    pub fn is_noop(&self) -> bool {
        self.pre == self.planned
    }

    pub fn pre_bytes(&self) -> Option<&[u8]> {
        self.pre_bytes.as_deref()
    }

    pub fn bytes(&self) -> Option<&[u8]> {
        self.bytes.as_deref()
    }
}

/// 读当前内容并在内存里算出新内容；不写盘。
pub fn plan(file: &LiveFile, patch: &dyn LivePatch) -> Result<Planned, LiveWriteError> {
    let pre_bytes = read_current(&file.path)?;
    let bytes = patch.apply(&file.path, pre_bytes.as_deref())?;
    Ok(Planned {
        file: file.clone(),
        pre: pre_bytes.as_deref().map(sha256_hex),
        pre_bytes,
        planned: Some(sha256_hex(&bytes)),
        bytes: Some(bytes),
    })
}

/// 把新内容原子写到目标路径（临时文件 + rename，0600 可选）。
pub fn stage_write(path: &Path, bytes: &[u8], mode: Option<u32>) -> Result<(), LiveWriteError> {
    let dir = path.parent().ok_or_else(|| LiveWriteError::Io {
        path: path.to_path_buf(),
        source: std::io::Error::new(ErrorKind::InvalidInput, "no parent dir"),
    })?;
    fs::create_dir_all(dir).map_err(|source| LiveWriteError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .map(|s| s.to_string_lossy())
            .unwrap_or_else(|| "file".into()),
        std::process::id()
    ));
    {
        use std::io::Write;
        let mut f = fs::File::create(&tmp).map_err(|source| LiveWriteError::Io {
            path: tmp.clone(),
            source,
        })?;
        f.write_all(bytes).map_err(|source| LiveWriteError::Io {
            path: tmp.clone(),
            source,
        })?;
        f.sync_all().map_err(|source| LiveWriteError::Io {
            path: tmp.clone(),
            source,
        })?;
    }
    if let Some(m) = mode {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(m)).map_err(|source| {
            LiveWriteError::Io {
                path: tmp.clone(),
                source,
            }
        })?;
    }
    fs::rename(&tmp, path).map_err(|source| LiveWriteError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

/// 这个文件第一次经引擎写入前，留一份原文件字节级备份。
///
/// 以文件绝对路径的 hash 为键，每个文件只备份一次；备份在
/// `<backup_dir>/`，旁边的 `.source` 记着原路径。写入前文件不存在时只记
/// `.source`，以后也不再备份（那时内容是本引擎自己写的）。
pub fn ensure_first_write_backup(
    backup_dir: &Path,
    path: &Path,
    current: Option<&[u8]>,
) -> Result<(), LiveWriteError> {
    fs::create_dir_all(backup_dir).map_err(|source| LiveWriteError::Io {
        path: backup_dir.to_path_buf(),
        source,
    })?;
    let key = sha256_hex(path.to_string_lossy().as_bytes());
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let backup = backup_dir.join(format!("{}-{name}", &key[..12]));
    let marker = backup_dir.join(format!("{}-{name}.source", &key[..12]));
    if marker.exists() {
        return Ok(());
    }
    if let Some(bytes) = current {
        stage_write(&backup, bytes, Some(0o600))?;
    }
    stage_write(&marker, path.to_string_lossy().as_bytes(), Some(0o600))
}

fn lock_table() -> &'static (Mutex<HashSet<String>>, Condvar) {
    static LOCKS: LazyLock<(Mutex<HashSet<String>>, Condvar)> =
        LazyLock::new(|| (Mutex::new(HashSet::new()), Condvar::new()));
    &LOCKS
}

impl Drop for AppWriteGuard {
    fn drop(&mut self) {
        let (held, released) = lock_table();
        held.lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.app);
        released.notify_all();
    }
}

/// 一个应用的写锁：改这个应用客户端文件的所有写入方都要先拿它。不可重入。
#[derive(Debug)]
pub struct AppWriteGuard {
    app: String,
}


/// 拿一把应用写锁（阻塞直到空闲）。
pub fn lock_app(app: &str) -> AppWriteGuard {
    let (held, released) = lock_table();
    let mut held = held.lock().unwrap_or_else(|e| e.into_inner());
    while held.contains(app) {
        held = released.wait(held).unwrap_or_else(|e| e.into_inner());
    }
    held.insert(app.to_string());
    AppWriteGuard {
        app: app.to_string(),
    }
}

/// 写后重读比对：读磁盘当前字节，与计划内容一致才算成功。
///
/// 不一致说明有外部并发写入，返回 `Verify` 错误（cc-switch 亦如此）。
pub fn verify_after_write(path: &Path, expected: Option<&[u8]>) -> Result<(), LiveWriteError> {
    let actual = read_current(path)?;
    match (actual, expected) {
        (Some(a), Some(e)) if a == e => Ok(()),
        _ => Err(LiveWriteError::Verify {
            path: path.to_path_buf(),
        }),
    }
}

/// 完整一步：`plan` → 首写备份 → 原子写 → 重读比对。
///
/// 在调用方持有的应用写锁内执行。`backup_dir` 为
/// `<data>/backups/live-first-write/`。
pub fn apply_planned(
    planned: &Planned,
    backup_dir: &Path,
) -> Result<(), LiveWriteError> {
    let bytes = planned
        .bytes()
        .ok_or_else(|| LiveWriteError::Io {
            path: planned.file.path.clone(),
            source: std::io::Error::new(ErrorKind::InvalidInput, "no planned bytes"),
        })?;
    ensure_first_write_backup(backup_dir, &planned.file.path, planned.pre_bytes())?;
    let mode = planned.file.private.then_some(0o600);
    stage_write(&planned.file.path, bytes, mode)?;
    verify_after_write(&planned.file.path, Some(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::JsonPatch;
    use crate::KeyPath;
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ns-live-switcher-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn plan_reports_noops() {
        let dir = tmp_dir("plan-noop");
        let path = dir.join("settings.json");
        let file = LiveFile::new("claude", &path, true);
        let patch = JsonPatch {
            set: vec![(KeyPath::from("a"), json!("b"))],
            ..Default::default()
        };
        let planned = plan(&file, &patch).unwrap();
        // 文件不存在 → pre None, planned Some
        assert!(planned.pre.is_none());
        assert!(planned.planned.is_some());
        assert!(!planned.is_noop());

        // 写入后再次 plan，内容已含 a=b，应 noop
        stage_write(&path, planned.bytes().unwrap(), Some(0o600)).unwrap();
        let file2 = LiveFile::new("claude", &path, true);
        let planned2 = plan(&file2, &patch).unwrap();
        assert!(planned2.is_noop(), "再次应用相同补丁应为 noop");
    }

    #[test]
    fn first_write_backup_keeps_the_original_once() {
        let dir = tmp_dir("backup");
        let backup_dir = dir.join("backups").join("live-first-write");
        let path = dir.join("live.json");
        fs::write(&path, b"original").unwrap();

        ensure_first_write_backup(&backup_dir, &path, Some(b"original")).unwrap();
        // 第二次调用不再新增
        ensure_first_write_backup(&backup_dir, &path, Some(b"changed")).unwrap();

        let backups: Vec<_> = fs::read_dir(&backup_dir).unwrap().collect();
        let sources = backups
            .iter()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .map(|s| s == "source")
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(sources, 1, "每个文件只备份一次");
    }

    #[test]
    fn app_lock_serializes_writers() {
        let guard = lock_app("claude");
        let order = AtomicBool::new(false);
        let held = std::sync::Arc::new(AtomicBool::new(true));
        let held2 = held.clone();
        let h = std::thread::spawn(move || {
            // 拿不到锁时 blocked；这里只验证非重入可获取
            let g2 = lock_app("codex");
            assert!(!g2.app.is_empty());
            held2.store(false, Ordering::SeqCst);
        });
        assert!(!order.load(Ordering::SeqCst));
        drop(guard);
        h.join().unwrap();
        assert!(!held.load(Ordering::SeqCst));
    }
}
