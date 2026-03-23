//! Crash reporter: captures panic and signal crashes to disk for next-launch reporting.
//!
//! Two capture paths handle different crash types:
//! - **Panic hook**: full stdlib access, writes JSON crash file directly
//! - **Signal handler**: async-signal-safe only, writes raw addresses to a pre-opened fd

mod symbolicate;

#[cfg(test)]
mod tests;

use crate::config;
use crate::settings;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

const CRASH_FILE_NAME: &str = "crash-report.json";
const RAW_CRASH_FILE_NAME: &str = "crash-report.raw";
const CRASH_FILE_VERSION: u32 = 1;
/// If the crash file is less than this many seconds old, it's a potential crash loop.
const CRASH_LOOP_THRESHOLD_SECS: u64 = 5;

static APP_START_TIME: OnceLock<Instant> = OnceLock::new();
static CACHED_SETTINGS: OnceLock<ActiveSettings> = OnceLock::new();

/// Active settings snapshot cached at startup for inclusion in crash reports.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveSettings {
    pub indexing_enabled: Option<bool>,
    pub ai_provider: Option<String>,
    pub mcp_enabled: Option<bool>,
    pub verbose_logging: Option<bool>,
}

/// The crash report written to disk (JSON).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReport {
    pub version: u32,
    pub timestamp: String,
    pub signal: Option<String>,
    pub panic_message: Option<String>,
    pub backtrace_frames: Vec<String>,
    pub thread_name: Option<String>,
    pub thread_count: usize,
    pub app_version: String,
    pub os_version: String,
    pub arch: String,
    pub uptime_secs: f64,
    pub active_settings: ActiveSettings,
    /// True if this crash happened less than 5 seconds after the previous launch
    /// (potential crash loop). The frontend uses this to suppress auto-send.
    #[serde(default)]
    pub possible_crash_loop: bool,
}

/// Initializes the crash reporter: panic hook, signal handlers, and settings cache.
/// Call this early in app startup, before anything that might crash.
pub fn init<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    APP_START_TIME.get_or_init(Instant::now);

    let Ok(data_dir) = config::resolved_app_data_dir(app) else {
        log::warn!("Crash reporter: couldn't resolve app data dir, skipping init");
        return;
    };

    let crash_path = data_dir.join(CRASH_FILE_NAME);
    let raw_crash_path = data_dir.join(RAW_CRASH_FILE_NAME);

    // Cache active settings for crash reports, using the same loader as the rest of the app
    cache_active_settings(app);

    // Process any pending crash file from a previous session
    process_pending_crash(&crash_path, &raw_crash_path);

    install_panic_hook(crash_path);

    #[cfg(unix)]
    install_signal_handlers(&raw_crash_path);
}

/// Returns the pending crash report from a previous session, if any.
/// Returns `None` if the file doesn't exist, is corrupt, or can't be parsed.
/// Used by milestone 2 (crash report dialog) to check for pending reports.
pub fn take_pending_crash_report<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<CrashReport> {
    let data_dir = config::resolved_app_data_dir(app).ok()?;
    let crash_path = data_dir.join(CRASH_FILE_NAME);
    read_crash_report(&crash_path)
}

// --- Panic hook ---

fn install_panic_hook(crash_path: PathBuf) {
    let default_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |info| {
        let report = build_panic_report(info);

        if let Err(e) = write_crash_report(&crash_path, &report) {
            // Can't use log here (might be the thing that panicked).
            // Write to stderr via libc::write to be safe even in a broken state.
            #[allow(clippy::print_stderr, reason = "log may be the thing that panicked")]
            {
                eprintln!("Crash reporter: couldn't write crash file: {e}");
            }
        }

        // Call the default hook so the app still aborts normally
        default_hook(info);
    }));
}

