//! Hand-rolled `fern` Dispatch tree replacing `tauri-plugin-log`.
//!
//! Why we replaced the plugin: `tauri-plugin-log` routes through the global `log` facade
//! with a single shared level. We want the file target locked at Debug (so error report
//! bundles carry useful context) while the terminal stays at Info by default (less noise
//! for devs running `pnpm dev`). The plugin can't express that.
//!
//! Tree shape:
//!
//! ```text
//! root Dispatch (level Trace — pure ceiling, per-chain filters do real gating)
//! ├── stdout chain
//! │     .level(Info)                       // default; AtomicU8 below can bump to Debug
//! │     .level_for(<from RUST_LOG>, ..)
//! │     .filter(stdout-threshold AtomicU8) // verbose-toggle gate, no dispatch rebuild
//! │     .chain(io::stderr())                // stderr (matches plugin behavior, keeps
//! │                                          // stdout free for piped output)
//! └── file chain
//!       .level(Debug)                      // always, regardless of RUST_LOG/verbose
//!       .chain(<file_rotate writer>)       // 50 MB per file, KeepN
//! ```
//!
//! Format mirrors the previous plugin output: `HH:MM:SS.mmm LEVEL target  message`.
//! Stdout adds ANSI level colors; the file writer gets plain text so `cat` is readable.
//!
//! Runtime mutability:
//!
//! - **Stdout level** flips between Info and Debug via [`set_stdout_threshold`]. An
//!   `AtomicU8` consulted by the chain's filter — no dispatch rebuild, no log lines lost.
//! - **File rotation cap** is fixed at startup (file-rotate doesn't expose a live
//!   reconfigure). `set_keep_count` + `eager_prune` in the parent module already covered
//!   this — same restart-required envelope as the plugin had.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};

use file_rotate::{ContentLimit, FileRotate, compression::Compression, suffix::AppendCount};

/// Stdout threshold packed into one byte. `0..=5` matches the integer values of
/// `log::LevelFilter` (`Off=0`, `Error=1`, ..., `Trace=5`). Default = `Info`.
///
/// The fern filter reads this on every record, so the verbose toggle takes effect
/// without rebuilding the dispatch — and no records are lost during the swap.
static STDOUT_THRESHOLD: AtomicU8 = AtomicU8::new(log::LevelFilter::Info as u8);

/// Updates the stdout threshold. Records with severity below the threshold are dropped
/// from the stdout chain only — the file chain always sees Debug+ (when enabled).
pub fn set_stdout_threshold(level: log::LevelFilter) {
    STDOUT_THRESHOLD.store(level as u8, Ordering::Relaxed);
}

/// Returns the current stdout threshold.
pub fn stdout_threshold() -> log::LevelFilter {
    level_filter_from_u8(STDOUT_THRESHOLD.load(Ordering::Relaxed))
}

/// Renders the current local time as an ISO 8601 stamp with millisecond precision plus
/// a `±HH:MM` offset (e.g., `2026-04-25T01:18:28.218+02:00`).
///
/// File chain only — the stdout chain stays terse with `HH:MM:SS.mmm` because devs
/// reading the live terminal already know the date and time. The file lives forever and
/// gets shipped to triage; ISO + offset means a reader anywhere on the planet can pin
/// down exactly when a line was written. Cheap parsing target for the Flow B
/// window-anchored bundle, too.
pub(crate) fn file_timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string()
}

fn level_filter_from_u8(v: u8) -> log::LevelFilter {
    match v {
        0 => log::LevelFilter::Off,
        1 => log::LevelFilter::Error,
        2 => log::LevelFilter::Warn,
        3 => log::LevelFilter::Info,
        4 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    }
}

/// Initialization options assembled in `lib.rs` before the dispatch is built.
pub struct InitOptions {
    /// Where rotated log files live. `None` disables the file chain entirely
    /// (matches `advanced.maxLogStorageMb = 0`).
    pub log_dir: Option<PathBuf>,
    /// `KeepSome(N)` equivalent for file-rotate (`ceil(cap_mb / 50)`).
    pub keep_count: usize,
    /// Optional `RUST_LOG` value. When `None` we apply our defaults
    /// (Info on stdout, Debug on the file chain) plus the noise overrides.
    pub rust_log: Option<String>,
}

