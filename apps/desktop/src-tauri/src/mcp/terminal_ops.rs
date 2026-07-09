//! Terminal-ops ring buffer: the last few write operations that SETTLED
//! (completed, cancelled, or failed), for the `await operation_complete`
//! condition and the `queue` tool.
//!
//! **Why a ring at all.** The operation manager removes an op from its registry
//! the moment it settles (removal-on-terminal, `write_operations/DETAILS.md`),
//! and `operations-changed` fires AFTER that removal — so its snapshot never
//! carries a terminal status (`LifecycleStatus` never reaches `Done`/`Cancelled`/
//! `Failed` on a live record). The terminal outcome lives ONLY in the dedicated
//! terminal events (`write-complete` / `write-cancelled` / `write-error`). So an
//! `await operation_complete <id>` that arrives just after the op finished would
//! find the id in neither the live set nor anywhere else and hang. This ring
//! records those terminal events at their emit site (the `TauriEventSink`, the
//! same emit-site pattern as `listing_errors`) so the await can report an honest
//! terminal status instead.
//!
//! Ring-buffered to 20 entries: enough for a slow agent to catch a recent
//! settle, small enough that a busy batch session pays a bounded memory cost. A
//! settled op can be pushed off by newer settles before a slow agent awaits it;
//! that's acceptable — the await then returns an honest "unknown operationId"
//! rather than a wrong answer.

use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::file_system::write_operations::WriteOperationType;

/// How a write operation ended. Maps one-to-one from the terminal event kind
/// (`write-complete` → `Completed`, `write-cancelled` → `Cancelled`,
/// `write-error` → `Failed`); a typed enum, not a message substring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalStatus {
    Completed,
    Cancelled,
    Failed,
}

impl TerminalStatus {
    /// The lower-case token surfaced to agents (in `await` result text).
    pub fn as_token(self) -> &'static str {
        match self {
            TerminalStatus::Completed => "completed",
            TerminalStatus::Cancelled => "cancelled",
            TerminalStatus::Failed => "failed",
        }
    }
}

/// One settled operation.
#[derive(Debug, Clone)]
pub struct TerminalOp {
    pub operation_id: String,
    pub operation_type: WriteOperationType,
    pub status: TerminalStatus,
    /// Wall-clock millis since UNIX epoch; matches JS `Date.now()`.
    pub settled_at_unix_ms: u64,
}

const CAPACITY: usize = 20;

static BUFFER: LazyLock<Mutex<VecDeque<TerminalOp>>> =
    LazyLock::new(|| Mutex::new(VecDeque::with_capacity(CAPACITY)));

/// Record a settled operation. Called from `TauriEventSink`'s terminal emit
/// sites right where the `write-complete` / `write-cancelled` / `write-error`
/// event fires, so MCP-visible state matches what the FE saw.
pub fn record(operation_id: &str, operation_type: WriteOperationType, status: TerminalStatus) {
    let settled_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let entry = TerminalOp {
        operation_id: operation_id.to_string(),
        operation_type,
        status,
        settled_at_unix_ms,
    };
    let mut buf = match BUFFER.lock() {
        Ok(b) => b,
        Err(poisoned) => poisoned.into_inner(),
    };
    if buf.len() == CAPACITY {
        buf.pop_front();
    }
    buf.push_back(entry);
}

/// The most recent terminal record for `operation_id`, if it's still in the
/// ring. Walks newest-first so a re-used id (shouldn't happen — ids are unique)
/// would resolve to its latest settle.
pub fn lookup(operation_id: &str) -> Option<TerminalOp> {
    let buf = match BUFFER.lock() {
        Ok(b) => b,
        Err(poisoned) => poisoned.into_inner(),
    };
    buf.iter().rev().find(|op| op.operation_id == operation_id).cloned()
}

#[cfg(test)]
pub fn clear_for_test() {
    if let Ok(mut buf) = BUFFER.lock() {
        buf.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_then_lookup_returns_type_and_status() {
        clear_for_test();
        record("op-1", WriteOperationType::Copy, TerminalStatus::Completed);
        let found = lookup("op-1").expect("recorded op is found");
        assert_eq!(found.operation_id, "op-1");
        assert_eq!(found.operation_type, WriteOperationType::Copy);
        assert_eq!(found.status, TerminalStatus::Completed);
    }

    #[test]
    fn lookup_unknown_id_is_none() {
        clear_for_test();
        record("op-1", WriteOperationType::Move, TerminalStatus::Cancelled);
        assert!(lookup("op-nope").is_none());
    }

    #[test]
    fn each_status_maps_to_its_token() {
        assert_eq!(TerminalStatus::Completed.as_token(), "completed");
        assert_eq!(TerminalStatus::Cancelled.as_token(), "cancelled");
        assert_eq!(TerminalStatus::Failed.as_token(), "failed");
    }

    #[test]
    fn buffer_drops_oldest_past_capacity() {
        clear_for_test();
        for i in 0..(CAPACITY + 5) {
            record(&format!("op-{i}"), WriteOperationType::Delete, TerminalStatus::Completed);
        }
        // The five oldest are gone; the earliest surviving id is `op-5`.
        assert!(lookup("op-4").is_none(), "op-4 should have been evicted");
        assert!(lookup("op-5").is_some(), "op-5 should still be present");
        assert!(lookup(&format!("op-{}", CAPACITY + 4)).is_some(), "newest present");
    }

    #[test]
    fn lookup_returns_the_latest_settle_for_an_id() {
        // Ids are unique in production, but the newest-first walk means a
        // re-recorded id resolves to its latest status. Pins that contract.
        clear_for_test();
        record("op-x", WriteOperationType::Copy, TerminalStatus::Cancelled);
        record("op-x", WriteOperationType::Copy, TerminalStatus::Completed);
        assert_eq!(lookup("op-x").expect("present").status, TerminalStatus::Completed);
    }
}
