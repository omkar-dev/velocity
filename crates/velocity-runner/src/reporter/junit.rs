use std::io::Write;

use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use velocity_common::{Result, StepStatus, SuiteResult, TestResult, TestStatus, VelocityError};

/// Write a JUnit XML report for the suite result to the given file path.
pub fn write_junit(result: &SuiteResult, path: &str) -> Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            VelocityError::Config(format!("failed to create report directory: {e}"))
        })?;
    }

    let file = std::fs::File::create(path).map_err(|e| {
        VelocityError::Config(format!("failed to create JUnit report at {path}: {e}"))
    })?;

    let mut writer = Writer::new_with_indent(file, b' ', 2);

    writer
        .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .map_err(xml_err)?;

    // <testsuites>
    let mut testsuites = BytesStart::new("testsuites");
    testsuites.push_attribute(("tests", result.total.to_string().as_str()));
    testsuites.push_attribute(("failures", result.failed.to_string().as_str()));
    testsuites.push_attribute((
        "time",
        format!("{:.3}", result.duration.as_secs_f64()).as_str(),
    ));
    writer
        .write_event(Event::Start(testsuites))
        .map_err(xml_err)?;

    // <testsuite>
    let mut testsuite = BytesStart::new("testsuite");
    testsuite.push_attribute(("name", "velocity"));
    testsuite.push_attribute(("tests", result.total.to_string().as_str()));
    testsuite.push_attribute(("failures", result.failed.to_string().as_str()));
    testsuite.push_attribute(("skipped", result.skipped.to_string().as_str()));
    testsuite.push_attribute((
        "time",
        format!("{:.3}", result.duration.as_secs_f64()).as_str(),
    ));
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    testsuite.push_attribute(("timestamp", timestamp.as_str()));
    writer
        .write_event(Event::Start(testsuite))
        .map_err(xml_err)?;

    for test in &result.tests {
        write_testcase(&mut writer, test)?;
    }

    // </testsuite>
    writer
        .write_event(Event::End(BytesEnd::new("testsuite")))
        .map_err(xml_err)?;

    // </testsuites>
    writer
        .write_event(Event::End(BytesEnd::new("testsuites")))
        .map_err(xml_err)?;

    Ok(())
}

fn write_testcase<W: Write>(writer: &mut Writer<W>, test: &TestResult) -> Result<()> {
    let mut testcase = BytesStart::new("testcase");
    testcase.push_attribute(("name", test.test_name.as_str()));
    testcase.push_attribute(("classname", "velocity"));
    testcase.push_attribute((
        "time",
        format!("{:.3}", test.duration.as_secs_f64()).as_str(),
    ));

    match test.status {
        TestStatus::Failed => {
            writer
                .write_event(Event::Start(testcase))
                .map_err(xml_err)?;

            let mut failure = BytesStart::new("failure");
            let msg = test.error_message.as_deref().unwrap_or("Test failed");
            failure.push_attribute(("message", msg));
            failure.push_attribute(("type", "AssertionError"));

            // Build failure body with step details
            let body = build_failure_body(test);
            writer.write_event(Event::Start(failure)).map_err(xml_err)?;
            writer
                .write_event(Event::Text(BytesText::new(&body)))
                .map_err(xml_err)?;
            writer
                .write_event(Event::End(BytesEnd::new("failure")))
                .map_err(xml_err)?;

            // system-out with screenshot paths
            if !test.screenshots.is_empty() {
                writer
                    .write_event(Event::Start(BytesStart::new("system-out")))
                    .map_err(xml_err)?;
                let screenshot_text = test
                    .screenshots
                    .iter()
                    .map(|p| format!("[[ATTACHMENT|{}]]", p.display()))
                    .collect::<Vec<_>>()
                    .join("\n");
                writer
                    .write_event(Event::Text(BytesText::new(&screenshot_text)))
                    .map_err(xml_err)?;
                writer
                    .write_event(Event::End(BytesEnd::new("system-out")))
                    .map_err(xml_err)?;
            }

            writer
                .write_event(Event::End(BytesEnd::new("testcase")))
                .map_err(xml_err)?;
        }
        TestStatus::Skipped => {
            writer
                .write_event(Event::Start(testcase))
                .map_err(xml_err)?;
            let skipped = BytesStart::new("skipped");
            writer.write_event(Event::Empty(skipped)).map_err(xml_err)?;
            writer
                .write_event(Event::End(BytesEnd::new("testcase")))
                .map_err(xml_err)?;
        }
        _ => {
            // Passed or retried -- self-closing testcase
            writer
                .write_event(Event::Empty(testcase))
                .map_err(xml_err)?;
        }
    }

    Ok(())
}

fn build_failure_body(test: &TestResult) -> String {
    let mut lines = Vec::new();
    for step in &test.steps {
        let status = match step.status {
            StepStatus::Passed => "PASS",
            StepStatus::Failed => "FAIL",
            StepStatus::Skipped => "SKIP",
        };
        lines.push(format!(
            "Step {}: {} [{}] ({:.1}s)",
            step.step_index,
            step.action_name,
            status,
            step.duration.as_secs_f64()
        ));
        if let Some(ref err) = step.error_message {
            lines.push(format!("  Error: {err}"));
        }
    }
    lines.join("\n")
}

fn xml_err(e: std::io::Error) -> VelocityError {
    VelocityError::Config(format!("XML write error: {e}"))
}
