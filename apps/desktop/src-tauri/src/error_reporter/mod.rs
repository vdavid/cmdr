//! Error reporter: builds a privacy-redacted zip bundle of recent logs + manifest and
//! (optionally) uploads it to the ingestion server. Used by the user-initiated flow in
//! Phase 4 and, later, by the auto-send flow in Phase 5.
//!
//! The bundle layout is:
//!
//! ```text
//! manifest.json
//! logs/<filename>          # one entry per recent log file, redacted line-by-line
//! logs/<next-filename>
//! ...
//! ```
//!
//! Every log line passes through [`crate::redact::redact_line`] before it hits the zip.
//! The manifest mirrors `ActiveSettings` from the crash reporter so triage is consistent
//! across the two report types.

#[cfg(debug_assertions)]
use crate::config;
use crate::crash_reporter::ActiveSettings;
use crate::logging;
use crate::redact;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
#[cfg(debug_assertions)]
use std::path::PathBuf;
use std::sync::OnceLock;
use zip::ZipArchive;
use zip::write::{SimpleFileOptions, ZipWriter};

#[cfg(test)]
mod tests;

pub mod auto_dispatcher;

#[cfg(test)]
mod auto_dispatcher_tests;

/// Log an error and (if Flow B is opted in) feed it to the auto-dispatcher.
///
/// Drop-in replacement for [`log::error!`] at user-visible failure sites — anything that
/// already produces a user-facing toast or that an end user would consider "this didn't
/// work." Don't migrate noisy library-level errors (`smb2`, `nusb`, etc.); the goal is
/// signal, not coverage.
///
/// The macro evaluates its arguments exactly once. The `format!()` happens whether or not
/// the auto-dispatcher is enabled — same cost as the underlying `log::error!`. The
/// dispatcher's hot path bails out on a single atomic load when the opt-in flag is off.
///
/// ```ignore
/// use cmdr_lib::log_error;
/// log_error!("couldn't mount SMB share at {}: {}", host, err);
/// log_error!(target: "cmdr_lib::network", "DNS lookup failed: {err}");
/// ```
#[macro_export]
macro_rules! log_error {
    (target: $target:expr, $($arg:tt)+) => {{
        let __msg = format!($($arg)+);
        log::error!(target: $target, "{}", __msg);
        $crate::error_reporter::auto_dispatcher::on_error_logged($target, &__msg);
    }};
    ($($arg:tt)+) => {{
        let __msg = format!($($arg)+);
        log::error!("{}", __msg);
        $crate::error_reporter::auto_dispatcher::on_error_logged(module_path!(), &__msg);
    }};
}

/// Same unambiguous alphabet the server uses for license short codes and error report IDs.
/// Kept in sync with `apps/api-server/src/license.ts` — avoids `0`/`O`, `1`/`I`/`L`.
const SHORT_ID_ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
const SHORT_ID_LEN: usize = 5;
const SHORT_ID_PREFIX: &str = "ERR";

/// Max lines in the first-lines preview sample shown in the dialog.
const SAMPLE_FIRST_LINES: usize = 5;
/// Max lines in the last-lines preview sample.
const SAMPLE_LAST_LINES: usize = 20;

/// Flavor of the bundle — kept separate so Phase 5's auto-sender can share the same builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BundleKind {
    User,
    Auto,
}

/// Metadata written into `manifest.json` at the root of the bundle.
/// Mirrors the shape expected by `apps/api-server/src/error-report.ts`'s `ErrorReportMeta`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleManifest {
    pub id: String,
    pub kind: BundleKind,
    pub app_version: String,
    pub os_version: String,
    pub arch: String,
    pub active_settings: ActiveSettings,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_note: Option<String>,
    pub generated_at: String,
}

/// In-memory bundle ready to upload (or save to disk in dev).
#[derive(Debug, Clone)]
pub struct BuiltBundle {
    pub id: String,
    pub zip_bytes: Vec<u8>,
    pub manifest: BundleManifest,
    pub total_redacted_lines: usize,
    pub sample_first: Vec<String>,
    pub sample_last: Vec<String>,
}

/// Server response shape from `POST /error-report`.
#[derive(Debug, Clone, Deserialize)]
pub struct UploadResult {
    pub id: String,
}

