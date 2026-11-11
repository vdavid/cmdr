//! Linux: the XDG icon theme (`crate::linux_icons`), asked by icon id. A GTK lookup would want
//! the main thread and fails silently from a rayon or tokio worker, so there's none.

use std::path::Path;

use image::DynamicImage;
use rayon::prelude::*;

use super::super::image_to_data_url;
use super::PathIcons;
pub(crate) use super::unix::extended_attribute;
use crate::config::ICON_SIZE;

/// Fetches icon for a specific file path via XDG icon theme lookup.
pub(crate) fn fetch_icon_for_path(path: &Path) -> Option<String> {
    let icon_id = if path.is_dir() {
        "dir".to_string()
    } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        format!("ext:{}", ext.to_lowercase())
    } else {
        "file".to_string()
    };
    crate::linux_icons::get_icon_for_id(&icon_id, ICON_SIZE as u16).and_then(|img| image_to_data_url(&img))
}

/// The XDG theme lookup makes no XPC calls and can't descend into a cloud provider, so rayon's
/// pool is fine here.
pub(crate) fn fetch_path_icons(paths: Vec<String>) -> PathIcons {
    paths
        .par_iter()
        .map(|path| {
            let data_url = fetch_icon_for_path(Path::new(path));
            (format!("path:{}", path), data_url)
        })
        .collect()
}

/// Looks the id up directly in the XDG icon theme (no temp files needed).
pub(crate) fn icon_for_id(icon_id: &str, _use_app_icons_for_documents: bool) -> Option<String> {
    crate::linux_icons::get_icon_for_id(icon_id, ICON_SIZE as u16).and_then(|img| image_to_data_url(&img))
}

/// No bundle to read past a cache.
pub(crate) fn fresh_extension_icon(_ext: &str, _use_app_icons_for_documents: bool) -> Option<DynamicImage> {
    None
}

/// No autorelease pool to drain.
pub(crate) fn with_object_pool<T, F: FnOnce() -> T>(f: F) -> T {
    f()
}

/// Linux has no Finder tags, so this is never reached with a colored one.
pub(crate) fn tag_tinting_disabled() -> bool {
    false
}
