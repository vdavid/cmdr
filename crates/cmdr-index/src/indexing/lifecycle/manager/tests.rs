//! Unit tests for the pure `get_status` helper.
//!
//! `IndexManager::get_status` itself needs a full manager (and thus an
//! `AppHandle`), which the module's testing bar keeps under integration
//! coverage. `live_scan_counters` is the snapshot-and-calibration combining
//! it delegates to; pinning that here exercises every field `get_status`
//! surfaces — live bytes from the scan snapshot and the tier-2 used-bytes
//! denominator from the stashed calibration — without an `AppHandle`.
use super::*;
use crate::indexing::scanner::ScanProgressSnapshot;

fn snapshot(entries: u64, dirs: u64, bytes: u64) -> ScanProgressSnapshot {
    ScanProgressSnapshot {
        entries_scanned: entries,
        dirs_found: dirs,
        bytes_scanned: bytes,
    }
}

fn calibration(used_bytes: Option<u64>) -> ScanCalibration {
    ScanCalibration {
        prior: crate::indexing::store::ScanCalibration::default(),
        volume_used_bytes: used_bytes,
        run_kind: ScanRunKind::FirstScan,
    }
}

#[test]
fn live_counters_reflect_snapshot_bytes_and_calibration_used_bytes() {
    // Mid-scan: an active snapshot plus a calibration carrying the tier-2
    // denominator. get_status must surface both, apples-to-apples with what
    // the 500 ms progress event emits.
    let counters = live_scan_counters(
        Some(snapshot(42_000, 1_200, 905_000_000)),
        Some(calibration(Some(746_000_000))),
    );
    assert_eq!(counters.entries_scanned, 42_000);
    assert_eq!(counters.dirs_found, 1_200);
    assert_eq!(counters.bytes_scanned, 905_000_000);
    assert_eq!(counters.volume_used_bytes, Some(746_000_000));
}

#[test]
fn live_counters_carry_the_running_scans_per_kind_calibration() {
    // The reload path must seed its ETA off the SAME per-kind bucket the live
    // path used. Falling back to the unsuffixed meta keys would hand a full
    // walk the ~5x slower change check's duration — the exact bug the split
    // exists to kill, reintroduced by a window reload.
    let mut cal = calibration(Some(746_000_000));
    cal.run_kind = ScanRunKind::ChangeCheck;
    cal.prior = crate::indexing::store::ScanCalibration {
        total_entries: Some(5_100_000),
        total_physical_bytes: None,
        scan_duration_ms: Some(1_180_696),
    };
    let counters = live_scan_counters(Some(snapshot(42_000, 1_200, 905_000_000)), Some(cal));
    assert_eq!(counters.prior_total_entries, Some(5_100_000));
    assert_eq!(counters.prior_scan_duration_ms, Some(1_180_696));
}

#[test]
fn live_counters_carry_the_running_scans_kind() {
    // A mid-scan window reload misses `index-scan-started`, so `get_status` is
    // the only way back to the run-kind header. Without this the checklist
    // would silently drop it (or, worse, guess) for the rest of the run.
    let mut change_check = calibration(Some(746_000_000));
    change_check.run_kind = ScanRunKind::ChangeCheck;
    let counters = live_scan_counters(Some(snapshot(42_000, 1_200, 905_000_000)), Some(change_check));
    assert_eq!(counters.scan_run_kind, Some(ScanRunKind::ChangeCheck));
}

#[test]
fn live_counters_are_zero_with_no_active_scan() {
    // No scan handle and no calibration (the idle / between-scans state):
    // every live counter reads 0 and the tier-2 denominator is absent.
    let counters = live_scan_counters(None, None);
    assert_eq!(counters, LiveScanCounters::default());
    assert_eq!(counters.bytes_scanned, 0);
    assert_eq!(counters.volume_used_bytes, None);
}

#[test]
fn live_counters_omit_used_bytes_when_space_info_failed() {
    // First scan where the space-info fetch failed: a live snapshot exists,
    // but the tier-2 denominator is `None`, so the FE falls back to tier 1 /
    // counter-only. The live bytes still flow through.
    let counters = live_scan_counters(Some(snapshot(10, 3, 4_096)), Some(calibration(None)));
    assert_eq!(counters.bytes_scanned, 4_096);
    assert_eq!(counters.volume_used_bytes, None);
}

