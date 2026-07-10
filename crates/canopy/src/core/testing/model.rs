//! Shared diagnostics for property-based state-machine tests.

use std::fmt::Debug;

use proptest::test_runner::{TestCaseError, TestCaseResult};

/// Add a numbered, failure-marked operation trace to a model-test failure.
pub fn trace_result<T: Debug>(
    result: TestCaseResult,
    operations: &[T],
    failure_at: usize,
) -> TestCaseResult {
    result.map_err(|error| {
        let mut trace = String::from("operation trace:");
        for (index, operation) in operations.iter().enumerate().take(failure_at + 1) {
            let marker = if index == failure_at { ">" } else { " " };
            trace.push_str(&format!("\n{marker} {index:03}: {operation:?}"));
        }
        TestCaseError::fail(format!("{error}\n{trace}"))
    })
}
