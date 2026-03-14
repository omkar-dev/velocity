use std::future::Future;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::error::{ErrorKind, Result, VelocityError};
use crate::traits::{HealthStatus, PlatformDriver};
use crate::types::{DeviceInfo, Direction, Element, Key, Selector};

/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub backoff_multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_secs(5),
            backoff_multiplier: 2.0,
        }
    }
}

/// Circuit breaker states.
const CLOSED: u8 = 0;
const OPEN: u8 = 1;
const HALF_OPEN: u8 = 2;

/// Prevents cascade failures when the underlying driver becomes unresponsive.
///
/// v2 enhancements:
/// - Gradual ramp-up in half-open state (allows limited requests before full close)
/// - Tracing events on state transitions for observability
/// - Configurable from YAML
pub struct CircuitBreaker {
    state: AtomicU8,
    failure_count: AtomicU32,
    last_failure_epoch_ms: AtomicU32,
    half_open_successes: AtomicU32,
    threshold: u32,
    reset_timeout: Duration,
    half_open_max_requests: u32,
    name: String,
}

impl CircuitBreaker {
    pub fn new(threshold: u32, reset_timeout: Duration) -> Self {
        Self::named("global", threshold, reset_timeout, 2)
    }

    /// Create a named circuit breaker (e.g., per-device).
    pub fn named(name: &str, threshold: u32, reset_timeout: Duration, half_open_max: u32) -> Self {
        Self {
            state: AtomicU8::new(CLOSED),
            failure_count: AtomicU32::new(0),
            last_failure_epoch_ms: AtomicU32::new(0),
            half_open_successes: AtomicU32::new(0),
            threshold,
            reset_timeout,
            half_open_max_requests: half_open_max,
            name: name.to_string(),
        }
    }

    pub fn is_open(&self) -> bool {
        let state = self.state.load(Ordering::SeqCst);
        if state == OPEN {
            let last = self.last_failure_epoch_ms.load(Ordering::SeqCst);
            let now = epoch_ms_u32();
            if now.saturating_sub(last) > self.reset_timeout.as_millis() as u32 {
                tracing::info!(breaker = %self.name, "circuit breaker transitioning to half-open");
                self.state.store(HALF_OPEN, Ordering::SeqCst);
                self.half_open_successes.store(0, Ordering::SeqCst);
                return false;
            }
            return true;
        }
        false
    }

    /// Current state name for diagnostics.
    pub fn state_name(&self) -> &'static str {
        match self.state.load(Ordering::SeqCst) {
            CLOSED => "closed",
            OPEN => "open",
            HALF_OPEN => "half-open",
            _ => "unknown",
        }
    }

    pub fn on_success(&self) {
        let prev_state = self.state.load(Ordering::SeqCst);
        if prev_state == HALF_OPEN {
            let successes = self.half_open_successes.fetch_add(1, Ordering::SeqCst) + 1;
            if successes >= self.half_open_max_requests {
                tracing::info!(
                    breaker = %self.name,
                    successes,
                    "circuit breaker closing after successful half-open ramp-up"
                );
                self.failure_count.store(0, Ordering::SeqCst);
                self.state.store(CLOSED, Ordering::SeqCst);
            }
        } else {
            self.failure_count.store(0, Ordering::SeqCst);
            self.state.store(CLOSED, Ordering::SeqCst);
        }
    }

    pub fn on_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        self.last_failure_epoch_ms
            .store(epoch_ms_u32(), Ordering::SeqCst);

        let prev_state = self.state.load(Ordering::SeqCst);

        if prev_state == HALF_OPEN {
            // Any failure in half-open goes straight back to open
            tracing::warn!(
                breaker = %self.name,
                "circuit breaker re-opening from half-open after failure"
            );
            self.state.store(OPEN, Ordering::SeqCst);
        } else if count >= self.threshold {
            tracing::warn!(
                breaker = %self.name,
                failures = count,
                threshold = self.threshold,
                "circuit breaker opening"
            );
            self.state.store(OPEN, Ordering::SeqCst);
        }
    }

    /// Failure count for diagnostics.
    pub fn failure_count(&self) -> u32 {
        self.failure_count.load(Ordering::SeqCst)
    }
}

fn epoch_ms_u32() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u32
}

/// A driver wrapper that automatically retries transient failures
/// and trips a circuit breaker on sustained failures.
pub struct ResilientDriver {
    inner: Arc<dyn PlatformDriver>,
    retry_policy: RetryPolicy,
    circuit_breaker: CircuitBreaker,
}