fn build_panic_report(info: &std::panic::PanicHookInfo<'_>) -> CrashReport {
    let backtrace = std::backtrace::Backtrace::force_capture();
    let backtrace_frames = parse_backtrace_frames(&backtrace.to_string());

    let message = extract_panic_message(info);
    let sanitized_message = message.map(|m| sanitize_panic_message(&m));

    let thread = std::thread::current();
    let thread_name = thread.name().map(String::from);

    CrashReport {
        version: CRASH_FILE_VERSION,
        timestamp: now_iso8601(),
        signal: Some("panic".to_string()),
        panic_message: sanitized_message,
        backtrace_frames,
        thread_name,
        thread_count: current_thread_count(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os_version: get_os_version(),
        arch: std::env::consts::ARCH.to_string(),
        uptime_secs: uptime_secs(),
        active_settings: CACHED_SETTINGS.get().cloned().unwrap_or_default(),
        possible_crash_loop: false,
    }
}

fn extract_panic_message(info: &std::panic::PanicHookInfo<'_>) -> Option<String> {
    // Try to get the payload as &str or String
    let payload = info.payload();
    if let Some(s) = payload.downcast_ref::<&str>() {
        return Some((*s).to_string());
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return Some(s.clone());
    }
    // Fall back to Display if PanicMessage is available (Rust 1.81+)
    Some(info.to_string())
}

/// Strip file paths from panic messages to prevent PII leaks.
/// Matches Unix paths (/Users/..., /home/..., /tmp/...) and Windows paths (C:\...).
fn sanitize_panic_message(message: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        // Match Unix absolute paths and Windows drive paths.
        // Captures paths like /Users/foo/bar.rs:42:5 or C:\Users\foo\bar.rs
        Regex::new(
            r#"(?x)
            (?:
                /(?:Users|home|tmp|var|private|opt|usr|nix)[/][^\s"':;,)}\]]+
              | [A-Z]:\\[^\s"':;,)}\]]+
            )"#,
        )
        .expect("valid regex")
    });
    re.replace_all(message, "<path>").into_owned()
}

fn parse_backtrace_frames(backtrace_str: &str) -> Vec<String> {
    // Each frame line from std::backtrace looks like:
    //   0: std::backtrace::Backtrace::create
    //   1: cmdr_lib::crash_reporter::build_panic_report
    // We keep the function name part, stripping the frame number prefix.
    backtrace_str
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            // Skip empty lines and lines that are just addresses or "at ..." source locations
            if trimmed.is_empty() || trimmed.starts_with("at ") {
                return None;
            }
            // Strip leading frame number: "  12: some::function" -> "some::function"
            if let Some(idx) = trimmed.find(": ") {
                let prefix = &trimmed[..idx];
                if prefix.trim().chars().all(|c| c.is_ascii_digit()) {
                    return Some(trimmed[idx + 2..].to_string());
                }
            }
            Some(trimmed.to_string())
        })
        .collect()
}

// --- Signal handler (Unix only) ---

#[cfg(unix)]
mod signal_handler {
    use std::os::unix::io::RawFd;
    use std::path::Path;
    use std::sync::atomic::{AtomicI32, Ordering};

    /// Max stack frames to capture in the signal handler.
    const MAX_FRAMES: usize = 256;

    /// Pre-opened fd for writing raw crash data. Set at init, read in signal handler.
    static RAW_FD: AtomicI32 = AtomicI32::new(-1);

    // The raw crash file format (binary):
    //   - 4 bytes: magic "CMCR"
    //   - 4 bytes: version (u32 LE)
    //   - 4 bytes: signal number (i32 LE)
    //   - 4 bytes: frame count (u32 LE)
    //   - N * 8 bytes: instruction pointer addresses (u64 LE)
    //   - 32 bytes: app version (zero-padded ASCII)
    const MAGIC: &[u8; 4] = b"CMCR";
    const VERSION: u32 = 1;
    const APP_VERSION_FIELD_LEN: usize = 32;

    unsafe extern "C" {
        /// macOS/glibc `backtrace()` from execinfo.h — async-signal-safe on macOS.
        fn backtrace(buffer: *mut *mut libc::c_void, size: libc::c_int) -> libc::c_int;
    }

