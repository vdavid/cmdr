//! Windows: no icon provider yet (`SHGetFileInfoW` / `IShellItemImageFactory` is the one to add),
//! so every lookup answers "no icon" and the frontend keeps its fallback glyphs.

use std::path::Path;

use image::DynamicImage;

use super::PathIcons;

/// No icon provider yet.
pub(crate) fn fetch_icon_for_path(_path: &Path) -> Option<String> {
    None
}

/// No icon provider yet, so no work is spawned either.
pub(crate) fn fetch_path_icons(paths: Vec<String>) -> PathIcons {
    paths.into_iter().map(|path| (format!("path:{}", path), None)).collect()
}

/// No direct lookup by id.
pub(crate) fn icon_for_id(_icon_id: &str, _use_app_icons_for_documents: bool) -> Option<String> {
    None
}

/// No bundle to read past a cache.
pub(crate) fn fresh_extension_icon(_ext: &str, _use_app_icons_for_documents: bool) -> Option<DynamicImage> {
    None
}

/// No object pool to drain.
pub(crate) fn with_object_pool<T, F: FnOnce() -> T>(f: F) -> T {
    f()
}

/// No Finder tags.
pub(crate) fn tag_tinting_disabled() -> bool {
    false
}