/// Build an error report bundle in memory. No network. No disk writes (except reading logs).
///
/// `user_note` is trimmed and dropped if empty. Callers are expected to cap its length
/// (the commands layer enforces 100 000 chars) — we store it verbatim.
pub fn build_bundle<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    kind: BundleKind,
    user_note: Option<String>,
) -> Result<BuiltBundle, String> {
    let id = generate_short_id();

    // Read recent log files into a BTreeMap so the zip order is deterministic — same
    // inputs, same bytes out. This matters for the preview hash and for byte-level tests.
    let mut redacted_files: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut total_redacted_lines: usize = 0;
    let mut live_file_name: Option<String> = None;

    if let Some(dir) = logging::log_dir() {
        let files = logging::list_recent_log_files(dir);
        if let Some(first) = files.first()
            && let Some(name) = first.file_name().and_then(|n| n.to_str())
        {
            live_file_name = Some(name.to_string());
        }
        for path in files {
            let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Ok(file) = std::fs::File::open(&path) else {
                log::warn!(
                    target: "cmdr_lib::error_reporter",
                    "Skipping log file {} (couldn't open)",
                    path.display()
                );
                continue;
            };
            let reader = BufReader::new(file);
            let mut redacted_lines: Vec<String> = Vec::new();
            for line in reader.lines().map_while(Result::ok) {
                redacted_lines.push(redact::redact_line(&line).into_owned());
            }
            total_redacted_lines += redacted_lines.len();
            redacted_files.insert(file_name.to_string(), redacted_lines);
        }
    }

    // Derive samples from the most recent log file (the live one in normal operation).
    let (sample_first, sample_last) = match live_file_name.as_ref().and_then(|name| redacted_files.get(name)) {
        Some(lines) => {
            let first: Vec<String> = lines.iter().take(SAMPLE_FIRST_LINES).cloned().collect();
            let start = lines.len().saturating_sub(SAMPLE_LAST_LINES);
            let last: Vec<String> = lines[start..].to_vec();
            (first, last)
        }
        None => (Vec::new(), Vec::new()),
    };

    let manifest = BundleManifest {
        id: id.clone(),
        kind,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os_version: get_os_version(),
        arch: std::env::consts::ARCH.to_string(),
        active_settings: cached_active_settings(app).clone(),
        user_note: user_note.and_then(|n| {
            let trimmed = n.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }),
        generated_at: chrono::Utc::now().to_rfc3339(),
    };

    let zip_bytes = build_zip(&manifest, &redacted_files).map_err(|e| format!("build zip: {e}"))?;

    Ok(BuiltBundle {
        id,
        zip_bytes,
        manifest,
        total_redacted_lines,
        sample_first,
        sample_last,
    })
}

/// Build the zip archive from a manifest and a set of already-redacted log files.
///
/// Split out from [`build_bundle`] so tests can feed synthetic inputs without needing
/// a Tauri app handle or a real log directory.
fn build_zip(manifest: &BundleManifest, redacted_files: &BTreeMap<String, Vec<String>>) -> std::io::Result<Vec<u8>> {
    let mut buf: Vec<u8> = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        let mut writer = ZipWriter::new(cursor);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        let manifest_json =
            serde_json::to_string_pretty(manifest).map_err(|e| std::io::Error::other(format!("manifest: {e}")))?;
        writer.start_file("manifest.json", opts)?;
        writer.write_all(manifest_json.as_bytes())?;

        for (file_name, lines) in redacted_files {
            writer.start_file(format!("logs/{file_name}"), opts)?;
            for line in lines {
                writer.write_all(line.as_bytes())?;
                writer.write_all(b"\n")?;
            }
        }

        writer.finish()?;
    }
    Ok(buf)
}

/// Generate a short ID like `ERR-8F3A2` using rejection sampling (no modulo bias).
pub fn generate_short_id() -> String {
    let mut rng = rand::rng();
    let alphabet_len = SHORT_ID_ALPHABET.len(); // 31
    // 256 - (256 % 31) = 232 — bytes at or above this would skew the distribution.
    let max_unbiased = 256 - (256 % alphabet_len);
    let mut out = String::with_capacity(SHORT_ID_PREFIX.len() + 1 + SHORT_ID_LEN);
    out.push_str(SHORT_ID_PREFIX);
    out.push('-');
    let mut remaining = SHORT_ID_LEN;
    while remaining > 0 {
        let byte: u8 = rng.random();
        if (byte as usize) < max_unbiased {
            out.push(SHORT_ID_ALPHABET[(byte as usize) % alphabet_len] as char);
            remaining -= 1;
        }
    }
    out
}

/// POST the bundle to the ingestion server. In dev/CI this skips the network call and
/// synthesizes a response using the locally generated ID.
pub async fn upload(zip_bytes: Vec<u8>, manifest: &BundleManifest, server_url: &str) -> Result<UploadResult, String> {
    let should_skip = cfg!(debug_assertions) || std::env::var("CI").is_ok();
    if should_skip {
        log::info!(
            target: "cmdr_lib::error_reporter",
            "Skipping error report upload (dev mode or CI). Local ID: {}",
            manifest.id,
        );
        return Ok(UploadResult {
            id: manifest.id.clone(),
        });
    }

    let meta_json = serde_json::to_string(manifest).map_err(|e| format!("serialize manifest: {e}"))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client: {e}"))?;

    let form = reqwest::multipart::Form::new()
        .part(
            "bundle",
            reqwest::multipart::Part::bytes(zip_bytes)
                .file_name(format!("{}.zip", manifest.id))
                .mime_str("application/zip")
                .map_err(|e| format!("bundle part: {e}"))?,
        )
        .text("meta", meta_json);

    let response = client
        .post(server_url)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("upload request: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("server returned {}", response.status()));
    }

    response
        .json::<UploadResult>()
        .await
        .map_err(|e| format!("parse upload response: {e}"))
}