/// Builds the Dispatch tree and installs it as the global logger.
///
/// Idempotent failure mode: returns `Err` if a previous call (or another logger) has
/// already taken over the `log` facade. In tests this is fine — they don't need the real
/// logger.
pub fn init(opts: InitOptions) -> Result<(), fern::InitError> {
    // Apply RUST_LOG up front so `set_stdout_threshold` is the only post-init knob.
    // Defaults: stdout Info (noise-free), file Debug (catches error-report context).
    let mut stdout_default = log::LevelFilter::Info;
    let mut stdout_overrides: Vec<(String, log::LevelFilter)> = Vec::new();
    if let Some(rust_log) = opts.rust_log.as_deref() {
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

    // Stdout chain. Stderr (not stdout) so logs don't pollute commands that pipe stdout
    // — same behavior the previous plugin had.
    let mut stdout_chain = fern::Dispatch::new()
        .format(|out, message, record| {
            let now = chrono::Local::now();
            let ts = now.format("%H:%M:%S%.3f");
            let target = record.target().strip_prefix("cmdr_lib::").unwrap_or(record.target());
            let level = record.level();
            let color = match level {
                log::Level::Error => "\x1b[31m", // red
                log::Level::Warn => "\x1b[33m",  // yellow
                log::Level::Info => "\x1b[32m",  // green
                log::Level::Debug => "\x1b[36m", // cyan
                log::Level::Trace => "\x1b[35m", // magenta
            };
            out.finish(format_args!("{ts} {color}{level:<5}\x1b[0m {target}  {message}"));
        })
        // Ceiling — the live AtomicU8 filter below does the real gating.
        .level(log::LevelFilter::Trace)
        // Live verbose-toggle gate: drops anything below the current AtomicU8.
        .filter(|metadata| metadata.level() <= stdout_threshold())
        .chain(std::io::stderr());

    // Default noise suppression — applied to the stdout chain only. The file chain
    // intentionally keeps these at Debug so error reports include them.
    for (module, level) in default_noise_overrides() {
        stdout_chain = stdout_chain.level_for(module, level);
    }
    // RUST_LOG per-module overrides (also stdout-only).
    for (module, level) in stdout_overrides {
        stdout_chain = stdout_chain.level_for(module, level);
    }

    // Build the root.
    let mut root = fern::Dispatch::new().level(log::LevelFilter::Trace).chain(stdout_chain);

    // File chain: always Debug, plain text (no ANSI), rotation managed by file-rotate.
    if let Some(dir) = opts.log_dir.as_ref()
        && opts.keep_count > 0
    {
        std::fs::create_dir_all(dir).ok();
        let log_path = dir.join("cmdr.log");
        // 50 MB per file, AppendCount(N-1) keeps the live file + N-1 rotated copies = N total.
        // file-rotate's count is "extras beyond the live file"; we want N total.
        let rotated_extras = opts.keep_count.saturating_sub(1).max(1);
        let rotator = FileRotate::new(
            log_path,
            AppendCount::new(rotated_extras),
            ContentLimit::Bytes(50_000_000),
            Compression::None,
            None,
        );
        let writer: Box<dyn Write + Send + 'static> = Box::new(MutexWriter(Mutex::new(rotator)));

        let file_chain = fern::Dispatch::new()
            .format(|out, message, record| {
                let target = record.target().strip_prefix("cmdr_lib::").unwrap_or(record.target());
                let level = record.level();
                out.finish(format_args!(
                    "{ts} {level:<5} {target}  {message}",
                    ts = file_timestamp(),
                ));
            })
            .level(log::LevelFilter::Debug)
            .chain(writer);
        root = root.chain(file_chain);
    }

    root.apply()?;
    // log::set_max_level controls the per-record filter the `log` macros consult before
    // even constructing the record. Set it to Trace so per-chain filters can see
    // everything; if either chain wants to drop, it does so itself.
    log::set_max_level(log::LevelFilter::Trace);
    Ok(())
}

/// Defaults applied to the stdout chain to suppress known-noisy crates. The file chain
/// keeps these at Debug (it inherits the chain's Debug level) so error report bundles
/// still capture them.
fn default_noise_overrides() -> Vec<(&'static str, log::LevelFilter)> {
    vec![
        ("nusb", log::LevelFilter::Warn),
        ("zbus", log::LevelFilter::Warn),
        ("tracing::span", log::LevelFilter::Warn),
        ("smb2", log::LevelFilter::Warn),
        ("tao", log::LevelFilter::Warn),
    ]
}

fn parse_level_filter(s: &str) -> Option<log::LevelFilter> {
    match s.to_lowercase().as_str() {
        "trace" => Some(log::LevelFilter::Trace),
        "debug" => Some(log::LevelFilter::Debug),
        "info" => Some(log::LevelFilter::Info),
        "warn" | "warning" => Some(log::LevelFilter::Warn),
        "error" => Some(log::LevelFilter::Error),
        "off" => Some(log::LevelFilter::Off),
        _ => None,
    }
}

/// Adapts `FileRotate` (which is `Write` but not `Sync`) into a `Send + Sync` writer fern
/// can swallow. fern accepts `Box<dyn Write + Send>` for chain targets; the lock here
/// guards `FileRotate`'s internal cursor across the (rare) concurrent `log!` calls from
/// multiple threads.
struct MutexWriter<W: Write + Send>(Mutex<W>);

impl<W: Write + Send> Write for MutexWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut guard = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut guard = self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.flush()
    }
}

/// In-memory buffer the test writer appends to.
#[cfg(test)]
type TestBuffer = std::sync::Arc<Mutex<Vec<u8>>>;

