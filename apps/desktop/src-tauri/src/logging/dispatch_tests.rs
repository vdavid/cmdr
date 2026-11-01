//! Tests for the dispatch tree, through a test-only copy of it that writes to in-memory buffers.

use super::*;
use std::sync::Mutex;

/// In-memory buffer the test writer appends to.
type TestBuffer = std::sync::Arc<Mutex<Vec<u8>>>;

/// What [`build_dispatch_for_test`] hands back: the assembled logger plus handles to the
/// stdout / file capture buffers.
struct TestDispatch {
    /// Forwarded from `Dispatch::into_log()`: fern's effective level ceiling. Tests
    /// don't need it but `into_log()` returns it anyway.
    #[allow(dead_code, reason = "Returned by fern; useful for tests that want to assert it")]
    filter: log::LevelFilter,
    logger: Box<dyn log::Log>,
    stdout: TestBuffer,
    file: TestBuffer,
}

/// Builds the same dispatch tree shape `init` would, but writes both chains to
/// caller-supplied in-memory buffers and skips the global `apply()`. Test-only.
///
/// `color` stands in for `target_style::color_enabled()`, which a test can't influence:
/// a test runner's stderr is a pipe, so the real call would always answer false.
fn build_dispatch_for_test(rust_log: Option<&str>, file_chain_enabled: bool, color: bool) -> TestDispatch {
    use std::sync::Arc;

    let stdout_buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let file_buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));

    // Mirror init()'s RUST_LOG parsing.
    let mut stdout_default = log::LevelFilter::Info;
    let mut stdout_overrides: Vec<(String, log::LevelFilter)> = Vec::new();
    if let Some(rust_log) = rust_log {
        for directive in rust_log.split(',') {
            let directive = directive.trim();
            if directive.is_empty() {
                continue;
            }
            if let Some((module, level_str)) = directive.split_once('=') {
                if let Some(level) = parse_level_filter(level_str) {
                    stdout_overrides.push((module.to_string(), level));
                }
            } else if let Some(level) = parse_level_filter(directive) {
                stdout_default = level;
            }
        }
    }
    set_stdout_threshold(stdout_default);

    let mut stdout_chain = fern::Dispatch::new()
        .format(move |out, message, record| write_terminal_line(out, message, record, color))
        .level(log::LevelFilter::Trace)
        .filter(|metadata| metadata.level() <= stdout_threshold())
        .chain(Box::new(SharedBufferWriter(stdout_buf.clone())) as Box<dyn Write + Send>);

    for (module, level) in default_noise_overrides() {
        stdout_chain = stdout_chain.level_for(module, level);
    }
    for (module, level) in stdout_overrides {
        stdout_chain = stdout_chain.level_for(module, level);
    }

    let mut root = root_dispatch().chain(stdout_chain);

    if file_chain_enabled {
        root = root.chain(file_chain(Box::new(SharedBufferWriter(file_buf.clone()))));
    }

    let (filter, logger) = root.into_log();
    TestDispatch {
        filter,
        logger,
        stdout: stdout_buf,
        file: file_buf,
    }
}

struct SharedBufferWriter(std::sync::Arc<Mutex<Vec<u8>>>);

impl Write for SharedBufferWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut guard = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// All dispatch tests touch the process-global `STDOUT_THRESHOLD`. Serialize them
/// with a single mutex so parallel test runs don't see each other's threshold flips.
fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Synthesizes a `log::Record` and sends it through the test logger. Avoids the
/// global `log` facade so tests are isolated.
fn dispatch(logger: &dyn log::Log, level: log::Level, target: &str, message: &str) {
    // `format_args!` borrows from temporaries. Keep them alive across the call.
    let args = format_args!("{message}");
    let record = log::Record::builder().level(level).target(target).args(args).build();
    if logger.enabled(record.metadata()) {
        logger.log(&record);
    }
}

/// [`dispatch`] with the call site's module path, which is what decides a line's provenance.
fn dispatch_from(logger: &dyn log::Log, level: log::Level, target: &str, module_path: &str, message: &str) {
    let args = format_args!("{message}");
    let record = log::Record::builder()
        .level(level)
        .target(target)
        .module_path(Some(module_path))
        .args(args)
        .build();
    if logger.enabled(record.metadata()) {
        logger.log(&record);
    }
}

