use super::bundle_builder::{
    PreparedFile, build_bundle_streaming, build_zip, load_and_filter_log_file, prepare_user_note, zip_dt,
};
use super::bundle_capper::cap_bundle_to_bytes;
use super::*;
use crate::redact;
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use zip::{DateTime as ZipDateTime, ZipArchive};

fn sample_manifest() -> BundleManifest {
    BundleManifest {
        id: "ERR-AB23X".to_string(),
        kind: BundleKind::User,
        build_mode: BuildMode::Release,
        app_version: "0.0.0-test".to_string(),
        os_version: "macOS test".to_string(),
        arch: "aarch64".to_string(),
        active_settings: ResolvedSettings {
            indexing_enabled: true,
            ai_provider: "local".to_string(),
            mcp_enabled: false,
            mcp_port: crate::mcp::config::DEFAULT_PORT,
            verbose_logging: false,
            max_log_storage_mb: 200,
            error_reports_enabled: false,
            crash_reports_enabled: false,
        },
        log_levels: LogLevelSnapshot {
            stdout_default: "info".to_string(),
            stdout_current: "info".to_string(),
            file_chain: "debug".to_string(),
            stdout_module_overrides: Vec::new(),
        },
        breadcrumbs: Vec::new(),
        user_note: Some("This thing failed".to_string()),
        generated_at: "2026-04-23T10:00:00+00:00".to_string(),
    }
}

/// Build a `PreparedFile` straight from an iterator of lines (strings) plus an mtime.
fn prepared(lines: Vec<&str>, mtime: SystemTime) -> PreparedFile {
    PreparedFile {
        lines: lines.into_iter().map(String::from).collect(),
        mtime,
    }
}

fn read_zip_entries(zip_bytes: &[u8]) -> BTreeMap<String, String> {
    let mut archive = ZipArchive::new(std::io::Cursor::new(zip_bytes)).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let name = entry.name().to_string();
        let mut body = String::new();
        entry.read_to_string(&mut body).unwrap();
        out.insert(name, body);
    }
    out
}

/// Reads each entry's stored mtime alongside its raw bytes.
fn read_zip_entries_with_mtime(zip_bytes: &[u8]) -> Vec<(String, ZipDateTime, Vec<u8>)> {
    let mut archive = ZipArchive::new(std::io::Cursor::new(zip_bytes)).unwrap();
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let name = entry.name().to_string();
        let mtime = entry.last_modified().unwrap_or_default();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        out.push((name, mtime, bytes));
    }
    out
}

#[test]
fn build_zip_contains_manifest_and_log_entries() {
    let now = SystemTime::now();
    let mut files: BTreeMap<String, PreparedFile> = BTreeMap::new();
    files.insert(
        "cmdr.log".to_string(),
        prepared(vec!["redacted line 1", "redacted line 2"], now),
    );
    files.insert(
        "cmdr.log.2025-01-01-00-00-00".to_string(),
        prepared(vec!["older line"], now - Duration::from_secs(3600)),
    );

    let manifest = sample_manifest();
    let bytes = build_zip(&manifest, &files, now).unwrap();
    let entries = read_zip_entries(&bytes);

    assert!(entries.contains_key("manifest.json"));
    assert!(entries.contains_key("logs/cmdr.log"));
    assert!(entries.contains_key("logs/cmdr.log.2025-01-01-00-00-00"));

    // Manifest is valid JSON and round-trips back to the same struct.
    let manifest_json = entries.get("manifest.json").unwrap();
    let parsed: BundleManifest = serde_json::from_str(manifest_json).unwrap();
    assert_eq!(parsed.id, manifest.id);
    assert_eq!(parsed.kind, BundleKind::User);
    assert_eq!(parsed.user_note.as_deref(), Some("This thing failed"));

    // Log lines are joined with `\n` and trail with one.
    let cmdr_log = entries.get("logs/cmdr.log").unwrap();
    assert_eq!(cmdr_log, "redacted line 1\nredacted line 2\n");
}

