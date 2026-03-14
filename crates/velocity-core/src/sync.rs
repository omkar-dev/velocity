use std::collections::HashMap;
use std::time::{Duration, Instant};

use velocity_common::{PlatformDriver, Result, SyncConfig, VelocityError};

use crate::tree_diff::TreeDiff;

/// Tracks historical stabilization times per selector key for prediction.
struct StabilityHistory {
    samples: Vec<Duration>,
    max_samples: usize,
}

impl StabilityHistory {
    fn new() -> Self {
        Self {
            samples: Vec::with_capacity(10),
            max_samples: 10,
        }
    }

    fn record(&mut self, duration: Duration) {
        if self.samples.len() >= self.max_samples {
            self.samples.remove(0);
        }
        self.samples.push(duration);
    }

    fn predict(&self) -> Option<Duration> {
        if self.samples.len() < 3 {
            return None;
        }
        let avg: f64 = self.samples.iter().map(|d| d.as_secs_f64()).sum::<f64>()
            / self.samples.len() as f64;
        let variance: f64 = self
            .samples
            .iter()
            .map(|d| (d.as_secs_f64() - avg).powi(2))
            .sum::<f64>()
            / self.samples.len() as f64;

        // Only predict if variance is low (consistent pattern)
        if variance < (avg * 0.5).powi(2) {
            Some(Duration::from_secs_f64(avg * 1.2)) // 20% safety margin
        } else {
            None
        }
    }
}

pub struct AdaptiveSyncEngine {
    config: SyncConfig,
    history: HashMap<String, StabilityHistory>,
    tree_diff: TreeDiff,
}

impl AdaptiveSyncEngine {
    pub fn new(config: SyncConfig) -> Self {
        Self {
            config,
            history: HashMap::new(),
            tree_diff: TreeDiff::new(),
        }
    }

    /// Reset the tree diff tracker (call after mutating actions like tap/input).
    pub fn invalidate_tree_diff(&mut self) {
        self.tree_diff.reset();
    }

    /// Get a reference to the sync config.
    pub fn config(&self) -> &SyncConfig {
        &self.config
    }

    /// Wait for UI to stabilize, using prediction when available.
    pub async fn wait_for_idle(
        &mut self,
        driver: &dyn PlatformDriver,
        device_id: &str,
    ) -> Result<()> {
        self.wait_for_idle_keyed(driver, device_id, "global").await
    }

    /// Wait for idle with a selector-specific history key for better predictions.
    pub async fn wait_for_idle_keyed(
        &mut self,
        driver: &dyn PlatformDriver,
        device_id: &str,
        key: &str,
    ) -> Result<()> {
        let start = Instant::now();

        // Try prediction fast path
        if let Some(predicted) = self.history.get(key).and_then(|h| h.predict()) {
            if predicted < Duration::from_millis(200) {
                tokio::time::sleep(predicted).await;
                if self.verify_stable(driver, device_id).await? {
                    self.record_sample(key, start.elapsed());
                    return Ok(());
                }
                // Prediction missed — fall through to polling
            }
        }

        // Conservative polling fallback
        let result = self.poll_for_stable(driver, device_id).await;
        if result.is_ok() {
            self.record_sample(key, start.elapsed());
        }
        result
    }

    /// Verify stability by comparing two hierarchy snapshots 50ms apart.
    /// Uses TreeDiff for O(1) hash comparison when possible.
    async fn verify_stable(
        &mut self,
        driver: &dyn PlatformDriver,
        device_id: &str,
    ) -> Result<bool> {
        let tree1 = driver.get_hierarchy(device_id).await?;
        self.tree_diff.diff(&tree1);
        tokio::time::sleep(Duration::from_millis(50)).await;
        let tree2 = driver.get_hierarchy(device_id).await?;
        Ok(self.tree_diff.is_stable(&tree2))
    }

    /// Adaptive polling: uses TreeDiff for incremental comparison.
    /// Aggressive when changes detected, relaxed when stable.
    async fn poll_for_stable(
        &mut self,
        driver: &dyn PlatformDriver,
        device_id: &str,
    ) -> Result<()> {
        let deadline = Instant::now() + Duration::from_millis(self.config.timeout_ms);
        let required = self.config.stability_count;

        let mut consecutive_stable = 0u32;
        let mut interval = Duration::from_millis(self.config.interval_ms);
        let min_interval =
            Duration::from_millis(self.config.interval_ms / 4).max(Duration::from_millis(20));
        let max_interval = Duration::from_millis(self.config.interval_ms * 4);

        loop {
            if Instant::now() >= deadline {
                return Err(VelocityError::SyncTimeout {
                    timeout_ms: self.config.timeout_ms,
                    stable_count: consecutive_stable,
                    required,
                });
            }

            let hierarchy = driver.get_hierarchy(device_id).await?;
            let diff_result = self.tree_diff.diff(&hierarchy);

            if diff_result.unchanged {
                consecutive_stable += 1;
                if consecutive_stable >= required {
                    return Ok(());
                }
                if self.config.adaptive {
                    interval = (interval * 3 / 2).min(max_interval);
                }
            } else {
                consecutive_stable = 0;
                if self.config.adaptive {
                    interval = min_interval;
                }
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            let sleep_time = interval.min(remaining);
            if sleep_time.is_zero() {
                return Err(VelocityError::SyncTimeout {
                    timeout_ms: self.config.timeout_ms,
                    stable_count: consecutive_stable,
                    required,
                });
            }
            tokio::time::sleep(sleep_time).await;
        }
    }

    fn record_sample(&mut self, key: &str, duration: Duration) {
        self.history
            .entry(key.to_string())
            .or_insert_with(StabilityHistory::new)
            .record(duration);
    }
}

// Keep the old name as an alias for backwards compatibility in executor.rs
pub type SmartPollingSyncEngine = AdaptiveSyncEngine;

#[cfg(test)]
mod tests {
    use super::*;
    use velocity_common::{
        DeviceInfo, Direction, Element, Key, Rect, Selector,
    };

