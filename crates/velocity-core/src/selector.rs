use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use velocity_common::{Element, PlatformDriver, Result, Selector, VelocityError};

const MAX_CACHE_SIZE: usize = 256;
const DEFAULT_TTL: Duration = Duration::from_secs(30);

pub struct SelectorEngine {
    cache: HashMap<String, CachedElement>,
    generation: AtomicU64,
    max_size: usize,
    ttl: Duration,
}

#[derive(Clone)]
struct CachedElement {
    element: Element,
    cached_at: Instant,
    source_generation: u64,
}

impl SelectorEngine {
    pub fn new() -> Self {
        Self {
            cache: HashMap::with_capacity(64),
            generation: AtomicU64::new(0),
            max_size: MAX_CACHE_SIZE,
            ttl: DEFAULT_TTL,
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
        let element = driver.find_element(device_id, selector).await.map_err(|_| {
            VelocityError::ElementNotFound {
                selector: format!("{selector}"),
                timeout_ms: 0,
                screenshot: None,
                hierarchy_snapshot: None,
            }
        })?;

        // Evict oldest entries if cache is full
        if self.cache.len() >= self.max_size {
            self.evict_oldest();
        }

        self.cache.insert(
            cache_key,
            CachedElement {
                element: element.clone(),
                cached_at: Instant::now(),
                source_generation: current_gen,
            },
        );

        Ok(element)
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
