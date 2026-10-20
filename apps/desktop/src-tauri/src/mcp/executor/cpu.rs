//! The `cpu_diagnostics` tool: where this process's CPU went, per named thread, over a window.
//!
//! The sibling of `memory_diagnostics`, over `cmdr_fs::process_cpu` (which owns the FFI, the
//! diff, and why each number is read the way it is). It retires the hand-written
//! `proc_pidinfo` recipe every idle-CPU investigation used to re-derive
//! (`docs/notes/performance/idle-census-2026-09-27.md` § "A better CPU instrument").
//!
//! Two ways to set the window:
//!
//! - `measure` (the default) blocks for `windowSeconds` and diffs.
//! - `mark` stores a baseline and returns at once; each later `sinceMark` diffs against it
//!   without blocking. That's the shape for an idle census: a 20-minute window shouldn't
//!   hold an MCP call open, and the baseline stays put across reads, so they're cumulative.

use serde_json::Value;

use super::{ToolError, ToolResult};

/// Window when the caller doesn't say: long enough to see idle threads at a few hundredths of a
/// percent (10 ms of a 10 s window is 0.1%), short enough to wait for.
const DEFAULT_WINDOW_SECONDS: u64 = 10;
/// The longest blocking window. Longer ones go through `mark` / `sinceMark`.
const MAX_WINDOW_SECONDS: u64 = 60;
/// Individual threads when the caller doesn't say. `threadsByName` always lists every group.
const DEFAULT_MAX_THREADS: usize = 20;
const MAX_MAX_THREADS: usize = 200;

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Measure { window_seconds: u64 },
    Mark,
    SinceMark,
}

#[derive(Debug, PartialEq, Eq)]
struct CpuRequest {
    action: Action,
    max_threads: usize,
}

fn parse_request(params: &Value) -> Result<CpuRequest, ToolError> {
    let window = optional_whole(params, "windowSeconds")?;
    let max_threads = match optional_whole(params, "maxThreads")? {
        None => DEFAULT_MAX_THREADS,
        Some(n) if n <= MAX_MAX_THREADS as u64 => n as usize,
        Some(_) => {
            return Err(ToolError::invalid_params(format!(
                "'maxThreads' must be between 0 and {MAX_MAX_THREADS}"
            )));
        }
    };
    let action = match params.get("action").and_then(Value::as_str) {
        None | Some("measure") => {
            let window_seconds = window.unwrap_or(DEFAULT_WINDOW_SECONDS);
            if !(1..=MAX_WINDOW_SECONDS).contains(&window_seconds) {
                return Err(ToolError::invalid_params(format!(
                    "'windowSeconds' must be between 1 and {MAX_WINDOW_SECONDS}. For a longer window, call with action 'mark', then 'sinceMark' later."
                )));
            }
            Action::Measure { window_seconds }
        }
        Some(other) if window.is_some() => {
            return Err(ToolError::invalid_params(format!(
                "'windowSeconds' only applies to action 'measure', not '{other}'"
            )));
        }
        Some("mark") => Action::Mark,
        Some("sinceMark") => Action::SinceMark,
        Some(other) => {
            return Err(ToolError::invalid_params(format!(
                "Unknown action '{other}'. Use 'measure', 'mark', or 'sinceMark'."
            )));
        }
    };
    Ok(CpuRequest { action, max_threads })
}

fn optional_whole(params: &Value, key: &str) -> Result<Option<u64>, ToolError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .ok_or_else(|| ToolError::invalid_params(format!("'{key}' must be a whole number of 0 or more"))),
    }
}

#[cfg(target_os = "macos")]
mod live {
    use std::sync::Mutex;
    use std::time::Duration;

    use cmdr_fs::process_cpu::{CpuCensus, CpuSnapshot};
    use serde_json::json;

    use super::{Action, CpuRequest, ToolError, ToolResult};
    use crate::ignore_poison::IgnorePoison;

    /// The `mark` baseline. One per process: the tool answers about the process, and two
    /// callers wanting separate baselines can each `measure` instead.
    static MARK: Mutex<Option<CpuSnapshot>> = Mutex::new(None);

    fn take() -> Result<CpuSnapshot, ToolError> {
        CpuSnapshot::take().ok_or_else(|| ToolError::internal("Couldn't read this process's threads from the kernel."))
    }

