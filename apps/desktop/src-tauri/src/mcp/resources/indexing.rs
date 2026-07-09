//! The `cmdr://indexing` resource builder.
//!
//! Reports per-volume drive-indexing status for LLM readers: one block per
//! known (registered) volume in the default view, and a deep debug view for a
//! single volume via `?volume=<id>`.
//!
//! Two consumers: an agent asking "can I trust search on this volume?" (the
//! freshness + phase summary) and David debugging "why is this scan stuck?" (the
//! `?volume=` timeline, trigger, and watcher stats). Both read the same indexing
//! module APIs; freshness is never re-derived here (the one transition table
//! lives in `indexing/freshness.rs`).
//!
//! The builders (`build_indexing_text`, `build_volume_debug_text`) are pure over
//! an injected [`VolumeIndexingSnapshot`] (the `resources/transfers.rs`
//! snapshot-then-format precedent), so the formatting is unit-testable without a
//! live index. `snapshot_indexing` / `snapshot_volume_indexing` do the global
//! reads.

use crate::indexing::freshness::Freshness;
use crate::indexing::{ActivityPhase, PhaseRecord};
use crate::search::format_size;

/// One volume's indexing status, snapshotted from the indexing read APIs. Plain
/// data so the text builders stay pure and testable.
#[derive(Debug, Clone)]
pub(crate) struct VolumeIndexingSnapshot {
    pub volume_id: String,
    /// `local` / `smb` / `mtp`, or `None` when the volume has no registered
    /// instance (kind is carried by the instance).
    pub kind: Option<&'static str>,
    /// Whether an index is registered and being kept live. `false` ⇒ off / gray.
    pub enabled: bool,
    /// Freshness color; `None` ⇒ off / not-indexed.
    pub freshness: Option<Freshness>,
    /// The indexer's current activity phase.
    pub activity_phase: ActivityPhase,
    /// How long the current phase has been running.
    pub phase_duration_ms: u64,
    /// Whether a scan is running (drives the "scan progress" line).
    pub scanning: bool,
    pub entries_scanned: u64,
    pub dirs_found: u64,
    pub bytes_scanned: u64,
    /// The volume's used bytes at scan start (the first-scan progress
    /// denominator); present only while a calibrated scan runs.
    pub volume_used_bytes: Option<u64>,
    /// Live DB entry / dir counts and total file size.
    pub db_entry_count: Option<u64>,
    pub db_dir_count: Option<u64>,
    pub db_file_size: Option<u64>,
    /// Unix seconds of the last completed scan (for "last scan: … ago").
    pub scan_completed_at: Option<u64>,
    /// The last completed scan's wall-clock duration.
    pub scan_duration_ms: Option<u64>,
    /// Deep debug detail, present only for the `?volume=<id>` view.
    pub debug: Option<VolumeIndexingDebug>,
}

/// The deep-view-only fields (timeline, trigger, watcher / live-event stats, DB
/// internals). Filled by `snapshot_volume_indexing`, absent in the default view.
#[derive(Debug, Clone)]
pub(crate) struct VolumeIndexingDebug {
    pub watcher_active: bool,
    pub live_event_count: u64,
    pub must_scan_count: u64,
    pub must_scan_rescans_completed: u64,
    pub verifying: bool,
    pub db_main_size: Option<u64>,
    pub db_wal_size: Option<u64>,
    pub db_page_count: Option<u64>,
    pub db_freelist_count: Option<u64>,
    pub phase_history: Vec<PhaseRecord>,
}

/// The four pipeline steps the FE step checklist advances through. Rendered when
/// a volume is scanning so an agent can see where a scan is. Network volumes
/// (SMB/MTP) emit only `Scanning → Live` phase events, so the middle steps flip
/// to done together at `Live` — honest, not a bug.
const PIPELINE_STEPS: [&str; 4] = ["Scan", "Aggregate sizes", "Reconcile", "Go live"];

/// The pipeline-step index a phase corresponds to (`Scan` = 0 … `Go live` = 3).
/// `Idle` sits past the end (nothing in flight). Used to mark done / current /
/// pending in the checklist.
fn phase_step_index(phase: &ActivityPhase) -> usize {
    match phase {
        ActivityPhase::Replaying | ActivityPhase::Scanning => 0,
        ActivityPhase::Aggregating => 1,
        ActivityPhase::Reconciling => 2,
        ActivityPhase::Live => 3,
        ActivityPhase::Idle => PIPELINE_STEPS.len(),
    }
}

