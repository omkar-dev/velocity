use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::stream::{self, StreamExt};
use tracing::{debug, info};
use velocity_common::{
    PlatformDriver, Result, RuntimeConfig, SuiteResult, TestCase, TestResult, TestStatus,
    VelocityError,
};
use velocity_core::TestExecutor;

use crate::farm::DeviceFarm;
use crate::scheduler;

/// Runs tests in parallel across multiple devices from a `DeviceFarm`.
///
/// Each test acquires a device lease, executes on that device, and returns
/// the device when complete. Concurrency is bounded by the number of
/// available devices in the farm.
pub struct ParallelRunner;

impl ParallelRunner {
    pub async fn run(
        suite: velocity_common::TestSuite,
        config: RuntimeConfig,
        driver: Arc<dyn PlatformDriver>,
        concurrency: usize,
    ) -> Result<SuiteResult> {
        let suite_start = Instant::now();

        // Set up device farm
        let farm = Arc::new(DeviceFarm::new(driver.clone(), concurrency));
        let device_count = farm.refresh().await?;
        if device_count == 0 {
            return Err(VelocityError::Config(
                "No booted devices found for parallel execution".to_string(),
            ));
        }

        let effective_concurrency = device_count.min(concurrency);
        info!(
            devices = device_count,
            concurrency = effective_concurrency,
            "starting parallel execution"
        );

        // Filter tests
        let mut tests = scheduler::filter_by_tags(&suite.tests, &config.tags);
        if let Some(ref pattern) = config.test_filter {
            tests = scheduler::filter_by_name(&tests, pattern);
        }

        let total_tests = tests.len();
        let retry_count = config.retry_count.unwrap_or(suite.config.retry.count);
        let app_id = Arc::new(suite.app_id.clone());
        let suite_config = Arc::new(suite.config.clone());
        let fail_fast = config.fail_fast;

        println!("\n  Velocity v{} (parallel)", env!("CARGO_PKG_VERSION"));
        println!("  Tests: {total_tests}, Devices: {device_count}");
        println!();

        let fail_fast_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let results: Vec<TestResult> = stream::iter(tests.into_iter().enumerate())
            .map(|(idx, test)| {
                let farm = farm.clone();
                let driver = driver.clone();
                let app_id = app_id.clone();
                let suite_config = suite_config.clone();
                let fail_fast_flag = fail_fast_flag.clone();

                async move {
                    // Check fail-fast before starting
                    if fail_fast
                        && fail_fast_flag.load(std::sync::atomic::Ordering::Relaxed)
                    {
                        return TestResult {
                            test_name: test.name.clone(),
                            status: TestStatus::Skipped,
                            duration: Duration::ZERO,
                            steps: vec![],
                            retries: 0,
                            error_message: Some("Skipped due to --fail-fast".to_string()),
                            screenshots: vec![],
                        };
                    }

                    let lease = match farm.acquire().await {
                        Ok(l) => l,
                        Err(e) => {
                            return TestResult {
                                test_name: test.name.clone(),
                                status: TestStatus::Failed,
                                duration: Duration::ZERO,
                                steps: vec![],
                                retries: 0,
                                error_message: Some(format!("Failed to acquire device: {e}")),
                                screenshots: vec![],
                            };
                        }
                    };

                    let device_id = lease.device_id().to_string();
                    debug!(test = %test.name, device = %device_id, idx, "executing test");

                    let result = run_test_with_retries(
                        &*driver,
                        &suite_config,
                        &test,
                        &device_id,
                        &app_id,
                        retry_count,
                    )
                    .await;

                    let status_icon = match result.status {
                        TestStatus::Passed => "\x1b[32m●\x1b[0m",
                        TestStatus::Failed => "\x1b[31m●\x1b[0m",
                        TestStatus::Skipped | TestStatus::Retried => "\x1b[33m●\x1b[0m",
                    };
                    let status_label = match result.status {
                        TestStatus::Passed => "PASSED",
                        TestStatus::Failed => "FAILED",
                        TestStatus::Skipped => "SKIPPED",
                        TestStatus::Retried => "RETRIED",
                    };
                    let dur = result.duration.as_secs_f64();
                    println!(
                        "  {status_icon} {} [{device_id}] ... {status_label} {dur:.1}s",
                        test.name
                    );

                    if result.status == TestStatus::Failed {
                        if let Some(ref msg) = result.error_message {
                            println!("    └ {msg}");
                        }
                        if fail_fast {
                            fail_fast_flag
                                .store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    }

                    // Lease is dropped here, returning device to pool
                    result
                }
            })
            .buffer_unordered(effective_concurrency)
            .collect()
            .await;

        let suite_duration = suite_start.elapsed();
        let passed = results.iter().filter(|r| r.status == TestStatus::Passed).count();
        let failed = results.iter().filter(|r| r.status == TestStatus::Failed).count();
        let skipped = results.iter().filter(|r| r.status == TestStatus::Skipped).count();
        let retried = results.iter().filter(|r| r.status == TestStatus::Retried).count();

        println!();
        println!("  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!(
            "  {passed} passed, {failed} failed{} — {:.1}s (parallel, {effective_concurrency} devices)",
            if retried > 0 {
                format!(" ({retried} retried)")
            } else {
                String::new()
            },
            suite_duration.as_secs_f64()
        );

        Ok(SuiteResult {
            total: results.len(),
            passed,
            failed,
            skipped,
            retried,
            duration: suite_duration,
            tests: results,
            shard_index: None,
            shard_total: None,
        })
    }
}

async fn run_test_with_retries(
    driver: &dyn PlatformDriver,
    suite_config: &velocity_common::SuiteConfig,
    test: &TestCase,
    device_id: &str,
    app_id: &str,
    max_retries: u32,
) -> TestResult {
    let test_start = Instant::now();
    let mut executor = TestExecutor::new(driver, suite_config.clone());

    let mut last_result = match executor.execute_test(test, device_id, app_id).await {
        Ok(r) => r,
        Err(e) => TestResult {
            test_name: test.name.clone(),
            status: TestStatus::Failed,
            duration: test_start.elapsed(),
            steps: vec![],
            retries: 0,
            error_message: Some(e.to_string()),
            screenshots: vec![],
        },
    };

    let mut attempts = 0u32;
    while last_result.status == TestStatus::Failed && attempts < max_retries {
        attempts += 1;
        last_result = match executor.execute_test(test, device_id, app_id).await {
            Ok(r) => r,
            Err(e) => TestResult {
                test_name: test.name.clone(),
                status: TestStatus::Failed,
                duration: test_start.elapsed(),
                steps: vec![],
                retries: attempts,
                error_message: Some(e.to_string()),
                screenshots: vec![],
            },
        };
    }

    if attempts > 0 && last_result.status == TestStatus::Passed {
        last_result.status = TestStatus::Retried;
    }

    last_result.retries = attempts;
    last_result.duration = test_start.elapsed();
    last_result
}