#[test]
fn redaction_is_applied_to_log_lines() {
    let now = SystemTime::now();
    // Simulate the bundle-builder pipeline: redact each input line and then zip.
    let raw_lines = [
        "INFO  cmdr_lib::network  Mounted /Users/john/Documents/budget.pdf",
        "WARN  cmdr_lib::mtp  Failed to connect to john@host.local",
        "DEBUG smb2 SMB share at smb://john@nas.local/share/file.txt",
    ];
    let redacted: Vec<&str> = raw_lines
        .iter()
        .map(|line| Box::leak(redact::redact_line(line).into_owned().into_boxed_str()) as &str)
        .collect();

    let mut files = BTreeMap::new();
    files.insert("cmdr.log".to_string(), prepared(redacted, now));

    let bytes = build_zip(&sample_manifest(), &files, now).unwrap();
    let entries = read_zip_entries(&bytes);
    let log_body = entries.get("logs/cmdr.log").unwrap();

    // No raw `/Users/<name>/` substrings should survive.
    assert!(
        !log_body.contains("/Users/john"),
        "redactor should have rewritten `/Users/john/...` (got: {log_body})"
    );
    // The path-shape token from the redactor should appear instead.
    assert!(
        log_body.contains("$HOME"),
        "expected `$HOME` token in redacted output (got: {log_body})"
    );
}

#[test]
fn prepare_user_note_redacts_paths_in_auto_notes() {
    // Regression: v0.21.0 auto-send bundles shipped `userNote` verbatim, which leaked
    // `/Users/<name>/...` from updater error messages. The redactor must scrub it.
    let salt = [0u8; 16];
    let raw = "auto-send: 1 error within 60s, first: FE:updater | Couldn't find .app bundle in path: /Users/jane/projects/cmdr/target/release/Cmdr";
    let redacted = prepare_user_note(raw, BundleKind::Auto, &salt).expect("non-empty note");
    assert!(
        !redacted.contains("/Users/jane"),
        "expected `/Users/jane` to be redacted (got: {redacted})"
    );
    assert!(
        redacted.contains("$HOME"),
        "expected `$HOME` token after redaction (got: {redacted})"
    );
    // The prefix that doesn't contain a path should pass through unchanged.
    assert!(
        redacted.starts_with("auto-send: 1 error within 60s, first: FE:updater"),
        "non-path prefix should survive (got: {redacted})"
    );
}

#[test]
fn prepare_user_note_leaves_user_notes_verbatim() {
    // User-typed notes are previewed in the dialog and shipped verbatim. We trust the
    // user to know what they're sharing; a path they typed in is a path they want sent.
    let salt = [0u8; 16];
    let raw = "I opened /Users/jane/Documents/budget.pdf and the app froze";
    let kept = prepare_user_note(raw, BundleKind::User, &salt).expect("non-empty note");
    assert_eq!(kept, raw);
}

#[test]
fn prepare_user_note_drops_empty_and_whitespace_for_both_kinds() {
    let salt = [0u8; 16];
    assert!(prepare_user_note("", BundleKind::Auto, &salt).is_none());
    assert!(prepare_user_note("   \t  ", BundleKind::Auto, &salt).is_none());
    assert!(prepare_user_note("", BundleKind::User, &salt).is_none());
    assert!(prepare_user_note("\n  \n", BundleKind::User, &salt).is_none());
}

#[test]
fn prepare_user_note_redacts_each_line_of_multiline_auto_note() {
    // Defensive: auto_dispatcher's format string is single-line, but `state.first_message`
    // is arbitrary; a multi-line message must still get redacted on every line.
    let salt = [0u8; 16];
    let raw = "auto-send: first error\nlocation: /Users/jane/projects/foo\ndetail: also /Users/jane/Documents/x.pdf";
    let redacted = prepare_user_note(raw, BundleKind::Auto, &salt).expect("non-empty note");
    assert!(
        !redacted.contains("/Users/jane"),
        "every line must be redacted (got: {redacted})"
    );
    // The split-on-newline preserves the line count.
    assert_eq!(redacted.matches('\n').count(), 2);
}

#[test]
fn build_mode_serializes_lowercase() {
    let release = serde_json::to_string(&BuildMode::Release).unwrap();
    assert_eq!(release, "\"release\"");
    let debug = serde_json::to_string(&BuildMode::Debug).unwrap();
    assert_eq!(debug, "\"debug\"");
}

#[test]
fn manifest_serializes_build_mode_with_camel_case_key() {
    let now = SystemTime::now();
    let manifest = sample_manifest();
    let bytes = build_zip(&manifest, &BTreeMap::new(), now).unwrap();
    let entries = read_zip_entries(&bytes);
    let manifest_json = entries.get("manifest.json").unwrap();
    assert!(
        manifest_json.contains("\"buildMode\": \"release\""),
        "expected `\"buildMode\": \"release\"` in manifest JSON, got: {manifest_json}",
    );

    // Debug build serializes to "debug".
    let mut debug_manifest = sample_manifest();
    debug_manifest.build_mode = BuildMode::Debug;
    let bytes = build_zip(&debug_manifest, &BTreeMap::new(), now).unwrap();
    let entries = read_zip_entries(&bytes);
    let manifest_json = entries.get("manifest.json").unwrap();
    assert!(
        manifest_json.contains("\"buildMode\": \"debug\""),
        "expected `\"buildMode\": \"debug\"` in manifest JSON, got: {manifest_json}",
    );
}

