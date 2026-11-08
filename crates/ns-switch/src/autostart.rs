//! 开机自启（macOS launchd LaunchAgent）。
//!
//! 写 `~/Library/LaunchAgents/com.ns-switch.plist` 并 `launchctl load`。
//! 非 macOS（未实现）返回 `Ok(false)`（不启用）。

use std::path::PathBuf;

const LABEL: &str = "com.ns-switch";
const PLIST_REL: &str = "Library/LaunchAgents/com.ns-switch.plist";

/// plist 路径。
pub fn plist_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(PLIST_REL))
}

/// 是否已启用（plist 存在）。
pub fn is_enabled() -> bool {
    plist_path().map(|p| p.exists()).unwrap_or(false)
}

/// 启用开机自启（写 plist + launchctl load）。
pub fn enable() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let path = plist_path().ok_or("无法定位 HOME")?;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <false/>
</dict>
</plist>
"#,
            label = LABEL,
            exe = exe.display(),
        );
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, plist).map_err(|e| e.to_string())?;
        let status = std::process::Command::new("launchctl")
            .args(["load", &path.display().to_string()])
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            // load 失败但文件已写：仍算启用（下次登录生效）。
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = plist_path();
        Err("开机自启仅支持 macOS".into())
    }
}

/// 禁用开机自启（unload + 删 plist）。
pub fn disable() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let path = plist_path().ok_or("无法定位 HOME")?;
        if path.exists() {
            let _ = std::process::Command::new("launchctl")
                .args(["unload", &path.display().to_string()])
                .status();
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = plist_path();
        Err("开机自启仅支持 macOS".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_constant() {
        assert_eq!(LABEL, "com.ns-switch");
    }

    #[test]
    fn plist_path_under_home() {
        if let Some(p) = plist_path() {
            assert!(p.to_string_lossy().contains("Library/LaunchAgents"));
        }
    }
}
