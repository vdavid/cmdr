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
        app_fate: AppFate::Unconfirmed,
        reported_in_session: false,
        build_mode: Some("release".to_string()),
        short_id: Some("CRASH-A2345".to_string()),
        diag_id: "diag_12345678-1234-1234-1234-1234567890ab".to_string(),
        email: Some("tester@example.com".to_string()),
        system_snapshot: Some(crate::diagnostics_snapshot::SystemSnapshot::collect_stable(dir.path())),
        image_base: Some("0x104f2c000".to_string()),
    };

    write_crash_report(&path, &report).unwrap();
    let loaded = read_crash_report(&path).unwrap();
    let snapshot = loaded
        .system_snapshot
        .as_ref()
        .expect("snapshot roundtrips inside the crash report");
    assert!(snapshot.live.is_none(), "crash-report snapshots are stable-only");

    // Without the base, the absolute frame addresses can't be compared across
    // installs or resolved with atos (ASLR re-slides every launch).
    assert_eq!(loaded.image_base.as_deref(), Some("0x104f2c000"));
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
        f.write_all(&2u32.to_le_bytes()).unwrap(); // version
        f.write_all(&11i32.to_le_bytes()).unwrap(); // signal (SIGSEGV)
        let addresses: Vec<u64> = vec![0x1000, 0x2000, 0x3000];
        f.write_all(&(addresses.len() as u32).to_le_bytes()).unwrap(); // frame count
        f.write_all(&0x1_0000_0000u64.to_le_bytes()).unwrap(); // main-image load address
        for addr in &addresses {
            f.write_all(&addr.to_le_bytes()).unwrap();
        }
        // App version (32 bytes, zero-padded)
        let mut version_buf = [0u8; 32];
        let version = b"0.8.2";
        version_buf[..version.len()].copy_from_slice(version);
        f.write_all(&version_buf).unwrap();
        drop(f);

        let (signal, addrs, image_base, ver) = signal_handler::read_raw_crash(&path).unwrap();
        assert_eq!(signal, 11);
        assert_eq!(addrs, vec![0x1000, 0x2000, 0x3000]);
        // Without this the absolute addresses are meaningless off-machine (ASLR).
        assert_eq!(image_base, 0x1_0000_0000);
        assert_eq!(ver, "0.8.2");
    }

    #[test]
    fn raw_crash_bad_magic_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash-report.raw");

        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"BAAD").unwrap();
        f.write_all(&[0u8; 52]).unwrap(); // fill to minimum size
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
        f.write_all(&2u32.to_le_bytes()).unwrap();
        f.write_all(&11i32.to_le_bytes()).unwrap();
        f.write_all(&100u32.to_le_bytes()).unwrap(); // claims 100 frames
        f.write_all(&0u64.to_le_bytes()).unwrap(); // image base
        f.write_all(&[0u8; 32]).unwrap(); // but only has version field, no frame data
        drop(f);

        assert!(signal_handler::read_raw_crash(&path).is_none());
        assert!(!path.exists());
    }
}

/// Set on the child process this test re-launches, holding the dir its crash file goes in.
const PANIC_CHILD_DIR_ENV: &str = "CMDR_TEST_PANIC_HOOK_CHILD_DIR";