#[test]
fn build_mode_current_matches_cfg() {
    let expected = if cfg!(debug_assertions) {
        BuildMode::Debug
    } else {
        BuildMode::Release
    };
    assert_eq!(BuildMode::current(), expected);
}

#[test]
fn short_id_matches_expected_format() {
    let re = regex::Regex::new("^ERR-[23456789ABCDEFGHJKMNPQRSTUVWXYZ]{5}$").unwrap();
    for _ in 0..200 {
        let id = generate_short_id();
        assert!(re.is_match(&id), "ID `{id}` doesn't match the expected ERR-XXXXX shape");
    }
}

#[test]
fn short_id_is_statistically_unique() {
    let mut seen = HashSet::new();
    for _ in 0..1000 {
        seen.insert(generate_short_id());
    }
    // ID space is 31^5 ≈ 28.6 M. The birthday paradox predicts ~0.02 collisions
    // in 1000 samples on average, so insisting on zero collisions trips ~1.7% of
    // CI runs on a perfectly healthy RNG. Allow up to 10 collisions: that's a
    // multi-million-sigma cushion from real entropy, and a genuinely broken RNG
    // (say, outputs only ~100 distinct values) would produce hundreds.
    assert!(
        seen.len() >= 990,
        "expected at least 990 distinct IDs in 1000 samples, got {}",
        seen.len()
    );
}

#[test]
fn zip_is_a_valid_zip_archive() {
    let now = SystemTime::now();
    let mut files = BTreeMap::new();
    files.insert("cmdr.log".to_string(), prepared(vec!["line one"], now));
    let bytes = build_zip(&sample_manifest(), &files, now).unwrap();
    let archive = ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    assert!(archive.len() >= 2, "expected at least manifest + one log entry");
}

#[test]
fn cap_bundle_is_no_op_when_under_cap() {
    let now = SystemTime::now();
    let mut files = BTreeMap::new();
    files.insert("cmdr.log".to_string(), prepared(vec!["short"], now));
    let bytes = build_zip(&sample_manifest(), &files, now).unwrap();
    let original_len = bytes.len();
    let original = bytes.clone();

    let capped = cap_bundle_to_mb(bytes, 10);
    assert_eq!(capped.len(), original_len);
    assert_eq!(capped, original);
}

/// Generates pseudo-random text that doesn't deflate well, so a 30 MB input stays
/// 30 MB-ish in the zip too.
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn synthetic_lines(file_idx: u32, count: u32) -> Vec<String> {
    (0u32..count)
        .map(|i| {
            let token: String = (0u32..30)
                .map(|k| {
                    let n = splitmix64(((file_idx as u64) << 40) ^ ((i as u64) << 16) ^ k as u64);
                    char::from(33u8 + ((n & 0xFF) as u8 % 90))
                })
                .collect();
            format!("INFO file={file_idx} idx={i} body={token}")
        })
        .collect()
}

