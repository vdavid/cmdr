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
//! root Dispatch (level Trace: pure ceiling, per-chain filters do real gating; drops `NEVER_LOGGED`)
//! ├── stdout chain
//! │     .level(Info)                       // default; AtomicU8 below can bump to Debug
//! │     .level_for(<from RUST_LOG>, ..)
//! │     .filter(stdout-threshold AtomicU8) // verbose-toggle gate, no dispatch rebuild
//! │     .chain(io::stderr())                // stderr (matches plugin behavior, keeps
//! │                                          // stdout free for piped output)
//! └── file chain
//!       .level(Debug)                      // always, regardless of RUST_LOG/verbose
//!       .level_for("mdns_sd", Info)        // its dumps were most of the file
//!       .chain(<RotatingFile writer>)      // 50 MB per file, KeepN
//! ```
//!
//! Both chains render `HH:MM:SS.mmm LEVEL target  message`. The terminal chain adds ANSI
//! (level colors, plus the fixed-width colored target head `target_style` describes) when
//! stderr is a terminal; the file writer gets plain text so `cat` is readable, an ISO stamp
//! instead of the clock time, and a `·` before the target of our own lines (`provenance`).
//!
//! Runtime mutability:
//!
//! - **Stdout level** flips between Info and Debug via [`set_stdout_threshold`]. An `AtomicU8`
//!   consulted by the chain's filter, no dispatch rebuild, no log lines lost.
//! - **File rotation cap** is fixed at startup (the `RotatingFile` is built once).
//!   `set_keep_count` + `eager_prune` in the parent module already covered this. Same
//!   restart-required envelope as the plugin had.

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};

/// Stdout threshold packed into one byte. `0..=5` matches the integer values of
/// `log::LevelFilter` (`Off=0`, `Error=1`, ..., `Trace=5`). Default = `Info`.
///
/// The fern filter reads this on every record, so the verbose toggle takes effect
/// without rebuilding the dispatch, and no records are lost during the swap.
static STDOUT_THRESHOLD: AtomicU8 = AtomicU8::new(log::LevelFilter::Info as u8);

/// Updates the stdout threshold. Records with severity below the threshold are dropped
/// from the stdout chain only. The file chain always sees Debug+ (when enabled).
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
/// File chain only. The stdout chain stays terse with `HH:MM:SS.mmm` because devs
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
    /// Total log files kept, live one included (`ceil(cap_mb / 50)`).
    pub keep_count: usize,
    /// Optional `RUST_LOG` value. When `None` we apply our defaults
    /// (Info on stdout, Debug on the file chain) plus the noise overrides.
    pub rust_log: Option<String>,
}

/// Builds the Dispatch tree and installs it as the global logger.
///
/// Idempotent failure mode: returns `Err` if a previous call (or another logger) has
/// already taken over the `log` facade. In tests this is fine; they don't need the real
/// logger.
pub fn init(opts: InitOptions) -> Result<(), fern::InitError> {
    // Start the optional per-line RAM gauge (no-op unless `CMDR_LOG_RAM_USE` is set)
    // before any format closure can run.
    super::ram_gauge::init();

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

    // Capture the resolved stdout default + per-module overrides for the error-report
    // manifest. The error reporter's `log_level_overrides::record` is `OnceLock`-backed,
    // so subsequent calls (only in tests) are no-ops.
    {
        let mut all_overrides: Vec<(String, log::LevelFilter)> = default_noise_overrides()
            .into_iter()
            .map(|(m, l)| (m.to_string(), l))
            .collect();
        all_overrides.extend(stdout_overrides.iter().cloned());
        crate::error_reporter::log_level_overrides::record(stdout_default, all_overrides);
    }

    // Stdout chain. Stderr (not stdout) so logs don't pollute commands that pipe stdout.
    // Same behavior the previous plugin had.
    // Resolved once, not per record: a process's stderr doesn't turn into a terminal
    // halfway through a run, and `is_terminal()` is a syscall.
    let color = super::target_style::color_enabled();
    let mut stdout_chain = fern::Dispatch::new()
        .format(move |out, message, record| write_terminal_line(out, message, record, color))
        // Ceiling: the live AtomicU8 filter below does the real gating.
        .level(log::LevelFilter::Trace)
        // Live verbose-toggle gate: drops anything below the current AtomicU8.
        .filter(|metadata| metadata.level() <= stdout_threshold())
        // Stderr writes can fail with `BrokenPipe` (parent process closed the
        // inherited fd, or our stdout is piped into a reader that already exited).
        // Without the wrapper, fern's fallback path would re-attempt stderr,
        // also fail, and panic — killing the tokio task that happened to be
        // logging. See `BrokenPipeTolerantWriter` doc for the longer story.
        .chain(Box::new(BrokenPipeTolerantWriter(std::io::stderr())) as Box<dyn Write + Send>);

    // Default noise suppression, applied to the stdout chain only. The file chain keeps
    // these at Debug for local debugging; error reports leave out third-party Debug anyway.
    for (module, level) in default_noise_overrides() {
        stdout_chain = stdout_chain.level_for(module, level);
    }
    // RUST_LOG per-module overrides (also stdout-only).
    for (module, level) in stdout_overrides {
        stdout_chain = stdout_chain.level_for(module, level);
    }

    // Build the root.
    let mut root = root_dispatch().chain(stdout_chain);

    // File chain: always Debug, plain text (no ANSI), rotated by `RotatingFile`.
    if let Some(dir) = opts.log_dir.as_ref()
        && opts.keep_count > 0
    {
        std::fs::create_dir_all(dir).ok();
        let log_path = dir.join("cmdr.log");
        // 50 MB per file: the live file + N-1 rotated copies = N total.
        let rotated_extras = opts.keep_count.saturating_sub(1).max(1);
        let rotator = super::rotating_file::RotatingFile::new(log_path, rotated_extras, 50_000_000);
        // The coalescing writer prepends the ISO timestamp itself (so the dedup key stays
        // timestamp-free) and collapses identical-line floods so a runaway loop can't peg a
        // core through the log file. It needs no interior mutex: fern serializes every
        // record through its own writer mutex. See `coalesce.rs`.
        let writer: Box<dyn Write + Send + 'static> = Box::new(super::coalesce::CoalescingWriter::new(rotator));
        root = root.chain(file_chain(writer));
    }

    root.apply()?;
    // log::set_max_level controls the per-record filter the `log` macros consult before
    // even constructing the record. Set it to Trace so per-chain filters can see
    // everything; if either chain wants to drop, it does so itself.
    log::set_max_level(log::LevelFilter::Trace);
    // One findable session-start anchor in the log file. Every other line carries
    // its own ISO-8601 stamp, so this is just the human-readable header that lets
    // a triager confirm "yes, this file starts with the launch I'm investigating."
    log::info!(
        target: "cmdr_lib::logging",
        "Logger initialized: pid={pid}, app_version={version}",
        pid = std::process::id(),
        version = env!("CARGO_PKG_VERSION"),
    );
    Ok(())
}