/// Regression anchor for the real-hardware "SMB Rescan indexes nothing" bug:
/// `force_rescan` routes by the TYPED volume kind, so an SMB/MTP rescan hits
/// the `Volume`-trait scanner — NOT the local guarded-walker `start_scan`, which ran
/// over the network mount, walked nothing, and falsely marked the index
/// complete. Pre-fix `force_scan` called `start_scan` unconditionally, so an
/// SMB id wrongly mapped to `LocalWalker`; this pins the correct mapping.
/// The reconcile-vs-truncate boundary: reconcile ONLY a previously-completed,
/// populated index. A sentinel-only DB (`entry_count == 1`, never scanned) takes
/// the FRESH/truncate guarded-walker rebuild. `> 1` not `> 0` — the latter would send a brand-new
/// user's first `/` scan down the serial reconcile (the onboarding bug). AND the
/// completeness gate: a populated-but-never-completed partial (`scan_completed_at`
/// absent) also takes the fast guarded-walker rebuild, because reconciling its
/// add-everything delta wedges the serial walk (the ~15-min "looks hung" bug on a
/// real `/`). The sentinel-makes-it-1 fact is verified against a fresh store below.
#[test]
fn local_rescan_reconciles_only_beyond_the_root_sentinel() {
    // Completeness gate: even a populated DB does NOT reconcile if the prior scan
    // never completed.
    assert!(!local_rescan_reconciles(0, true), "empty DB ⇒ fresh/truncate path");
    assert!(
        !local_rescan_reconciles(1, true),
        "sentinel-only DB (never scanned) ⇒ fresh/truncate path, NOT reconcile"
    );
    assert!(
        local_rescan_reconciles(2, true),
        "populated AND prior-completed ⇒ reconcile path"
    );
    assert!(
        !local_rescan_reconciles(2, false),
        "populated but never-completed partial ⇒ fast guarded-walker rebuild, NOT reconcile"
    );

    // A fresh store has exactly the ROOT sentinel, so its entry_count is 1 and
    // the predicate routes it to the fresh path — the onboarding guarantee.
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("sentinel.db");
    let store = IndexStore::open(&db_path).expect("open store");
    let count = IndexStore::get_entry_count(store.read_conn()).expect("count");
    assert_eq!(count, 1, "a fresh DB holds only the ROOT sentinel");
    assert!(
        !local_rescan_reconciles(count, true),
        "so a fresh DB takes the truncate path"
    );
}

#[test]
fn force_rescan_routes_smb_and_mtp_to_the_trait_scanner_not_the_local_walker() {
    assert_eq!(
        rescan_scanner_for_kind(IndexVolumeKind::Smb),
        RescanScanner::VolumeTrait,
        "an SMB rescan must walk the Volume trait from the share root, not walk the mount with the local guarded walker",
    );
    assert_eq!(
        rescan_scanner_for_kind(IndexVolumeKind::Mtp),
        RescanScanner::VolumeTrait,
        "an MTP rescan must walk the Volume trait, not the local guarded walker",
    );
    assert_eq!(
        rescan_scanner_for_kind(IndexVolumeKind::Local),
        RescanScanner::LocalWalker,
        "only a local disk uses the guarded-walker + FSEvents scanner",
    );
}

#[test]
fn journal_replay_is_gated_on_the_kind_having_a_journal_not_a_stored_event_id() {
    // Regression lock (plan Decision 2): the shared local event loop persists
    // `last_event_id` for ANY local-scanner volume, so a completed
    // `LocalExternal` index carries the SAME persisted state as the boot disk
    // (a stored event id + a completed scan) — yet it has no `.fseventsd`
    // journal to replay. Replay must gate on `has_event_journal()`, NOT on
    // `stored_event_id.is_some()`. A future collapse back to an id-based gate
    // routes `LocalExternal` into an empty/garbage replay and fails here.
    let completed = true;
    let id = Some(42);

    // The boot disk HAS a journal → replays.
    assert!(
        should_replay_journal(IndexVolumeKind::Local, true, completed, id),
        "the boot disk replays its FSEvents journal",
    );
    // A local external drive with the IDENTICAL persisted state has NO journal
    // → must NOT replay (this is the load-bearing assertion).
    assert!(
        !should_replay_journal(IndexVolumeKind::LocalExternal, true, completed, id),
        "a local external drive has no journal and must never replay",
    );

    // The other conditions still hold for a journaled volume: no platform
    // replay support (Linux), no completed scan, or no positive stored id all
    // route to a scan.
    assert!(
        !should_replay_journal(IndexVolumeKind::Local, false, completed, id),
        "no platform replay support ⇒ scan, not replay",
    );
    assert!(!should_replay_journal(IndexVolumeKind::Local, true, false, id));
    assert!(!should_replay_journal(IndexVolumeKind::Local, true, completed, None));
    assert!(!should_replay_journal(IndexVolumeKind::Local, true, completed, Some(0)));
}