    fn census(start: &CpuSnapshot, end: &CpuSnapshot, max_threads: usize) -> ToolResult {
        serde_json::to_value(CpuCensus::between(start, end, max_threads))
            .map_err(|e| ToolError::internal(format!("Couldn't serialize the census: {e}")))
    }

    pub(super) async fn run(request: CpuRequest) -> ToolResult {
        match request.action {
            Action::Measure { window_seconds } => {
                let start = take()?;
                tokio::time::sleep(Duration::from_secs(window_seconds)).await;
                census(&start, &take()?, request.max_threads)
            }
            Action::Mark => {
                let snapshot = take()?;
                let threads = snapshot.threads.len();
                *MARK.lock_ignore_poison() = Some(snapshot);
                Ok(json!({ "marked": true, "threads": threads }))
            }
            Action::SinceMark => {
                let start = MARK.lock_ignore_poison().clone().ok_or_else(|| {
                    ToolError::invalid_params("No mark yet. Call with action 'mark' first, then 'sinceMark' later.")
                })?;
                census(&start, &take()?, request.max_threads)
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub async fn execute_cpu_diagnostics(params: &Value) -> ToolResult {
    live::run(parse_request(params)?).await
}

/// Everywhere else the `proc_pidinfo` and Mach queries behind the census don't exist, so the
/// honest answer is a refusal, never a census of zeros.
#[cfg(not(target_os = "macos"))]
pub async fn execute_cpu_diagnostics(params: &Value) -> ToolResult {
    parse_request(params)?;
    Err(ToolError::internal(
        "CPU diagnostics are macOS only: the per-thread kernel queries the census reads don't exist on this platform.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_bare_call_measures_the_default_window() {
        assert_eq!(
            parse_request(&json!({})).expect("valid"),
            CpuRequest {
                action: Action::Measure {
                    window_seconds: DEFAULT_WINDOW_SECONDS
                },
                max_threads: DEFAULT_MAX_THREADS,
            }
        );
    }

    #[test]
    fn mark_and_since_mark_parse_with_a_thread_cap() {
        assert_eq!(
            parse_request(&json!({ "action": "mark" })).expect("valid").action,
            Action::Mark
        );
        let since = parse_request(&json!({ "action": "sinceMark", "maxThreads": 5 })).expect("valid");
        assert_eq!(since.action, Action::SinceMark);
        assert_eq!(since.max_threads, 5);
    }

    /// A long blocking window would hold the MCP call open, so it's refused (the message names
    /// `mark` as the way out).
    #[test]
    fn a_window_past_the_cap_is_refused_and_points_at_mark() {
        let err = parse_request(&json!({ "windowSeconds": 61 })).expect_err("too long");
        assert_eq!(err.code, crate::mcp::protocol::INVALID_PARAMS);
        parse_request(&json!({ "windowSeconds": 60 })).expect("the cap itself is fine");
        parse_request(&json!({ "windowSeconds": 0 })).expect_err("zero window");
    }

    /// `windowSeconds` beside `sinceMark` would silently mean nothing.
    #[test]
    fn a_window_beside_a_non_measure_action_is_refused() {
        parse_request(&json!({ "action": "sinceMark", "windowSeconds": 5 })).expect_err("window is measure-only");
    }

    #[test]
    fn unknown_actions_and_bad_numbers_are_refused() {
        parse_request(&json!({ "action": "sample" })).expect_err("unknown action");
        parse_request(&json!({ "maxThreads": 201 })).expect_err("cap");
        parse_request(&json!({ "maxThreads": -1 })).expect_err("negative");
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn since_mark_without_a_mark_is_refused_then_diffs_after_one() {
        // Runs in its own process under nextest, so the static starts empty.
        execute_cpu_diagnostics(&json!({ "action": "sinceMark" }))
            .await
            .expect_err("no mark yet");
        let marked = execute_cpu_diagnostics(&json!({ "action": "mark" }))
            .await
            .expect("mark");
        assert_eq!(marked["marked"], json!(true));
        let census = execute_cpu_diagnostics(&json!({ "action": "sinceMark", "maxThreads": 3 }))
            .await
            .expect("census");
        assert!(census["threads"].as_array().expect("threads").len() <= 3);
        assert!(census["threadCount"]["end"].as_u64().expect("count") > 0);
        assert!(census["process"]["percentOfOneCore"].is_number());
    }
}
