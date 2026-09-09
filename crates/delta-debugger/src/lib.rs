//! Oracle-driven delta debugging and success-preserving minimization.

use serde::{Deserialize, Serialize};
use std::future::Future;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TestOutcome {
    Pass,
    Fail,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimizeResult<T> {
    pub items: Vec<T>,
    pub tests_executed: usize,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MinimizeError {
    #[error("the initial input does not produce the target outcome")]
    InitialOutcomeMismatch,
}

/// Return a 1-minimal subset that preserves `target`.
///
/// This is a ddmin-family divide-and-conquer reducer. It tests complements first,
/// then subsets, increasing granularity only when neither direction can reduce the input.
pub fn ddmin<T, F>(
    items: Vec<T>,
    target: TestOutcome,
    mut test: F,
) -> Result<MinimizeResult<T>, MinimizeError>
where
    T: Clone,
    F: FnMut(&[T]) -> TestOutcome,
{
    let mut tests = 1;
    if test(&items) != target {
        return Err(MinimizeError::InitialOutcomeMismatch);
    }
    if items.len() < 2 {
        return Ok(MinimizeResult {
            items,
            tests_executed: tests,
        });
    }

    let mut current = items;
    let mut n = 2usize;

    loop {
        if current.len() < 2 {
            break;
        }
        n = n.min(current.len());
        let ranges = partitions(current.len(), n);
        let mut reduced = false;

        for (start, end) in &ranges {
            let complement = complement(&current, *start, *end);
            tests += 1;
            if test(&complement) == target {
                current = complement;
                n = n.saturating_sub(1).max(2);
                reduced = true;
                break;
            }
        }
        if reduced {
            continue;
        }

        for (start, end) in &ranges {
            let subset = current[*start..*end].to_vec();
            tests += 1;
            if test(&subset) == target {
                current = subset;
                n = 2;
                reduced = true;
                break;
            }
        }
        if reduced {
            continue;
        }

        if n >= current.len() {
            break;
        }
        n = (n * 2).min(current.len());
    }

    Ok(MinimizeResult {
        items: current,
        tests_executed: tests,
    })
}

/// Async ddmin variant for real consumer oracles.
///
/// The callback owns each candidate vector so it can cross an await point safely. The reduction
/// order matches [`ddmin`], preserving deterministic trial ordering for the same input.
pub async fn ddmin_async<T, F, Fut>(
    items: Vec<T>,
    target: TestOutcome,
    mut test: F,
) -> Result<MinimizeResult<T>, MinimizeError>
where
    T: Clone,
    F: FnMut(Vec<T>) -> Fut,
    Fut: Future<Output = TestOutcome>,
{
    let mut tests = 1;
    if test(items.clone()).await != target {
        return Err(MinimizeError::InitialOutcomeMismatch);
    }
    if items.len() < 2 {
        return Ok(MinimizeResult {
            items,
            tests_executed: tests,
        });
    }

    let mut current = items;
    let mut n = 2usize;

    loop {
        if current.len() < 2 {
            break;
        }
        n = n.min(current.len());
        let ranges = partitions(current.len(), n);
        let mut reduced = false;

        for (start, end) in &ranges {
            let candidate = complement(&current, *start, *end);
            tests += 1;
            if test(candidate.clone()).await == target {
                current = candidate;
                n = n.saturating_sub(1).max(2);
                reduced = true;
                break;
            }
        }
        if reduced {
            continue;
        }

        for (start, end) in &ranges {
            let candidate = current[*start..*end].to_vec();
            tests += 1;
            if test(candidate.clone()).await == target {
                current = candidate;
                n = 2;
                reduced = true;
                break;
            }
        }
        if reduced {
            continue;
        }

        if n >= current.len() {
            break;
        }
        n = (n * 2).min(current.len());
    }

    Ok(MinimizeResult {
        items: current,
        tests_executed: tests,
    })
}

/// Convenience wrapper for minimizing a provider-response field set while consumer success remains true.
pub fn minimize_success<T, F>(
    items: Vec<T>,
    test: F,
) -> Result<MinimizeResult<T>, MinimizeError>
where
    T: Clone,
    F: FnMut(&[T]) -> TestOutcome,
{
    ddmin(items, TestOutcome::Pass, test)
}

/// Async convenience wrapper for success-preserving response minimization.
pub async fn minimize_success_async<T, F, Fut>(
    items: Vec<T>,
    test: F,
) -> Result<MinimizeResult<T>, MinimizeError>
where
    T: Clone,
    F: FnMut(Vec<T>) -> Fut,
    Fut: Future<Output = TestOutcome>,
{
    ddmin_async(items, TestOutcome::Pass, test).await
}

fn complement<T: Clone>(items: &[T], start: usize, end: usize) -> Vec<T> {
    items[..start]
        .iter()
        .chain(items[end..].iter())
        .cloned()
        .collect()
}

fn partitions(len: usize, n: usize) -> Vec<(usize, usize)> {
    let base = len / n;
    let remainder = len % n;
    let mut start = 0usize;
    let mut result = Vec::with_capacity(n);
    for index in 0..n {
        let size = base + usize::from(index < remainder);
        let end = start + size;
        result.push((start, end));
        start = end;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimizes_failure_to_single_trigger() {
        let result = ddmin(vec![1, 2, 3, 4, 5, 6], TestOutcome::Fail, |items| {
            if items.contains(&3) {
                TestOutcome::Fail
            } else {
                TestOutcome::Pass
            }
        })
        .unwrap();
        assert_eq!(result.items, vec![3]);
    }

    #[test]
    fn minimizes_success_to_fields_the_consumer_needs() {
        let result = minimize_success(
            vec!["id", "name", "email", "avatar", "metadata"],
            |items| {
                if items.contains(&"id") && items.contains(&"email") {
                    TestOutcome::Pass
                } else {
                    TestOutcome::Fail
                }
            },
        )
        .unwrap();
        assert_eq!(result.items.len(), 2);
        assert!(result.items.contains(&"id"));
        assert!(result.items.contains(&"email"));
    }

    #[tokio::test]
    async fn async_minimization_preserves_same_reduction_semantics() {
        let result = minimize_success_async(
            vec!["id", "name", "email", "avatar", "metadata"],
            |items| async move {
                if items.contains(&"id") && items.contains(&"email") {
                    TestOutcome::Pass
                } else {
                    TestOutcome::Fail
                }
            },
        )
        .await
        .unwrap();
        assert_eq!(result.items.len(), 2);
        assert!(result.items.contains(&"id"));
        assert!(result.items.contains(&"email"));
    }

    #[test]
    fn refuses_to_minimize_wrong_initial_outcome() {
        let error = ddmin(vec![1, 2], TestOutcome::Fail, |_| TestOutcome::Pass).unwrap_err();
        assert_eq!(error, MinimizeError::InitialOutcomeMismatch);
    }
}