/// What [`build_dispatch_for_test`] hands back: the assembled logger plus handles to the
/// stdout / file capture buffers.
#[cfg(test)]
struct TestDispatch {
    /// Forwarded from `Dispatch::into_log()` — fern's effective level ceiling. Tests
    /// don't need it but `into_log()` returns it anyway.
    #[allow(dead_code, reason = "Returned by fern; useful for tests that want to assert it")]
    filter: log::LevelFilter,
    logger: Box<dyn log::Log>,
    stdout: TestBuffer,
    file: TestBuffer,
}

/// Builds the same dispatch tree shape `init` would, but writes both chains to
/// caller-supplied in-memory buffers and skips the global `apply()`. Test-only.
#[cfg(test)]
fn build_dispatch_for_test(rust_log: Option<&str>, file_chain_enabled: bool) -> TestDispatch {
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
        .format(|out, message, record| {
            out.finish(format_args!("{} {}", record.level(), message));
        })
        .level(log::LevelFilter::Trace)
        .filter(|metadata| metadata.level() <= stdout_threshold())
        .chain(Box::new(SharedBufferWriter(stdout_buf.clone())) as Box<dyn Write + Send>);

    for (module, level) in default_noise_overrides() {
        stdout_chain = stdout_chain.level_for(module, level);
    }
    for (module, level) in stdout_overrides {
        stdout_chain = stdout_chain.level_for(module, level);
    }

    let mut root = fern::Dispatch::new().level(log::LevelFilter::Trace).chain(stdout_chain);

    if file_chain_enabled {
        let file_chain = fern::Dispatch::new()
            .format(|out, message, record| {
                out.finish(format_args!("{} {}", record.level(), message));
            })
            .level(log::LevelFilter::Debug)
            .chain(Box::new(SharedBufferWriter(file_buf.clone())) as Box<dyn Write + Send>);
        root = root.chain(file_chain);
    }

    let (filter, logger) = root.into_log();
    TestDispatch {
        filter,
        logger,
        stdout: stdout_buf,
        file: file_buf,
    }
}

#[cfg(test)]
struct SharedBufferWriter(std::sync::Arc<Mutex<Vec<u8>>>);

#[cfg(test)]
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

#[cfg(test)]
mod tests {
    use super::*;

    /// All dispatch tests touch the process-global `STDOUT_THRESHOLD` — serialize them
    /// with a single mutex so parallel test runs don't see each other's threshold flips.
    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Synthesizes a `log::Record` and sends it through the test logger. Avoids the
    /// global `log` facade so tests are isolated.
    fn dispatch(logger: &dyn log::Log, level: log::Level, target: &str, message: &str) {
        // `format_args!` borrows from temporaries — keep them alive across the call.
        let args = format_args!("{message}");
        let record = log::Record::builder().level(level).target(target).args(args).build();
        if logger.enabled(record.metadata()) {
            logger.log(&record);
        }
    }

    fn buf_to_string(buf: &Mutex<Vec<u8>>) -> String {
        let guard = buf.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        String::from_utf8_lossy(&guard).into_owned()
    }

    /// The headline: a Debug record reaches the file chain but is dropped from stdout
    /// when stdout is at its Info default. Per-output filtering — the whole point of
    /// this rewrite.
    #[test]
    fn debug_record_hits_file_only() {
        let _g = test_lock();
        let d = build_dispatch_for_test(None, true);
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
        let d = build_dispatch_for_test(None, true);
        dispatch(&*d.logger, log::Level::Info, "cmdr_lib::test", "shared-info");
        assert!(buf_to_string(&d.stdout).contains("shared-info"));
        assert!(buf_to_string(&d.file).contains("shared-info"));
    }

    /// Verbose toggle flip: bumping stdout to Debug starts capturing what was being
    /// dropped. No dispatch rebuild — same logger instance, AtomicU8 update only.
    #[test]
    fn stdout_threshold_flip_admits_debug() {
        let _g = test_lock();
        let d = build_dispatch_for_test(None, false);
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
        let d = build_dispatch_for_test(None, false);
        dispatch(&*d.logger, log::Level::Info, "cmdr_lib::test", "stdout-only");
        assert!(buf_to_string(&d.stdout).contains("stdout-only"));
        assert!(buf_to_string(&d.file).is_empty());
    }

    /// RUST_LOG per-module overrides apply to stdout only — the file chain stays at
    /// Debug regardless. So `RUST_LOG=cmdr_lib::test=warn` silences this module on the
    /// terminal while the file still gets the full Info/Debug stream.
    #[test]
    fn rust_log_module_override_applies_to_stdout_only() {
        let _g = test_lock();
        let d = build_dispatch_for_test(Some("cmdr_lib::test=warn,info"), true);
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
    /// offset. Triage reads logs from arbitrary timezones — a bare `HH:MM:SS.mmm` with
    /// no date or offset is impossible to correlate with anything else.
    #[test]
    fn file_timestamp_is_iso8601_with_offset() {
        let ts = file_timestamp();
        let re =
            regex::Regex::new(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}[+-]\d{2}:\d{2}$").expect("static regex");
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
}
