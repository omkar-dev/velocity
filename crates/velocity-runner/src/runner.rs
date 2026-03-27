use std::time::{Duration, Instant};

use velocity_common::{
    PlatformDriver, Result, RuntimeConfig, SuiteResult, TestCase, TestResult, TestStatus,
    VelocityError,
};
use velocity_core::TestExecutor;

use crate::history;
use crate::scheduler;

pub struct SuiteRunner;

impl SuiteRunner {
    pub async fn run(
        suite: velocity_common::TestSuite,
        config: RuntimeConfig,
        driver: &dyn PlatformDriver,
    ) -> Result<SuiteResult> {
        let suite_start = Instant::now();

        let suite_timeout = config.suite_timeout_ms.map(Duration::from_millis);

        // Determine device
        let device_id = match &config.device_id {
            Some(id) => id.clone(),
            None => {
                let devices = driver.list_devices().await?;
                devices
                    .first()
                    .ok_or_else(|| {
                        VelocityError::Config("No devices found. Boot a device first.".to_string())
                    })?
                    .id
                    .clone()
            }
        };

        // Filter by tags
        let mut tests = scheduler::filter_by_tags(&suite.tests, &config.tags);

        // Filter by name pattern
        if let Some(ref pattern) = config.test_filter {
            tests = scheduler::filter_by_name(&tests, pattern);
        }

        // Shard
        let (tests, shard_index, shard_total) = match (config.shard_index, config.shard_total) {
            (Some(idx), Some(total)) => {
                let hist = history::load(&config.artifacts_dir).ok();
                let sharded = scheduler::shard_tests(tests, idx, total, hist.as_ref());
                (sharded, Some(idx), Some(total))
            }
            _ => (tests, None, None),
        };

        let total_tests = tests.len();
        let retry_count = config.retry_count.unwrap_or(suite.config.retry.count);
        let app_id = suite.app_id.clone();

        println!("\n  Velocity v{}", env!("CARGO_PKG_VERSION"));
        println!(
            "  Tests: {total_tests}{}",
            match (shard_index, shard_total) {
                (Some(i), Some(t)) => format!(" (shard {}/{t})", i + 1),
                _ => String::new(),
            }
        );
        println!();

        let mut executor = TestExecutor::new(driver, suite.config.clone(), &app_id);
        let mut hist = history::load(&config.artifacts_dir).unwrap_or_default();
        let mut results: Vec<TestResult> = Vec::with_capacity(total_tests);

        for test in &tests {
            if let Some(timeout) = suite_timeout {
                if suite_start.elapsed() >= timeout {
                    return Err(VelocityError::SuiteTimeout {
                        timeout_ms: timeout.as_millis() as u64,
                        completed: results.len(),
                        total: total_tests,
                    });
                }
            }

            let result =
                run_test_with_retries(&mut executor, test, &device_id, &app_id, retry_count).await;

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
            println!("  {status_icon} {} ... {status_label} {dur:.1}s", test.name);

            if result.status == TestStatus::Failed {
                if let Some(ref msg) = result.error_message {
                    println!("    └ {msg}");
                }
            }

            // Check for resource regression if profiling is enabled
            if let Some(ref peak) = result.resource_peak {
                if let Some(baseline) = hist.resource_baselines.get(&test.name) {
                    if let Some(warning) = history::check_regression(
                        baseline,
                        peak,
                        suite.config.performance.heap_growth_threshold_pct,
                    ) {
                        println!("    \x1b[33m⚠ {warning}\x1b[0m");
                    }
                }
            }

            let failed = result.status == TestStatus::Failed;
            results.push(result);

            if config.fail_fast && failed {
                for remaining in tests.iter().skip(results.len()) {
                    results.push(TestResult {
                        test_name: remaining.name.clone(),
                        status: TestStatus::Skipped,
                        duration: Duration::ZERO,
                        steps: vec![],
                        retries: 0,
                        error_message: Some("Skipped due to --fail-fast".to_string()),
                        screenshots: vec![],
                        resource_peak: None,
                    });
                }
                break;
            }
        }

        let suite_duration = suite_start.elapsed();
        let passed = results
            .iter()
            .filter(|r| r.status == TestStatus::Passed)
            .count();
        let failed = results
            .iter()
            .filter(|r| r.status == TestStatus::Failed)
            .count();
        let skipped = results
            .iter()
            .filter(|r| r.status == TestStatus::Skipped)
            .count();
        let retried = results
            .iter()
            .filter(|r| r.status == TestStatus::Retried)
            .count();

        // Update history
        history::update(&mut hist, &results);
        let _ = history::save(&config.artifacts_dir, &hist);

        println!();
        println!("  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!(
            "  {passed} passed, {failed} failed{} — {:.1}s",
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
            shard_index,
            shard_total,
        })
    }
}

async fn run_test_with_retries(
    executor: &mut TestExecutor<'_>,
    test: &TestCase,
    device_id: &str,
    app_id: &str,
    max_retries: u32,
) -> TestResult {
    let test_start = Instant::now();
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
            resource_peak: None,
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
                resource_peak: None,
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
