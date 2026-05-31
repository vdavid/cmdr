//! Icon retrieval and caching for file types.
//!
//! Parallelism: Uses rayon's global thread pool (auto-detects CPU cores).
//! Benchmarked on M1 Mac: 10 files→3.7ms, 50→8ms, 100→12.8ms, 200→21ms.
//! Custom thread counts showed no improvement, so we use auto-detect.

pub mod special_folders;

use crate::config::ICON_SIZE;
use crate::ignore_poison::RwLockIgnorePoison;
use base64::Engine;
use image::{DynamicImage, ImageFormat, imageops::FilterType};
#[cfg(target_os = "macos")]
use objc2::rc::autoreleasepool;
use rayon::prelude::*;
use std::collections::{HashMap, VecDeque};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

// file_icon_provider uses GTK on Linux which requires main-thread access and
// fails silently from rayon/tokio threads. On Linux we use freedesktop-icons instead.
#[cfg(target_os = "macos")]
use file_icon_provider::get_file_icon;

/// Prefix marking per-path (per-folder) icon keys. Unlike `dir` / `ext:*` / `file`
/// (an inherently bounded set), `path:` keys grow with the number of distinct
/// directories visited, so they're capped by an LRU backstop (see `PATH_KEY_CAP`).
const PATH_KEY_PREFIX: &str = "path:";

/// Backstop LRU cap for `path:`-keyed entries. Folder icons are unbounded in the
/// number of directories a user can visit; without a cap, a long session browsing
/// thousands of distinct folders would accumulate one base64 WebP data-URL per
/// folder forever (only cleared wholesale on theme/accent change). A few hundred
/// covers any plausible visible/recent working set; the rest evict oldest-first.
/// `dir` / `ext:*` / `file` / `symlink*` keys stay uncapped (inherently bounded).
const PATH_KEY_CAP: usize = 256;

/// In-memory icon cache plus an LRU recency queue for the `path:` subset.
///
/// `entries` holds every icon (`dir`, `ext:*`, `path:*`, …) keyed by icon id.
/// `path_lru` tracks the insertion/refresh order of *only* the `path:` keys so we
/// can evict the oldest when their count exceeds `PATH_KEY_CAP`. Non-`path:` keys
/// never enter `path_lru` and are never evicted by the cap.
struct IconCache {
    entries: HashMap<String, String>,
    /// `path:` keys in least-recently-inserted-first order. Front = oldest.
    path_lru: VecDeque<String>,
}

impl IconCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            path_lru: VecDeque::new(),
        }
    }

    /// Inserts or refreshes an icon. For `path:` keys, maintains the LRU order and
    /// evicts the oldest `path:` entries once the cap is exceeded.
    fn insert(&mut self, icon_id: String, data_url: String) {
        if icon_id.starts_with(PATH_KEY_PREFIX) {
            // Refresh recency: drop any existing position, then push to the back.
            if self.entries.contains_key(&icon_id) {
                self.path_lru.retain(|k| k != &icon_id);
            }
            self.path_lru.push_back(icon_id.clone());
            self.entries.insert(icon_id, data_url);
            // Evict oldest `path:` entries beyond the cap.
            while self.path_lru.len() > PATH_KEY_CAP {
                if let Some(evicted) = self.path_lru.pop_front() {
                    self.entries.remove(&evicted);
                }
            }
        } else {
            self.entries.insert(icon_id, data_url);
        }
    }

    /// Removes entries (and their LRU bookkeeping) matching `pred`.
    fn retain(&mut self, pred: impl Fn(&str) -> bool) {
        self.entries.retain(|key, _| pred(key));
        self.path_lru.retain(|key| pred(key));
    }
}

/// Cache for generated icons (icon_id -> base64 WebP data URL), with an LRU cap on
/// the unbounded `path:` subset.
static ICON_CACHE: LazyLock<RwLock<IconCache>> = LazyLock::new(|| RwLock::new(IconCache::new()));

/// Gets cached icon data URL for the given icon ID, if available.
fn get_cached_icon(icon_id: &str) -> Option<String> {
    ICON_CACHE.read_ignore_poison().entries.get(icon_id).cloned()
}