/// Our own lines carry the provenance tag whatever their target; a third-party crate's
/// don't, even under a target that reads like ours. Error reports ship DEBUG only from
/// tagged lines (`provenance.rs`).
#[test]
fn our_file_lines_carry_the_own_tag_and_nobody_elses_do() {
    let _g = test_lock();
    let d = build_dispatch_for_test(None, true, false);
    dispatch_from(
        &*d.logger,
        log::Level::Debug,
        "copy",
        "cmdr_operations::transfer",
        "ours-short-target",
    );
    dispatch_from(
        &*d.logger,
        log::Level::Debug,
        "FE:viewer",
        "cmdr_lib::commands::logging",
        "ours-fe",
    );
    dispatch_from(
        &*d.logger,
        log::Level::Debug,
        "copy",
        "some_crate::copy",
        "foreign-same-target",
    );
    dispatch(&*d.logger, log::Level::Debug, "copy", "no-module-path");
    let f = buf_to_string(&d.file);
    assert!(f.contains("DEBUG ·copy  ours-short-target"), "{f}");
    assert!(f.contains("DEBUG ·FE:viewer  ours-fe"), "{f}");
    assert!(f.contains("DEBUG copy  foreign-same-target"), "{f}");
    assert!(f.contains("DEBUG copy  no-module-path"), "{f}");
}

/// `mdns_sd`'s debug output was most of every bundle and of the log's disk use; the file
/// keeps its Info and above.
#[test]
fn mdns_sd_debug_stays_out_of_the_file() {
    let _g = test_lock();
    let d = build_dispatch_for_test(None, true, false);
    let mdns = "mdns_sd::service_daemon";
    dispatch_from(&*d.logger, log::Level::Debug, mdns, mdns, "mdns-debug-dump");
    dispatch_from(&*d.logger, log::Level::Warn, mdns, mdns, "mdns-warning");
    let f = buf_to_string(&d.file);
    assert!(!f.contains("mdns-debug-dump"), "{f}");
    assert!(f.contains("mdns-warning"), "{f}");
}

/// `rmcp` logs a request's arguments at Debug, so an `unlock` password would land in the file and in
/// error reports. Nothing it logs reaches either chain, at any level, whatever `RUST_LOG` asks for.
#[test]
fn no_rmcp_line_reaches_either_chain_even_when_rust_log_asks() {
    let _g = test_lock();
    for rust_log in [None, Some("trace"), Some("rmcp=trace"), Some("rmcp::service=trace")] {
        let d = build_dispatch_for_test(rust_log, true, false);
        for target in [
            "rmcp",
            "rmcp::service",
            "rmcp::transport::streamable_http_server::tower",
        ] {
            dispatch(&*d.logger, log::Level::Debug, target, "a-password-in-a-request");
            dispatch(&*d.logger, log::Level::Error, target, "a-password-in-a-request");
        }
        // A crate that only starts with the name isn't this one.
        dispatch(&*d.logger, log::Level::Error, "rmcpish", "somebody-else");
        for (chain, text) in [("terminal", buf_to_string(&d.stdout)), ("file", buf_to_string(&d.file))] {
            assert!(
                !text.contains("a-password-in-a-request"),
                "{chain} under {rust_log:?}: {text:?}"
            );
            assert!(text.contains("somebody-else"), "{chain} under {rust_log:?}: {text:?}");
        }
    }
}

fn buf_to_string(buf: &Mutex<Vec<u8>>) -> String {
    let guard = buf.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    String::from_utf8_lossy(&guard).into_owned()
}

/// The terminal shows a marked value as itself, markers gone, raw or escaped; the file keeps the
/// line exactly as written, since error reports read the markers from it.
#[test]
fn the_terminal_strips_markers_and_the_file_keeps_them() {
    use cmdr_fs::log_private::{CLOSER, LogKind};
    let _g = test_lock();
    let d = build_dispatch_for_test(None, true, false);
    let opener = LogKind::FilePath.opener();
    let raw = format!("opened {opener}/Users/me/Cafe\\u{{301}} \\\"x\\\".pdf{CLOSER} in 3ms");
    let detail = format!("failed detail={:?}", format!("can't read {opener}/tmp/a b{CLOSER}"));
    dispatch(&*d.logger, log::Level::Info, "cmdr_lib::test", &raw);
    dispatch(&*d.logger, log::Level::Info, "cmdr_lib::test", &detail);

    let s = buf_to_string(&d.stdout);
    assert!(s.contains("opened /Users/me/Cafe\u{301} \"x\".pdf in 3ms"), "{s:?}");
    assert!(s.contains("failed detail=\"can't read /tmp/a b\""), "{s:?}");
    assert!(!s.chars().any(cmdr_fs::log_private::is_reserved), "{s:?}");
    assert!(!s.contains("u{fd"), "{s:?}");
    let f = buf_to_string(&d.file);
    assert!(f.contains(&raw) && f.contains(&detail), "{f:?}");
}

