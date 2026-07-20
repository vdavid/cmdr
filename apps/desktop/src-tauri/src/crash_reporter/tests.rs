use super::*;

#[test]
fn crash_report_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);

    let report = CrashReport {
        version: CRASH_FILE_VERSION,
        timestamp: "2026-03-22T10:00:00+00:00".to_string(),
        signal: Some("panic".to_string()),
        panic_message: Some("called `unwrap()` on Err value".to_string()),
        backtrace_frames: vec![
            "cmdr_lib::some_module::some_function".to_string(),
            "std::rt::lang_start".to_string(),
        ],
        thread_name: Some("main".to_string()),
        thread_count: 8,
        app_version: "0.8.2".to_string(),
        os_version: "macOS 15.3".to_string(),
        arch: "aarch64".to_string(),
        uptime_secs: 42.5,
        active_settings: ActiveSettings {
            indexing_enabled: Some(true),
            ai_provider: Some("openai".to_string()),
            mcp_enabled: Some(false),
            verbose_logging: None,
        },
        possible_crash_loop: false,
        build_mode: Some("release".to_string()),
        short_id: Some("CRASH-A2345".to_string()),
        diag_id: "diag_12345678-1234-1234-1234-1234567890ab".to_string(),
        email: Some("tester@example.com".to_string()),
        system_snapshot: Some(crate::diagnostics_snapshot::SystemSnapshot::collect_stable(dir.path())),
    };

    write_crash_report(&path, &report).unwrap();
    let loaded = read_crash_report(&path).unwrap();
    let snapshot = loaded
        .system_snapshot
        .as_ref()
        .expect("snapshot roundtrips inside the crash report");
    assert!(snapshot.live.is_none(), "crash-report snapshots are stable-only");

    assert_eq!(loaded.version, CRASH_FILE_VERSION);
    assert_eq!(loaded.timestamp, "2026-03-22T10:00:00+00:00");
    assert_eq!(loaded.signal.as_deref(), Some("panic"));
    assert_eq!(loaded.panic_message.as_deref(), Some("called `unwrap()` on Err value"));
    assert_eq!(loaded.backtrace_frames.len(), 2);
    assert_eq!(loaded.thread_name.as_deref(), Some("main"));
    assert_eq!(loaded.thread_count, 8);
    assert_eq!(loaded.app_version, "0.8.2");
    assert_eq!(loaded.os_version, "macOS 15.3");
    assert_eq!(loaded.arch, "aarch64");
    assert!((loaded.uptime_secs - 42.5).abs() < f64::EPSILON);
    assert_eq!(loaded.active_settings.indexing_enabled, Some(true));
    assert_eq!(loaded.active_settings.ai_provider.as_deref(), Some("openai"));
    assert_eq!(loaded.active_settings.mcp_enabled, Some(false));
    assert_eq!(loaded.active_settings.verbose_logging, None);
    assert!(!loaded.possible_crash_loop);
    assert_eq!(loaded.build_mode.as_deref(), Some("release"));
    assert_eq!(loaded.short_id.as_deref(), Some("CRASH-A2345"));
    assert_eq!(loaded.diag_id, "diag_12345678-1234-1234-1234-1234567890ab");
    assert_eq!(loaded.email.as_deref(), Some("tester@example.com"));
}

/// A crash report serializes the `diag_` id under `diagId` and the optional `email`
/// under `email` (camelCase), and never carries an `anal_` analytics id anywhere.
#[test]
fn crash_report_serializes_diag_id_and_email_camelcase_never_anal() {
    let report = make_test_report();
    let json = serde_json::to_string(&report).unwrap();
    assert!(json.contains("\"diagId\""), "missing diagId key: {json}");
    assert!(json.contains("\"email\""), "missing email key: {json}");
    assert!(report.diag_id.starts_with("diag_"), "diag id must use diag_ prefix");
    // The unjoinability invariant: the analytics id must never appear on a crash report.
    assert!(
        !json.contains("anal_"),
        "crash report must never carry an anal_ id: {json}"
    );
}

/// Old crash files written before the diag/email columns existed still parse: `diag_id`
/// defaults to empty and `email` to `None`.
#[test]
fn crash_report_without_diag_or_email_parses() {
    let json = r#"{
        "version": 1,
        "timestamp": "2026-03-22T10:00:00+00:00",
        "signal": "panic",
        "panicMessage": null,
        "backtraceFrames": [],
        "threadName": null,
        "threadCount": 0,
        "appVersion": "0.8.2",
        "osVersion": "macOS 15.3",
        "arch": "aarch64",
        "uptimeSecs": 0.0,
        "activeSettings": {}
    }"#;
    let report: CrashReport = serde_json::from_str(json).unwrap();
    assert_eq!(report.diag_id, "");
    assert_eq!(report.email, None);
}

#[test]
fn corrupt_crash_file_returns_none_and_deletes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);

    // Write garbage
    std::fs::write(&path, "not json at all {{{").unwrap();
    assert!(path.exists());

    let result = read_crash_report(&path);
    assert!(result.is_none());
    assert!(!path.exists(), "corrupt file should be deleted");
}