/// Headline test for fix #6: pre-fix, capping a 30 MB bundle to 1 MB produced a
/// 542-byte zip with only the manifest. Post-fix: capping trims old content from the
/// head of the newest file, ships the tail under (or just over) the cap, and proves
/// the LAST input line survives.
///
/// Uses the bytes-precision [`cap_bundle_to_bytes`] entry point (the prod API
/// [`cap_bundle_to_mb`] is a wrapper for the same logic at MB granularity). The
/// behavior under test is scale-invariant — pre-fix, the bug reproduced at any input
/// size as long as the cap engaged. A 100 KB cap with a ~250 KB input exercises the
/// same trim-from-head-keep-tail logic in ~50 ms instead of the ~2 s warm / >8 s
/// contended that 200 000 synthetic lines + 1 MB cap used to cost. See the test
/// commit for the previous shape if a future regression seems to need the bigger
/// fixture.
#[test]
fn cap_bundle_keeps_newest_lines_and_manifest() {
    let now = SystemTime::now();
    let mut files: BTreeMap<String, PreparedFile> = BTreeMap::new();
    // 4 000 synthetic high-entropy lines (~60 bytes each = ~240 KB raw,
    // deflates to ~130 KB at level 1 — comfortably over the 100 KB cap below).
    let lines = synthetic_lines(0, 4_000);
    let last_line = lines.last().cloned().expect("synthetic lines must exist");
    let lines_owned: Vec<String> = lines;
    files.insert(
        "cmdr.log".to_string(),
        PreparedFile {
            lines: lines_owned,
            mtime: now,
        },
    );

    let bytes = build_zip(&sample_manifest(), &files, now).unwrap();
    assert!(
        bytes.len() > 110_000,
        "test setup needs a bigger input zip than the cap (got {} bytes)",
        bytes.len()
    );

    let cap_bytes_value = 100 * 1024;
    let capped = cap_bundle_to_bytes(bytes.clone(), cap_bytes_value);

    // Tolerance: cap may exceed by ~10% to honor the minimum-tail floor on the newest
    // file. 1.1 × cap is the documented contract.
    let upper = cap_bytes_value * 11 / 10;
    assert!(
        capped.len() <= upper,
        "capped bundle is {} bytes, expected <= {} bytes (cap {} bytes + 10% headroom)",
        capped.len(),
        upper,
        cap_bytes_value,
    );
    assert!(
        capped.len() < bytes.len(),
        "capping should have shrunk the bundle ({} -> {})",
        bytes.len(),
        capped.len()
    );

    // Manifest is preserved verbatim.
    let entries = read_zip_entries(&capped);
    let manifest_json = entries.get("manifest.json").expect("manifest must survive capping");
    let parsed: BundleManifest = serde_json::from_str(manifest_json).unwrap();
    assert_eq!(parsed.id, "ERR-AB23X");

    // `logs/cmdr.log` is present and contains the LAST line of the input, proving we
    // trimmed from the head, not the tail.
    let cmdr_log = entries
        .get("logs/cmdr.log")
        .expect("the newest log entry must survive capping");
    assert!(
        cmdr_log.lines().any(|l| l == last_line),
        "the LAST input line must survive (got first/last few: {:?} ... {:?})",
        cmdr_log.lines().take(3).collect::<Vec<_>>(),
        cmdr_log.lines().rev().take(3).collect::<Vec<_>>(),
    );
    // ...and we dropped the FIRST line (otherwise we kept everything, which would
    // mean the cap didn't actually do anything).
    assert!(
        !cmdr_log.contains("idx=0 "),
        "expected the head to be trimmed; idx=0 still present"
    );
}

/// Newer files win the budget race over older files. Even with a tight cap, the
/// newest file gets its tail in; older files may be dropped entirely.
///
/// Like the headline test, scale-invariant: pre-fix the newest-wins logic was
/// broken at any size. Uses a 100 KB cap and ~3 000 lines per file to stay under
/// the 8 s nextest cap with headroom (was 50 000 × 2 files = 5 s warm / >8 s
/// contended).
#[test]
fn cap_bundle_prefers_newer_files() {
    let now = SystemTime::now();
    let older = now - Duration::from_secs(86_400);
    let mut files: BTreeMap<String, PreparedFile> = BTreeMap::new();
    files.insert(
        "cmdr.log".to_string(),
        PreparedFile {
            lines: synthetic_lines(0, 3_000),
            mtime: now,
        },
    );
    files.insert(
        "cmdr.log.1".to_string(),
        PreparedFile {
            lines: synthetic_lines(1, 3_000),
            mtime: older,
        },
    );
    let bytes = build_zip(&sample_manifest(), &files, now).unwrap();
    let capped = cap_bundle_to_bytes(bytes, 100 * 1024);
    let entries = read_zip_entries(&capped);

    assert!(
        entries.contains_key("logs/cmdr.log"),
        "newest file must survive (got entries: {:?})",
        entries.keys().collect::<Vec<_>>()
    );
    // Manifest always present.
    assert!(entries.contains_key("manifest.json"));
}

/// Fix #1: each entry must carry a real mtime, not the DOS epoch (1980-01-01).
#[test]
fn zip_entries_carry_real_mtimes() {
    let now = SystemTime::now();
    let log_mtime = now - Duration::from_secs(7200);
    let mut files: BTreeMap<String, PreparedFile> = BTreeMap::new();
    files.insert("cmdr.log".to_string(), prepared(vec!["line one"], log_mtime));

    let bytes = build_zip(&sample_manifest(), &files, now).unwrap();
    let entries = read_zip_entries_with_mtime(&bytes);

    let manifest_mtime = entries
        .iter()
        .find(|(name, _, _)| name == "manifest.json")
        .map(|(_, m, _)| *m)
        .expect("manifest entry");
    assert!(
        manifest_mtime.year() >= 2026,
        "manifest mtime fell back to the DOS epoch (year={})",
        manifest_mtime.year()
    );

    let log_entry_mtime = entries
        .iter()
        .find(|(name, _, _)| name == "logs/cmdr.log")
        .map(|(_, m, _)| *m)
        .expect("log entry");
    let expected = zip_dt(log_mtime);
    assert_eq!(
        log_entry_mtime.cmp(&expected),
        std::cmp::Ordering::Equal,
        "log entry mtime should match the prepared file's mtime (got {log_entry_mtime:?}, expected {expected:?})"
    );
}

