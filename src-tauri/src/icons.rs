//! Icon retrieval and caching for file types.
//!
//! Parallelism: Uses rayon's global thread pool (auto-detects CPU cores).
//! Benchmarked on M1 Mac: 10 files→3.7ms, 50→8ms, 100→12.8ms, 200→21ms.
//! Custom thread counts showed no improvement, so we use auto-detect.

use crate::config::ICON_SIZE;
use base64::Engine;
use file_icon_provider::get_file_icon;
use image::{DynamicImage, ImageFormat, imageops::FilterType};
use rayon::prelude::*;
use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Cache for generated icons (icon_id -> base64 WebP data URL)
static ICON_CACHE: RwLock<Option<HashMap<String, String>>> = RwLock::new(None);

/// Initializes the icon cache if not already done.
fn ensure_cache() {
    let cache = ICON_CACHE.read().unwrap();
    if cache.is_some() {
        return;
    }
    drop(cache);
    let mut cache = ICON_CACHE.write().unwrap();
    if cache.is_none() {
        *cache = Some(HashMap::new());
    }
}

/// Gets cached icon data URL for the given icon ID, if available.
fn get_cached_icon(icon_id: &str) -> Option<String> {
    ensure_cache();
    let cache = ICON_CACHE.read().unwrap();
    cache.as_ref()?.get(icon_id).cloned()
}

/// Caches an icon data URL.
fn cache_icon(icon_id: String, data_url: String) {
    ensure_cache();
    let mut cache = ICON_CACHE.write().unwrap();
    if let Some(ref mut map) = *cache {
        map.insert(icon_id, data_url);
    }
}

/// Converts an image to a base64 WebP data URL.
fn image_to_data_url(img: &DynamicImage) -> Option<String> {
    // Resize to configured size
    let resized = img.resize_exact(ICON_SIZE, ICON_SIZE, FilterType::Lanczos3);

    // Encode as WebP
    let mut buffer = Cursor::new(Vec::new());
    resized.write_to(&mut buffer, ImageFormat::WebP).ok()?;

    // Convert to base64 data URL
    let base64 = base64::engine::general_purpose::STANDARD.encode(buffer.into_inner());
    Some(format!("data:image/webp;base64,{}", base64))
}

/// Fetches icon for a specific file path.
fn fetch_icon_for_path(path: &Path) -> Option<String> {
    // Get icon from OS (size is u16)
    let icon = get_file_icon(path, ICON_SIZE as u16).ok()?;

    // file_icon_provider returns Icon with width, height, and RGBA pixels
    let img = image::RgbaImage::from_raw(icon.width, icon.height, icon.pixels)?;
    let dynamic_img = DynamicImage::ImageRgba8(img);

    image_to_data_url(&dynamic_img)
}

/// Gets the sample file path to use for fetching an icon by ID.
/// For extension-based icons, we create an actual temp file since the OS may need it to exist.
fn get_sample_path_for_icon_id(icon_id: &str) -> Option<PathBuf> {
    if icon_id == "dir" || icon_id == "symlink-dir" {
        // Use home directory as sample directory (symlinks to dirs get folder icon)
        return dirs::home_dir();
    }
    if icon_id == "symlink-file" || icon_id == "symlink" || icon_id == "file" {
        // Generic file icon - use /etc/hosts which exists on all macOS systems
        return Some(PathBuf::from("/etc/hosts"));
    }
    if let Some(ext) = icon_id.strip_prefix("ext:") {
        // Create an actual temp file with the extension
        // macOS Launch Services needs the file to exist to get the correct icon
        let temp_path = std::env::temp_dir().join(format!("rusty_commander_icon_sample.{}", ext));
        // Create the file if it doesn't exist (empty file is fine)
        if !temp_path.exists() {
            let _ = std::fs::File::create(&temp_path);
        }
        return Some(temp_path);
    }
    None
}

/// Fetches icons for the given icon IDs that are not already cached.
/// Returns a map of icon_id -> data URL.
pub fn get_icons(icon_ids: Vec<String>) -> HashMap<String, String> {
    let mut result = HashMap::new();

    for icon_id in icon_ids {
        // Check cache first
        if let Some(cached) = get_cached_icon(&icon_id) {
            result.insert(icon_id, cached);
            continue;
        }

        // Not cached, fetch it
        if let Some(sample_path) = get_sample_path_for_icon_id(&icon_id)
            && let Some(data_url) = fetch_icon_for_path(&sample_path)
        {
            cache_icon(icon_id.clone(), data_url.clone());
            result.insert(icon_id, data_url);
        }
    }

    result
}

/// Refreshes icons for a directory listing.
/// Fetches icons in parallel for:
/// 1. All unique extensions (checking for file association changes)
/// 2. All directory paths (for custom folder icons)
///
/// Returns only the icons that were successfully fetched, regardless of cache state.
/// This allows the frontend to detect changes by comparing with its cached icons.
pub fn refresh_icons_for_directory(directory_paths: Vec<String>, extensions: Vec<String>) -> HashMap<String, String> {
    let mut result = HashMap::new();

    // Fetch extension icons in parallel (uses rayon's global pool)
    if !extensions.is_empty() {
        let ext_results: Vec<(String, Option<String>)> = extensions
            .par_iter()
            .map(|ext| {
                let icon_id = format!("ext:{}", ext.to_lowercase());
                let sample_path = std::env::temp_dir().join(format!("rusty_commander_icon_sample.{}", ext));
                // Create the file if it doesn't exist
                if !sample_path.exists() {
                    let _ = std::fs::File::create(&sample_path);
                }
                let data_url = fetch_icon_for_path(&sample_path);
                (icon_id, data_url)
            })
            .collect();

        for (icon_id, data_url) in ext_results {
            if let Some(url) = data_url {
                cache_icon(icon_id.clone(), url.clone());
                result.insert(icon_id, url);
            }
        }
    }

    // Fetch directory icons by exact path in parallel
    if !directory_paths.is_empty() {
        let dir_results: Vec<(String, Option<String>)> = directory_paths
            .par_iter()
            .map(|path| {
                let path_buf = PathBuf::from(path);
                let data_url = fetch_icon_for_path(&path_buf);
                // Use path as the icon ID for directories
                (format!("path:{}", path), data_url)
            })
            .collect();

        for (icon_id, data_url) in dir_results {
            if let Some(url) = data_url {
                // Update cache
                cache_icon(icon_id.clone(), url.clone());
                result.insert(icon_id, url);
            }
        }
    }

    result
}
