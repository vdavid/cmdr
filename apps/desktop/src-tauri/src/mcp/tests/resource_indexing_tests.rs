//! Tests for the `cmdr://indexing` builder: pure formatting helpers plus the
//! per-volume text builders over injected snapshots (no live index needed).

use crate::indexing::ActivityPhase;
use crate::indexing::freshness::Freshness;
use crate::mcp::resources::indexing::{
    VolumeIndexingDebug, VolumeIndexingSnapshot, build_indexing_text, build_volume_debug_text, format_duration_human,
    format_number, freshness_token,
};

#[test]
fn test_format_duration_human() {
    assert_eq!(format_duration_human(0), "0ms");
    assert_eq!(format_duration_human(500), "500ms");
    assert_eq!(format_duration_human(1_000), "1.0s");
    assert_eq!(format_duration_human(47_100), "47.1s");
    assert_eq!(format_duration_human(60_000), "1m");
    assert_eq!(format_duration_human(252_000), "4m 12s");
    assert_eq!(format_duration_human(3_600_000), "1h 00m");
    assert_eq!(format_duration_human(3_723_000), "1h 02m");
}

#[test]
fn test_format_number() {
    assert_eq!(format_number(0), "0");
    assert_eq!(format_number(999), "999");
    assert_eq!(format_number(1_000), "1,000");
    assert_eq!(format_number(142_301), "142,301");
    assert_eq!(format_number(1_000_000), "1,000,000");
}

#[test]
fn test_freshness_token_mapping() {
    assert_eq!(freshness_token(Some(Freshness::Fresh)), "fresh");
    assert_eq!(freshness_token(Some(Freshness::Scanning)), "scanning");
    assert_eq!(freshness_token(Some(Freshness::Stale)), "stale");
    assert_eq!(freshness_token(None), "off");
}

/// A registered volume with sensible defaults; individual tests override fields.
fn base_snapshot(volume_id: &str, kind: &'static str) -> VolumeIndexingSnapshot {
    VolumeIndexingSnapshot {
        volume_id: volume_id.to_string(),
        kind: Some(kind),
        enabled: true,
        freshness: Some(Freshness::Fresh),
        activity_phase: ActivityPhase::Live,
        phase_duration_ms: 0,
        scanning: false,
        entries_scanned: 0,
        dirs_found: 0,
        bytes_scanned: 0,
        volume_used_bytes: None,
        db_entry_count: Some(100_000),
        db_dir_count: Some(8_000),
        db_file_size: Some(47_400_000),
        scan_completed_at: None,
        scan_duration_ms: None,
        debug: None,
    }
}

#[test]
fn test_empty_state() {
    let text = build_indexing_text(&[], 1_000);
    // Explains this resource is registered-indexes-only and points at cmdr://state.
    assert!(text.contains("No volumes have a registered index"), "got: {text}");
    assert!(text.contains("cmdr://state"), "got: {text}");
}

#[test]
fn test_default_view_fresh_volume() {
    let mut smb = base_snapshot("smb-nas", "smb");
    // Completed 5 minutes (300 s) before "now", took 4m 12s.
    smb.scan_completed_at = Some(1_000_000 - 300);
    smb.scan_duration_ms = Some(252_000);

    let text = build_indexing_text(std::slice::from_ref(&smb), 1_000_000);
    assert!(
        text.starts_with("Indexing status for 1 registered index ("),
        "got: {text}"
    );
    assert!(text.contains("smb-nas (smb):"), "got: {text}");
    assert!(text.contains("status: fresh"), "got: {text}");
    assert!(text.contains("phase: Live"), "got: {text}");
    assert!(text.contains("db: 100,000 entries, 8,000 dirs"), "got: {text}");
    assert!(text.contains("last scan: 5m ago (took 4m 12s)"), "got: {text}");
    // No scan-progress line or checklist for a fresh volume.
    assert!(!text.contains("scan progress:"), "got: {text}");
    assert!(!text.contains("steps:"), "got: {text}");
}

