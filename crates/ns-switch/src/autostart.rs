//! 开机自启：macOS launchd / Linux XDG autostart / Windows 注册表 Run。

use std::path::PathBuf;

const LABEL: &str = "com.ns-switch";
#[cfg(any(target_os = "macos", test))]
const PLIST_REL: &str = "Library/LaunchAgents/com.ns-switch.plist";
#[cfg(any(target_os = "linux", test))]
const DESKTOP_REL: &str = ".config/autostart/ns-switch.desktop";

#[cfg(any(target_os = "windows", test))]
const WIN_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(any(target_os = "windows", test))]
const WIN_RUN_NAME: &str = "ns-switch";

/// macOS plist 路径。
#[cfg(any(target_os = "macos", test))]
pub fn plist_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(PLIST_REL))
}

/// Linux autostart .desktop 路径。
#[cfg(any(target_os = "linux", test))]
pub fn desktop_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(DESKTOP_REL))
}

/// 是否已启用。
pub fn is_enabled() -> bool {
    #[cfg(target_os = "macos")]
    {
        return plist_path().map(|p| p.exists()).unwrap_or(false);
    }
    #[cfg(target_os = "linux")]
    {
        return desktop_path().map(|p| p.exists()).unwrap_or(false);
    }
    #[cfg(target_os = "windows")]
    {
        let out = std::process::Command::new("reg")
            .args(["query", WIN_RUN_KEY, "/v", WIN_RUN_NAME])
            .output();
        return out.map(|o| o.status.success()).unwrap_or(false);
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        false
    }
}

/// 启用开机自启。
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
        let _ = std::process::Command::new("launchctl")
            .args(["load", &path.display().to_string()])
            .status();
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        let path = desktop_path().ok_or("无法定位 HOME")?;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let desktop = linux_desktop_content(&exe.display().to_string());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, desktop).map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let args = win_reg_add_args(&exe.display().to_string());
        let status = std::process::Command::new("reg")
            .args(&args)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("写入注册表 Run 失败".into());
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err("当前平台不支持开机自启".into())
    }
}

/// 禁用开机自启。
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
    #[cfg(target_os = "linux")]
    {
        let path = desktop_path().ok_or("无法定位 HOME")?;
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        let args = win_reg_delete_args();
        let _ = std::process::Command::new("reg")
            .args(&args)
            .status();
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err("当前平台不支持开机自启".into())
    }
}

#[cfg(any(target_os = "linux", test))]
/// Linux XDG autostart `.desktop` 内容（纯函数，便于单测）。
pub fn linux_desktop_content(exe: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=ns-switch\nExec={}\nX-GNOME-Autostart-enabled=true\nTerminal=false\n",
        exe
    )
}

#[cfg(any(target_os = "windows", test))]
/// Windows `reg add` 命令参数（纯函数，便于单测）。
pub fn win_reg_add_args(exe: &str) -> Vec<String> {
    vec![
        "add".into(),
        WIN_RUN_KEY.into(),
        "/v".into(),
        WIN_RUN_NAME.into(),
        "/t".into(),
        "REG_SZ".into(),
        "/d".into(),
        exe.into(),
        "/f".into(),
    ]
}

#[cfg(any(target_os = "windows", test))]
/// Windows `reg delete` 命令参数。
pub fn win_reg_delete_args() -> Vec<String> {
    vec![
        "delete".into(),
        WIN_RUN_KEY.into(),
        "/v".into(),
        WIN_RUN_NAME.into(),
        "/f".into(),
    ]
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_constant() {
        assert_eq!(LABEL, "com.ns-switch");
    }

    #[test]
    fn path_helpers_under_home() {
        if let Some(p) = plist_path() {
            assert!(p.to_string_lossy().contains("Library/LaunchAgents"));
        }
        if let Some(p) = desktop_path() {
            assert!(p.to_string_lossy().contains(".config/autostart"));
        }
    }

    #[test]
    fn linux_desktop_content_valid() {
        let c = linux_desktop_content("/usr/bin/ns-switch");
        assert!(c.starts_with("[Desktop Entry]\n"));
        assert!(c.contains("Type=Application"));
        assert!(c.contains("Exec=/usr/bin/ns-switch"));
        assert!(c.contains("X-GNOME-Autostart-enabled=true"));
        assert!(c.contains("Terminal=false"));
    }

    #[test]
    fn win_reg_add_args_wellformed() {
        let args = win_reg_add_args("C:\\\\ns-switch.exe");
        assert_eq!(args[0], "add");
        assert!(args[1].contains("CurrentVersion\\Run"));
        assert_eq!(args[2], "/v");
        assert_eq!(args[3], "ns-switch");
        assert_eq!(args[4], "/t");
        assert_eq!(args[5], "REG_SZ");
        assert_eq!(args[6], "/d");
        assert_eq!(args[7], "C:\\\\ns-switch.exe");
        assert_eq!(args[8], "/f");
    }

    #[test]
    fn win_reg_delete_args_wellformed() {
        let args = win_reg_delete_args();
        assert_eq!(args[0], "delete");
        assert!(args[1].contains("CurrentVersion\\Run"));
        assert_eq!(args[2], "/v");
        assert_eq!(args[3], "ns-switch");
        assert_eq!(args[4], "/f");
    }
}
