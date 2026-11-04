//! Golden traces: what each kind of operation SAYS and RECORDS, end to end through
//! the engine's public entry points, pinned as JSON fixtures under
//! `crates/cmdr-operations/tests/golden/` (one per op kind and engine).
//!
//! A trace is the op's events in order with their payloads (progress and scan
//! ticks left out: how many arrive is timing, not behavior), the per-source
//! events as a set (a concurrent copy finishes its sources in any order), the
//! typed answer the entry point gave, and the journal rows it left. Ids,
//! timestamps, rates, and the scratch directory are normalized away.
//!
//! The scenarios and the normalizer live here, not in a test file, because two
//! hosts run them: this crate's own cells (`golden_trace_tests.rs`, every
//! engine, natively) and `cmdr-listing-wasm`'s (the volume engine on the demo's
//! host, natively AND as a wasm test build under Node), so the browser's
//! `JoinSet` ordering, inline op-log writer, and inline `spawn_blocking` must
//! produce the same traces the native engine blesses. Behind the `golden`
//! feature (dev-dependencies only), so no shipped build carries it.

mod scenarios;

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_operation_log::store::{open_read_connection, read_operation, read_operation_items, recent_operations};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::Notify;

pub use scenarios::{Scenario, Sides, VOLUME_GOLDENS, VolumeWorld, World};

use crate::conflict_slot::ArmedConflict;
use crate::host::OperationEventSink;
use crate::types::{
    ConflictInfo, DryRunResult, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent, WriteConflictResolvedEvent,
    WriteErrorEvent, WriteLockedSourcesEvent, WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent,
    WriteSourcesGoneEvent,
};

// ── The recording sink ───────────────────────────────────────────────

/// Records every event an operation emits, by its wire name, and wakes a waiter
/// once the operation settles.
// DEFAULT-OK: a recorder that has heard nothing yet is exactly empty lists.
#[derive(Default)]
pub struct TraceSink {
    ordered: Mutex<Vec<Value>>,
    items: Mutex<Vec<Value>>,
    settled: Notify,
}

impl TraceSink {
    fn record(&self, name: &str, payload: &impl Serialize) {
        let payload = serde_json::to_value(payload).expect("event payloads serialize");
        self.ordered
            .lock_ignore_poison()
            .push(json!({ "event": name, "payload": payload }));
    }

    async fn wait_settled(&self) {
        cmdr_fs::rt::time::timeout(Duration::from_secs(20), self.settled.notified())
            .await
            .expect("the operation settles within 20 s");
    }
}

impl OperationEventSink for TraceSink {
    fn emit_progress(&self, _event: WriteProgressEvent) {}
    fn emit_complete(&self, event: WriteCompleteEvent) {
        self.record("write-complete", &event);
    }
    fn emit_cancelled(&self, event: WriteCancelledEvent) {
        self.record("write-cancelled", &event);
    }
    fn emit_error(&self, event: WriteErrorEvent) {
        self.record("write-error", &event);
    }
    fn emit_conflict(&self, event: ArmedConflict) {
        self.record("write-conflict", &event.into_event());
    }
    fn emit_conflict_resolved(&self, event: WriteConflictResolvedEvent) {
        self.record("write-conflict-resolved", &event);
    }
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
        let payload = serde_json::to_value(&event).expect("event payloads serialize");
        self.items.lock_ignore_poison().push(payload);
    }
    fn emit_sources_gone(&self, event: WriteSourcesGoneEvent) {
        self.record("write-sources-gone", &event);
    }
    fn emit_locked_sources(&self, event: WriteLockedSourcesEvent) {
        self.record("write-locked-sources", &event);
    }
    fn emit_scan_progress(&self, _event: ScanProgressEvent) {}
    fn emit_scan_conflict(&self, conflict: ConflictInfo) {
        self.record("scan-conflict", &conflict);
    }
    fn emit_dry_run_complete(&self, result: DryRunResult) {
        self.record("dry-run-complete", &result);
    }
    fn emit_settled(&self, event: WriteSettledEvent) {
        self.record("write-settled", &event);
        self.settled.notify_one();
    }
}

// ── Normalizing ──────────────────────────────────────────────────────

/// Keys whose value is a moment, a duration, a rate, or an id: timing and
/// identity, never behavior.
fn is_volatile_key(key: &str) -> bool {
    // Ids an operation is minted with (a volume id is behavior, so it stays).
    const IDS: [&str; 7] = [
        "operationId",
        "conflictId",
        "previewId",
        "opId",
        "itemId",
        "rollsBackOpId",
        "inverseOpId",
    ];
    if IDS.contains(&key) || key == "mtime" {
        return true;
    }
    // camelCase suffixes for a moment or a span: `startedAt`, `durationMs`.
    if ["At", "Ms", "Secs", "Seconds"]
        .iter()
        .any(|suffix| key.ends_with(suffix))
    {
        return true;
    }
    let lower = key.to_ascii_lowercase();
    ["duration", "elapsed", "persecond", "throughput", "eta"]
        .iter()
        // allowed-error-string-match: matches JSON key NAMES in a trace, not an error or state value.
        .any(|word| lower == *word || lower.contains(word) && *word != "eta")
}

