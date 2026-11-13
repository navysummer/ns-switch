//! 首选终端启动（对齐 cc-switch `TerminalSettings`）。
//!
//! 设置页可选终端（macOS：Terminal/iTerm2/Ghostty/WezTerm/Warp/kitty/Alacritty；
//! Linux：GNOME Terminal/Konsole/Xfce/kitty/Alacritty/xterm；Windows：cmd/
//! PowerShell/Windows Terminal）。「打开配置目录」「在终端恢复会话」都按此选择
//! 启动；命令构造是纯函数（`*_spec`），便于单测与跨平台核对。

use std::path::Path;
use std::process::Command;

/// 当前平台可选终端：`(存储值, 显示名)`。
pub fn options() -> &'static [(&'static str, &'static str)] {
    #[cfg(target_os = "macos")]
    {
        &[
            ("terminal", "Terminal"),
            ("iterm2", "iTerm2"),
            ("ghostty", "Ghostty"),
            ("wezterm", "WezTerm"),
            ("warp", "Warp"),
            ("kitty", "kitty"),
            ("alacritty", "Alacritty"),
        ]
    }
    #[cfg(target_os = "linux")]
    {
        &[
            ("gnome-terminal", "GNOME Terminal"),
            ("konsole", "Konsole"),
            ("xfce4-terminal", "Xfce Terminal"),
            ("kitty", "kitty"),
            ("alacritty", "Alacritty"),
            ("xterm", "xterm"),
        ]
    }
    #[cfg(target_os = "windows")]
    {
        &[
            ("cmd", "命令提示符"),
            ("powershell", "PowerShell"),
            ("wt", "Windows Terminal"),
        ]
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        &[]
    }
}

/// 平台默认终端值。
pub fn default_value() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "terminal"
    }
    #[cfg(target_os = "linux")]
    {
        "gnome-terminal"
    }
    #[cfg(target_os = "windows")]
    {
        "cmd"
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        ""
    }
}

/// 规范化设置值：空或本平台不支持 → 平台默认。
pub fn resolve(pref: &str) -> &'static str {
    if let Some((value, _)) = options().iter().find(|(v, _)| *v == pref) {
        return value;
    }
    default_value()
}

/// Shell 单引号转义（路径含空格/引号时安全）。
#[cfg(any(target_os = "macos", target_os = "linux", test))]
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// AppleScript 字符串转义。
#[cfg(any(target_os = "macos", test))]
fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// 启动规格：程序 + 参数。
pub type Spec = (String, Vec<String>);

/// macOS 启动规格。`command = None` 表示仅打开目录。
#[cfg(any(target_os = "macos", test))]
pub fn macos_spec(pref: &str, dir: &str, command: Option<&str>) -> Spec {
    let full = match command {
        Some(c) => format!("cd {} && {}", sh_quote(dir), c),
        None => String::new(),
    };
    match (pref, command.is_some()) {
        ("iterm2", false) => ("open".into(), vec!["-a".into(), "iTerm".into(), dir.into()]),
        ("iterm2", true) => (
            "osascript".into(),
            vec![
                "-e".into(),
                format!(
                    "tell application \"iTerm\" to create window with default profile command \"{}\"",
                    applescript_escape(&full)
                ),
            ],
        ),
        ("ghostty", false) => (
            "open".into(),
            vec![
                "-a".into(),
                "Ghostty".into(),
                "--args".into(),
                format!("--working-directory={dir}"),
            ],
        ),
        ("ghostty", true) => (
            "open".into(),
            vec![
                "-a".into(),
                "Ghostty".into(),
                "--args".into(),
                format!("--working-directory={dir}"),
                "-e".into(),
                "sh".into(),
                "-c".into(),
                full,
            ],
        ),
        ("wezterm", false) => (
            "wezterm".into(),
            vec!["start".into(), "--cwd".into(), dir.into()],
        ),
        ("wezterm", true) => (
            "wezterm".into(),
            vec![
                "start".into(),
                "--cwd".into(),
                dir.into(),
                "--".into(),
                "sh".into(),
                "-c".into(),
                full,
            ],
        ),
        ("warp", false) => ("open".into(), vec!["-a".into(), "Warp".into(), dir.into()]),
        ("kitty", false) => (
            "open".into(),
            vec![
                "-a".into(),
                "kitty".into(),
                "--args".into(),
                "--directory".into(),
                dir.into(),
            ],
        ),
        ("kitty", true) => (
            "open".into(),
            vec![
                "-a".into(),
                "kitty".into(),
                "--args".into(),
                "--directory".into(),
                dir.into(),
                "sh".into(),
                "-c".into(),
                full,
            ],
        ),
        ("alacritty", false) => (
            "open".into(),
            vec![
                "-a".into(),
                "Alacritty".into(),
                "--args".into(),
                "--working-directory".into(),
                dir.into(),
            ],
        ),
        ("alacritty", true) => (
            "open".into(),
            vec![
                "-a".into(),
                "Alacritty".into(),
                "--args".into(),
                "--working-directory".into(),
                dir.into(),
                "-e".into(),
                "sh".into(),
                "-c".into(),
                full,
            ],
        ),
        (_, true) => (
            "osascript".into(),
            vec![
                "-e".into(),
                format!(
                    "tell application \"Terminal\" to do script \"{}\"",
                    applescript_escape(&full)
                ),
            ],
        ),
        _ => (
            "open".into(),
            vec!["-a".into(), "Terminal".into(), dir.into()],
        ),
    }
}

