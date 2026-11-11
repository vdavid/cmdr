//! macOS: `NSWorkspace` renders a path's icon (`workspace.rs`), the default app's bundle answers
//! an extension past Launch Services' cache (`crate::macos_icons`), and every call drains its
//! autoreleased objects.

mod workspace;

use std::path::{Path, PathBuf};

use image::DynamicImage;
use objc2::rc::autoreleasepool;

use super::super::image_to_data_url;
use super::PathIcons;
use crate::config::ICON_SIZE;

/// 8 MB stack per thread: enough for deep FileProvider XPC chains that
/// NSWorkspace's per-path icon lookup descends into on cloud folders.
const ICON_THREAD_STACK_SIZE: usize = 8 * 1024 * 1024;

/// Global-domain key behind Finder > Settings > Tags > "Tint folders based on tags". Absent (the
/// default) means tinting is on (verified on macOS 27.0.1: `defaults write -g … -bool true` turns
/// a red-tagged custom folder back to the accent color, 2026-10-29).
const TAG_TINTING_DISABLED_KEY: &str = "AppleDisableTagBasedIconTinting";

/// Fetches icon for a specific file path via the OS icon provider (macOS).
pub(crate) fn fetch_icon_for_path(path: &Path) -> Option<String> {
    let img = workspace::render_file_icon(path, ICON_SIZE as u16)?;
    image_to_data_url(&DynamicImage::ImageRgba8(img))
}

/// Fetches per-path folder icons, keyed `path:{path}`, on dedicated 8 MB-stack OS
/// threads (macOS) to survive `fileproviderd` XPC depth on cloud folders. The
/// `data_url` is `None` when the OS returned no icon.
pub(crate) fn fetch_path_icons(paths: Vec<String>) -> PathIcons {
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

/// With app icons for documents on, an `ext:` id asks the default app's bundle first.
pub(crate) fn icon_for_id(icon_id: &str, use_app_icons_for_documents: bool) -> Option<String> {
    if !use_app_icons_for_documents {
        return None;
    }
    let ext = icon_id.strip_prefix("ext:")?;
    super::super::fetch_fresh_extension_icon(ext, true)
}

/// Gets the icon directly from the default app's bundle, which bypasses the Launch Services
/// icon cache.
pub(crate) fn fresh_extension_icon(ext: &str, use_app_icons_for_documents: bool) -> Option<DynamicImage> {
    crate::macos_icons::fetch_fresh_icon_for_extension(ext, use_app_icons_for_documents)
}

/// Drains autoreleased ObjC objects per call (UTType, Launch Services, and NSWorkspace calls
/// accumulate otherwise, on rayon threads especially).
pub(crate) fn with_object_pool<T, F: FnOnce() -> T>(f: F) -> T {
    autoreleasepool(|_| f())
}

/// Reads Finder's "Tint folders based on tags" switch, re-syncing the global domain first so a
/// change made while Cmdr runs is seen. CFPreferences is thread-safe, so no main-thread hop.
pub(crate) fn tag_tinting_disabled() -> bool {
    use objc2_core_foundation::{CFPreferencesAppSynchronize, CFPreferencesGetAppBooleanValue, CFString};
    // SAFETY: `kCFPreferencesAnyApplication` is an immutable `'static` `CFStringRef` the framework
    // initialises before any Rust in this process runs; reading it is `unsafe` only because it
    // crosses the FFI boundary.
    let application = unsafe { objc2_core_foundation::kCFPreferencesAnyApplication };
    // A failed sync only means reading the cached value, still a valid answer.
    let _ = CFPreferencesAppSynchronize(application);
    let key = CFString::from_str(TAG_TINTING_DISABLED_KEY);
    // SAFETY: `key` and `application` are live `CFString`s for the whole call, and a null
    // out-pointer is documented as "don't report validity".
    unsafe { CFPreferencesGetAppBooleanValue(&key, application, std::ptr::null_mut()) }
}