#[test]
fn truncated_crash_file_returns_none_and_deletes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);

    // Write valid JSON that's missing required fields
    std::fs::write(&path, r#"{"version": 1, "timestamp": "2026-01-01"}"#).unwrap();
    assert!(path.exists());

    let result = read_crash_report(&path);
    assert!(result.is_none());
    assert!(!path.exists(), "truncated file should be deleted");
}

#[test]
fn wrong_version_crash_file_returns_none_and_deletes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);

    let mut report = make_test_report();
    report.version = 99;
    let json = serde_json::to_string(&report).unwrap();
    std::fs::write(&path, json).unwrap();

    let result = read_crash_report(&path);
    assert!(result.is_none());
    assert!(!path.exists());
}

#[test]
fn empty_crash_file_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);

    std::fs::write(&path, "").unwrap();
    let result = read_crash_report(&path);
    assert!(result.is_none());
}

#[test]
fn nonexistent_crash_file_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("does-not-exist.json");
    let result = read_crash_report(&path);
    assert!(result.is_none());
}

// Sanitization is delegated to `crate::redact` (see `redact/tests.rs` for full pattern
// coverage). These tests verify the wrapper still strips the same PII the old sanitizer
// did, just with the new path-shape-preserving output.

#[test]
fn sanitize_unix_home_path() {
    let msg = r#"No such file or directory (os error 2): /Users/john/Documents/secret-project/file.txt"#;
    let sanitized = sanitize_panic_message(msg);
    assert!(!sanitized.contains("/Users/john"));
    assert!(!sanitized.contains("secret-project"));
    assert!(sanitized.contains("$HOME"));
}

#[test]
fn sanitize_linux_home_path() {
    let msg = "failed to open /home/alice/.ssh/id_rsa: permission denied";
    let sanitized = sanitize_panic_message(msg);
    assert!(!sanitized.contains("/home/alice"));
    assert!(!sanitized.contains("id_rsa"));
    assert!(sanitized.contains("$HOME"));
}

#[test]
fn sanitize_windows_path() {
    let msg = r"couldn't read C:\Users\Bob\Desktop\passwords.txt";
    let sanitized = sanitize_panic_message(msg);
    assert!(!sanitized.contains(r"C:\Users\Bob"));
    assert!(sanitized.contains("$HOME"));
}

#[test]
fn sanitize_tmp_path() {
    let msg = "error at /tmp/build-abc123/src/main.rs:42:5";
    let sanitized = sanitize_panic_message(msg);
    assert!(!sanitized.contains("/tmp/build-abc123"));
    // Path-shape preservation keeps the `/tmp/` prefix and the file extension.
    assert!(sanitized.contains("/tmp/"));
    assert!(sanitized.contains("<file>.rs"));
}

#[test]
fn sanitize_preserves_non_path_content() {
    let msg = "called `Option::unwrap()` on a `None` value";
    let sanitized = sanitize_panic_message(msg);
    assert_eq!(sanitized, msg);
}

#[test]
fn sanitize_multiple_paths() {
    let msg = "copy /Users/a/src to /Users/b/dst failed";
    let sanitized = sanitize_panic_message(msg);
    assert!(!sanitized.contains("/Users/a"));
    assert!(!sanitized.contains("/Users/b"));
    // Two paths → two $HOME replacements.
    assert_eq!(sanitized.matches("$HOME").count(), 2);
}

#[test]
fn crash_loop_detection_recent_timestamp() {
    // A timestamp from "just now" should be detected as a crash loop
    let recent = chrono::Utc::now().to_rfc3339();
    assert!(is_crash_loop(&recent));
}