/// The freshness token an agent matches on: `fresh` / `scanning` / `stale`, or
/// `off` when no index is registered. This is the single mapping the resource
/// and the `await index_status` condition share.
pub(crate) fn freshness_token(freshness: Option<Freshness>) -> &'static str {
    match freshness {
        Some(Freshness::Fresh) => "fresh",
        Some(Freshness::Scanning) => "scanning",
        Some(Freshness::Stale) => "stale",
        None => "off",
    }
}

/// Format a duration in milliseconds as a human-readable string.
pub fn format_duration_human(ms: u64) -> String {
    if ms < 1_000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        let secs = ms as f64 / 1000.0;
        format!("{:.1}s", secs)
    } else if ms < 3_600_000 {
        let mins = ms / 60_000;
        let secs = (ms % 60_000) / 1000;
        if secs == 0 {
            format!("{}m", mins)
        } else {
            format!("{}m {:02}s", mins, secs)
        }
    } else {
        let hours = ms / 3_600_000;
        let mins = (ms % 3_600_000) / 60_000;
        format!("{}h {:02}m", hours, mins)
    }
}

/// Format a number with comma separators.
pub fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// The `db: …` line, or `None` when no counts are known (never-scanned volume).
fn db_line(snap: &VolumeIndexingSnapshot) -> Option<String> {
    let (entries, dirs) = (snap.db_entry_count?, snap.db_dir_count?);
    let size = snap
        .db_file_size
        .map(|s| format!(", {}", format_size(s)))
        .unwrap_or_default();
    Some(format!(
        "{} entries, {} dirs{}",
        format_number(entries),
        format_number(dirs),
        size
    ))
}

/// The `scan progress: …` line while a scan is running, with a percent + ETA
/// when the used-bytes denominator is known. `None` when not scanning or no
/// bytes have been counted yet.
fn scan_progress_line(snap: &VolumeIndexingSnapshot) -> Option<String> {
    if !snap.scanning {
        return None;
    }
    let mut line = format!(
        "{} entries, {} dirs, {}",
        format_number(snap.entries_scanned),
        format_number(snap.dirs_found),
        format_size(snap.bytes_scanned)
    );
    if let Some(total) = snap.volume_used_bytes
        && total > 0
        && snap.bytes_scanned > 0
    {
        let pct = ((snap.bytes_scanned as f64 / total as f64) * 100.0).min(100.0) as u8;
        let mut detail = format!("{pct}% of {}", format_size(total));
        // ETA from the current phase's elapsed time: rate = bytes / elapsed,
        // remaining = (total - done) / rate. Guard every division.
        let elapsed_s = snap.phase_duration_ms / 1000;
        if elapsed_s > 0 && snap.bytes_scanned <= total {
            let rate = snap.bytes_scanned / elapsed_s;
            if let Some(eta_s) = (total - snap.bytes_scanned).checked_div(rate) {
                detail.push_str(&format!(", ETA {}", format_duration_human(eta_s * 1000)));
            }
        }
        line.push_str(&format!(" ({detail})"));
    }
    Some(line)
}

/// The `last scan: …` line: relative age plus the scan's duration, or `none yet`.
fn last_scan_line(snap: &VolumeIndexingSnapshot, now_unix_s: u64) -> String {
    match snap.scan_completed_at {
        Some(completed) => {
            let ago = now_unix_s.saturating_sub(completed);
            let took = snap
                .scan_duration_ms
                .map(|ms| format!(" (took {})", format_duration_human(ms)))
                .unwrap_or_default();
            format!("{} ago{}", format_duration_human(ago * 1000), took)
        }
        None => "none yet".to_string(),
    }
}

