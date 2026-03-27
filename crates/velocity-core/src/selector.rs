use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tracing::info;
use velocity_common::{Element, HealingConfig, PlatformDriver, Result, Selector, VelocityError};

use crate::healing::SelectorHealer;

const MAX_CACHE_SIZE: usize = 256;
const DEFAULT_TTL: Duration = Duration::from_secs(30);

pub struct SelectorEngine {
    cache: HashMap<String, CachedElement>,
    generation: AtomicU64,
    max_size: usize,
    ttl: Duration,
    healer: SelectorHealer,
}

#[derive(Clone)]
struct CachedElement {
    element: Element,
    cached_at: Instant,
    source_generation: u64,
}

impl SelectorEngine {
    pub fn new() -> Self {
        Self::with_healing(HealingConfig::default())
    }

    pub fn with_healing(config: HealingConfig) -> Self {
        let healer_config = crate::healing::HealingConfig {
            enabled: config.enabled,
            confidence_threshold: config.confidence_threshold,
            persist_healed: config.persist_healed,
            persist_path: None,
        };
        Self {
            cache: HashMap::with_capacity(64),
            generation: AtomicU64::new(0),
            max_size: MAX_CACHE_SIZE,
            ttl: DEFAULT_TTL,
            healer: SelectorHealer::new(healer_config),
        }
    }

    /// Increment the generation counter, invalidating all cached entries
    /// that don't match the new generation.
    pub fn invalidate_generation(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Clear the entire cache. Use after app lifecycle events
    /// (launch, stop, screen transitions).
    pub fn invalidate_cache(&mut self) {
        self.cache.clear();
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    pub async fn find_element(
        &mut self,
        driver: &dyn PlatformDriver,
        device_id: &str,
        selector: &Selector,
    ) -> Result<Element> {
        let cache_key = format!("{selector}");
        let current_gen = self.generation.load(Ordering::SeqCst);

        // Check cache first
        if let Some(cached) = self.cache.get(&cache_key) {
            if self.is_valid(cached, current_gen) {
                // Verify the cached element still exists
                if driver
                    .is_element_visible(device_id, &cached.element)
                    .await
                    .unwrap_or(false)
                {
                    return Ok(cached.element.clone());
                }
                // Element no longer valid, remove from cache
                self.cache.remove(&cache_key);
            } else {
                self.cache.remove(&cache_key);
            }
        }

        // Full tree query
        let result = driver.find_element(device_id, selector).await;

        match result {
            Ok(element) => {
                // Record successful find for future healing reference
                self.healer.record_success(selector, &element);
                self.cache_element(&cache_key, element.clone(), current_gen);
                Ok(element)
            }
            Err(_) => {
                // Primary lookup failed — attempt self-healing
                if let Some((healed_element, confidence)) =
                    self.healer.try_heal(driver, device_id, selector).await
                {
                    info!(
                        selector = %selector,
                        confidence = confidence,
                        healed_type = healed_element.element_type,
                        healed_label = healed_element.label.as_deref().unwrap_or(""),
                        "selector healed: using alternate element"
                    );
                    self.cache_element(&cache_key, healed_element.clone(), current_gen);
                    Ok(healed_element)
                } else {
                    Err(VelocityError::ElementNotFound {
                        selector: format!("{selector}"),
                        timeout_ms: 0,
                        screenshot: None,
                        hierarchy_snapshot: None,
                    })
                }
            }
        }
    }

    /// Persist any healed mappings to disk (call at end of test run).
    pub fn persist_healed_mappings(&self) -> Result<()> {
        self.healer.persist()
    }

    /// Get access to the healer for reporting.
    pub fn healer(&self) -> &SelectorHealer {
        &self.healer
    }

    fn cache_element(&mut self, cache_key: &str, element: Element, current_gen: u64) {
        if self.cache.len() >= self.max_size {
            self.evict_oldest();
        }
        self.cache.insert(
            cache_key.to_string(),
            CachedElement {
                element,
                cached_at: Instant::now(),
                source_generation: current_gen,
            },
        );
    }

    fn is_valid(&self, cached: &CachedElement, current_gen: u64) -> bool {
        cached.source_generation == current_gen && cached.cached_at.elapsed() < self.ttl
    }

    fn evict_oldest(&mut self) {
        if let Some(oldest_key) = self
            .cache
            .iter()
            .min_by_key(|(_, v)| v.cached_at)
            .map(|(k, _)| k.clone())
        {
            self.cache.remove(&oldest_key);
        }
    }
}

impl Default for SelectorEngine {
    fn default() -> Self {
        Self::new()
    }
}
