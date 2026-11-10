//! 自动更新：检查 GitHub 最新 release 版本（平台差异分支留扩展点）。

/// 当前内置版本（从 Cargo.toml 读取，构建时 env! 不可用则默认）。
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 检查更新：返回最新版本字符串（若有更新）。网络失败返回 `None`。
pub fn check_update() -> Option<String> {
    // GitHub Releases API（仓库与 Cargo.toml `repository` 一致）。
    let url = "https://api.github.com/repos/navysummer/ns-switch/releases/latest";
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
    if is_newer(&latest, CURRENT_VERSION) {
        Some(latest)
    } else {
        None
    }
}

/// 版本号数字分段比较：`latest > current` 返回 `true`。
///
/// 解析 `major.minor.patch[.build]`（非数字段视为 0）。相等或更旧返回 `false`。
pub fn is_newer(latest: &str, current: &str) -> bool {
    let l = parse_version(latest);
    let c = parse_version(current);
    l > c
}

/// 解析版本号为可比较的元组（长度 4，缺省段补 0）。
fn parse_version(v: &str) -> (u64, u64, u64, u64) {
    let mut parts = v
        .trim_start_matches('v')
        .split('.')
        .map(|s| s.chars().take_while(|c| c.is_ascii_digit()).collect::<String>())
        .map(|s| s.parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// 下载最新 release 资产到临时目录并解压安装（macOS/Unix：`install.sh` 或自解压）。
///
/// 返回安装说明。当前实现下载 `ns-switch-<tag>-<os>-<arch>.tar.gz` 资产；
/// 找不到资产返回错误说明。
pub fn download_and_install(tag: &str) -> Result<String, String> {
    let os = if cfg!(target_os = "macos") { "darwin" } else if cfg!(target_os = "windows") { "windows" } else { "linux" };
    let arch = if cfg!(target_arch = "aarch64") { "aarch64" } else { "x86_64" };
    let asset = format!("ns-switch-{tag}-{os}-{arch}.tar.gz");
    let url = format!(
        "https://github.com/navysummer/ns-switch/releases/download/{tag}/{asset}"
    );
    let tmp = std::env::temp_dir().join(format!("ns-switch-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败: {e}"))?;
    let archive = tmp.join(&asset);
    let status = std::process::Command::new("curl")
        .arg("-sSL")
        .arg("--max-time")
        .arg("120")
        .arg("-o")
        .arg(&archive)
        .arg(&url)
        .status()
        .map_err(|e| format!("curl 执行失败: {e}"))?;
    if !status.success() {
        return Err(format!("下载失败（HTTP 非 0）：{asset}"));
    }
    // 解压（tar 在 Unix 可用）。
    let extract = std::process::Command::new("tar")
        .arg("xzf")
        .arg(&archive)
        .arg("-C")
        .arg(&tmp)
        .status()
        .map_err(|e| format!("tar 执行失败: {e}"))?;
    if !extract.success() {
        return Err("解压失败".into());
    }
    // 安装：解压出的二进制复制到 cargo bin（若可用），否则报告位置。
    let bin_dir = std::env::var("CARGO_HOME")
        .map(|h| std::path::PathBuf::from(h).join("bin"))
        .or_else(|_| {
            std::env::var("HOME")
                .map(|h| std::path::PathBuf::from(h).join(".cargo").join("bin"))
        })
        .ok();
    let bin = find_binary(&tmp);
    let Some(bin) = bin else {
        return Ok(format!("已解压到 {}（未找到二进制，请手动安装）", tmp.display()));
    };
    if let Some(bin_dir) = bin_dir {
        if bin_dir.exists() {
            let dest = bin_dir.join("ns-switch");
            std::fs::copy(&bin, &dest).map_err(|e| format!("复制失败: {e}"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
            }
            return Ok(format!("已安装到 {}，重启应用生效", dest.display()));
        }
    }
    Ok(format!("已解压到 {}（请手动把 ns-switch 放入 PATH）", tmp.display()))
}

/// 在解压目录递归查找 `ns-switch` 可执行。
fn find_binary(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    fn walk(d: &std::path::Path) -> Option<std::path::PathBuf> {
        for entry in std::fs::read_dir(d).ok()?.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if let Some(f) = walk(&p) {
                    return Some(f);
                }
            } else if p.file_name().map(|n| n == "ns-switch").unwrap_or(false) {
                return Some(p);
            }
        }
        None
    }
    walk(dir)
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

    #[test]
    fn is_newer_compares_semver() {
        assert!(is_newer("1.2.0", "1.1.9"));
        assert!(is_newer("2.0.0", "1.9.9"));
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("0.9.0", "1.0.0"));
        assert!(!is_newer("1.0.0", "1.0.1"));
    }

    #[test]
    fn is_newer_handles_prefix_and_partial() {
        assert!(is_newer("v2.0", "1.0"));
        assert!(!is_newer("v1.0", "1.0"));
        // 非数字段视为 0。
        assert!(is_newer("1.1-beta", "1.0"));
        assert!(!is_newer("1.0-beta", "1.0"));
    }
}