/// Render one volume's summary block (shared by the default and deep views).
fn push_volume_summary(lines: &mut Vec<String>, snap: &VolumeIndexingSnapshot, now_unix_s: u64) {
    let header = match snap.kind {
        Some(kind) => format!("{} ({}):", snap.volume_id, kind),
        None => format!("{}:", snap.volume_id),
    };
    lines.push(header);

    let status = if snap.enabled {
        freshness_token(snap.freshness)
    } else {
        "off"
    };
    lines.push(format!("  status: {status}"));

    // An off / not-indexed volume has nothing more worth showing.
    if !snap.enabled {
        return;
    }

    lines.push(format!("  phase: {}", snap.activity_phase));

    if let Some(progress) = scan_progress_line(snap) {
        lines.push(format!("  scan progress: {progress}"));
    }

    // The step checklist, only while scanning (it answers "where is the scan?").
    if snap.freshness == Some(Freshness::Scanning) {
        let current = phase_step_index(&snap.activity_phase);
        lines.push("  steps:".to_string());
        for (i, step) in PIPELINE_STEPS.iter().enumerate() {
            let marker = if i < current {
                "[x]"
            } else if i == current {
                "[~]"
            } else {
                "[ ]"
            };
            lines.push(format!("    {marker} {step}"));
        }
    }

    if let Some(db) = db_line(snap) {
        lines.push(format!("  db: {db}"));
    }

    lines.push(format!("  last scan: {}", last_scan_line(snap, now_unix_s)));
}

/// Build the default `cmdr://indexing` text: one summary block per known volume,
/// or an honest empty state. `now_unix_s` is injected so tests don't touch the
/// wall clock.
pub(crate) fn build_indexing_text(snapshots: &[VolumeIndexingSnapshot], now_unix_s: u64) -> String {
    if snapshots.is_empty() {
        return "No volumes are being indexed.".to_string();
    }

    let mut lines = Vec::new();
    lines.push(format!(
        "Indexing status for {} volume{}:",
        snapshots.len(),
        if snapshots.len() == 1 { "" } else { "s" }
    ));
    for snap in snapshots {
        lines.push(String::new());
        push_volume_summary(&mut lines, snap, now_unix_s);
    }
    lines.join("\n")
}

/// Build the deep `?volume=<id>` text: the summary block plus the debug detail
/// (watcher / live-event stats, DB internals, phase timeline with triggers).
pub(crate) fn build_volume_debug_text(snap: &VolumeIndexingSnapshot, now_unix_s: u64) -> String {
    let mut lines = Vec::new();
    push_volume_summary(&mut lines, snap, now_unix_s);

    if let Some(debug) = &snap.debug {
        lines.push(String::new());
        lines.push("  debug:".to_string());
        lines.push(format!(
            "    watcher: {}, {} live events",
            if debug.watcher_active { "on" } else { "off" },
            format_number(debug.live_event_count)
        ));
        lines.push(format!("    verifying: {}", if debug.verifying { "yes" } else { "no" }));
        lines.push(format!(
            "    mustScan: {} events, {} rescans completed",
            format_number(debug.must_scan_count),
            format_number(debug.must_scan_rescans_completed)
        ));

        let mut db_detail = Vec::new();
        if let Some(main) = debug.db_main_size {
            db_detail.push(format!("main {}", format_size(main)));
        }
        if let Some(wal) = debug.db_wal_size
            && wal > 0
        {
            db_detail.push(format!("WAL {}", format_size(wal)));
        }
        if let (Some(pages), Some(free)) = (debug.db_page_count, debug.db_freelist_count)
            && free > 0
        {
            db_detail.push(format!("{} pages, {} free", format_number(pages), format_number(free)));
        }
        if !db_detail.is_empty() {
            lines.push(format!("    db detail: {}", db_detail.join(", ")));
        }

        // Trigger of the current (or last) phase.
        if let Some(last) = debug.phase_history.last()
            && !last.trigger.is_empty()
        {
            lines.push(format!("    trigger: {}", last.trigger));
        }

        if !debug.phase_history.is_empty() {
            lines.push("    history:".to_string());
            for (i, record) in debug.phase_history.iter().enumerate() {
                let is_current = i == debug.phase_history.len() - 1 && record.duration_ms.is_none();
                let duration_str = match record.duration_ms {
                    Some(ms) => format!("{:>8}", format_duration_human(ms)),
                    None => format!("{:>8}", format_duration_human(snap.phase_duration_ms)),
                };
                let phase_name = format!("{:<14}", record.phase.to_string());
                let mut line = format!("      {}  {} {}", record.started_at, phase_name, duration_str);
                if !record.stats.is_empty() {
                    let stats_str: Vec<String> = record.stats.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
                    line.push_str(&format!("  {}", stats_str.join(", ")));
                }
                if is_current {
                    line.push_str("  <- now");
                }
                lines.push(line);
            }
        }
    }

    let _ = now_unix_s;
    lines.join("\n")
}