impl ResilientDriver {
    pub fn new(inner: Arc<dyn PlatformDriver>) -> Self {
        Self {
            inner,
            retry_policy: RetryPolicy::default(),
            circuit_breaker: CircuitBreaker::new(5, Duration::from_secs(30)),
        }
    }

    pub fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }

    pub fn inner(&self) -> &Arc<dyn PlatformDriver> {
        &self.inner
    }

    async fn with_retry<F, Fut, T>(&self, operation: F) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        if self.circuit_breaker.is_open() {
            return Err(VelocityError::Config(
                "Circuit breaker is open — driver unresponsive".to_string(),
            ));
        }

        let max = self.retry_policy.max_retries;
        let mut backoff = self.retry_policy.initial_backoff;

        for attempt in 0..=max {
            match operation().await {
                Ok(v) => {
                    self.circuit_breaker.on_success();
                    return Ok(v);
                }
                Err(e) => {
                    let kind = e.kind();
                    let retries_for_kind = kind.max_retries().min(max);

                    if attempt < retries_for_kind && !kind.is_permanent() {
                        self.circuit_breaker.on_failure();

                        // Try session restart for session-related errors
                        if kind == ErrorKind::SessionExpired || kind == ErrorKind::ConnectionLost {
                            let _ = self.inner.restart_session().await;
                        }

                        tokio::time::sleep(backoff).await;
                        backoff = Duration::from_secs_f64(
                            (backoff.as_secs_f64() * self.retry_policy.backoff_multiplier)
                                .min(self.retry_policy.max_backoff.as_secs_f64()),
                        );
                        continue;
                    }

                    self.circuit_breaker.on_failure();
                    return Err(e);
                }
            }
        }

        unreachable!()
    }
}

#[async_trait::async_trait]
impl PlatformDriver for ResilientDriver {
    async fn prepare(&self, device_id: &str) -> Result<()> {
        let id = device_id.to_string();
        self.with_retry(|| {
            let id = id.clone();
            let inner = self.inner.clone();
            async move { inner.prepare(&id).await }
        })
        .await
    }

    async fn cleanup(&self) {
        self.inner.cleanup().await;
    }

    async fn health_check(&self) -> HealthStatus {
        if self.circuit_breaker.is_open() {
            return HealthStatus::Unhealthy;
        }
        self.inner.health_check().await
    }

    async fn restart_session(&self) -> Result<()> {
        self.inner.restart_session().await
    }

