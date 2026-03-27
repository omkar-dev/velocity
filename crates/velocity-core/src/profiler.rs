use std::time::{SystemTime, UNIX_EPOCH};

use velocity_common::{ResourceDelta, ResourceSnapshot};

/// Accumulates resource metrics during test execution.
/// No background tasks, no Arc/Mutex — pure synchronous data accumulation.
/// Only constructed when `PerformanceConfig.enabled` is true.
pub struct ResourceProfiler {
    package: String,
    baseline: Option<ResourceSnapshot>,
    peak: Option<ResourceSnapshot>,
    last_before: Option<ResourceSnapshot>,
}

impl ResourceProfiler {
    pub fn new(package: String) -> Self {
        Self {
            package,
            baseline: None,
            peak: None,
            last_before: None,
        }
    }

    pub fn package(&self) -> &str {
        &self.package
    }

    /// Build a ResourceSnapshot from raw metrics tuple.
    pub fn snapshot_from_raw(raw: (u64, u64, u64, f32)) -> ResourceSnapshot {
        ResourceSnapshot {
            java_heap_kb: raw.0,
            native_heap_kb: raw.1,
            total_pss_kb: raw.2,
            cpu_percent: raw.3,
            timestamp_ms: now_ms(),
        }
    }

    /// Record a pre-action snapshot. Sets baseline on first call.
    pub fn record_before(&mut self, snap: ResourceSnapshot) {
        if self.baseline.is_none() {
            self.baseline = Some(snap.clone());
        }
        self.update_peak(&snap);
        self.last_before = Some(snap);
    }

    /// Record a post-action snapshot and return the delta from the last pre-action snapshot.
    pub fn record_after(&mut self, snap: ResourceSnapshot) -> Option<ResourceDelta> {
        self.update_peak(&snap);
        let before = self.last_before.take()?;
        let heap_growth = (snap.java_heap_kb as i64 + snap.native_heap_kb as i64)
            - (before.java_heap_kb as i64 + before.native_heap_kb as i64);
        Some(ResourceDelta {
            before,
            after: snap,
            heap_growth_kb: heap_growth,
        })
    }

    fn update_peak(&mut self, snap: &ResourceSnapshot) {
        match &self.peak {
            Some(p) if p.total_pss_kb >= snap.total_pss_kb => {}
            _ => self.peak = Some(snap.clone()),
        }
    }

    pub fn baseline(&self) -> Option<&ResourceSnapshot> {
        self.baseline.as_ref()
    }

    pub fn peak(&self) -> Option<&ResourceSnapshot> {
        self.peak.as_ref()
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snap(java: u64, native: u64, pss: u64, cpu: f32) -> ResourceSnapshot {
        ResourceSnapshot {
            java_heap_kb: java,
            native_heap_kb: native,
            total_pss_kb: pss,
            cpu_percent: cpu,
            timestamp_ms: 1000,
        }
    }

    #[test]
    fn baseline_set_on_first_record() {
        let mut profiler = ResourceProfiler::new("com.test".into());
        assert!(profiler.baseline().is_none());

        profiler.record_before(make_snap(1000, 500, 2000, 5.0));
        assert_eq!(profiler.baseline().unwrap().java_heap_kb, 1000);
    }

    #[test]
    fn delta_computes_heap_growth() {
        let mut profiler = ResourceProfiler::new("com.test".into());
        profiler.record_before(make_snap(1000, 500, 2000, 5.0));

        let delta = profiler
            .record_after(make_snap(1200, 600, 2400, 8.0))
            .unwrap();
        assert_eq!(delta.heap_growth_kb, 300); // (1200+600) - (1000+500)
    }

    #[test]
    fn delta_detects_negative_growth() {
        let mut profiler = ResourceProfiler::new("com.test".into());
        profiler.record_before(make_snap(1000, 500, 2000, 5.0));

        let delta = profiler
            .record_after(make_snap(800, 400, 1500, 3.0))
            .unwrap();
        assert_eq!(delta.heap_growth_kb, -300);
    }

    #[test]
    fn peak_tracks_highest_pss() {
        let mut profiler = ResourceProfiler::new("com.test".into());
        profiler.record_before(make_snap(1000, 500, 2000, 5.0));
        profiler.record_after(make_snap(1200, 600, 3000, 8.0));
        profiler.record_before(make_snap(900, 400, 1500, 3.0));

        assert_eq!(profiler.peak().unwrap().total_pss_kb, 3000);
    }

    #[test]
    fn record_after_without_before_returns_none() {
        let mut profiler = ResourceProfiler::new("com.test".into());
        assert!(profiler
            .record_after(make_snap(1000, 500, 2000, 5.0))
            .is_none());
    }
}