/// Linux 启动规格。`command = None` 表示仅打开目录。
#[cfg(any(target_os = "linux", test))]
pub fn linux_spec(pref: &str, dir: &str, command: Option<&str>) -> Spec {
    let full = match command {
        Some(c) => format!("cd {} && {}; exec $SHELL", sh_quote(dir), c),
        None => String::new(),
    };
    match (pref, command.is_some()) {
        ("gnome-terminal", false) => (
            "gnome-terminal".into(),
            vec![format!("--working-directory={dir}")],
        ),
        ("gnome-terminal", true) => (
            "gnome-terminal".into(),
            vec![format!("--working-directory={dir}"), "--".into(), "sh".into(), "-c".into(), full],
        ),
        ("konsole", false) => ("konsole".into(), vec!["--workdir".into(), dir.into()]),
        ("konsole", true) => (
            "konsole".into(),
            vec![
                "--workdir".into(),
                dir.into(),
                "-e".into(),
                "sh".into(),
                "-c".into(),
                full,
            ],
        ),
        ("xfce4-terminal", false) => (
            "xfce4-terminal".into(),
            vec![format!("--working-directory={dir}")],
        ),
        ("xfce4-terminal", true) => (
            "xfce4-terminal".into(),
            vec![
                format!("--working-directory={dir}"),
                "-x".into(),
                "sh".into(),
                "-c".into(),
                full,
            ],
        ),
        ("kitty", false) => ("kitty".into(), vec!["--directory".into(), dir.into()]),
        ("kitty", true) => (
            "kitty".into(),
            vec![
                "--directory".into(),
                dir.into(),
                "sh".into(),
                "-c".into(),
                full,
            ],
        ),
        ("alacritty", false) => (
            "alacritty".into(),
            vec!["--working-directory".into(), dir.into()],
        ),
        ("alacritty", true) => (
            "alacritty".into(),
            vec![
                "--working-directory".into(),
                dir.into(),
                "-e".into(),
                "sh".into(),
                "-c".into(),
                full,
            ],
        ),
        (_, true) => (
            "xterm".into(),
            vec![
                "-cd".into(),
                dir.into(),
                "-e".into(),
                "sh".into(),
                "-c".into(),
                full,
            ],
        ),
        _ => ("xterm".into(), vec!["-cd".into(), dir.into()]),
    }
}

/// Windows 启动规格。`command = None` 表示仅打开目录。
#[cfg(any(target_os = "windows", test))]
pub fn windows_spec(pref: &str, dir: &str, command: Option<&str>) -> Spec {
    let dir_lit = dir.replace('\'', "''");
    let full = match command {
        Some(c) => format!("cd /d \"{dir}\" && {c}"),
        None => String::new(),
    };
    match (pref, command.is_some()) {
        ("powershell", false) => (
            "powershell".into(),
            vec![
                "-NoExit".into(),
                "-Command".into(),
                format!("Set-Location -LiteralPath '{dir_lit}'"),
            ],
        ),
        ("powershell", true) => (
            "powershell".into(),
            vec![
                "-NoExit".into(),
                "-Command".into(),
                format!("Set-Location -LiteralPath '{dir_lit}'; {}", command.unwrap_or("")),
            ],
        ),
        ("wt", false) => (
            "wt".into(),
            vec!["-d".into(), dir.into()],
        ),
        ("wt", true) => (
            "wt".into(),
            vec!["-d".into(), dir.into(), "cmd".into(), "/K".into(), full],
        ),
        (_, false) => ("cmd".into(), vec!["/K".into(), format!("cd /d \"{dir}\"")]),
        (_, true) => ("cmd".into(), vec!["/K".into(), full]),
    }
}