    pub fn install(raw_crash_path: &Path) {
        // Pre-open the fd for the raw crash file. O_WRONLY | O_CREAT | O_TRUNC
        // will be applied at write time by truncating to 0 first.
        let path_cstr = match std::ffi::CString::new(raw_crash_path.as_os_str().as_encoded_bytes()) {
            Ok(c) => c,
            Err(_) => {
                log::warn!("Crash reporter: invalid raw crash path, signal handlers not installed");
                return;
            }
        };

        let fd = unsafe {
            libc::open(
                path_cstr.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
                0o644,
            )
        };
        if fd < 0 {
            log::warn!("Crash reporter: couldn't open raw crash file, signal handlers not installed");
            return;
        }
        RAW_FD.store(fd, Ordering::SeqCst);

        // Register signal handlers for SIGSEGV, SIGBUS, SIGABRT
        for sig in [libc::SIGSEGV, libc::SIGBUS, libc::SIGABRT] {
            unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = signal_handler as *const () as usize;
                action.sa_flags = libc::SA_SIGINFO | libc::SA_RESETHAND;
                libc::sigemptyset(&mut action.sa_mask);
                libc::sigaction(sig, &action, std::ptr::null_mut());
            }
        }
    }

    /// Async-signal-safe signal handler. Only uses write() and _exit().
    extern "C" fn signal_handler(sig: libc::c_int, _info: *mut libc::siginfo_t, _ctx: *mut libc::c_void) {
        let fd = RAW_FD.load(Ordering::SeqCst);
        if fd < 0 {
            unsafe { libc::_exit(128 + sig) };
        }

        // Seek to beginning and truncate
        unsafe {
            libc::lseek(fd, 0, libc::SEEK_SET);
            libc::ftruncate(fd, 0);
        }

        // Capture raw instruction pointer addresses
        let mut frames: [*mut libc::c_void; MAX_FRAMES] = [std::ptr::null_mut(); MAX_FRAMES];
        let frame_count = unsafe { backtrace(frames.as_mut_ptr(), MAX_FRAMES as libc::c_int) };
        let frame_count = if frame_count < 0 { 0 } else { frame_count as u32 };

        // Write header: magic + version + signal + frame_count
        write_bytes(fd, MAGIC);
        write_bytes(fd, &VERSION.to_le_bytes());
        write_bytes(fd, &sig.to_le_bytes());
        write_bytes(fd, &frame_count.to_le_bytes());

        // Write frame addresses as u64 LE
        for frame in frames.iter().take(frame_count as usize) {
            let addr = *frame as u64;
            write_bytes(fd, &addr.to_le_bytes());
        }

        // Write app version (zero-padded to fixed length)
        let version_bytes = env!("CARGO_PKG_VERSION").as_bytes();
        let mut version_buf = [0u8; APP_VERSION_FIELD_LEN];
        let copy_len = version_bytes.len().min(APP_VERSION_FIELD_LEN);
        version_buf[..copy_len].copy_from_slice(&version_bytes[..copy_len]);
        write_bytes(fd, &version_buf);

        // Close and re-raise to get the default behavior (core dump, etc.)
        unsafe {
            libc::close(fd);
            libc::raise(sig);
        }
    }

    /// Async-signal-safe write helper.
    fn write_bytes(fd: RawFd, buf: &[u8]) {
        let mut written = 0;
        while written < buf.len() {
            let n = unsafe { libc::write(fd, buf[written..].as_ptr().cast(), buf.len() - written) };
            if n <= 0 {
                break;
            }
            written += n as usize;
        }
    }

    /// Reads the raw crash file and returns (signal, frame_addresses, app_version).
    /// Returns None if the file doesn't exist or is corrupt.
    pub fn read_raw_crash(path: &Path) -> Option<(i32, Vec<u64>, String)> {
        let data = std::fs::read(path).ok()?;

        // Minimum size: magic(4) + version(4) + signal(4) + frame_count(4) + version_field(32)
        if data.len() < 48 {
            log::info!("Crash reporter: raw crash file too small, discarding");
            let _ = std::fs::remove_file(path);
            return None;
        }

        if &data[0..4] != MAGIC {
            log::info!("Crash reporter: raw crash file bad magic, discarding");
            let _ = std::fs::remove_file(path);
            return None;
        }

        let version = u32::from_le_bytes(data[4..8].try_into().ok()?);
        if version != VERSION {
            log::info!("Crash reporter: raw crash file version mismatch ({version}), discarding");
            let _ = std::fs::remove_file(path);
            return None;
        }

        let signal = i32::from_le_bytes(data[8..12].try_into().ok()?);
        let frame_count = u32::from_le_bytes(data[12..16].try_into().ok()?) as usize;

        let frames_end = 16 + frame_count * 8;
        let expected_len = frames_end + APP_VERSION_FIELD_LEN;
        if data.len() < expected_len {
            log::info!("Crash reporter: raw crash file truncated, discarding");
            let _ = std::fs::remove_file(path);
            return None;
        }

        let mut addresses = Vec::with_capacity(frame_count);
        for i in 0..frame_count {
            let offset = 16 + i * 8;
            let addr = u64::from_le_bytes(data[offset..offset + 8].try_into().ok()?);
            addresses.push(addr);
        }

        let version_slice = &data[frames_end..frames_end + APP_VERSION_FIELD_LEN];
        let app_version = std::str::from_utf8(version_slice)
            .ok()?
            .trim_end_matches('\0')
            .to_string();

        Some((signal, addresses, app_version))
    }
}