/// The end-to-end proof, and the one test that installs the REAL global hook. It has to run
/// in its own process: `set_hook` is process-wide, so doing it in-tree would hand every
/// other test's deliberate panic a crash-file write and a courier.
///
/// What the child proves, all at once: the hook installed by `install_panic_hook` is
/// reached by a panic on a background thread, the crash file lands, a courier is dispatched
/// for in-session delivery, and **the process is still alive afterwards** — the whole point
/// of the lock-poison policy this delivery path exists to serve.
#[test]
fn a_background_panic_writes_its_report_dispatches_a_courier_and_leaves_the_app_running() {
    const TEST_PATH: &str = concat!(
        "crash_reporter::tests::",
        "a_background_panic_writes_its_report_dispatches_a_courier_and_leaves_the_app_running"
    );

    if let Ok(dir) = std::env::var(PANIC_CHILD_DIR_ENV) {
        panic_hook_child(Path::new(&dir));
        return;
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let exe = std::env::current_exe().expect("current test binary");
    let output = std::process::Command::new(exe)
        .args(["--exact", TEST_PATH, "--nocapture", "--test-threads=1"])
        .env(PANIC_CHILD_DIR_ENV, dir.path())
        .output()
        .expect("re-launch the test binary");
    assert!(
        output.status.success(),
        "the child must survive the background panic and pass its own assertions.\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let report = read_crash_report(&dir.path().join(CRASH_FILE_NAME)).expect("the child wrote a crash report");
    assert_eq!(report.signal.as_deref(), Some("panic"));
    assert_eq!(report.thread_name.as_deref(), Some("test-background"));
    assert!(
        report
            .panic_message
            .as_deref()
            .is_some_and(|m| m.contains("a deliberate background panic")),
        "the report carries the panic message: {:?}",
        report.panic_message
    );
    assert!(!report.backtrace_frames.is_empty(), "the report carries a backtrace");
}

/// The child half of the test above. Runs in a process of its own.
fn panic_hook_child(dir: &Path) {
    CRASH_PATH
        .set(dir.join(CRASH_FILE_NAME))
        .expect("the child sets the crash path exactly once");
    install_panic_hook();

    let before = panic_courier::couriers_started_for_test();
    let panicked = std::thread::Builder::new()
        .name("test-background".to_string())
        .spawn(|| panic!("a deliberate background panic"))
        .expect("spawn the background thread")
        .join();
    assert!(
        panicked.is_err(),
        "the background thread died, as a panicking thread does"
    );

    // Still executing, which is the property: a background panic no longer takes the app
    // with it, and the reporting path we hung off the hook didn't turn it into an abort.
    // Two seconds is a backstop, not a guess: the hook spawns the courier inline, so this
    // is true within microseconds or never, and the parent's own nextest cap is 8 s.
    crate::test_support::wait_until(
        std::time::Duration::from_secs(2),
        "the hook to dispatch a courier for in-session delivery",
        || panic_courier::couriers_started_for_test() > before,
    );
}

#[test]
fn the_first_panic_of_a_session_lands_on_disk() {
    // The next-launch path for a FATAL panic. In-session delivery is additive; this is
    // still the only thing that survives the process dying.
    let dir = tempfile::tempdir().unwrap();
    let crash_path = dir.path().join(CRASH_FILE_NAME);
    let written = AtomicBool::new(false);

    assert!(write_first_crash_report(
        Some(&crash_path),
        &written,
        &make_test_report()
    ));
    let loaded = read_crash_report(&crash_path).expect("the report reads back");
    assert_eq!(loaded.app_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(loaded.panic_message.as_deref(), Some("test panic"));
}

#[test]
fn a_second_panic_does_not_clobber_the_first_report() {
    // The pending crash file holds one report, and the first panic is the causal one.
    // Later panics still reach triage through the log tail the courier writes.
    let dir = tempfile::tempdir().unwrap();
    let crash_path = dir.path().join(CRASH_FILE_NAME);
    let written = AtomicBool::new(false);

    let mut first = make_test_report();
    first.panic_message = Some("the root cause".to_string());
    assert!(write_first_crash_report(Some(&crash_path), &written, &first));

    let mut second = make_test_report();
    second.panic_message = Some("the consequence".to_string());
    assert!(
        !write_first_crash_report(Some(&crash_path), &written, &second),
        "the second panic must not write"
    );

    let loaded = read_crash_report(&crash_path).expect("the first report is still there");
    assert_eq!(loaded.panic_message.as_deref(), Some("the root cause"));
}

#[test]
fn a_session_with_no_data_dir_still_takes_a_panic_in_stride() {
    // `init` bails before setting a crash path when the data dir won't resolve. The hook
    // is installed regardless, so this branch has to be a quiet no-op, not a panic.
    let written = AtomicBool::new(false);
    assert!(!write_first_crash_report(None, &written, &make_test_report()));
    assert!(
        !written.load(Ordering::SeqCst),
        "a session with no path must not burn the one write it might get later"
    );
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
        app_fate: AppFate::Unconfirmed,
        reported_in_session: false,
        build_mode: Some("debug".to_string()),
        short_id: Some(crate::short_id::generate(CRASH_SHORT_ID_PREFIX)),
        diag_id: "diag_00000000-0000-4000-8000-000000000000".to_string(),
        email: None,
        system_snapshot: None,
        image_base: Some("0x104f2c000".to_string()),
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

// --- App fate: what the next-launch dialog is allowed to claim ---

/// A pending report on disk, `fate` as its recorded [`AppFate`] and a timestamp old
/// enough that crash-loop detection stays out of the way.
fn write_pending_report(path: &Path, fate: AppFate) {
    let report = CrashReport {
        timestamp: "2026-03-22T10:00:00+00:00".to_string(),
        app_fate: fate,
        ..make_test_report()
    };
    write_crash_report(path, &report).unwrap();
}

#[test]
fn a_crash_file_written_before_app_fate_existed_claims_nothing() {
    // The dialog picks its opening sentence from `app_fate`, so the value an older file
    // reads back as decides what the user is told about a crash we know nothing about.
    // `Unknown` is the only honest answer; a `survived: bool` would have said "the app
    // quit unexpectedly" here on no evidence at all.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);

    let mut json = serde_json::to_value(make_test_report()).unwrap();
    json.as_object_mut().unwrap().remove("appFate");
    std::fs::write(&path, serde_json::to_string(&json).unwrap()).unwrap();

    let loaded = read_crash_report(&path).expect("a crash file from an older build still parses");
    assert_eq!(loaded.app_fate, AppFate::Unknown);
}

#[test]
fn an_unconfirmed_panic_resolves_to_ended_at_the_next_launch() {
    // The hook writes `Unconfirmed` because it runs at panic initiation, before anyone
    // knows. A process that lived would have upgraded it; still finding it unconfirmed a
    // launch later IS the proof that the app went down with the panic.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);
    write_pending_report(&path, AppFate::Unconfirmed);

    process_pending_crash(&path, &dir.path().join(RAW_CRASH_FILE_NAME));

    assert_eq!(read_crash_report(&path).unwrap().app_fate, AppFate::Ended);
}

#[test]
fn a_confirmed_survival_is_never_downgraded_at_the_next_launch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);
    write_pending_report(&path, AppFate::KeptRunning);

    process_pending_crash(&path, &dir.path().join(RAW_CRASH_FILE_NAME));

    assert_eq!(
        read_crash_report(&path).unwrap().app_fate,
        AppFate::KeptRunning,
        "the app was seen alive after this panic; the next launch can't unsee it"
    );
}

#[test]
fn an_older_crash_file_stays_unknown_at_the_next_launch() {
    // Resolution applies to `Unconfirmed` only. Promoting `Unknown` to `Ended` would
    // invent the very claim the tri-state exists to avoid.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);
    write_pending_report(&path, AppFate::Unknown);

    process_pending_crash(&path, &dir.path().join(RAW_CRASH_FILE_NAME));

    assert_eq!(read_crash_report(&path).unwrap().app_fate, AppFate::Unknown);
}

// --- Already delivered in-session: the next launch stays quiet ---

#[test]
fn a_report_already_delivered_in_session_is_dropped_at_the_next_launch() {
    // The user was already told about this panic, in the session it happened in. Telling
    // them again at the next launch spends trust and buys nothing, so the file goes.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);
    let report = CrashReport {
        timestamp: "2026-03-22T10:00:00+00:00".to_string(),
        app_fate: AppFate::KeptRunning,
        reported_in_session: true,
        ..make_test_report()
    };
    write_crash_report(&path, &report).unwrap();

    process_pending_crash(&path, &dir.path().join(RAW_CRASH_FILE_NAME));

    assert!(
        !path.exists(),
        "a delivered report must be deleted, not left to re-offer itself on every launch"
    );
}

#[test]
fn a_report_that_never_went_out_is_still_offered() {
    // The mirror case, and the one that must not regress: a panic the app survived whose
    // in-session delivery never landed (error reports opted out, no network, or the app
    // died before the 60 s window fired) is the whole reason the next-launch path exists.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);
    let report = CrashReport {
        timestamp: "2026-03-22T10:00:00+00:00".to_string(),
        app_fate: AppFate::KeptRunning,
        reported_in_session: false,
        ..make_test_report()
    };
    write_crash_report(&path, &report).unwrap();

    process_pending_crash(&path, &dir.path().join(RAW_CRASH_FILE_NAME));

    let kept = read_crash_report(&path).expect("an undelivered report survives to the next launch");
    assert_eq!(kept.app_fate, AppFate::KeptRunning);
}

#[test]
fn a_crash_file_written_before_the_delivery_stamp_existed_is_still_offered() {
    // `false` is honest as a default here: it claims only that nothing recorded a delivery,
    // which is exactly true of an older file, and lands on the behavior it already had.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);

    let mut json = serde_json::to_value(make_test_report()).unwrap();
    json.as_object_mut().unwrap().remove("reportedInSession");
    std::fs::write(&path, serde_json::to_string(&json).unwrap()).unwrap();

    process_pending_crash(&path, &dir.path().join(RAW_CRASH_FILE_NAME));

    let kept = read_crash_report(&path).expect("an older crash file behaves exactly as it did before");
    assert!(!kept.reported_in_session);
}

#[test]
fn recording_a_delivery_stamps_the_report_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);
    write_crash_report(&path, &make_test_report()).unwrap();

    survival::record_in_session_delivery(&path);

    assert!(read_crash_report(&path).unwrap().reported_in_session);
}

#[test]
fn recording_a_delivery_with_no_crash_file_is_a_quiet_no_op() {
    // Every successful Flow B upload calls this, and almost none of them follow a panic.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CRASH_FILE_NAME);

    survival::record_in_session_delivery(&path);

    assert!(!path.exists(), "a delivery notice must never conjure a report");
}