/// 按当前平台生成启动规格。
pub fn spec(pref: &str, dir: &str, command: Option<&str>) -> Spec {
    #[cfg(target_os = "macos")]
    {
        macos_spec(resolve(pref), dir, command)
    }
    #[cfg(target_os = "linux")]
    {
        linux_spec(resolve(pref), dir, command)
    }
    #[cfg(target_os = "windows")]
    {
        windows_spec(resolve(pref), dir, command)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = (pref, dir, command);
        (String::new(), Vec::new())
    }
}

/// 在首选终端打开目录（`command = None`）或执行命令。
pub fn launch(dir: &Path, command: Option<&str>, pref: &str) -> std::io::Result<()> {
    let dir_str = dir.to_string_lossy().into_owned();
    let (program, args) = spec(pref, &dir_str, command);
    if program.is_empty() {
        return Err(std::io::Error::other("unsupported platform"));
    }
    Command::new(program).args(args).spawn().map(|_| ())
}

/// 在首选终端执行命令（会话恢复）。
pub fn run_command(dir: &Path, command: &str, pref: &str) -> std::io::Result<()> {
    launch(dir, Some(command), pref)
}

/// 在首选终端打开目录。
pub fn open_at(dir: &Path, pref: &str) -> std::io::Result<()> {
    launch(dir, None, pref)
}

#[cfg(test)]
mod tests {
    use super::{linux_spec, macos_spec, resolve, sh_quote, windows_spec};

    #[test]
    fn default_value_is_supported() {
        let v = resolve("");
        assert!(super::options().iter().any(|(value, _)| *value == v));
        // 未知值回退默认。
        assert_eq!(resolve("definitely-not-a-terminal"), v);
    }

    #[test]
    fn sh_quote_handles_spaces_and_quotes() {
        assert_eq!(sh_quote("/tmp/a b"), "'/tmp/a b'");
        assert_eq!(sh_quote("/tmp/it's"), r"'/tmp/it'\''s'");
    }

    #[test]
    fn macos_terminal_open_uses_open_app() {
        let (prog, args) = macos_spec("terminal", "/tmp/p", None);
        assert_eq!(prog, "open");
        assert_eq!(args, vec!["-a", "Terminal", "/tmp/p"]);
    }

    #[test]
    fn macos_terminal_command_uses_applescript() {
        let (prog, args) = macos_spec("terminal", "/tmp/p", Some("claude --resume abc"));
        assert_eq!(prog, "osascript");
        let script = &args[1];
        assert!(script.contains("do script"));
        assert!(script.contains("cd '/tmp/p' && claude --resume abc"));
    }

    #[test]
    fn macos_ghostty_and_wezterm_honor_directory() {
        let (prog, args) = macos_spec("ghostty", "/tmp/p", Some("codex resume x"));
        assert_eq!(prog, "open");
        assert!(args.iter().any(|a| a == "--working-directory=/tmp/p"));
        let (prog, args) = macos_spec("wezterm", "/tmp/p", Some("codex resume x"));
        assert_eq!(prog, "wezterm");
        assert!(args.iter().any(|a| a == "--cwd"));
        assert!(args.iter().any(|a| a == "/tmp/p"));
    }

    #[test]
    fn linux_specs_use_workdir_flags() {
        let (prog, args) = linux_spec("gnome-terminal", "/tmp/p", None);
        assert_eq!(prog, "gnome-terminal");
        assert_eq!(args, vec!["--working-directory=/tmp/p"]);
        let (_, args) = linux_spec("konsole", "/tmp/p", Some("echo hi"));
        assert!(args.iter().any(|a| a == "--workdir"));
        let (_, args) = linux_spec("xterm", "/tmp/p", None);
        assert!(args.iter().any(|a| a == "-cd"));
    }

    #[test]
    fn windows_specs_use_d_cd() {
        let (prog, args) = windows_spec("cmd", r"C:\tmp", None);
        assert_eq!(prog, "cmd");
        assert!(args.iter().any(|a| a.contains("cd /d \"")));
        let (prog, args) = windows_spec("wt", r"C:\tmp", Some("claude --resume x"));
        assert_eq!(prog, "wt");
        assert!(args.iter().any(|a| a == "-d"));
        let (prog, args) = windows_spec("powershell", r"C:\tmp", None);
        assert_eq!(prog, "powershell");
        assert!(args.iter().any(|a| a.contains("Set-Location")));
    }
}
