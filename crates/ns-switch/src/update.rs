//! 自动更新：检查 GitHub 最新 release 版本（平台差异分支留扩展点）。

/// 当前内置版本（从 Cargo.toml 读取，构建时 env! 不可用则默认）。
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 检查更新：返回最新版本字符串（若有更新）。网络失败返回 `None`。
pub fn check_update() -> Option<String> {
    // GitHub Releases API（占位仓库；真实项目替换 repo）。
    let url = "https://api.github.com/repos/ns-switch/ns-switch/releases/latest";
    let out = std::process::Command::new("curl")
        .arg("-sS")
        .arg("--max-time")
        .arg("5")
        .arg("-H")
        .arg("User-Agent: ns-switch")
        .arg(url)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let tag: serde_json::Value = serde_json::from_str(&text).ok()?;
    let latest = tag.get("tag_name")?.as_str()?.trim_start_matches('v').to_string();
    if latest != CURRENT_VERSION {
        Some(latest)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_version_parses() {
        // 版本必须是 semver 形。
        let parts: Vec<&str> = CURRENT_VERSION.split('.').collect();
        assert!(parts.len() >= 2);
        assert!(parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())));
    }
}