/// Caches an icon data URL.
fn cache_icon(icon_id: String, data_url: String) {
    ICON_CACHE.write_ignore_poison().insert(icon_id, data_url);
}

/// Clears all cached icons for extension-based entries.
/// Called when the "use app icons for documents" setting changes.
pub fn clear_extension_icon_cache() {
    // Only remove extension-based icons (ext:xxx), keep directory icons
    ICON_CACHE.write_ignore_poison().retain(|key| !key.starts_with("ext:"));
}

/// Clears all cached icons for directory entries (`dir`, `symlink-dir`,
/// `path:*`, `special:*`). Called when the system theme or accent color changes,
/// since macOS folder icons (including the special-folder glyphs) are tinted by
/// the current appearance.
pub fn clear_directory_icon_cache() {
    ICON_CACHE.write_ignore_poison().retain(|key| {
        key != "dir"
            && key != "symlink-dir"
            && !key.starts_with(PATH_KEY_PREFIX)
            && !key.starts_with(special_folders::SPECIAL_KEY_PREFIX)
    });
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

/// Fetches icon for a specific file path via the OS icon provider (macOS).
#[cfg(target_os = "macos")]
fn fetch_icon_for_path(path: &Path) -> Option<String> {
    // Get icon from OS (size is u16)
    let icon = get_file_icon(path, ICON_SIZE as u16).ok()?;

    // file_icon_provider returns Icon with width, height, and RGBA pixels
    let img = image::RgbaImage::from_raw(icon.width, icon.height, icon.pixels)?;
    let dynamic_img = DynamicImage::ImageRgba8(img);

    image_to_data_url(&dynamic_img)
}

/// Fetches icon for a specific file path via XDG icon theme lookup.
#[cfg(target_os = "linux")]
fn fetch_icon_for_path(path: &Path) -> Option<String> {
    let icon_id = if path.is_dir() {
        "dir".to_string()
    } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        format!("ext:{}", ext.to_lowercase())
    } else {
        "file".to_string()
    };
    crate::linux_icons::get_icon_for_id(&icon_id, ICON_SIZE as u16).and_then(|img| image_to_data_url(&img))
}

/// Gets icon for a path as base64 data URL.
/// Public API for use by volumes module.
pub fn get_icon_for_path(path: &str) -> Option<String> {
    fetch_icon_for_path(Path::new(path))
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
        let temp_path = std::env::temp_dir().join(format!("cmdr_icon_sample.{}", ext));
        // Create the file if it doesn't exist (empty file is fine)
        if !temp_path.exists() {
            let _ = std::fs::File::create(&temp_path);
        }
        return Some(temp_path);
    }
    None
}

