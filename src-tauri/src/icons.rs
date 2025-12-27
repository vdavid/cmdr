//! Icon retrieval and caching for file types.

use base64::Engine;
use file_icon_provider::get_file_icon;
use image::{DynamicImage, ImageFormat, imageops::FilterType};
use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;
use std::sync::RwLock;

/// Icon size in pixels (32x32 for retina display)
const ICON_SIZE: u32 = 32;

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
    // Resize to 32x32
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

/// Generates icon ID based on file properties.
/// This is called during list_directory.
pub fn generate_icon_id(is_dir: bool, is_symlink: bool, extension: Option<&str>) -> String {
    if is_symlink {
        return "symlink".to_string();
    }
    if is_dir {
        return "dir".to_string();
    }
    match extension {
        Some(ext) => format!("ext:{}", ext.to_lowercase()),
        None => "file".to_string(),
    }
}

/// Gets the sample file path to use for fetching an icon by ID.
/// For extension-based icons, we create a temp file with that extension.
fn get_sample_path_for_icon_id(icon_id: &str) -> Option<std::path::PathBuf> {
    if icon_id == "dir" {
        // Use home directory as sample directory
        return dirs::home_dir();
    }
    if icon_id == "symlink" {
        // Symlinks use a generic file icon
        return Some(std::path::PathBuf::from("/tmp"));
    }
    if icon_id == "file" {
        // Generic file with no extension
        return Some(std::path::PathBuf::from("/tmp/file"));
    }
    if let Some(ext) = icon_id.strip_prefix("ext:") {
        // Create a fake path with the extension to get the right icon
        return Some(std::path::PathBuf::from(format!("/tmp/sample.{}", ext)));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_icon_id_directory() {
        assert_eq!(generate_icon_id(true, false, None), "dir");
    }

    #[test]
    fn test_generate_icon_id_symlink() {
        assert_eq!(generate_icon_id(false, true, Some("txt")), "symlink");
    }

    #[test]
    fn test_generate_icon_id_extension() {
        assert_eq!(generate_icon_id(false, false, Some("PDF")), "ext:pdf");
        assert_eq!(generate_icon_id(false, false, Some("jpg")), "ext:jpg");
    }

    #[test]
    fn test_generate_icon_id_no_extension() {
        assert_eq!(generate_icon_id(false, false, None), "file");
    }
}