#[cfg(unix)]
fn install_signal_handlers(raw_crash_path: &Path) {
    signal_handler::install(raw_crash_path);
}

// --- Crash file I/O ---

fn write_crash_report(path: &Path, report: &CrashReport) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report).map_err(|e| format!("serialize crash report: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("write crash file: {e}"))
}

fn read_crash_report(path: &Path) -> Option<CrashReport> {
    let contents = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<CrashReport>(&contents) {
        Ok(report) if report.version == CRASH_FILE_VERSION => Some(report),
        Ok(report) => {
            log::info!(
                "Crash reporter: crash file version {} != expected {CRASH_FILE_VERSION}, discarding",
                report.version
            );
            let _ = std::fs::remove_file(path);
            None
        }
        Err(e) => {
            log::info!("Crash reporter: corrupt crash file ({e}), discarding");
            let _ = std::fs::remove_file(path);
            None
        }
    }
}

/// Process any pending raw signal crash file from a previous session.
/// Symbolicates if the version matches, then converts to JSON format.
fn process_pending_crash(crash_json_path: &Path, raw_crash_path: &Path) {
    // Check for a JSON crash report with crash loop detection
    if let Some(mut report) = read_crash_report(crash_json_path) {
        if is_crash_loop(&report.timestamp) {
            report.possible_crash_loop = true;
            // Re-write with the flag set
            let _ = write_crash_report(crash_json_path, &report);
        }
        // JSON report exists, leave it for the frontend to handle
        return;
    }

    // Check for a raw signal crash file
    #[cfg(unix)]
    if raw_crash_path.exists() {
        if let Some((signal, addresses, crash_app_version)) = signal_handler::read_raw_crash(raw_crash_path) {
            let current_version = env!("CARGO_PKG_VERSION");
            let versions_match = crash_app_version == current_version;

            let backtrace_frames = if versions_match {
                symbolicate::symbolicate_addresses(&addresses)
            } else {
                log::info!(
                    "Crash reporter: version mismatch (crash={crash_app_version}, \
                     current={current_version}), sending raw addresses"
                );
                addresses.iter().map(|a| format!("0x{a:016x}")).collect()
            };

            let signal_name = signal_name(signal);

            let report = CrashReport {
                version: CRASH_FILE_VERSION,
                timestamp: now_iso8601(),
                signal: Some(signal_name),
                panic_message: None,
                backtrace_frames,
                thread_name: None,
                thread_count: 0,
                app_version: crash_app_version,
                os_version: get_os_version(),
                arch: std::env::consts::ARCH.to_string(),
                uptime_secs: 0.0, // Unknown for signal crashes from previous session
                active_settings: CACHED_SETTINGS.get().cloned().unwrap_or_default(),
                possible_crash_loop: false,
            };

            if let Err(e) = write_crash_report(crash_json_path, &report) {
                log::warn!("Crash reporter: couldn't write symbolicated crash report: {e}");
            }
        }

        let _ = std::fs::remove_file(raw_crash_path);
    }
}

/// Cache active settings for crash reports, using the app's settings loader.
/// This piggybacks on `settings::load_settings` so defaults stay in sync.
/// Fields that are `None` in the settings struct mean "user hasn't changed this" —
/// the frontend registry owns the defaults. We pass through `None` as-is; the crash
/// report consumer can interpret null as "default."
fn cache_active_settings<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let s = settings::load_settings(app);
    let settings = ActiveSettings {
        indexing_enabled: s.indexing_enabled,
        ai_provider: s.ai_provider,
        mcp_enabled: s.developer_mcp_enabled,
        verbose_logging: s.verbose_logging,
    };
    let _ = CACHED_SETTINGS.set(settings);
}