/// Fetches icons for the given icon IDs that are not already cached.
///
/// When `use_app_icons_for_documents` is true and on macOS, extension-based icons
/// are fetched from app bundles (showing the app's icon as fallback). When false,
/// the system's default document icons are used (Finder-style with app badge).
///
/// Returns a map of icon_id -> data URL.
pub fn get_icons(icon_ids: Vec<String>, use_app_icons_for_documents: bool) -> HashMap<String, String> {
    let mut result = HashMap::new();

    // Special system folders (`special:downloads`, …) fetch their icon from the
    // folder's REAL path, which can be a cloud-synced location (Desktop &
    // Documents iCloud sync) whose NSWorkspace lookup descends into
    // `fileproviderd`. Route them through the dedicated 8 MB-stack fetch (same as
    // the `path:` branch), NOT the generic per-id loop below, which runs on the
    // calling thread. The result stays keyed by `special:{name}` (bounded), not
    // by the real path.
    let mut remaining = Vec::with_capacity(icon_ids.len());
    let mut special_to_fetch: Vec<(String, String)> = Vec::new();
    for icon_id in icon_ids {
        if let Some(cached) = get_cached_icon(&icon_id) {
            result.insert(icon_id, cached);
            continue;
        }
        if icon_id.starts_with(special_folders::SPECIAL_KEY_PREFIX) {
            if let Some(real_path) = special_folders::real_path_for_icon_id(&icon_id) {
                special_to_fetch.push((icon_id, real_path.to_string_lossy().into_owned()));
            }
            // Unknown special name (no resolved standard location): skip; the
            // frontend keeps the `dir` fallback.
            continue;
        }
        remaining.push(icon_id);
    }

    if !special_to_fetch.is_empty() {
        let paths: Vec<String> = special_to_fetch.iter().map(|(_, path)| path.clone()).collect();
        let fetched = fetch_path_icons(paths);
        // `fetch_path_icons` returns `(path:{real_path}, data_url)` in input
        // order; re-key each back to its `special:{name}` id and cache under the
        // bounded key.
        for ((special_id, _), (_, data_url)) in special_to_fetch.into_iter().zip(fetched) {
            if let Some(url) = data_url {
                cache_icon(special_id.clone(), url.clone());
                result.insert(special_id, url);
            }
        }
    }

    for icon_id in remaining {
        // Cache was already checked above for this batch.

        // macOS: drain autoreleased ObjC objects per iteration
        // (fetch_fresh_extension_icon and fetch_icon_for_path call ObjC APIs)
        #[cfg(target_os = "macos")]
        let fetched = autoreleasepool(|_| {
            if use_app_icons_for_documents
                && let Some(ext) = icon_id.strip_prefix("ext:")
                && let Some(data_url) = fetch_fresh_extension_icon(ext, true)
            {
                return Some(data_url);
            }

            if let Some(sample_path) = get_sample_path_for_icon_id(&icon_id)
                && let Some(data_url) = fetch_icon_for_path(&sample_path)
            {
                return Some(data_url);
            }
            None
        });

        #[cfg(not(target_os = "macos"))]
        let fetched = {
            // Silence unused variable warning when not on macOS
            let _ = use_app_icons_for_documents;

            // Linux: look up directly from XDG icon theme (no temp files needed)
            #[cfg(target_os = "linux")]
            if let Some(img) = crate::linux_icons::get_icon_for_id(&icon_id, ICON_SIZE as u16)
                && let Some(data_url) = image_to_data_url(&img)
            {
                Some(data_url)
            } else if let Some(sample_path) = get_sample_path_for_icon_id(&icon_id)
                && let Some(data_url) = fetch_icon_for_path(&sample_path)
            {
                Some(data_url)
            } else {
                None
            }

            #[cfg(not(target_os = "linux"))]
            if let Some(sample_path) = get_sample_path_for_icon_id(&icon_id)
                && let Some(data_url) = fetch_icon_for_path(&sample_path)
            {
                Some(data_url)
            } else {
                None
            }
        };

        if let Some(data_url) = fetched {
            cache_icon(icon_id.clone(), data_url.clone());
            result.insert(icon_id, data_url);
        }
    }

    result
}

/// Fetches a fresh icon for an extension, bypassing any OS cache.
/// On macOS, this goes directly to the app bundle. On other platforms, falls back to temp files.
///
/// When `use_app_icons_for_documents` is true, falls back to app icons for files without
/// document-specific icons. When false, uses Finder-style document icons.
fn fetch_fresh_extension_icon(ext: &str, use_app_icons_for_documents: bool) -> Option<String> {
    // On macOS, try to get the icon directly from the default app's bundle
    // This bypasses the Launch Services icon cache
    #[cfg(target_os = "macos")]
    {
        if let Some(img) = crate::macos_icons::fetch_fresh_icon_for_extension(ext, use_app_icons_for_documents) {
            return image_to_data_url(&img);
        }
    }

    // Silence unused variable warning on non-macOS platforms
    #[cfg(not(target_os = "macos"))]
    let _ = use_app_icons_for_documents;

    // Fallback: use temp file approach (works on all platforms, but may use cached icons)
    let sample_path = std::env::temp_dir().join(format!("cmdr_icon_sample.{}", ext));
    if !sample_path.exists() {
        let _ = std::fs::File::create(&sample_path);
    }
    fetch_icon_for_path(&sample_path)
}