/// Fix #4: log files older than 24h are filtered out of Flow A bundles.
#[test]
fn build_bundle_24h_filter_drops_old_files() {
    use std::fs;

    let dir = std::env::temp_dir().join(format!(
        "cmdr-error-reporter-24h-{}",
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    fs::create_dir_all(&dir).expect("temp dir");

    // Two files: one that looks fresh, one whose mtime we set to 48h ago via filetime.
    let fresh = dir.join("cmdr.log");
    fs::write(&fresh, "fresh line\n").expect("write fresh");
    let stale = dir.join("cmdr.log.1");
    fs::write(&stale, "stale line\n").expect("write stale");
    set_mtime_to_age(&stale, Duration::from_secs(48 * 3600));

    // Drive the file selection logic directly. `build_bundle` requires a Tauri app
    // handle; the per-file scope filter is exercised via `load_and_filter_log_file`.
    let now_utc = Utc::now();
    let now_system = SystemTime::now();
    let salt: [u8; 16] = [0u8; 16];
    // The legacy file-by-mtime filter was the Flow A path, but Flow A now uses the
    // streaming tail walker. Run this assertion against the legacy path via the
    // `Recent { window: 24h }` configuration, same behavior, easier-to-read intent.
    let scope = BundleScope::Recent {
        window: Duration::from_secs(24 * 3600),
    };
    let fresh_picked = load_and_filter_log_file(&fresh, scope, now_utc, now_system, &salt);
    let stale_picked = load_and_filter_log_file(&stale, scope, now_utc, now_system, &salt);

    let (fresh_lines, _) = fresh_picked.expect("fresh file always included");
    assert!(!fresh_lines.is_empty(), "fresh file lines should be kept");

    let (stale_lines, _) = stale_picked.expect("stale file should be reported, just empty");
    assert!(
        stale_lines.is_empty(),
        "stale (>24h) file lines should be filtered out, got {stale_lines:?}",
    );

    fs::remove_dir_all(&dir).ok();
}

/// Fix #5: Flow B's window-anchored scope keeps lines inside `[first - 30 min, now]`
/// and drops earlier ones when their leading ISO timestamp is parseable.
#[test]
fn build_bundle_window_scope_trims_old_lines() {
    use std::fs;

    let dir = std::env::temp_dir().join(format!(
        "cmdr-error-reporter-window-{}",
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    fs::create_dir_all(&dir).expect("temp dir");

    let now_utc = Utc::now();
    let first_error_at = now_utc - chrono::Duration::minutes(5);
    let lower_bound = first_error_at - chrono::Duration::minutes(30);
    let too_old = lower_bound - chrono::Duration::hours(1);
    let inside = first_error_at - chrono::Duration::minutes(10);

    let fmt = "%Y-%m-%dT%H:%M:%S%.3f%:z";
    let log_path = dir.join("cmdr.log");
    fs::write(
        &log_path,
        format!(
            "{old} OLD line that should be dropped\n{ok} NEW line that should be kept\n",
            old = too_old.with_timezone(&chrono::Local).format(fmt),
            ok = inside.with_timezone(&chrono::Local).format(fmt),
        ),
    )
    .expect("write log");

    let now_system = SystemTime::now();
    let salt: [u8; 16] = [0u8; 16];
    let (lines, _) = load_and_filter_log_file(
        &log_path,
        BundleScope::Window { first_error_at },
        now_utc,
        now_system,
        &salt,
    )
    .expect("file should be loaded");

    assert_eq!(lines.len(), 1, "expected exactly one line to survive: {lines:?}");
    assert!(
        lines[0].contains("NEW line that should be kept"),
        "kept the wrong line: {lines:?}"
    );

    fs::remove_dir_all(&dir).ok();
}

/// Sets the file's mtime to `now - age` via the `filetime` crate (already in our dep
/// tree via `file-rotate`). Cross-platform; tests stay dependency-free relative to
/// what the production crate already pulls in.
fn set_mtime_to_age(path: &Path, age: Duration) {
    let target = SystemTime::now()
        .checked_sub(age)
        .expect("system clock can produce a target mtime");
    let ft = filetime::FileTime::from_system_time(target);
    filetime::set_file_mtime(path, ft).expect("set_file_mtime");
}

/// Tests for `settings_defaults`: FE pushes the registry default map; the resolved
/// settings prefer it over the hardcoded fallback when the loader's `Option<_>`
/// fields are `None`.
mod settings_defaults_tests {
    use super::*;
    use crate::error_reporter::settings_defaults::{self, SettingValue};
    use crate::settings::loader::Settings;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Serialize tests in this module (`settings_defaults` is process-global state).
    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// With no FE defaults pushed, hardcoded fallbacks apply. This is the path that
    /// runs in unit tests and during the first few hundred ms after launch, before
    /// the FE has called `record_settings_defaults`.
    #[test]
    fn falls_back_to_hardcoded_when_no_defaults_recorded() {
        let _g = test_lock();
        settings_defaults::reset_for_test();

        let resolved = ResolvedSettings::from_settings(&Settings::default());
        assert!(
            resolved.indexing_enabled,
            "hardcoded default for indexing.enabled is true"
        );
        assert_eq!(resolved.ai_provider, "local");
        assert_eq!(resolved.mcp_port, crate::mcp::config::DEFAULT_PORT);
        assert_eq!(resolved.max_log_storage_mb, 200);
        assert!(!resolved.error_reports_enabled);
    }

    /// FE-pushed defaults override the hardcoded fallback, but the user's persisted
    /// value still wins over both. This is the load-bearing assertion: the registry
    /// is the source of truth for "what would the value be if the user hadn't
    /// touched it," and `from_settings` should honor that.
    #[test]
    fn fe_defaults_override_hardcoded_but_user_overrides_both() {
        let _g = test_lock();
        settings_defaults::reset_for_test();

        let mut map = HashMap::new();
        // FE registry says indexing is OFF by default (hypothetical; production says
        // true). If `from_settings` ever drifts back to the hardcoded fallback, this
        // test catches it.
        map.insert("indexing.enabled".to_string(), SettingValue::Bool(false));
        map.insert("developer.mcpPort".to_string(), SettingValue::Integer(12345));
        map.insert("ai.provider".to_string(), SettingValue::String("cloud".to_string()));
        settings_defaults::record(map);

        // Field with no user override → FE default applies.
        let mut settings = Settings::default();
        let resolved = ResolvedSettings::from_settings(&settings);
        assert!(
            !resolved.indexing_enabled,
            "FE default (false) overrides hardcoded (true)"
        );
        assert_eq!(resolved.mcp_port, 12345);
        assert_eq!(resolved.ai_provider, "cloud");

        // Field WITH user override → user value wins over FE default.
        settings.indexing_enabled = Some(true);
        let resolved = ResolvedSettings::from_settings(&settings);
        assert!(resolved.indexing_enabled, "user override beats FE default");

        settings_defaults::reset_for_test();
    }

    /// A garbage value in the FE map (wrong type for the field) falls through to
    /// the hardcoded fallback rather than panicking. Defensive: if FE registry ever
    /// ships a malformed default, manifests degrade rather than break.
    #[test]
    fn type_mismatch_falls_through_to_hardcoded() {
        let _g = test_lock();
        settings_defaults::reset_for_test();

        let mut map = HashMap::new();
        // Wrong types for these fields: String instead of Bool, Bool instead of Integer.
        map.insert(
            "indexing.enabled".to_string(),
            SettingValue::String("not a bool".to_string()),
        );
        map.insert("developer.mcpPort".to_string(), SettingValue::Bool(true));
        settings_defaults::record(map);

        let resolved = ResolvedSettings::from_settings(&Settings::default());
        assert!(resolved.indexing_enabled, "type mismatch → hardcoded fallback applies");
        assert_eq!(resolved.mcp_port, crate::mcp::config::DEFAULT_PORT);

        settings_defaults::reset_for_test();
    }
}

/// Tests for the Flow A streaming pipeline (`build_bundle_streaming`). These exercise
/// the tail-walker / streaming-zip / cap-early-termination pieces without going through
/// `build_bundle` (which needs a Tauri app handle for the manifest snapshot).
mod streaming_tests {
    use super::*;
    use std::fs;
    use std::io::Write as IoWrite;

    fn make_log_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cmdr-streaming-{name}-{}",
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn iso(ts: DateTime<Utc>) -> String {
        ts.with_timezone(&chrono::Local)
            .format("%Y-%m-%dT%H:%M:%S%.3f%:z")
            .to_string()
    }

    /// Streaming pipeline: full file fits in cap, tail-walker stops at the cutoff,
    /// only in-window lines survive. Manifest is preserved.
    #[test]
    fn streaming_keeps_in_window_lines() {
        let dir = make_log_dir("basic");
        let log = dir.join("cmdr.log");
        let now = Utc::now();

        let mut f = fs::File::create(&log).unwrap();
        writeln!(f, "{} INFO 2-hour-old line", iso(now - chrono::Duration::hours(2))).unwrap();
        writeln!(
            f,
            "{} INFO 30-min-old line (kept)",
            iso(now - chrono::Duration::minutes(30))
        )
        .unwrap();
        writeln!(
            f,
            "{} INFO 1-min-old line (kept)",
            iso(now - chrono::Duration::minutes(1))
        )
        .unwrap();
        drop(f);

        let cutoff = now - chrono::Duration::hours(1);
        let bundle = build_bundle_streaming(
            "ERR-TEST1".to_string(),
            sample_manifest(),
            vec![log.clone()],
            cutoff,
            SystemTime::now(),
            &[0u8; 16],
        )
        .expect("build_bundle_streaming");

        let entries = read_zip_entries(&bundle.zip_bytes);
        assert!(entries.contains_key("manifest.json"));
        let log_body = entries.get("logs/cmdr.log").expect("log entry present");
        assert!(!log_body.contains("2-hour-old"), "old line must be dropped: {log_body}");
        assert!(log_body.contains("30-min-old"));
        assert!(log_body.contains("1-min-old"));
        // Two kept lines.
        assert_eq!(bundle.total_redacted_lines, 2);
        // sample_first/last are populated.
        assert!(!bundle.sample_first.is_empty());
        assert!(!bundle.sample_last.is_empty());

        fs::remove_dir_all(&dir).ok();
    }

    /// Continuation lines (no leading timestamp) ride along with the previous
    /// timestamped line. Exercise via a panic-style block straddling the cutoff.
    #[test]
    fn streaming_keeps_panic_continuation_lines_intact() {
        let dir = make_log_dir("panic");
        let log = dir.join("cmdr.log");
        let now = Utc::now();
        let mut f = fs::File::create(&log).unwrap();
        writeln!(f, "{} INFO old1", iso(now - chrono::Duration::hours(2))).unwrap();
        writeln!(
            f,
            "{} INFO old2",
            iso(now - chrono::Duration::hours(1) - chrono::Duration::minutes(1))
        )
        .unwrap();
        writeln!(
            f,
            "{} ERROR something panicked",
            iso(now - chrono::Duration::minutes(30))
        )
        .unwrap();
        writeln!(f, "   stack: frame 0").unwrap();
        writeln!(f, "   stack: frame 1").unwrap();
        writeln!(f, "{} INFO recovered", iso(now - chrono::Duration::minutes(1))).unwrap();
        drop(f);

        let cutoff = now - chrono::Duration::hours(1);
        let bundle = build_bundle_streaming(
            "ERR-TEST2".to_string(),
            sample_manifest(),
            vec![log.clone()],
            cutoff,
            SystemTime::now(),
            &[0u8; 16],
        )
        .unwrap();

        let entries = read_zip_entries(&bundle.zip_bytes);
        let log_body = entries.get("logs/cmdr.log").expect("log entry");
        assert!(log_body.contains("something panicked"));
        assert!(log_body.contains("frame 0"));
        assert!(log_body.contains("frame 1"));
        assert!(log_body.contains("recovered"));
        assert!(!log_body.contains("old1"));
        assert!(!log_body.contains("old2"));

        fs::remove_dir_all(&dir).ok();
    }

    /// Cap stops streaming: a synthetic file forces the compressed cap before the
    /// timestamp boundary; the bundle is well-formed and contains some content.
    /// (The test asserts on byte ceiling and zip validity rather than line-by-line
    /// content, since which lines survive depends on the deflate buffer's flush timing.)
    #[test]
    fn streaming_stops_at_cap() {
        let dir = make_log_dir("cap");
        let log = dir.join("cmdr.log");
        let now = Utc::now();
        let mut f = fs::File::create(&log).unwrap();
        // 50 000 pseudo-random lines, all in-window, ~250 bytes each (incl. timestamp)
        // ≈ 12 MB raw. Pseudo-random body deflates poorly so the 1 MB cap should
        // trigger early termination in the streaming pipeline.
        let line_count = 50_000u32;
        for i in 0..line_count {
            let ts = iso(now - chrono::Duration::seconds((line_count - i) as i64));
            let token: String = (0..200)
                .map(|k| {
                    let n = (i as u64).wrapping_mul(2_654_435_761).wrapping_add(k as u64);
                    char::from(33u8 + ((n & 0xFF) as u8 % 90))
                })
                .collect();
            writeln!(f, "{ts} INFO body={token}").unwrap();
        }
        drop(f);

        let cutoff = now - chrono::Duration::hours(2);
        let bundle = build_bundle_streaming(
            "ERR-CAP".to_string(),
            sample_manifest(),
            vec![log.clone()],
            cutoff,
            SystemTime::now(),
            &[0u8; 16],
        )
        .unwrap();

        // Cap is 1 MB. Allow some overshoot for the deflater's flush buffer + the
        // central directory; ~1.5 MB is a comfortable upper bound.
        let cap_bytes = FLOW_A_BUNDLE_CAP_MB * 1024 * 1024;
        assert!(
            bundle.zip_bytes.len() <= cap_bytes + 512 * 1024,
            "expected zip <= cap + 512KB headroom; got {}",
            bundle.zip_bytes.len()
        );
        // And the bundle is a valid zip with manifest + at least one log entry.
        let entries = read_zip_entries(&bundle.zip_bytes);
        assert!(entries.contains_key("manifest.json"));
        assert!(entries.keys().any(|k| k.starts_with("logs/")));
        // Streaming must have terminated before consuming everything.
        assert!(
            (bundle.total_redacted_lines as u32) < line_count,
            "expected early termination; consumed all {} lines",
            bundle.total_redacted_lines,
        );

        fs::remove_dir_all(&dir).ok();
    }

    /// Empty + nonexistent files are handled cleanly.
    #[test]
    fn streaming_handles_empty_and_missing_files() {
        let dir = make_log_dir("empty");
        let empty = dir.join("cmdr.log");
        fs::write(&empty, b"").unwrap();
        let missing = dir.join("does-not-exist.log");

        let bundle = build_bundle_streaming(
            "ERR-EMPTY".to_string(),
            sample_manifest(),
            vec![missing, empty],
            Utc::now() - chrono::Duration::hours(1),
            SystemTime::now(),
            &[0u8; 16],
        )
        .unwrap();
        let entries = read_zip_entries(&bundle.zip_bytes);
        // Manifest is always present.
        assert!(entries.contains_key("manifest.json"));
        // No log entry for an empty file.
        assert!(!entries.keys().any(|k| k.starts_with("logs/")));
        fs::remove_dir_all(&dir).ok();
    }

    /// Performance smoke test: a single ~80 MB file (4× the cmdr.log rotation size on
    /// real machines fully populates a typical log dir) must build a bundle in well
    /// under a second. Marked `#[ignore]` so CI doesn't hold the wall-clock budget;
    /// run manually with `cargo nextest run streaming_perf -- --ignored`.
    ///
    /// The pre-streaming pipeline took 30+ seconds on this kind of input on a dev
    /// machine; the streaming path lands in 50–200 ms.
    #[test]
    #[ignore = "perf benchmark; run with --ignored"]
    fn streaming_perf_under_one_second_on_big_file() {
        let dir = make_log_dir("perf");
        let log = dir.join("cmdr.log");
        let now = Utc::now();

        // Build an ~80 MB file: 99 % of lines are 2 hours old (well outside the 1 hour
        // window), 1 % are recent. The streaming walker should bail almost immediately
        // when it hits the cutoff at the front edge of the recent section.
        let mut f = fs::File::create(&log).unwrap();
        for i in 0..400_000u32 {
            let ts_off_secs = if i < 396_000 {
                7200 + (400_000 - i) as i64 // old
            } else {
                ((400_000 - i) as i64).max(1) // recent
            };
            let ts = iso(now - chrono::Duration::seconds(ts_off_secs));
            // Pad each line out to ~200 bytes to hit ~80 MB total.
            writeln!(
                f,
                "{ts} INFO line index={i} body=xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
            )
            .unwrap();
        }
        drop(f);

        let cutoff = now - chrono::Duration::hours(1);
        let start = std::time::Instant::now();
        let bundle = build_bundle_streaming(
            "ERR-PERF".to_string(),
            sample_manifest(),
            vec![log.clone()],
            cutoff,
            SystemTime::now(),
            &[0u8; 16],
        )
        .unwrap();
        let elapsed = start.elapsed();

        // Clippy denies `print_stderr` crate-wide; `log::info!` is the clippy-clean
        // way to surface a perf result. Run with `cargo nextest run --nocapture
        // streaming_perf` and `RUST_LOG=cmdr_lib::error_reporter::perf=info` to see
        // the timing.
        log::info!(
            target: "cmdr_lib::error_reporter::perf",
            "streaming_perf: built {}-byte bundle in {elapsed:?}",
            bundle.zip_bytes.len(),
        );
        assert!(
            elapsed < Duration::from_secs(1),
            "streaming pipeline should finish in <1s on an 80 MB log; took {elapsed:?}",
        );
        fs::remove_dir_all(&dir).ok();
    }
}
