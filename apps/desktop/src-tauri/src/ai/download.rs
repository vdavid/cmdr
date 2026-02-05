//! AI model download utilities with progress reporting and resume support.

use super::extract::LLAMA_SERVER_BINARY;
use super::{DownloadProgress, ModelInfo};
use std::fs;
use std::io::Write;
use std::path::Path;
use tauri::{AppHandle, Emitter, Runtime};

/// Downloads the AI model with progress reporting and resume support.
///
/// The `is_cancelled` parameter is a function that checks if the download should be cancelled.
/// This allows the caller (manager.rs) to control cancellation via its internal state.
pub async fn download_file<R: Runtime, F>(
    app: &AppHandle<R>,
    url: &str,
    dest: &Path,
    is_cancelled: F,
) -> Result<(), String>
where
    F: Fn() -> bool,
{
    use futures_util::StreamExt;

    let client = reqwest::Client::new();

    // Check for resume (existing partial file)
    let existing_size = dest.metadata().map(|m| m.len()).unwrap_or(0);
    if existing_size > 0 {
        log::debug!("AI download: resuming from {} bytes", existing_size);
    }

    let mut request = client.get(url);
    if existing_size > 0 {
        request = request.header("Range", format!("bytes={existing_size}-"));
    }

    let response = request.send().await.map_err(|e| format!("Download failed: {e}"))?;

    if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(format!("Download failed: HTTP {}", response.status()));
    }

    let total_bytes = response.content_length().map(|cl| cl + existing_size).unwrap_or(0);

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dest)
        .map_err(|e| format!("Failed to open file: {e}"))?;

    let mut stream = response.bytes_stream();
    let mut downloaded = existing_size;
    let start_time = std::time::Instant::now();
    let mut last_emit = std::time::Instant::now();

    while let Some(chunk) = stream.next().await {
        // Check cancel
        if is_cancelled() {
            return Err(String::from("Download cancelled"));
        }

        let chunk = chunk.map_err(|e| format!("Download error: {e}"))?;
        file.write_all(&chunk).map_err(|e| format!("Write error: {e}"))?;
        downloaded += chunk.len() as u64;

        // Emit progress at most every 200ms
        if last_emit.elapsed() >= std::time::Duration::from_millis(200) {
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                ((downloaded - existing_size) as f64 / elapsed) as u64
            } else {
                0
            };
            let eta_seconds = if speed > 0 {
                (total_bytes.saturating_sub(downloaded)) / speed
            } else {
                0
            };

            let progress = DownloadProgress {
                bytes_downloaded: downloaded,
                total_bytes,
                speed,
                eta_seconds,
            };
            let _ = app.emit("ai-download-progress", &progress);
            last_emit = std::time::Instant::now();
        }
    }

    // Final progress emit
    let _ = app.emit(
        "ai-download-progress",
        &DownloadProgress {
            bytes_downloaded: downloaded,
            total_bytes: downloaded,
            speed: 0,
            eta_seconds: 0,
        },
    );

    Ok(())
}

/// Cleans up partial download files (binary and model).
pub fn cleanup_partial(ai_dir: &Path, model: &ModelInfo) {
    let _ = fs::remove_file(ai_dir.join(LLAMA_SERVER_BINARY));
    let _ = fs::remove_file(ai_dir.join(model.filename));
    // Also remove any dylibs that were extracted
    if let Ok(entries) = fs::read_dir(ai_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|ext| ext == "dylib") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}