/// Map an [`IndexVolumeKind`] to its resource token.
fn kind_token(kind: crate::indexing::IndexVolumeKind) -> &'static str {
    use crate::indexing::IndexVolumeKind;
    match kind {
        IndexVolumeKind::Local => "local",
        IndexVolumeKind::Smb => "smb",
        IndexVolumeKind::Mtp => "mtp",
    }
}

/// Collect one volume's snapshot from the indexing read APIs. `with_debug` fills
/// the deep-view detail. Freshness / scan facts come from
/// `get_volume_index_status`; phase, counts, and DB internals from
/// `get_debug_status`.
fn collect_volume_snapshot(volume_id: &str, with_debug: bool) -> VolumeIndexingSnapshot {
    let status = crate::indexing::get_volume_index_status(volume_id);
    let debug_status = crate::indexing::get_debug_status(volume_id).ok();
    let kind = crate::indexing::volume_kind(volume_id).map(kind_token);

    let (
        activity_phase,
        phase_duration_ms,
        scanning,
        entries_scanned,
        dirs_found,
        bytes_scanned,
        volume_used_bytes,
        db_entry_count,
        db_dir_count,
        db_file_size,
    ) = match &debug_status {
        Some(d) => (
            d.activity_phase.clone(),
            d.phase_duration_ms,
            d.base.scanning,
            d.base.entries_scanned,
            d.base.dirs_found,
            d.base.bytes_scanned,
            d.base.volume_used_bytes,
            d.live_entry_count,
            d.live_dir_count,
            d.base.db_file_size,
        ),
        None => (ActivityPhase::Idle, 0, false, 0, 0, 0, None, None, None, None),
    };

    let debug = if with_debug {
        debug_status.as_ref().map(|d| VolumeIndexingDebug {
            watcher_active: d.watcher_active,
            live_event_count: d.live_event_count,
            must_scan_count: d.must_scan_count,
            must_scan_rescans_completed: d.must_scan_rescans_completed,
            verifying: d.verifying,
            db_main_size: d.db_main_size,
            db_wal_size: d.db_wal_size,
            db_page_count: d.db_page_count,
            db_freelist_count: d.db_freelist_count,
            phase_history: d.phase_history.clone(),
        })
    } else {
        None
    };

    VolumeIndexingSnapshot {
        volume_id: volume_id.to_string(),
        kind,
        enabled: status.enabled,
        freshness: status.freshness,
        activity_phase,
        phase_duration_ms,
        scanning,
        entries_scanned,
        dirs_found,
        bytes_scanned,
        volume_used_bytes,
        db_entry_count,
        db_dir_count,
        db_file_size,
        scan_completed_at: status.scan_completed_at,
        scan_duration_ms: status.scan_duration_ms,
        debug,
    }
}

/// The set of known (registered) volume ids, root first then the rest sorted, so
/// the resource order is stable.
fn known_volume_ids() -> Vec<String> {
    let mut ids = crate::indexing::all_registered_volume_ids();
    ids.sort();
    ids.sort_by_key(|id| id != crate::indexing::ROOT_VOLUME_ID);
    ids
}

/// Snapshot every known volume for the default view.
pub(crate) fn snapshot_indexing() -> Vec<VolumeIndexingSnapshot> {
    known_volume_ids()
        .iter()
        .map(|id| collect_volume_snapshot(id, false))
        .collect()
}

/// Snapshot one volume with debug detail for `?volume=<id>`, or `None` when the
/// volume isn't a known (registered) index — an honest "no index" beats showing
/// another volume's global phase timeline.
pub(crate) fn snapshot_volume_indexing(volume_id: &str) -> Option<VolumeIndexingSnapshot> {
    if !crate::indexing::all_registered_volume_ids()
        .iter()
        .any(|id| id == volume_id)
    {
        return None;
    }
    Some(collect_volume_snapshot(volume_id, true))
}

/// Current wall-clock time in Unix seconds, for the production builders.
pub(crate) fn now_unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