/// #706: an S3 app path carries the account's access key id, and neither the file nor the
/// terminal keeps it, marked or not: every line names the place by host and bucket.
#[test]
fn neither_chain_keeps_an_s3_access_key_id() {
    use cmdr_fs::log_private::LogPrivate;
    let _g = test_lock();
    let d = build_dispatch_for_test(None, true, false);
    let path = "s3://AKIAEXAMPLEKEY@s3.eu-west-1.amazonaws.com:443/photos/a.jpg";
    let marked = format!("listing path={:?}", LogPrivate::dir_path(path));
    dispatch(&*d.logger, log::Level::Info, "cmdr_lib::test", &marked);
    dispatch(
        &*d.logger,
        log::Level::Info,
        "cmdr_lib::test",
        &format!("FE: newPath={path}"),
    );

    for (chain, text) in [("terminal", buf_to_string(&d.stdout)), ("file", buf_to_string(&d.file))] {
        assert!(!text.contains("AKIAEXAMPLEKEY"), "{chain}: {text:?}");
        assert_eq!(
            text.matches("s3://s3.eu-west-1.amazonaws.com:443/photos/a.jpg").count(),
            2,
            "{chain}: {text:?}"
        );
    }
}

/// The headline: a Debug record reaches the file chain but is dropped from stdout
/// when stdout is at its Info default. Per-output filtering: the whole point of
/// this rewrite.
#[test]
fn debug_record_hits_file_only() {
    let _g = test_lock();
    let d = build_dispatch_for_test(None, true, false);
    dispatch(&*d.logger, log::Level::Debug, "cmdr_lib::test", "secret-debug");
    let s = buf_to_string(&d.stdout);
    let f = buf_to_string(&d.file);
    assert!(
        !s.contains("secret-debug"),
        "stdout should drop Debug at Info threshold (got {s:?})"
    );
    assert!(
        f.contains("secret-debug"),
        "file should always capture Debug (got {f:?})"
    );
}

/// Info records hit both chains.
#[test]
fn info_record_hits_both() {
    let _g = test_lock();
    let d = build_dispatch_for_test(None, true, false);
    dispatch(&*d.logger, log::Level::Info, "cmdr_lib::test", "shared-info");
    assert!(buf_to_string(&d.stdout).contains("shared-info"));
    assert!(buf_to_string(&d.file).contains("shared-info"));
}

/// The head gets a fixed-width column, so every message on a terminal starts at a
/// predictable place and the subsystem is scannable down the left edge.
#[test]
fn terminal_line_pads_a_bare_head() {
    let _g = test_lock();
    let d = build_dispatch_for_test(None, false, false);
    dispatch(&*d.logger, log::Level::Info, "cmdr_lib::logging", "ready");
    let s = buf_to_string(&d.stdout);
    assert!(s.contains("INFO  logging         ready"), "got {s:?}");
}

/// The module path picks up where the padded head ends, so `downloads::watcher`
/// reads as one target with a gap in the middle rather than two words.
#[test]
fn terminal_line_pads_the_head_and_keeps_the_tail_attached() {
    let _g = test_lock();
    let d = build_dispatch_for_test(None, false, false);
    dispatch(&*d.logger, log::Level::Info, "downloads::watcher", "watching");
    let s = buf_to_string(&d.stdout);
    assert!(s.contains("INFO  downloads     ::watcher  watching"), "got {s:?}");
}

/// A head longer than the column overflows rather than being cut: a truncated
/// subsystem name is worse than one misaligned line.
#[test]
fn an_overlong_head_is_never_truncated() {
    let _g = test_lock();
    let d = build_dispatch_for_test(None, false, false);
    dispatch(&*d.logger, log::Level::Info, "a_very_long_subsystem_name", "hi");
    let s = buf_to_string(&d.stdout);
    assert!(s.contains("a_very_long_subsystem_name  hi"), "got {s:?}");
}