// --- Helpers ---

fn now_iso8601() -> String {
    // Use chrono (already a dependency) for ISO 8601 timestamp
    chrono::Utc::now().to_rfc3339()
}

fn uptime_secs() -> f64 {
    APP_START_TIME.get().map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0)
}

fn current_thread_count() -> usize {
    #[cfg(target_os = "macos")]
    {
        // Mach API: get thread list for the current task, return the count.
        unsafe extern "C" {
            fn mach_task_self() -> libc::mach_port_t;
        }
        unsafe {
            let mut thread_list: libc::mach_port_t = 0;
            let mut thread_count: u32 = 0;
            let kr = libc::task_threads(
                mach_task_self(),
                std::ptr::addr_of_mut!(thread_list) as *mut *mut libc::mach_port_t,
                std::ptr::addr_of_mut!(thread_count) as *mut u32,
            );
            if kr == libc::KERN_SUCCESS {
                // Deallocate the thread list (we only needed the count)
                libc::vm_deallocate(
                    mach_task_self(),
                    thread_list as libc::vm_address_t,
                    (thread_count as usize) * size_of::<libc::mach_port_t>(),
                );
                thread_count as usize
            } else {
                0
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if let Some(count) = line.strip_prefix("Threads:") {
                    return count.trim().parse().unwrap_or(0);
                }
            }
        }
        0
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        0
    }
}

fn get_os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        // Use sw_vers for macOS version
        if let Ok(output) = std::process::Command::new("sw_vers").arg("-productVersion").output() {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !version.is_empty() {
                return format!("macOS {version}");
            }
        }
        "macOS (unknown version)".to_string()
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(release) = std::fs::read_to_string("/etc/os-release") {
            for line in release.lines() {
                if let Some(name) = line.strip_prefix("PRETTY_NAME=") {
                    return name.trim_matches('"').to_string();
                }
            }
        }
        "Linux (unknown distro)".to_string()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        std::env::consts::OS.to_string()
    }
}

#[cfg(unix)]
fn signal_name(sig: i32) -> String {
    match sig {
        libc::SIGSEGV => "SIGSEGV".to_string(),
        libc::SIGBUS => "SIGBUS".to_string(),
        libc::SIGABRT => "SIGABRT".to_string(),
        other => format!("signal {other}"),
    }
}

/// Check if the crash timestamp indicates a crash loop (< 5 seconds before current launch).
fn is_crash_loop(crash_timestamp: &str) -> bool {
    let Ok(crash_time) = chrono::DateTime::parse_from_rfc3339(crash_timestamp) else {
        return false;
    };
    let now = chrono::Utc::now();
    let elapsed = now.signed_duration_since(crash_time);
    elapsed.num_seconds() >= 0 && (elapsed.num_seconds() as u64) < CRASH_LOOP_THRESHOLD_SECS
}
