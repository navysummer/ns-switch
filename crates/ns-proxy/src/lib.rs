//! ns-proxy：本地路由代理。
//!
//! 监听本地端口，接收 Anthropic Messages / OpenAI Chat Completions 请求，
//! 经格式转换转发到真实供应商，响应转回。供 Claude Code / Codex / Gemini
//! 等在路由模式下把 live 配置指向 `127.0.0.1`。

pub mod convert;
pub mod failover;
pub mod log;
pub mod rectify;
pub mod server;
pub use failover::{CircuitBreaker, CircuitState, FailoverQueue, probe};
pub use log::{RequestLog, RequestLogEntry};
pub use rectify::{ImageMode, rectify_anthropic};
pub use server::{ProxyConfig, Upstream, bind, serve, serve_listener};