    async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        self.with_retry(|| {
            let inner = self.inner.clone();
            async move { inner.list_devices().await }
        })
        .await
    }

    async fn boot_device(&self, device_id: &str) -> Result<()> {
        let id = device_id.to_string();
        self.with_retry(|| {
            let id = id.clone();
            let inner = self.inner.clone();
            async move { inner.boot_device(&id).await }
        })
        .await
    }

    async fn shutdown_device(&self, device_id: &str) -> Result<()> {
        self.inner.shutdown_device(device_id).await
    }

    async fn install_app(&self, device_id: &str, app_path: &str) -> Result<()> {
        let id = device_id.to_string();
        let path = app_path.to_string();
        self.with_retry(|| {
            let id = id.clone();
            let path = path.clone();
            let inner = self.inner.clone();
            async move { inner.install_app(&id, &path).await }
        })
        .await
    }

    async fn launch_app(&self, device_id: &str, app_id: &str, clear_state: bool) -> Result<()> {
        let did = device_id.to_string();
        let aid = app_id.to_string();
        self.with_retry(|| {
            let did = did.clone();
            let aid = aid.clone();
            let inner = self.inner.clone();
            async move { inner.launch_app(&did, &aid, clear_state).await }
        })
        .await
    }

    async fn stop_app(&self, device_id: &str, app_id: &str) -> Result<()> {
        self.inner.stop_app(device_id, app_id).await
    }

    async fn find_element(&self, device_id: &str, selector: &Selector) -> Result<Element> {
        let did = device_id.to_string();
        let sel = selector.clone();
        self.with_retry(|| {
            let did = did.clone();
            let sel = sel.clone();
            let inner = self.inner.clone();
            async move { inner.find_element(&did, &sel).await }
        })
        .await
    }

    async fn find_elements(&self, device_id: &str, selector: &Selector) -> Result<Vec<Element>> {
        let did = device_id.to_string();
        let sel = selector.clone();
        self.with_retry(|| {
            let did = did.clone();
            let sel = sel.clone();
            let inner = self.inner.clone();
            async move { inner.find_elements(&did, &sel).await }
        })
        .await
    }

    async fn get_hierarchy(&self, device_id: &str) -> Result<Element> {
        let did = device_id.to_string();
        self.with_retry(|| {
            let did = did.clone();
            let inner = self.inner.clone();
            async move { inner.get_hierarchy(&did).await }
        })
        .await
    }

    async fn tap(&self, device_id: &str, element: &Element) -> Result<()> {
        let did = device_id.to_string();
        let el = element.clone();
        self.with_retry(|| {
            let did = did.clone();
            let el = el.clone();
            let inner = self.inner.clone();
            async move { inner.tap(&did, &el).await }
        })
        .await
    }

    async fn double_tap(&self, device_id: &str, element: &Element) -> Result<()> {
        let did = device_id.to_string();
        let el = element.clone();
        self.with_retry(|| {
            let did = did.clone();
            let el = el.clone();
            let inner = self.inner.clone();
            async move { inner.double_tap(&did, &el).await }
        })
        .await
    }

    async fn long_press(&self, device_id: &str, element: &Element, duration_ms: u64) -> Result<()> {
        let did = device_id.to_string();
        let el = element.clone();
        self.with_retry(|| {
            let did = did.clone();
            let el = el.clone();
            let inner = self.inner.clone();
            async move { inner.long_press(&did, &el, duration_ms).await }
        })
        .await
    }

    async fn input_text(&self, device_id: &str, element: &Element, text: &str) -> Result<()> {
        let did = device_id.to_string();
        let el = element.clone();
        let txt = text.to_string();
        self.with_retry(|| {
            let did = did.clone();
            let el = el.clone();
            let txt = txt.clone();
            let inner = self.inner.clone();
            async move { inner.input_text(&did, &el, &txt).await }
        })
        .await
    }

    async fn clear_text(&self, device_id: &str, element: &Element) -> Result<()> {
        let did = device_id.to_string();
        let el = element.clone();
        self.with_retry(|| {
            let did = did.clone();
            let el = el.clone();
            let inner = self.inner.clone();
            async move { inner.clear_text(&did, &el).await }
        })
        .await
    }

    async fn swipe(&self, device_id: &str, direction: Direction) -> Result<()> {
        let did = device_id.to_string();
        self.with_retry(|| {
            let did = did.clone();
            let inner = self.inner.clone();
            async move { inner.swipe(&did, direction).await }
        })
        .await
    }

    async fn swipe_coords(&self, device_id: &str, from: (i32, i32), to: (i32, i32)) -> Result<()> {
        let did = device_id.to_string();
        self.with_retry(|| {
            let did = did.clone();
            let inner = self.inner.clone();
            async move { inner.swipe_coords(&did, from, to).await }
        })
        .await
    }

    async fn press_key(&self, device_id: &str, key: Key) -> Result<()> {
        let did = device_id.to_string();
        let k = key.clone();
        self.with_retry(|| {
            let did = did.clone();
            let k = k.clone();
            let inner = self.inner.clone();
            async move { inner.press_key(&did, k).await }
        })
        .await
    }

    async fn screenshot(&self, device_id: &str) -> Result<Vec<u8>> {
        let did = device_id.to_string();
        self.with_retry(|| {
            let did = did.clone();
            let inner = self.inner.clone();
            async move { inner.screenshot(&did).await }
        })
        .await
    }

    async fn screen_size(&self, device_id: &str) -> Result<(i32, i32)> {
        let did = device_id.to_string();
        self.with_retry(|| {
            let did = did.clone();
            let inner = self.inner.clone();
            async move { inner.screen_size(&did).await }
        })
        .await
    }

    async fn get_element_text(&self, device_id: &str, element: &Element) -> Result<String> {
        let did = device_id.to_string();
        let el = element.clone();
        self.with_retry(|| {
            let did = did.clone();
            let el = el.clone();
            let inner = self.inner.clone();
            async move { inner.get_element_text(&did, &el).await }
        })
        .await
    }

    async fn is_element_visible(&self, device_id: &str, element: &Element) -> Result<bool> {
        let did = device_id.to_string();
        let el = element.clone();
        self.with_retry(|| {
            let did = did.clone();
            let el = el.clone();
            let inner = self.inner.clone();
            async move { inner.is_element_visible(&did, &el).await }
        })
        .await
    }
}