/// `value` with volatile keys blanked and `scratch` (and its canonical twin)
/// spelled `<tmp>`, so two runs compare equal.
fn normalize(value: Value, scratch: &[String]) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let value = if is_volatile_key(&key) && !value.is_null() {
                        Value::String("<volatile>".into())
                    } else {
                        normalize(value, scratch)
                    };
                    (key, value)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(|v| normalize(v, scratch)).collect()),
        Value::String(text) => {
            let mut text = text;
            for dir in scratch {
                text = text.replace(dir.as_str(), "<tmp>");
            }
            Value::String(text)
        }
        other => other,
    }
}

// ── A run, and its trace ─────────────────────────────────────────────

/// Every op the journal at `db` holds (oldest first), each with its items,
/// projected onto what's behavior.
fn journal(db: &Path) -> Value {
    let conn = open_read_connection(db).expect("a read connection");
    let ops = recent_operations(&conn, 50).expect("the journal reads");
    let mut rows = Vec::new();
    for op in ops.iter().rev() {
        let row = read_operation(&conn, &op.op_id).expect("reads").expect("the op row");
        let mut items: Vec<Value> = read_operation_items(&conn, &op.op_id, 1_000)
            .expect("items read")
            .into_iter()
            .map(|item| {
                json!({
                    "entryType": format!("{:?}", item.entry_type),
                    "rowRole": format!("{:?}", item.row_role),
                    "source": item.source_name,
                    "dest": item.dest_name,
                    "size": item.size,
                    "outcome": format!("{:?}", item.outcome),
                })
            })
            .collect();
        items.sort_by_key(|item| sorted_keys(item).to_string());
        rows.push(json!({
            "kind": format!("{:?}", row.kind),
            "initiator": format!("{:?}", row.initiator),
            "status": format!("{:?}", row.execution_status),
            "rollback": format!("{:?}", row.rollback_state),
            "notRollbackable": row.not_rollbackable_reason.map(|r| format!("{r:?}")),
            "reverses": row.rolls_back_op_id.is_some(),
            "sourceVolume": row.source_volume_id,
            "destVolume": row.dest_volume_id,
            "itemCount": row.item_count,
            "itemsDone": row.items_done,
            "bytesTotal": row.bytes_total,
            "items": items,
        }));
    }
    Value::Array(rows)
}

/// What one scenario produced: the entry points' answers, the events, and the
/// journal, normalized.
fn trace(world: &impl World, answers: Vec<Value>, sinks: &[&TraceSink]) -> Value {
    let events: Vec<Value> = sinks
        .iter()
        .map(|sink| {
            let mut items = sink.items.lock_ignore_poison().clone();
            items.sort_by_key(|item| sorted_keys(item).to_string());
            json!({
                "ordered": sink.ordered.lock_ignore_poison().clone(),
                "sourceItems": items,
            })
        })
        .collect();
    normalize(
        json!({ "answers": answers, "events": events, "journal": journal(world.journal_db()) }),
        &world.scratch(),
    )
}

/// A trace as its fixture file spells it, so a fixture compares as text. Keys
/// are sorted here, ❌ never left to `serde_json::Map`: the workspace build
/// turns on `preserve_order` (through `schemars`), where a map keeps insertion
/// order, so the same trace would render two ways.
pub fn render(trace: &Value) -> String {
    serde_json::to_string_pretty(&sorted_keys(trace)).expect("renders") + "\n"
}

/// `value` with every object's keys in sorted order, at every depth.
fn sorted_keys(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            Value::Object(keys.into_iter().map(|k| (k.clone(), sorted_keys(&map[k]))).collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted_keys).collect()),
        other => other.clone(),
    }
}

fn answer<T: Serialize, E: Serialize>(result: &Result<T, E>) -> Value {
    match result {
        Ok(value) => json!({ "ok": serde_json::to_value(value).expect("answers serialize") }),
        Err(error) => json!({ "err": serde_json::to_value(error).expect("errors serialize") }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: the workspace build turns on `serde_json/preserve_order` (through
    /// `schemars`), where an object keeps insertion order, so every fixture
    /// "moved" in `pnpm check rust-tests` but not under `-p cmdr-operations`.
    #[test]
    fn a_trace_renders_its_keys_sorted_whatever_the_map_order() {
        let mut inner = serde_json::Map::new();
        inner.insert("zeta".into(), json!(1));
        inner.insert("alpha".into(), json!(2));
        let mut outer = serde_json::Map::new();
        outer.insert("b".into(), json!([Value::Object(inner)]));
        outer.insert("a".into(), json!(null));
        let rendered = render(&Value::Object(outer));
        assert_eq!(
            rendered,
            "{\n  \"a\": null,\n  \"b\": [\n    {\n      \"alpha\": 2,\n      \"zeta\": 1\n    }\n  ]\n}\n"
        );
    }
}