/// Piped or redirected output gets no ANSI at all: `pnpm dev > log.txt` should be
/// readable, and the escape sequences would otherwise land in the file.
#[test]
fn a_non_terminal_gets_plain_text() {
    let _g = test_lock();
    let d = build_dispatch_for_test(None, false, false);
    dispatch(&*d.logger, log::Level::Error, "downloads::watcher", "boom");
    let s = buf_to_string(&d.stdout);
    assert!(!s.contains('\x1b'), "expected no escape sequences, got {s:?}");
}

/// On a terminal the head takes its own color and the tail goes gray.
#[test]
fn a_terminal_paints_the_head_and_dims_the_tail() {
    let _g = test_lock();
    let d = build_dispatch_for_test(None, false, true);
    dispatch(&*d.logger, log::Level::Info, "downloads::watcher", "watching");
    let s = buf_to_string(&d.stdout);
    let head = super::super::target_style::head_color("downloads");
    assert!(s.contains(&format!("{head}downloads     ")), "head not painted: {s:?}");
    assert!(s.contains("\x1b[38;5;245m::watcher\x1b[0m"), "tail not dimmed: {s:?}");
}

/// A bare head carries no tail sequence wrapping nothing. Most lines have no tail,
/// so the wasted bytes would be on most of the output.
#[test]
fn a_bare_head_emits_no_tail_sequence() {
    let _g = test_lock();
    let d = build_dispatch_for_test(None, false, true);
    dispatch(&*d.logger, log::Level::Info, "logging", "ready");
    let s = buf_to_string(&d.stdout);
    assert!(!s.contains("\x1b[38;5;245m"), "stray tail color: {s:?}");
}

/// Verbose toggle flip: bumping stdout to Debug starts capturing what was being
/// dropped. No dispatch rebuild: same logger instance, AtomicU8 update only.
#[test]
fn stdout_threshold_flip_admits_debug() {
    let _g = test_lock();
    let d = build_dispatch_for_test(None, false, false);
    dispatch(&*d.logger, log::Level::Debug, "cmdr_lib::test", "before-flip");
    assert!(!buf_to_string(&d.stdout).contains("before-flip"));

    set_stdout_threshold(log::LevelFilter::Debug);
    dispatch(&*d.logger, log::Level::Debug, "cmdr_lib::test", "after-flip");
    assert!(
        buf_to_string(&d.stdout).contains("after-flip"),
        "after threshold flip, debug should pass: {:?}",
        buf_to_string(&d.stdout),
    );

    // Restore for sibling tests.
    set_stdout_threshold(log::LevelFilter::Info);
}

/// Cap = 0 → no file chain, but stdout still works.
#[test]
fn no_file_chain_when_disabled() {
    let _g = test_lock();
    let d = build_dispatch_for_test(None, false, false);
    dispatch(&*d.logger, log::Level::Info, "cmdr_lib::test", "stdout-only");
    assert!(buf_to_string(&d.stdout).contains("stdout-only"));
    assert!(buf_to_string(&d.file).is_empty());
}

/// RUST_LOG per-module overrides apply to stdout only. The file chain stays at
/// Debug regardless. So `RUST_LOG=cmdr_lib::test=warn` silences this module on the
/// terminal while the file still gets the full Info/Debug stream.
#[test]
fn rust_log_module_override_applies_to_stdout_only() {
    let _g = test_lock();
    let d = build_dispatch_for_test(Some("cmdr_lib::test=warn,info"), true, false);
    dispatch(&*d.logger, log::Level::Info, "cmdr_lib::test", "filtered-on-stdout");
    assert!(
        !buf_to_string(&d.stdout).contains("filtered-on-stdout"),
        "stdout should drop Info under module=warn override: {:?}",
        buf_to_string(&d.stdout),
    );
    assert!(
        buf_to_string(&d.file).contains("filtered-on-stdout"),
        "file chain ignores RUST_LOG and keeps Info: {:?}",
        buf_to_string(&d.file),
    );

    // Restore for sibling tests.
    set_stdout_threshold(log::LevelFilter::Info);
}

