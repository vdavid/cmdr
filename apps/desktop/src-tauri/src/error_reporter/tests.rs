use super::*;
use std::collections::{BTreeMap, HashSet};
use zip::ZipArchive;

fn sample_manifest() -> BundleManifest {
    BundleManifest {
        id: "ERR-AB23X".to_string(),
        kind: BundleKind::User,
        app_version: "0.0.0-test".to_string(),
        os_version: "macOS test".to_string(),
        arch: "aarch64".to_string(),
        active_settings: ActiveSettings::default(),
        user_note: Some("This thing failed".to_string()),
        generated_at: "2026-04-23T10:00:00+00:00".to_string(),
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

#[test]
fn build_zip_contains_manifest_and_log_entries() {
    let mut files: BTreeMap<String, Vec<String>> = BTreeMap::new();
    files.insert(
        "cmdr.log".to_string(),
        vec!["redacted line 1".to_string(), "redacted line 2".to_string()],
    );
    files.insert(
        "cmdr.log.2025-01-01-00-00-00".to_string(),
        vec!["older line".to_string()],
    );

    let manifest = sample_manifest();
    let bytes = build_zip(&manifest, &files).unwrap();
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
    // Simulate the bundle-builder pipeline: redact each input line and then zip.
    let raw_lines = [
        "INFO  cmdr_lib::network  Mounted /Users/john/Documents/budget.pdf",
        "WARN  cmdr_lib::mtp  Failed to connect to john@host.local",
        "DEBUG smb2 SMB share at smb://john@nas.local/share/file.txt",
    ];
    let redacted: Vec<String> = raw_lines
        .iter()
        .map(|line| redact::redact_line(line).into_owned())
        .collect();

    let mut files = BTreeMap::new();
    files.insert("cmdr.log".to_string(), redacted);

    let bytes = build_zip(&sample_manifest(), &files).unwrap();
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
        let id = generate_short_id();
        // ~28.6 million possible IDs; collisions in 1k samples are vanishingly rare.
        assert!(seen.insert(id), "duplicate ID generated within 1000 samples");
    }
}

#[test]
fn zip_is_a_valid_zip_archive() {
    let mut files = BTreeMap::new();
    files.insert("cmdr.log".to_string(), vec!["line one".to_string()]);
    let bytes = build_zip(&sample_manifest(), &files).unwrap();
    let archive = ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    assert!(archive.len() >= 2, "expected at least manifest + one log entry");
}

#[test]
fn cap_bundle_is_no_op_when_under_cap() {
    let mut files = BTreeMap::new();
    files.insert("cmdr.log".to_string(), vec!["short".to_string()]);
    let bytes = build_zip(&sample_manifest(), &files).unwrap();
    let original_len = bytes.len();
    let original = bytes.clone();

    let capped = cap_bundle_to_mb(bytes, 10);
    assert_eq!(capped.len(), original_len);
    assert_eq!(capped, original);
}

#[test]
fn cap_bundle_clips_correctly_when_over_cap() {
    // Build a bundle with several megabytes of log content so capping has work to do.
    // We need lines that don't compress well, so we use varied pseudo-random tokens
    // per line — `x`-runs deflate to nothing.
    let mut files = BTreeMap::new();
    // Use a strongly-mixed pseudo-random token per line so deflate can't squash everything.
    // The mixer is splitmix64 — good avalanche, no shared structure between adjacent lines.
    fn splitmix64(mut x: u64) -> u64 {
        x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = x;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    for f in 0u32..10 {
        let mut lines = Vec::new();
        for i in 0u32..15_000 {
            let token: String = (0u32..30)
                .map(|k| {
                    let n = splitmix64(((f as u64) << 40) ^ ((i as u64) << 16) ^ k as u64);
                    char::from(33u8 + ((n & 0xFF) as u8 % 90))
                })
                .collect();
            lines.push(format!("INFO file={f} idx={i} body={token}"));
        }
        files.insert(format!("cmdr.log.{f:02}"), lines);
    }
    let bytes = build_zip(&sample_manifest(), &files).unwrap();
    assert!(
        bytes.len() > 1_500_000,
        "test setup needs a bigger original bundle (got {} bytes)",
        bytes.len()
    );

    let cap_mb = 1; // tight cap so we definitely trim
    let capped = cap_bundle_to_mb(bytes.clone(), cap_mb);

    assert!(
        capped.len() <= cap_mb * 1024 * 1024,
        "capped bundle is {} bytes, expected <= {} bytes",
        capped.len(),
        cap_mb * 1024 * 1024
    );
    assert!(
        capped.len() < bytes.len(),
        "capping should have shrunk the bundle ({} -> {})",
        bytes.len(),
        capped.len()
    );

    // Manifest is preserved.
    let entries = read_zip_entries(&capped);
    assert!(entries.contains_key("manifest.json"));
    // At least some `logs/*` entry survived (we want diagnostic content, not just metadata).
    assert!(entries.keys().any(|k| k.starts_with("logs/")));
}