    fn hash_element(element: &Element) -> u64 {
        TreeDiff::hash_element(element)
    }

    use std::sync::atomic::{AtomicUsize, Ordering};

    fn make_element(text: &str) -> Element {
        Element {
            platform_id: "el1".to_string(),
            label: None,
            text: Some(text.to_string()),
            element_type: "Button".to_string(),
            bounds: Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 50,
            },
            enabled: true,
            visible: true,
            children: vec![],
        }
    }

    #[test]
    fn hash_same_elements_equal() {
        let a = make_element("hello");
        let b = make_element("hello");
        assert_eq!(hash_element(&a), hash_element(&b));
    }

    #[test]
    fn hash_different_elements_differ() {
        let a = make_element("hello");
        let b = make_element("world");
        assert_ne!(hash_element(&a), hash_element(&b));
    }

    struct MockDriver {
        call_count: AtomicUsize,
        elements: Vec<Element>,
    }

    #[async_trait::async_trait]
    impl PlatformDriver for MockDriver {
        async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
            Ok(vec![])
        }
        async fn boot_device(&self, _: &str) -> Result<()> {
            Ok(())
        }
        async fn shutdown_device(&self, _: &str) -> Result<()> {
            Ok(())
        }
        async fn install_app(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        async fn launch_app(&self, _: &str, _: &str, _: bool) -> Result<()> {
            Ok(())
        }
        async fn stop_app(&self, _: &str, _: &str) -> Result<()> {
            Ok(())
        }
        async fn find_element(&self, _: &str, _: &Selector) -> Result<Element> {
            Ok(make_element("mock"))
        }
        async fn find_elements(&self, _: &str, _: &Selector) -> Result<Vec<Element>> {
            Ok(vec![])
        }
        async fn get_hierarchy(&self, _: &str) -> Result<Element> {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
            let el = if idx < self.elements.len() {
                self.elements[idx].clone()
            } else {
                self.elements.last().unwrap().clone()
            };
            Ok(el)
        }
        async fn tap(&self, _: &str, _: &Element) -> Result<()> {
            Ok(())
        }
        async fn double_tap(&self, _: &str, _: &Element) -> Result<()> {
            Ok(())
        }
        async fn long_press(&self, _: &str, _: &Element, _: u64) -> Result<()> {
            Ok(())
        }
        async fn input_text(&self, _: &str, _: &Element, _: &str) -> Result<()> {
            Ok(())
        }
        async fn clear_text(&self, _: &str, _: &Element) -> Result<()> {
            Ok(())
        }
        async fn swipe(&self, _: &str, _: Direction) -> Result<()> {
            Ok(())
        }
        async fn swipe_coords(&self, _: &str, _: (i32, i32), _: (i32, i32)) -> Result<()> {
            Ok(())
        }
        async fn press_key(&self, _: &str, _: Key) -> Result<()> {
            Ok(())
        }
        async fn screenshot(&self, _: &str) -> Result<Vec<u8>> {
            Ok(vec![])
        }
        async fn screen_size(&self, _: &str) -> Result<(i32, i32)> {
            Ok((1080, 1920))
        }
        async fn get_element_text(&self, _: &str, _: &Element) -> Result<String> {
            Ok(String::new())
        }
        async fn is_element_visible(&self, _: &str, _: &Element) -> Result<bool> {
            Ok(true)
        }
    }

    #[tokio::test]
    async fn sync_stabilizes_after_consistent_hashes() {
        let stable = make_element("stable");
        let driver = MockDriver {
            call_count: AtomicUsize::new(0),
            elements: vec![
                make_element("changing"),
                stable.clone(),
                stable.clone(),
                stable.clone(),
                stable,
            ],
        };

        let config = SyncConfig {
            interval_ms: 10,
            stability_count: 3,
            timeout_ms: 5000,
            adaptive: false,
            ..Default::default()
        };

        let mut engine = AdaptiveSyncEngine::new(config);
        let result = engine.wait_for_idle(&driver, "test-device").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn sync_times_out_when_unstable() {
        let driver = MockDriver {
            call_count: AtomicUsize::new(0),
            elements: (0..100)
                .map(|i| make_element(&format!("changing-{i}")))
                .collect(),
        };

        let config = SyncConfig {
            interval_ms: 10,
            stability_count: 3,
            timeout_ms: 100,
            adaptive: false,
            ..Default::default()
        };

        let mut engine = AdaptiveSyncEngine::new(config);
        let result = engine.wait_for_idle(&driver, "test-device").await;
        assert!(matches!(result, Err(VelocityError::SyncTimeout { .. })));
    }

    #[test]
    fn stability_history_prediction() {
        let mut hist = StabilityHistory::new();
        // Record consistent ~100ms samples
        for _ in 0..5 {
            hist.record(Duration::from_millis(100));
        }
        let prediction = hist.predict();
        assert!(prediction.is_some());
        let predicted = prediction.unwrap();
        // Should be ~120ms (100ms * 1.2 safety margin)
        assert!(predicted.as_millis() >= 100 && predicted.as_millis() <= 150);
    }

    #[test]
    fn stability_history_no_prediction_with_high_variance() {
        let mut hist = StabilityHistory::new();
        hist.record(Duration::from_millis(50));
        hist.record(Duration::from_millis(500));
        hist.record(Duration::from_millis(100));
        hist.record(Duration::from_millis(400));
        assert!(hist.predict().is_none());
    }
}