/// File-chain timestamps must be ISO 8601 with millisecond precision and a `±HH:MM`
/// offset. Triage reads logs from arbitrary timezones. A bare `HH:MM:SS.mmm` with
/// no date or offset is impossible to correlate with anything else.
#[test]
fn file_timestamp_is_iso8601_with_offset() {
    let ts = file_timestamp();
    let re = regex::Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}[+-]\d{2}:\d{2}$").expect("static regex");
    assert!(
        re.is_match(&ts),
        "file_timestamp `{ts}` doesn't match the ISO-8601-with-offset shape"
    );
}

#[test]
fn parse_level_filter_known() {
    assert_eq!(parse_level_filter("info"), Some(log::LevelFilter::Info));
    assert_eq!(parse_level_filter("DEBUG"), Some(log::LevelFilter::Debug));
    assert_eq!(parse_level_filter("warn"), Some(log::LevelFilter::Warn));
    assert_eq!(parse_level_filter("warning"), Some(log::LevelFilter::Warn));
    assert_eq!(parse_level_filter("error"), Some(log::LevelFilter::Error));
    assert_eq!(parse_level_filter("trace"), Some(log::LevelFilter::Trace));
    assert_eq!(parse_level_filter("off"), Some(log::LevelFilter::Off));
    assert!(parse_level_filter("nope").is_none());
}

#[test]
fn level_filter_round_trip() {
    for f in [
        log::LevelFilter::Off,
        log::LevelFilter::Error,
        log::LevelFilter::Warn,
        log::LevelFilter::Info,
        log::LevelFilter::Debug,
        log::LevelFilter::Trace,
    ] {
        assert_eq!(level_filter_from_u8(f as u8), f);
    }
}

/// Stderr is the stdout chain's destination. When the parent process closes the
/// inherited fd — power user launches Cmdr from Terminal then closes the window,
/// or pipes Cmdr's output and the receiver exits early — writes start failing with
/// `BrokenPipe`. Without this wrapper, fern would fall back to a direct
/// `std::io::stderr()` write (same broken fd, same failure) and `panic!`. Every
/// tokio task that logs after that point dies with the fern message visible to the
/// user as a "task panicked" dialog.
#[test]
fn broken_pipe_tolerant_swallows_broken_pipe_on_write() {
    struct AlwaysBrokenPipe;
    impl Write for AlwaysBrokenPipe {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
        }
    }
    let mut w = BrokenPipeTolerantWriter(AlwaysBrokenPipe);
    // write_all is what fern calls on the chain target. It loops over `write`
    // until everything is consumed; the wrapper claims success on every byte.
    let result = w.write_all(b"the wire is closed but log() must not panic");
    assert!(result.is_ok(), "BrokenPipe must be swallowed: {result:?}");
    let flush = w.flush();
    assert!(flush.is_ok(), "BrokenPipe on flush must also be swallowed: {flush:?}");
}

/// Non-BrokenPipe failures must still propagate. A wrapper that swallows every error
/// would mask a genuine bug (full disk, permission denied) in some future writer
/// the same chain target wraps.
#[test]
fn broken_pipe_tolerant_propagates_other_io_errors() {
    struct AlwaysPermissionDenied;
    impl Write for AlwaysPermissionDenied {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        }
    }
    let mut w = BrokenPipeTolerantWriter(AlwaysPermissionDenied);
    let result = w.write(b"x");
    assert!(matches!(result, Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied));
    let flush = w.flush();
    assert!(matches!(flush, Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied));
}

/// Healthy writes pass through unchanged.
#[test]
fn broken_pipe_tolerant_passes_through_healthy_writes() {
    use std::sync::Arc;
    let buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let mut w = BrokenPipeTolerantWriter(SharedBufferWriter(buf.clone()));
    w.write_all(b"hello").expect("healthy write must succeed");
    w.flush().expect("healthy flush must succeed");
    let guard = buf.lock().unwrap();
    assert_eq!(&*guard, b"hello");
}

#[test]
fn stdout_threshold_setter_round_trips() {
    let _g = test_lock();
    // Save/restore around the test so other tests are unaffected (this is a
    // process-global atomic, like the verbose toggle in production).
    let saved = stdout_threshold();
    set_stdout_threshold(log::LevelFilter::Debug);
    assert_eq!(stdout_threshold(), log::LevelFilter::Debug);
    set_stdout_threshold(log::LevelFilter::Warn);
    assert_eq!(stdout_threshold(), log::LevelFilter::Warn);
    set_stdout_threshold(saved);
}
