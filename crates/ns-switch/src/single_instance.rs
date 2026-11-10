//! 单实例 + 深链转发。
//!
//! 策略：回环 TCP 固定端口。启动时尝试连接：
//! - 连接成功 → 已有实例在跑：把启动参数里的 `nsswitch://` URL 发给它，
//!   返回 [`TryPrimary::Secondary`]（调用方应立即退出）。
//! - 连接失败 → 本进程成为主实例：bind 监听，后台线程接收新实例转发的
//!   URL，经 `mpsc` 推给主线程（由 `SwitchApp` 的后台轮询消费）。
//!
//! 端口冲突（被无关进程占用）→ 回退：不启用单实例，直接成为主实例，
//! 深链仅按启动参数处理一次。绝不阻塞启动。

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

/// 固定端口（高位段，避免与常用服务冲突）。
const PORT: u16 = 47137;

/// 单实例启动结果。
pub enum TryPrimary {
    /// 本进程是主实例：`rx` 接收后续深链 URL。
    Primary(Receiver<String>),
    /// 已有实例在跑：已把深链转发过去，调用方应退出。
    Secondary,
    /// 端口被无关进程占用：不启用单实例，仅按启动参数处理。
    Fallback,
}

/// 尝试成为主实例。`argv_urls` 是启动参数里的深链 URL（仅主实例处理）。
///
/// 若已有实例在跑，把 `argv_urls` 转发给旧实例并返回 `Secondary`。
pub fn try_primary(argv_urls: Vec<String>) -> TryPrimary {
    // 先探测：连接成功说明已有实例。
    if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", PORT)) {
        for url in &argv_urls {
            let _ = writeln!(stream, "{url}");
        }
        let _ = stream.flush();
        return TryPrimary::Secondary;
    }

    // 尝试成为主实例。
    let listener = match TcpListener::bind(("127.0.0.1", PORT)) {
        Ok(l) => l,
        Err(_) => return TryPrimary::Fallback,
    };

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let tx: Arc<Mutex<Sender<String>>> = Arc::new(Mutex::new(tx));

    // 后台线程：accept 循环，每连接读取行并转发。
    thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(stream) = conn else { continue };
            let tx = tx.clone();
            thread::spawn(move || {
                let reader = BufReader::new(stream);
                for line in reader.lines().map_while(Result::ok) {
                    if line.starts_with("nsswitch://") {
                        if let Ok(tx) = tx.lock() {
                            let _ = tx.send(line);
                        }
                    }
                }
            });
        }
    });

    TryPrimary::Primary(rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_is_in_expected_range() {
        // 防止意外改成 0（动态分配）或常用端口。
        assert!(PORT > 1024 && PORT < 65535);
    }
}