/// Refreshes icons for a directory listing.
/// Fetches icons in parallel for:
/// 1. All unique extensions (checking for file association changes)
/// 2. All directory paths (for custom folder icons)
///
/// On macOS, extension icons are fetched directly from app bundles to bypass
/// the Launch Services icon cache, ensuring we always show the current association.
///
/// When `use_app_icons_for_documents` is true, falls back to app icons for files without
/// document-specific icons. When false, uses Finder-style document icons.
///
/// Returns only the icons that were successfully fetched, regardless of cache state.
/// This allows the frontend to detect changes by comparing with its cached icons.
pub fn refresh_icons_for_directory(
    directory_paths: Vec<String>,
    extensions: Vec<String>,
    use_app_icons_for_documents: bool,
) -> HashMap<String, String> {
    let mut result = HashMap::new();

    // Fetch extension icons in parallel (uses rayon's global pool)
    if !extensions.is_empty() {
        let ext_results: Vec<(String, Option<String>)> = extensions
            .par_iter()
            .map(|ext| {
                // macOS: drain autoreleased ObjC objects per rayon thread iteration
                // (UTType/Launch Services/NSWorkspace calls accumulate otherwise)
                #[cfg(target_os = "macos")]
                {
                    autoreleasepool(|_| {
                        let icon_id = format!("ext:{}", ext.to_lowercase());
                        let data_url = fetch_fresh_extension_icon(ext, use_app_icons_for_documents);
                        (icon_id, data_url)
                    })
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let icon_id = format!("ext:{}", ext.to_lowercase());
                    let data_url = fetch_fresh_extension_icon(ext, use_app_icons_for_documents);
                    (icon_id, data_url)
                }
            })
            .collect();

        for (icon_id, data_url) in ext_results {
            if let Some(url) = data_url {
                cache_icon(icon_id.clone(), url.clone());
                result.insert(icon_id, url);
            }
        }
    }

    // Fetch directory icons by exact REAL path. These descend into NSWorkspace on
    // real user folders, which for iCloud/Dropbox folders make synchronous XPC
    // round-trips into `fileproviderd` with deep override chains — enough to
    // overflow rayon's default 2 MB worker stack. So this branch runs on dedicated
    // 8 MB-stack OS threads (same pattern as `file_system/sync_status.rs` and
    // `open_with.rs`), NOT rayon. The extension branch above stays on rayon because
    // it fetches from sample temp paths that never descend into a cloud provider.
    if !directory_paths.is_empty() {
        let dir_results = fetch_path_icons(directory_paths);
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

/// 8 MB stack per thread: enough for deep FileProvider XPC chains that
/// NSWorkspace's per-path icon lookup descends into on cloud folders.
#[cfg(target_os = "macos")]
const ICON_THREAD_STACK_SIZE: usize = 8 * 1024 * 1024;

/// Fetches per-path folder icons, keyed `path:{path}`, on dedicated 8 MB-stack OS
/// threads (macOS) to survive `fileproviderd` XPC depth on cloud folders. The
/// `data_url` is `None` when the OS returned no icon.
#[cfg(target_os = "macos")]
fn fetch_path_icons(paths: Vec<String>) -> Vec<(String, Option<String>)> {
    let num_threads = paths
        .len()
        .min(std::thread::available_parallelism().map_or(4, |n| n.get()));

    std::thread::scope(|scope| {
        let chunk_size = paths.len().div_ceil(num_threads);
        let handles: Vec<_> = paths
            .chunks(chunk_size)
            .map(|chunk| {
                let chunk = chunk.to_vec();
                std::thread::Builder::new()
                    .stack_size(ICON_THREAD_STACK_SIZE)
                    .name("icon_path_fetch".into())
                    .spawn_scoped(scope, move || {
                        chunk
                            .into_iter()
                            .map(|path| {
                                // Drain autoreleased ObjC objects per path (NSWorkspace
                                // calls accumulate otherwise) on these threads, which
                                // lack AppKit's autorelease pool.
                                autoreleasepool(|_| {
                                    let data_url = fetch_icon_for_path(&PathBuf::from(&path));
                                    (format!("path:{}", path), data_url)
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .expect("failed to spawn icon path-fetch thread")
            })
            .collect();

        let mut results = Vec::with_capacity(paths.len());
        for handle in handles {
            results.extend(handle.join().expect("icon path-fetch thread panicked"));
        }
        results
    })
}

/// Non-macOS path-icon fetch. Linux resolves icons via the XDG theme lookup, which
/// makes no XPC calls and can't descend into a cloud provider, so rayon's pool is
/// fine here.
#[cfg(not(target_os = "macos"))]
fn fetch_path_icons(paths: Vec<String>) -> Vec<(String, Option<String>)> {
    paths
        .par_iter()
        .map(|path| {
            let data_url = fetch_icon_for_path(&PathBuf::from(path));
            (format!("path:{}", path), data_url)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_key(n: usize) -> String {
        format!("path:/folder/{n}")
    }

    #[test]
    fn path_keys_respect_lru_cap_and_evict_oldest_first() {
        let mut cache = IconCache::new();

        // Insert one more than the cap.
        for n in 0..=PATH_KEY_CAP {
            cache.insert(path_key(n), format!("url-{n}"));
        }

        // Cap is respected.
        assert_eq!(cache.path_lru.len(), PATH_KEY_CAP);
        let path_entries = cache.entries.keys().filter(|k| k.starts_with(PATH_KEY_PREFIX)).count();
        assert_eq!(path_entries, PATH_KEY_CAP);

        // The very first (oldest) inserted key was evicted.
        assert!(
            !cache.entries.contains_key(&path_key(0)),
            "oldest path: key should evict"
        );
        // The newest key survives.
        assert!(cache.entries.contains_key(&path_key(PATH_KEY_CAP)));
    }

    #[test]
    fn non_path_keys_are_never_evicted_by_the_cap() {
        let mut cache = IconCache::new();

        // Seed a handful of inherently-bounded keys.
        cache.insert("dir".to_string(), "dir-url".to_string());
        cache.insert("symlink-dir".to_string(), "symlink-dir-url".to_string());
        cache.insert("ext:txt".to_string(), "txt-url".to_string());
        cache.insert("file".to_string(), "file-url".to_string());

        // Overflow the path: keys well past the cap.
        for n in 0..(PATH_KEY_CAP * 3) {
            cache.insert(path_key(n), format!("url-{n}"));
        }

        // None of the bounded keys got evicted, and none leaked into the LRU queue.
        assert_eq!(cache.entries.get("dir").map(String::as_str), Some("dir-url"));
        assert_eq!(
            cache.entries.get("symlink-dir").map(String::as_str),
            Some("symlink-dir-url")
        );
        assert_eq!(cache.entries.get("ext:txt").map(String::as_str), Some("txt-url"));
        assert_eq!(cache.entries.get("file").map(String::as_str), Some("file-url"));
        assert_eq!(cache.path_lru.len(), PATH_KEY_CAP);
    }

    #[test]
    fn reinserting_a_path_key_refreshes_its_recency() {
        let mut cache = IconCache::new();

        // Fill exactly to the cap.
        for n in 0..PATH_KEY_CAP {
            cache.insert(path_key(n), format!("url-{n}"));
        }

        // Touch the oldest key again — it should move to the back (most recent).
        cache.insert(path_key(0), "refreshed".to_string());

        // Insert a new key, forcing one eviction. The refreshed key must survive;
        // the now-oldest (key 1) should be the one evicted.
        cache.insert(path_key(PATH_KEY_CAP), "new".to_string());

        assert_eq!(cache.path_lru.len(), PATH_KEY_CAP);
        assert_eq!(
            cache.entries.get(&path_key(0)).map(String::as_str),
            Some("refreshed"),
            "refreshed key should survive eviction"
        );
        assert!(
            !cache.entries.contains_key(&path_key(1)),
            "the now-oldest key should be evicted"
        );
        // No duplicate LRU entries after the refresh.
        let occurrences = cache.path_lru.iter().filter(|k| **k == path_key(0)).count();
        assert_eq!(occurrences, 1, "refreshed key must appear exactly once in the LRU");
    }

    #[test]
    fn retain_drops_path_lru_bookkeeping_too() {
        let mut cache = IconCache::new();
        cache.insert("dir".to_string(), "dir-url".to_string());
        for n in 0..10 {
            cache.insert(path_key(n), format!("url-{n}"));
        }

        // Mirror `clear_directory_icon_cache`: drop dir + path: keys.
        cache.retain(|key| key != "dir" && !key.starts_with(PATH_KEY_PREFIX));

        assert!(cache.entries.is_empty());
        assert!(
            cache.path_lru.is_empty(),
            "path_lru must not retain keys removed from entries"
        );
    }
}