/// Crates no line of which reaches any sink, whatever the level or `RUST_LOG` says.
///
/// `rmcp` logs every request it receives at Debug, arguments included, through `tracing`'s `log`
/// fallback. That would put an `unlock` password in `cmdr.log` and in error reports, and the file
/// chain keeps Debug always. `crate::mcp` logs what we want to keep of a request itself.
const NEVER_LOGGED: &[&str] = &["rmcp"];

/// Whether a record's target belongs to a crate in [`NEVER_LOGGED`].
pub(crate) fn never_logged(target: &str) -> bool {
    NEVER_LOGGED.iter().any(|name| {
        target
            .strip_prefix(name)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with("::"))
    })
}

/// The root of the tree: a pure ceiling plus [`NEVER_LOGGED`], applied above both chains so neither
/// `RUST_LOG` nor a chain's own level can bring those records back. The test dispatch builds it too.
fn root_dispatch() -> fern::Dispatch {
    fern::Dispatch::new()
        .level(log::LevelFilter::Trace)
        .filter(|metadata| !never_logged(metadata.target()))
}

/// The file chain: Debug and up, plain text, into `writer`. The test dispatch builds it too, so
/// the tests assert the line shape and levels that actually ship.
///
/// `mdns_sd` is the one crate held at Info here: its debug output was ~60% of a bundle's lines
/// and of the log's disk use, and an error report never ships it anyway (`provenance.rs`).
fn file_chain(writer: Box<dyn Write + Send>) -> fern::Dispatch {
    fern::Dispatch::new()
        .format(write_file_line)
        .level(log::LevelFilter::Debug)
        .level_for("mdns_sd", log::LevelFilter::Info)
        .chain(writer)
}

/// Renders one file line: `LEVEL [·]target  message`, the `·` marking our own code
/// (`provenance.rs`). No timestamp here: the coalescing writer adds it at emit time, keeping
/// the dedup key free of the ever-changing stamp.
fn write_file_line(out: fern::FormatCallback<'_>, message: &std::fmt::Arguments<'_>, record: &log::Record<'_>) {
    let target = record.target().strip_prefix("cmdr_lib::").unwrap_or(record.target());
    let level = record.level();
    let own = super::provenance::line_tag(record.module_path());
    // `ram` is empty unless `CMDR_LOG_RAM_USE` is set. When on, its value varies per line and
    // so becomes part of the dedup key; that's an accepted debug-mode tradeoff (see
    // `ram_gauge` § "Interaction with file-log dedup").
    let ram = super::ram_gauge::tag();
    let message = message_text(message);
    out.finish(format_args!("{level:<5} {ram}{own}{target}  {message}"));
}

/// A record's text, with every S3 app path's access key id taken out
/// (`cmdr_fs::volume::without_s3_key_ids`, #706). Both chains write through here, so no log
/// site, ours or the frontend's, marked or not, can put a key id in a line.
fn message_text<'a>(message: &std::fmt::Arguments<'a>) -> std::borrow::Cow<'a, str> {
    use std::borrow::Cow;
    match message.as_str() {
        Some(text) => cmdr_fs::volume::without_s3_key_ids(text),
        None => {
            let text = message.to_string();
            match cmdr_fs::volume::without_s3_key_ids(&text) {
                Cow::Borrowed(_) => Cow::Owned(text),
                Cow::Owned(shown) => Cow::Owned(shown),
            }
        }
    }
}