#[test]
fn crash_loop_detection_old_timestamp() {
    let old = (chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
    assert!(!is_crash_loop(&old));
}

#[test]
fn crash_loop_detection_invalid_timestamp() {
    assert!(!is_crash_loop("not-a-timestamp"));
}

#[test]
fn parse_backtrace_frames_extracts_function_names() {
    let backtrace = "   0: std::backtrace::Backtrace::create\n\
                       at /rustc/abc123/library/std/src/backtrace.rs:100\n\
                       1: cmdr_lib::crash_reporter::build_panic_report\n\
                       at src/crash_reporter/mod.rs:50\n\
                       2: std::panicking::rust_panic_with_hook\n";
    let frames = parse_backtrace_frames(backtrace);
    assert_eq!(frames.len(), 3);
    assert_eq!(frames[0], "std::backtrace::Backtrace::create");
    assert_eq!(frames[1], "cmdr_lib::crash_reporter::build_panic_report");
    assert_eq!(frames[2], "std::panicking::rust_panic_with_hook");
}

#[cfg(unix)]
mod signal_tests {
    use super::super::signal_handler;
    use std::io::Write as _;

    #[test]
    fn raw_crash_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash-report.raw");

        // Manually write a raw crash file in the expected format
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"CMCR").unwrap(); // magic
        f.write_all(&1u32.to_le_bytes()).unwrap(); // version
        f.write_all(&11i32.to_le_bytes()).unwrap(); // signal (SIGSEGV)
        let addresses: Vec<u64> = vec![0x1000, 0x2000, 0x3000];
        f.write_all(&(addresses.len() as u32).to_le_bytes()).unwrap(); // frame count
        for addr in &addresses {
            f.write_all(&addr.to_le_bytes()).unwrap();
        }
        // App version (32 bytes, zero-padded)
        let mut version_buf = [0u8; 32];
        let version = b"0.8.2";
        version_buf[..version.len()].copy_from_slice(version);
        f.write_all(&version_buf).unwrap();
        drop(f);

        let (signal, addrs, ver) = signal_handler::read_raw_crash(&path).unwrap();
        assert_eq!(signal, 11);
        assert_eq!(addrs, vec![0x1000, 0x2000, 0x3000]);
        assert_eq!(ver, "0.8.2");
    }

    #[test]
    fn raw_crash_bad_magic_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash-report.raw");

        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"BAAD").unwrap();
        f.write_all(&[0u8; 44]).unwrap(); // fill to minimum size
        drop(f);

        assert!(signal_handler::read_raw_crash(&path).is_none());
        assert!(!path.exists(), "bad magic file should be deleted");
    }

    #[test]
    fn raw_crash_too_small_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash-report.raw");

        std::fs::write(&path, b"tiny").unwrap();
        assert!(signal_handler::read_raw_crash(&path).is_none());
        assert!(!path.exists());
    }

    #[test]
    fn raw_crash_truncated_frames_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash-report.raw");

        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"CMCR").unwrap();
        f.write_all(&1u32.to_le_bytes()).unwrap();
        f.write_all(&11i32.to_le_bytes()).unwrap();
        f.write_all(&100u32.to_le_bytes()).unwrap(); // claims 100 frames
        f.write_all(&[0u8; 32]).unwrap(); // but only has version field, no frame data
        drop(f);

        assert!(signal_handler::read_raw_crash(&path).is_none());
        assert!(!path.exists());
    }
}

#[test]
fn integration_panic_child_creates_crash_file() {
    let dir = tempfile::tempdir().unwrap();
    let crash_path = dir.path().join(CRASH_FILE_NAME);

    // We can't easily test the full panic hook (it requires a Tauri app handle),
    // but we can test the core write path: build a report and write it.
    let report = make_test_report();
    write_crash_report(&crash_path, &report).unwrap();

    assert!(crash_path.exists());
    let loaded = read_crash_report(&crash_path).unwrap();
    assert_eq!(loaded.app_version, env!("CARGO_PKG_VERSION"));
}

fn make_test_report() -> CrashReport {
    CrashReport {
        version: CRASH_FILE_VERSION,
        timestamp: now_iso8601(),
        signal: Some("panic".to_string()),
        panic_message: Some("test panic".to_string()),
        backtrace_frames: vec!["test::frame".to_string()],
        thread_name: Some("main".to_string()),
        thread_count: 1,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os_version: "test".to_string(),
        arch: "test".to_string(),
        uptime_secs: 0.0,
        active_settings: ActiveSettings::default(),
        possible_crash_loop: false,
        build_mode: Some("debug".to_string()),
        short_id: Some(crate::short_id::generate(CRASH_SHORT_ID_PREFIX)),
        diag_id: "diag_00000000-0000-4000-8000-000000000000".to_string(),
        email: None,
        system_snapshot: None,
    }
}

#[test]
fn sanitize_caps_a_runaway_panic_message() {
    // `assert_eq!` on big structs produces multi-KB payloads. The whole report body is
    // capped at 64 KB by the ingestion endpoint, so an uncapped message would take the
    // entire report down with a 400 instead of just losing its own tail.
    let msg = "x".repeat(PANIC_MESSAGE_MAX_CHARS * 3);
    let sanitized = sanitize_panic_message(&msg);
    assert!(
        sanitized.chars().count() <= PANIC_MESSAGE_MAX_CHARS + PANIC_MESSAGE_TRUNCATION_MARKER.chars().count(),
        "capped message was {} chars",
        sanitized.chars().count()
    );
    assert!(sanitized.ends_with(PANIC_MESSAGE_TRUNCATION_MARKER));
}

#[test]
fn sanitize_caps_on_a_char_boundary() {
    // Truncating by byte index inside a multi-byte char panics inside the panic hook,
    // which would abort with no report at all.
    let msg = "é".repeat(PANIC_MESSAGE_MAX_CHARS * 2);
    let sanitized = sanitize_panic_message(&msg);
    assert!(sanitized.starts_with('é'));
    assert!(sanitized.ends_with(PANIC_MESSAGE_TRUNCATION_MARKER));
}

#[test]
fn sanitize_leaves_a_short_message_unmarked() {
    let sanitized = sanitize_panic_message("index out of bounds: the len is 3 but the index is 7");
    assert!(!sanitized.ends_with(PANIC_MESSAGE_TRUNCATION_MARKER));
}
