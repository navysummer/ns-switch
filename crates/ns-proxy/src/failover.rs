//! 故障转移：熔断器 + 健康探测 + 按队列顺序尝试。
//!
//! 纯逻辑部分（熔断状态机、队列选择）不依赖网络；健康探测用 `hyper`
//! 客户端探测上游 `/health`。代理转发时按队列顺序尝试：熔断中的供应商
//! 跳过，成功则重置熔断器，失败累计到阈值后打开熔断（进入冷却）。

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// 熔断状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// 正常，放行。
    Closed,
    /// 失败达阈值，拒绝（冷却期）。
    Open,
    /// 冷却结束，试探性放行一个请求。
    HalfOpen,
}

/// 单供应商熔断器。
#[derive(Debug)]
pub struct CircuitBreaker {
    threshold: u32,
    cooldown: Duration,
    failures: u32,
    opened_at: Option<Instant>,
    /// 半开试探是否已放行（半开期间只放一个）。
    half_open_trial: bool,
}

impl CircuitBreaker {
    pub fn new(threshold: u32, cooldown: Duration) -> Self {
        Self {
            threshold: threshold.max(1),
            cooldown,
            failures: 0,
            opened_at: None,
            half_open_trial: false,
        }
    }

    pub fn state(&self) -> CircuitState {
        match self.opened_at {
            None => CircuitState::Closed,
            Some(t) => {
                if t.elapsed() >= self.cooldown {
                    CircuitState::HalfOpen
                } else {
                    CircuitState::Open
                }
            }
        }
    }

    /// 是否放行请求。
    pub fn allow(&self) -> bool {
        match self.state() {
            CircuitState::Closed => true,
            CircuitState::HalfOpen => !self.half_open_trial,
            CircuitState::Open => false,
        }
    }

    /// 记录成功：重置计数，关闭熔断。
    pub fn record_success(&mut self) {
        self.failures = 0;
        self.opened_at = None;
        self.half_open_trial = false;
    }

    /// 记录失败：累计；达阈值打开熔断；半开试探失败立即重开。
    pub fn record_failure(&mut self) {
        match self.state() {
            CircuitState::HalfOpen => {
                self.opened_at = Some(Instant::now());
                self.half_open_trial = false;
                self.failures = self.threshold;
            }
            CircuitState::Open => {}
            CircuitState::Closed => {
                self.failures += 1;
                if self.failures >= self.threshold {
                    self.opened_at = Some(Instant::now());
                    self.half_open_trial = false;
                }
            }
        }
    }

    /// 半开放行一个试探请求（调用方在 allow 返回 true 且 state=HalfOpen 时调用）。
    pub fn begin_half_open_trial(&mut self) {
        self.half_open_trial = true;
    }
}

/// 故障转移队列：按优先级顺序保存供应商 id。
#[derive(Debug, Default)]
pub struct FailoverQueue {
    order: VecDeque<String>,
}

impl FailoverQueue {
    pub fn new(order: impl IntoIterator<Item = String>) -> Self {
        Self {
            order: order.into_iter().collect(),
        }
    }

    pub fn ids(&self) -> Vec<String> {
        self.order.iter().cloned().collect()
    }

    /// 按队列顺序选出第一个允许放行的供应商（跳过熔断中）。
    pub fn next_available(
        &self,
        breakers: &std::collections::HashMap<String, CircuitBreaker>,
    ) -> Option<String> {
        self.order.iter().find(|id| {
            breakers
                .get(*id)
                .map(|b| b.allow())
                .unwrap_or(true)
        }).cloned()
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

}

/// 探测上游 `/health`（GET）。成功返回 Ok(状态码)。
pub async fn probe(base_url: &str, api_key: &str) -> anyhow::Result<u16> {
    use bytes::Bytes;
    use http_body_util::Full;
    use hyper::Method;
    use hyper_util::rt::TokioExecutor;

    let client = hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build_http();
    let base = base_url.trim_end_matches('/');
    let uri: hyper::Uri = format!("{base}/health")
        .parse()
        .map_err(|e| anyhow::anyhow!("无效健康地址: {e}"))?;
    let req = hyper::Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("authorization", format!("Bearer {api_key}"))
        .body(Full::new(Bytes::new()))
        .map_err(|e| anyhow::anyhow!("构造健康请求失败: {e}"))?;
    let resp = client
        .request(req)
        .await
        .map_err(|e| anyhow::anyhow!("健康探测失败: {e}"))?;
    Ok(resp.status().as_u16())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breaker_opens_after_threshold() {
        let mut b = CircuitBreaker::new(3, Duration::from_secs(60));
        assert_eq!(b.state(), CircuitState::Closed);
        assert!(b.allow());
        b.record_failure();
        b.record_failure();
        assert_eq!(b.state(), CircuitState::Closed);
        b.record_failure();
        assert_eq!(b.state(), CircuitState::Open);
        assert!(!b.allow());
    }

    #[test]
    fn breaker_resets_on_success() {
        let mut b = CircuitBreaker::new(2, Duration::from_secs(60));
        b.record_failure();
        b.record_failure();
        assert_eq!(b.state(), CircuitState::Open);
        b.record_success();
        assert_eq!(b.state(), CircuitState::Closed);
        assert!(b.allow());
    }

    #[test]
    fn breaker_half_open_after_cooldown() {
        // 冷却 0 → 失败后立即半开，试探只放一个。
        let mut b = CircuitBreaker::new(1, Duration::ZERO);
        b.record_failure();
        assert_eq!(b.state(), CircuitState::HalfOpen);
        assert!(b.allow());
        b.begin_half_open_trial();
        assert!(!b.allow());
        // 半开试探失败 → 重开（冷却 0 → 又半开）。
        b.record_failure();
        assert_eq!(b.state(), CircuitState::HalfOpen);
    }

    #[test]
    fn failover_queue_skips_open_breakers() {
        let mut breakers = std::collections::HashMap::new();
        breakers.insert("a".to_string(), CircuitBreaker::new(1, Duration::from_secs(60)));
        breakers.get_mut("a").unwrap().record_failure(); // a 打开
        breakers.insert("b".to_string(), CircuitBreaker::new(1, Duration::from_secs(60)));
        let q = FailoverQueue::new(["a".to_string(), "b".to_string()]);
        assert_eq!(q.next_available(&breakers).as_deref(), Some("b"));
        // 全部熔断 → None。
        breakers.get_mut("b").unwrap().record_failure();
        assert_eq!(q.next_available(&breakers), None);
    }

    #[test]
    fn failover_queue_empty() {
        let q = FailoverQueue::new(Vec::<String>::new());
        assert!(q.is_empty());
        assert_eq!(q.next_available(&Default::default()), None);
    }
}
