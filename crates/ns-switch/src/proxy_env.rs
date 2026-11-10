//! 全局出站代理：为 ns-switch 自身出站请求（curl 子进程）应用代理环境变量。
//!
//! 语义与 cc-switch `GlobalProxySettings` 对齐：ns-switch 自己的 API 调用
//! （用量查询、GitHub 搜索、models.dev 定价、更新检查）经用户配置的出站代理，
//! 不影响客户端工具配置。`127.0.0.1`/`localhost` 走直连（本地路由代理不受影响）。

/// 常见本地代理端口：(端口, 主要协议, 是否同时支持 http+socks5)。
pub const PROXY_PORTS: &[(u16, &str, bool)] = &[
    (7890, "http", true),     // Clash mixed
    (7891, "socks5", false),  // Clash SOCKS
    (1080, "socks5", false),  // 通用 SOCKS5
    (8080, "http", false),    // 通用 HTTP
    (8888, "http", false),    // Charles/Fiddler
    (3128, "http", false),    // Squid
    (10808, "socks5", false), // V2Ray SOCKS
    (10809, "http", false),   // V2Ray HTTP
];

/// 从代理 URL 生成 curl 环境变量（`HTTPS_PROXY`/`HTTP_PROXY`/`ALL_PROXY` +
/// `NO_PROXY` 本地直连）。空 URL 返回空（直连）。
pub fn curl_env(proxy: &str) -> Vec<(&'static str, String)> {
    let p = proxy.trim();
    if p.is_empty() {
        return vec![
            ("NO_PROXY", "127.0.0.1,localhost".to_string()),
            ("no_proxy", "127.0.0.1,localhost".to_string()),
        ];
    }
    vec![
        ("HTTPS_PROXY", p.to_string()),
        ("https_proxy", p.to_string()),
        ("HTTP_PROXY", p.to_string()),
        ("http_proxy", p.to_string()),
        ("ALL_PROXY", p.to_string()),
        ("all_proxy", p.to_string()),
        ("NO_PROXY", "127.0.0.1,localhost".to_string()),
        ("no_proxy", "127.0.0.1,localhost".to_string()),
    ]
}

/// 测试代理连接：经代理请求多个公共端点，任一成功即可用。
/// 返回 (成功, 延迟毫秒, 错误)。
pub fn test_proxy(url: &str) -> (bool, u64, Option<String>) {
    let url = url.trim().to_string();
    if url.is_empty() {
        return (false, 0, Some("代理 URL 为空".into()));
    }
    let start = std::time::Instant::now();
    let env = curl_env(&url);
    let targets = [
        "https://api.anthropic.com",
        "https://httpbin.org/get",
        "https://www.google.com",
    ];
    for t in targets {
        let out = std::process::Command::new("curl")
            .args(["-sS", "-o", "/dev/null", "-w", "%{http_code}", "-m", "8", "-I", t])
            .envs(env.iter().map(|(k, v)| (*k, v.clone())))
            .output();
        if let Ok(o) = out {
            let code = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if o.status.success() || code.starts_with("2") || code.starts_with("3") {
                return (true, start.elapsed().as_millis() as u64, None);
            }
        }
    }
    (
        false,
        start.elapsed().as_millis() as u64,
        Some("所有测试目标不可达".into()),
    )
}

/// 扫描本地常见代理端口，返回 (URL, 协议, 端口)。
pub fn scan_local_proxies() -> Vec<(String, String, u16)> {
    let mut found = Vec::new();
    for &(port, primary, mixed) in PROXY_PORTS {
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        if std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(100)).is_ok()
        {
            found.push((
                format!("{primary}://127.0.0.1:{port}"),
                primary.to_string(),
                port,
            ));
            if mixed {
                let alt = if primary == "http" { "socks5" } else { "http" };
                found.push((format!("{alt}://127.0.0.1:{port}"), alt.to_string(), port));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curl_env_sets_proxy_vars() {
        let env = curl_env("http://127.0.0.1:7890");
        let get = |k: &str| env.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone());
        assert_eq!(get("HTTPS_PROXY").as_deref(), Some("http://127.0.0.1:7890"));
        assert_eq!(get("HTTP_PROXY").as_deref(), Some("http://127.0.0.1:7890"));
        assert_eq!(get("ALL_PROXY").as_deref(), Some("http://127.0.0.1:7890"));
        assert_eq!(get("NO_PROXY").as_deref(), Some("127.0.0.1,localhost"));
    }

    #[test]
    fn curl_env_empty_means_direct() {
        let env = curl_env("  ");
        let get = |k: &str| env.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone());
        assert_eq!(get("HTTPS_PROXY"), None);
        // 仍设置 NO_PROXY 防本地回环被系统代理影响
        assert_eq!(get("NO_PROXY").as_deref(), Some("127.0.0.1,localhost"));
    }

    #[test]
    fn curl_env_socks5_passthrough() {
        let env = curl_env("socks5://127.0.0.1:1080");
        let get = |k: &str| env.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone());
        assert_eq!(get("ALL_PROXY").as_deref(), Some("socks5://127.0.0.1:1080"));
    }

    #[test]
    fn scan_common_ports_format() {
        // 无本地代理时通常为空；格式正确性由常量保证
        let _ = scan_local_proxies();
        assert!(!PROXY_PORTS.is_empty());
        assert!(PROXY_PORTS.iter().all(|(p, t, _)| *p > 0 && !t.is_empty()));
    }

    #[test]
    fn test_proxy_empty_url_fails() {
        let (ok, _, err) = test_proxy("");
        assert!(!ok);
        assert!(err.is_some());
    }
}