/// Renders one terminal line: `HH:MM:SS.mmm LEVEL head::tail  message`.
///
/// The head (the target up to its first `::`) sits in a fixed-width column in its own
/// stable color and the tail trails it in gray, so a dev scanning `pnpm dev` output finds
/// the subsystem without reading. See `target_style`. With `color` false every sequence
/// is empty and the line is plain text, alignment intact.
///
/// The test dispatch calls this too, so the tests assert the format that actually ships.
fn write_terminal_line(
    out: fern::FormatCallback<'_>,
    message: &std::fmt::Arguments<'_>,
    record: &log::Record<'_>,
    color: bool,
) {
    let ts = chrono::Local::now().format("%H:%M:%S%.3f");
    let target = record.target().strip_prefix("cmdr_lib::").unwrap_or(record.target());
    let (head, tail) = super::target_style::split(target);
    let level = record.level();
    let (level_color, head_color, reset) = if color {
        (
            level_color(level),
            super::target_style::head_color(head),
            super::target_style::RESET,
        )
    } else {
        ("", "", "")
    };
    // Empty for a bare head, so a headless line doesn't carry a color sequence wrapping
    // nothing. The single trailing reset closes whichever color is still open.
    let tail_color = if color && !tail.is_empty() {
        super::target_style::TAIL_COLOR
    } else {
        ""
    };
    let ram = super::ram_gauge::tag();
    // A marked value reads as itself here, markers gone (`redact::display_line`); the file
    // keeps them for the error reporter.
    let text = message_text(message);
    let message = crate::redact::display_line(&text);
    out.finish(format_args!(
        "{ts} {level_color}{level:<5}{reset} {ram}{head_color}{head:<width$}{tail_color}{tail}{reset}  {message}",
        width = super::target_style::HEAD_WIDTH,
    ));
}

/// The terminal color for a level. Basic ANSI (not the 256-color cube the target column
/// uses) so it lands on whatever red/yellow/blue the dev's theme defines.
fn level_color(level: log::Level) -> &'static str {
    match level {
        log::Level::Error => "\x1b[31m", // red
        log::Level::Warn => "\x1b[33m",  // yellow
        log::Level::Info => "\x1b[34m",  // blue
        log::Level::Debug => "\x1b[36m", // cyan
        log::Level::Trace => "\x1b[35m", // magenta
    }
}

/// Defaults applied to the stdout chain to suppress known-noisy crates. The file chain
/// keeps these at Debug (it inherits the chain's Debug level) for local debugging; an
/// error report ships only our own crates' Debug (`provenance`).
fn default_noise_overrides() -> Vec<(&'static str, log::LevelFilter)> {
    vec![
        ("nusb", log::LevelFilter::Warn),
        ("zbus", log::LevelFilter::Warn),
        ("tracing::span", log::LevelFilter::Warn),
        ("smb2", log::LevelFilter::Warn),
        ("tao", log::LevelFilter::Warn),
        // genai warns `EMPTY CHOICE CONTENT` for any streamed delta carrying neither content,
        // nor reasoning, nor a tool call — which includes the opening `{"role":"assistant"}`
        // chunk of EVERY response, so a normal turn prints several. Nothing is wrong; upstream
        // marks the line `// TODO: use tracing debug` itself. Error rather than Warn because
        // the noise IS the warn; the module's only other line is a real stream failure at
        // Error, and that one still comes through. (genai 0.6.5,
        // `adapter/adapters/openai/streamer.rs`, read 2026-09-06.)
        ("genai::adapter::adapters::openai::streamer", log::LevelFilter::Error),
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

/// `Write` adapter that silently absorbs `BrokenPipe` errors so the logger never
/// panics when stderr is closed under us.
///
/// **Why**: fern's chain target receives a `Box<dyn Write + Send>`. On every record
/// fern calls `write_all(...)`; if that returns `Err`, fern falls back to a direct
/// `std::io::stderr()` write, and if THAT also fails fern calls `panic!`. When the
/// process inherited stderr from a parent that has since exited (Cmdr launched from
/// Terminal, terminal window closed) or from a pipe whose reader hung up
/// (`Cmdr 2>&1 | head -n 1`), every subsequent log call would tear down the calling
/// task — visible to the user as a "task panicked" dialog.
///
/// **What**: any `BrokenPipe` (`ErrorKind::BrokenPipe`) on `write` or `flush` is
/// reported as success (consuming the whole buffer). Other errors propagate
/// unchanged — full-disk, permission-denied, etc. shouldn't be silently swallowed,
/// because they'd hide real bugs in any writer this wrapper might end up wrapping in
/// future. The file chain is independent and keeps working when stderr is dead; the
/// dropped stderr lines are intentional (no one is reading them anyway).
struct BrokenPipeTolerantWriter<W: Write>(W);

impl<W: Write> Write for BrokenPipeTolerantWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self.0.write(buf) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(buf.len()),
            Err(e) => Err(e),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self.0.flush() {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