/// Cap the bundle to `cap_mb` megabytes, keeping the most recent log content.
///
/// If the zip is already under the cap, returns it untouched (zero allocation beyond
/// the `Vec` it was handed). Otherwise, rebuilds the zip: the manifest plus as many
/// `logs/*` entries (newest first, by in-zip order) as fit under the cap.
///
/// In-zip ordering: the builder uses a `BTreeMap` keyed by filename, so the live log
/// (`cmdr.log`) comes before rotated siblings (`cmdr.log.2025-...`) because `.` sorts
/// before any digit. That means iterating entry-index ascending gives us newest-first
/// for the log files themselves, which is what we want.
pub fn cap_bundle_to_mb(zip_bytes: Vec<u8>, cap_mb: usize) -> Vec<u8> {
    let cap_bytes = cap_mb * 1024 * 1024;
    if zip_bytes.len() <= cap_bytes {
        return zip_bytes;
    }

    // Re-open the archive, copy entries into a new one until we hit the cap.
    let Ok(mut archive) = ZipArchive::new(std::io::Cursor::new(&zip_bytes)) else {
        return zip_bytes;
    };

    let mut out_buf: Vec<u8> = Vec::with_capacity(cap_bytes);
    // Track approximate uncompressed bytes written so far so we can stop before the
    // bundle exceeds the cap. We can't read `out_buf.len()` while the `ZipWriter` is
    // mutably borrowing it.
    let mut written_estimate: usize = 0;
    let target = (cap_bytes * 9) / 10; // leave headroom for the central directory
    {
        let cursor = std::io::Cursor::new(&mut out_buf);
        let mut writer = ZipWriter::new(cursor);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        // First pass: always keep manifest.json.
        if let Ok(mut entry) = archive.by_name("manifest.json") {
            let mut data = Vec::new();
            if entry.read_to_end(&mut data).is_ok()
                && writer.start_file("manifest.json", opts).is_ok()
                && writer.write_all(&data).is_ok()
            {
                written_estimate += data.len();
            }
        }

        // Second pass: iterate the remaining entries in order, stop when we'd exceed cap.
        for i in 0..archive.len() {
            let Ok(mut entry) = archive.by_index(i) else { continue };
            let name = entry.name().to_string();
            if name == "manifest.json" {
                continue;
            }
            let mut data = Vec::new();
            if entry.read_to_end(&mut data).is_err() {
                continue;
            }
            if written_estimate + data.len() + 512 > target {
                break;
            }
            if writer.start_file(&name, opts).is_err() {
                break;
            }
            if writer.write_all(&data).is_err() {
                break;
            }
            written_estimate += data.len();
        }

        if writer.finish().is_err() {
            return zip_bytes; // Better to send the uncapped bundle than nothing.
        }
    }

    if out_buf.len() > zip_bytes.len() {
        // Defensive: capping somehow produced a larger zip. Fall back to the original.
        return zip_bytes;
    }
    out_buf
}

/// Write the built bundle to the app data dir as `error-report-debug-<timestamp>.zip`.
/// Gated on `debug_assertions` by the caller (see `commands/error_reporter.rs`).
#[cfg(debug_assertions)]
pub fn save_bundle_to_disk<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    bundle: &BuiltBundle,
) -> Result<PathBuf, String> {
    let dir = config::resolved_app_data_dir(app)?;
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let path = dir.join(format!("error-report-debug-{timestamp}.zip"));
    std::fs::write(&path, &bundle.zip_bytes).map_err(|e| format!("write debug bundle: {e}"))?;
    Ok(path)
}

// --- Helpers ---

/// Cached snapshot of active settings. Populated lazily from the settings loader the
/// first time a bundle is built, then reused. Mirrors the crash reporter's cache but
/// stays local to this module so we don't depend on init ordering.
fn cached_active_settings<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> &'static ActiveSettings {
    static CACHE: OnceLock<ActiveSettings> = OnceLock::new();
    CACHE.get_or_init(|| {
        let s = crate::settings::load_settings(app);
        ActiveSettings {
            indexing_enabled: s.indexing_enabled,
            ai_provider: s.ai_provider,
            mcp_enabled: s.developer_mcp_enabled,
            verbose_logging: s.verbose_logging,
        }
    })
}

fn get_os_version() -> String {
    #[cfg(target_os = "macos")]
    {
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