#[test]
fn test_default_view_scanning_volume_has_progress_and_checklist() {
    let mut root = base_snapshot("root", "local");
    root.freshness = Some(Freshness::Scanning);
    root.activity_phase = ActivityPhase::Scanning;
    root.phase_duration_ms = 10_000; // 10 s elapsed
    root.scanning = true;
    root.entries_scanned = 12_345;
    root.dirs_found = 1_234;
    root.bytes_scanned = 5_000_000; // rate 500 KB/s over 10 s
    root.volume_used_bytes = Some(10_000_000); // 50% done → ETA ~10 s
    root.scan_completed_at = None;

    let text = build_indexing_text(std::slice::from_ref(&root), 1_000_000);
    assert!(text.contains("root (local):"), "got: {text}");
    assert!(text.contains("status: scanning"), "got: {text}");
    assert!(
        text.contains("scan progress: 12,345 entries, 1,234 dirs"),
        "got: {text}"
    );
    assert!(text.contains("50% of"), "got: {text}");
    assert!(text.contains("ETA"), "got: {text}");
    // Checklist: Scan is current, later steps pending.
    assert!(text.contains("steps:"), "got: {text}");
    assert!(text.contains("[~] Scan"), "got: {text}");
    assert!(text.contains("[ ] Aggregate sizes"), "got: {text}");
    assert!(text.contains("[ ] Reconcile"), "got: {text}");
    assert!(text.contains("[ ] Go live"), "got: {text}");
    assert!(text.contains("last scan: none yet"), "got: {text}");
}

#[test]
fn test_checklist_marks_earlier_steps_done() {
    let mut root = base_snapshot("root", "local");
    root.freshness = Some(Freshness::Scanning);
    root.activity_phase = ActivityPhase::Reconciling;
    root.scanning = false; // aggregation/reconcile phase, scan counts no longer live

    let text = build_indexing_text(std::slice::from_ref(&root), 1_000_000);
    assert!(text.contains("[x] Scan"), "got: {text}");
    assert!(text.contains("[x] Aggregate sizes"), "got: {text}");
    assert!(text.contains("[~] Reconcile"), "got: {text}");
    assert!(text.contains("[ ] Go live"), "got: {text}");
    // Not scanning: no live scan-progress line even though it's in the Scanning
    // freshness state.
    assert!(!text.contains("scan progress:"), "got: {text}");
}

#[test]
fn test_off_volume_shows_only_status() {
    let mut vol = base_snapshot("smb-gone", "smb");
    vol.enabled = false;
    vol.freshness = None;

    let text = build_indexing_text(std::slice::from_ref(&vol), 1_000_000);
    assert!(text.contains("smb-gone (smb):"), "got: {text}");
    assert!(text.contains("status: off"), "got: {text}");
    assert!(!text.contains("phase:"), "got: {text}");
    assert!(!text.contains("db:"), "got: {text}");
}

#[test]
fn test_multi_volume_ordering_and_count() {
    let root = base_snapshot("root", "local");
    let smb = base_snapshot("smb-nas", "smb");
    let text = build_indexing_text(&[root, smb], 1_000_000);
    assert!(
        text.starts_with("Indexing status for 2 registered indexes ("),
        "got: {text}"
    );
    let root_pos = text.find("root (local):").expect("root block");
    let smb_pos = text.find("smb-nas (smb):").expect("smb block");
    assert!(root_pos < smb_pos, "root should render before smb: {text}");
}

#[test]
fn test_deep_view_includes_debug_detail() {
    let mut root = base_snapshot("root", "local");
    root.activity_phase = ActivityPhase::Live;
    root.debug = Some(VolumeIndexingDebug {
        watcher_active: true,
        live_event_count: 1_234,
        must_scan_count: 5,
        must_scan_rescans_completed: 4,
        verifying: false,
        db_main_size: Some(40_000_000),
        db_wal_size: Some(2_100_000),
        db_page_count: Some(12_000),
        db_freelist_count: Some(300),
        phase_history: vec![crate::indexing::PhaseRecord {
            phase: ActivityPhase::Scanning,
            started_at: "10:00:00.000".to_string(),
            duration_ms: Some(252_000),
            trigger: "app launch, 7,284 pending FSEvents".to_string(),
            stats: vec![("raw_events".to_string(), "7284".to_string())],
        }],
    });

    let text = build_volume_debug_text(&root, 1_000_000);
    assert!(text.contains("root (local):"), "got: {text}");
    assert!(text.contains("debug:"), "got: {text}");
    assert!(text.contains("watcher: on, 1,234 live events"), "got: {text}");
    assert!(text.contains("mustScan: 5 events, 4 rescans completed"), "got: {text}");
    assert!(text.contains("db detail: main "), "got: {text}");
    assert!(text.contains("WAL "), "got: {text}");
    assert!(
        text.contains("trigger: app launch, 7,284 pending FSEvents"),
        "got: {text}"
    );
    assert!(text.contains("history:"), "got: {text}");
    assert!(text.contains("raw_events=7284"), "got: {text}");
}

#[test]
fn test_deep_view_without_debug_is_just_summary() {
    // A snapshot with `debug: None` still renders its summary (defensive: the
    // deep path only fills debug when the volume is registered).
    let root = base_snapshot("root", "local");
    let text = build_volume_debug_text(&root, 1_000_000);
    assert!(text.contains("root (local):"), "got: {text}");
    assert!(!text.contains("debug:"), "got: {text}");
}
