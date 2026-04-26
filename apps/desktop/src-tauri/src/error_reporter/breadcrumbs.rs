//! Breadcrumb ring buffer for error report triage.
//!
//! Records a rolling window of recent FE/BE events so triagers can see what led up
//! to an error. Sentry-style, but kept tiny: bounded queue, fire-and-forget recording,
//! shipped as part of the error report bundle's manifest.
//!
//! ## Why not just use logs?
//!
//! Logs are noisy and unstructured. Breadcrumbs are structured (kind + ctx) and
//! curated — only the kinds we care about during triage. Future work: replace
//! `last_user_action` entirely (it's the same idea with N=1).

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::Mutex;

/// Maximum number of breadcrumbs retained. Once exceeded, the oldest is dropped.
/// Sized so the bundle stays small (each entry is ~100-300 bytes serialized) but
/// covers a meaningful window of activity (typically a few minutes of normal use).
pub const MAX_BREADCRUMBS: usize = 50;

/// Cap on the message field. Guards against the FE accidentally pushing a pasted
/// blob in. Real breadcrumb messages are short labels.
pub const MAX_MESSAGE_CHARS: usize = 256;

/// Cap on the kind field. Real kinds are short ("nav", "command", "dialog", ...).
pub const MAX_KIND_CHARS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Breadcrumb {
    /// ISO-8601 UTC timestamp.
    pub at: String,
    /// Short label for the event category, e.g. "command", "nav", "dialog", "error".
    pub kind: String,
    /// Free-form short description.
    pub message: String,
    /// Optional structured context (e.g. `{ "from": "...", "to": "..." }`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctx: Option<Value>,
}

static BUFFER: Mutex<VecDeque<Breadcrumb>> = Mutex::new(VecDeque::new());

/// Append a breadcrumb. Drops the oldest entry if the buffer is full.
///
/// Silent on overflow / lock poisoning — breadcrumbs are best-effort instrumentation,
/// not a feature we'd ever surface a failure for.
pub fn record(kind: &str, message: &str, ctx: Option<Value>) {
    if kind.is_empty() || kind.chars().count() > MAX_KIND_CHARS {
        return;
    }
    let trimmed_message: String = message.chars().take(MAX_MESSAGE_CHARS).collect();
    let crumb = Breadcrumb {
        at: Utc::now().to_rfc3339(),
        kind: kind.to_string(),
        message: trimmed_message,
        ctx,
    };
    let Ok(mut guard) = BUFFER.lock() else {
        return;
    };
    if guard.len() >= MAX_BREADCRUMBS {
        guard.pop_front();
    }
    guard.push_back(crumb);
}

/// Snapshot of the current ring buffer, oldest first. Used at bundle-build time.
pub fn snapshot() -> Vec<Breadcrumb> {
    let Ok(guard) = BUFFER.lock() else {
        return Vec::new();
    };
    guard.iter().cloned().collect()
}

#[cfg(test)]
pub fn reset_for_test() {
    if let Ok(mut g) = BUFFER.lock() {
        g.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_snapshot_round_trip() {
        reset_for_test();
        record("nav", "to /Users", None);
        record("command", "open-palette", Some(serde_json::json!({"source": "key"})));
        let snap = snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].kind, "nav");
        assert_eq!(snap[1].kind, "command");
        assert_eq!(snap[1].ctx.as_ref().unwrap()["source"], "key");
    }

    #[test]
    fn ring_buffer_drops_oldest_when_full() {
        reset_for_test();
        for i in 0..(MAX_BREADCRUMBS + 5) {
            record("test", &format!("event-{i}"), None);
        }
        let snap = snapshot();
        assert_eq!(snap.len(), MAX_BREADCRUMBS);
        // Oldest 5 should be gone; first remaining is event-5
        assert_eq!(snap[0].message, "event-5");
        assert_eq!(snap.last().unwrap().message, format!("event-{}", MAX_BREADCRUMBS + 4));
    }

    #[test]
    fn rejects_empty_or_oversized_kind() {
        reset_for_test();
        record("", "x", None);
        record(&"k".repeat(MAX_KIND_CHARS + 1), "x", None);
        assert!(snapshot().is_empty());
    }

    #[test]
    fn message_is_truncated_to_max_chars() {
        reset_for_test();
        let huge = "a".repeat(MAX_MESSAGE_CHARS * 4);
        record("k", &huge, None);
        let snap = snapshot();
        assert_eq!(snap[0].message.chars().count(), MAX_MESSAGE_CHARS);
    }
}
