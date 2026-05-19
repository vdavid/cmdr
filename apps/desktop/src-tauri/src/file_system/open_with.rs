//! "Open with" candidate apps and launch wrappers, backed by `NSWorkspace`.
//!
//! Two responsibilities:
//! - **Listing candidates** for one or more selected files via `URLsForApplicationsToOpenURL:`
//!   (modern macOS 12+ API). Multi-selection picks the intersection of per-file candidate lists.
//!   Results are cached by lowercased extension for the session and invalidated when an app
//!   launches or terminates.
//! - **Launching** the chosen app with the full selection via
//!   `openURLs:withApplicationAtURL:configuration:completionHandler:` (one launch, multi-URL).
//!
//! Threading: `URLsForApplicationsToOpenURL:` is a synchronous LaunchServices call. On
//! cloud-stub files it descends into FileProvider XPC, which can blow rayon's 2 MB stack.
//! We use dedicated 8 MB-stack OS threads (same pattern as `sync_status.rs`).

use std::path::{Path, PathBuf};

/// Metadata for one candidate app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppCandidate {
    pub bundle_id: String,
    pub display_name: String,
    pub app_path: PathBuf,
    /// Pre-rasterized RGBA icon ready to feed `IconMenuItem`. `None` if the app has
    /// no `CFBundleIconFile` or its `.icns` couldn't be parsed.
    pub icon: Option<AppIcon>,
}

/// RGBA icon bytes plus dimensions, sized for a macOS context-menu item.
/// `IconMenuItem` accepts owned RGBA via `tauri::image::Image::new_owned`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppIcon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// "Open with" data for a (possibly multi-) selection.
#[derive(Clone, Debug, Default)]
pub struct OpenWithChoices {
    /// Apps that can open every file in the selection. First entry is the OS default
    /// (the default of the first selected file).
    pub candidates: Vec<AppCandidate>,
}

/// Picks the apps that appear in *every* per-file candidate list, preserving the order
/// from the first list. Pure function for testing.
pub fn intersect_candidate_lists(lists: &[Vec<PathBuf>]) -> Vec<PathBuf> {
    let Some(first) = lists.first() else {
        return Vec::new();
    };
    if lists.len() == 1 {
        return first.clone();
    }
    first
        .iter()
        .filter(|p| lists[1..].iter().all(|list| list.contains(p)))
        .cloned()
        .collect()
}

/// Lowercased extension (without the dot), or `None` if the file has none. Used as
/// the candidate-cache key.
pub fn extension_cache_key(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use objc2::rc::{Retained, autoreleasepool};
    use objc2_app_kit::{NSWorkspace, NSWorkspaceOpenConfiguration};
    use objc2_foundation::{NSArray, NSBundle, NSString, NSURL};
    use plist::Value;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{LazyLock, Mutex};
    use std::time::{Duration, Instant};

    /// Per-extension cache TTL fallback. The primary invalidation signal is
    /// `NSWorkspace.didLaunchApplicationNotification` (subscribed in
    /// `start_invalidation_observer`), but a TTL guards against missed signals (apps installed
    /// via Finder drag, system updates).
    const CACHE_TTL: Duration = Duration::from_secs(30 * 60);

    /// Stack size for LaunchServices worker threads. Must accommodate deep
    /// FileProvider XPC chains for cloud-stub files.
    const THREAD_STACK_SIZE: usize = 8 * 1024 * 1024;

    struct CacheEntry {
        candidates: Vec<PathBuf>,
        fetched_at: Instant,
    }

    /// Session-scoped cache of `URLsForApplicationsToOpenURL:` results, keyed by
    /// lowercased extension. `None` key handles extension-less files (rare, often
    /// re-queried).
    static EXT_CACHE: LazyLock<Mutex<HashMap<String, CacheEntry>>> = LazyLock::new(Default::default);

    /// Clears the candidate cache. Called from the NSWorkspace notification observer
    /// when an app launches or terminates.
    fn invalidate_cache() {
        let mut cache = EXT_CACHE.lock().expect("open_with cache mutex poisoned");
        cache.clear();
    }

    /// Subscribes to `NSWorkspaceDidLaunchApplicationNotification` and
    /// `NSWorkspaceDidTerminateApplicationNotification` to invalidate the candidate
    /// cache when the set of installed apps may have changed.
    pub fn start_invalidation_observer() {
        use block2::RcBlock;
        use objc2_foundation::NSNotification;
        use std::ptr::NonNull;

        let workspace = NSWorkspace::sharedWorkspace();
        let center = workspace.notificationCenter();

        let block = RcBlock::new(move |_n: NonNull<NSNotification>| {
            invalidate_cache();
        });
        // Observer is retained by the notification center for the app's lifetime.
        unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(objc2_app_kit::NSWorkspaceDidLaunchApplicationNotification),
                None,
                None,
                &block,
            );
            center.addObserverForName_object_queue_usingBlock(
                Some(objc2_app_kit::NSWorkspaceDidTerminateApplicationNotification),
                None,
                None,
                &block,
            );
        }
    }

    fn nsurl_from_path(path: &Path) -> Option<Retained<NSURL>> {
        let path_str = path.to_str()?;
        let ns_path = NSString::from_str(path_str);
        Some(NSURL::fileURLWithPath(&ns_path))
    }

    /// Calls `URLsForApplicationsToOpenURL:` synchronously. Runs on the calling thread;
    /// callers should arrange to be off the main thread and on a stack ≥ 8 MB.
    fn fetch_candidates_for_path(path: &Path) -> Vec<PathBuf> {
        autoreleasepool(|_| {
            let Some(url) = nsurl_from_path(path) else {
                return Vec::new();
            };
            let workspace = NSWorkspace::sharedWorkspace();
            let arr: Retained<NSArray<NSURL>> = workspace.URLsForApplicationsToOpenURL(&url);
            arr.iter()
                .filter_map(|app_url| app_url.path().map(|p| PathBuf::from(p.to_string())))
                .collect()
        })
    }

    /// Looks up cached candidates for the path's extension, falling back to a fresh
    /// query if not present or expired.
    fn candidates_for_path_cached(path: &Path) -> Vec<PathBuf> {
        let Some(key) = extension_cache_key(path) else {
            return fetch_candidates_for_path(path);
        };
        if let Some(entry) = EXT_CACHE.lock().expect("open_with cache").get(&key)
            && entry.fetched_at.elapsed() < CACHE_TTL
        {
            return entry.candidates.clone();
        }
        let candidates = fetch_candidates_for_path(path);
        EXT_CACHE.lock().expect("open_with cache").insert(
            key,
            CacheEntry {
                candidates: candidates.clone(),
                fetched_at: Instant::now(),
            },
        );
        candidates
    }

    /// Reads `CFBundleDisplayName` (preferring localized) and falls back to
    /// `CFBundleName`, then to the bundle's directory name.
    pub fn read_app_display_name(app_path: &Path) -> String {
        let plist_data = std::fs::read(app_path.join("Contents/Info.plist")).ok();
        let plist = plist_data.and_then(|data| plist::from_bytes::<Value>(&data).ok());
        if let Some(plist) = plist
            && let Some(dict) = plist.as_dictionary()
        {
            for key in &["CFBundleDisplayName", "CFBundleName"] {
                if let Some(name) = dict.get(key).and_then(|v| v.as_string()) {
                    return name.to_string();
                }
            }
        }
        // Fallback: directory name without ".app"
        app_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string()
    }

    /// Reads `CFBundleIdentifier` from Info.plist. Used as a stable menu-item ID.
    pub fn read_bundle_identifier(app_path: &Path) -> Option<String> {
        let p = app_path.to_str()?;
        let ns_path = NSString::from_str(p);
        let url = NSURL::fileURLWithPath(&ns_path);
        let bundle = NSBundle::bundleWithURL(&url)?;
        Some(bundle.bundleIdentifier()?.to_string())
    }

    /// Target pixel size for menu icons. macOS renders menu items at ~16pt; we load 32x32
    /// so Retina displays get @2x without upscaling, and @1x screens downscale a tiny bit.
    const MENU_ICON_SIZE: u32 = 32;

    /// Reads the app's main icon (`CFBundleIconFile` from Info.plist) and returns it as
    /// an RGBA buffer suitable for `tauri::image::Image::new_owned`. The `.icns` file
    /// usually contains multiple sizes. We prefer 32x32, falling back to larger sizes
    /// (resized via the `image` crate) if 32x32 isn't present.
    pub fn load_app_icon(app_path: &Path) -> Option<AppIcon> {
        use icns::{IconFamily, IconType};
        use image::{DynamicImage, RgbaImage, imageops::FilterType};

        let plist_data = std::fs::read(app_path.join("Contents/Info.plist")).ok()?;
        let plist: Value = plist::from_bytes(&plist_data).ok()?;
        let icon_name = plist
            .as_dictionary()?
            .get("CFBundleIconFile")
            .and_then(|v| v.as_string())?
            .to_string();
        let icon_filename = if icon_name.ends_with(".icns") {
            icon_name
        } else {
            format!("{icon_name}.icns")
        };
        let icon_path = app_path.join("Contents/Resources").join(&icon_filename);
        let file = std::fs::File::open(&icon_path).ok()?;
        let family = IconFamily::read(file).ok()?;

        // Preferred sizes in order: exact match first to avoid resampling, then larger
        // sizes that we'll downsample. Skip 16x16 (it's blurry on Retina).
        let candidates = [
            IconType::RGBA32_32x32,
            IconType::RGBA32_64x64,
            IconType::RGBA32_128x128,
            IconType::RGBA32_256x256,
            IconType::RGBA32_512x512,
        ];
        for icon_type in candidates {
            let Ok(icon) = family.get_icon_with_type(icon_type) else {
                continue;
            };
            let w = icon.width();
            let h = icon.height();
            let bytes = icon.into_data().to_vec();
            if w == MENU_ICON_SIZE && h == MENU_ICON_SIZE {
                return Some(AppIcon {
                    rgba: bytes,
                    width: w,
                    height: h,
                });
            }
            // Downsample to MENU_ICON_SIZE.
            let img = RgbaImage::from_raw(w, h, bytes)?;
            let resized =
                DynamicImage::ImageRgba8(img).resize_exact(MENU_ICON_SIZE, MENU_ICON_SIZE, FilterType::Lanczos3);
            let rgba = resized.to_rgba8();
            return Some(AppIcon {
                rgba: rgba.into_raw(),
                width: MENU_ICON_SIZE,
                height: MENU_ICON_SIZE,
            });
        }
        None
    }

    /// Computes the candidate apps for a (possibly multi-) selection. The first entry
    /// is the OS default for the first selected file.
    ///
    /// Spawns a single worker thread with an 8 MB stack to absorb deep FileProvider
    /// stacks. The caller blocks until results are ready; this should be invoked from
    /// a `spawn_blocking` task in Tauri command code.
    pub fn compute_open_with_choices(paths: Vec<PathBuf>) -> OpenWithChoices {
        if paths.is_empty() {
            return OpenWithChoices::default();
        }
        let handle = std::thread::Builder::new()
            .stack_size(THREAD_STACK_SIZE)
            .name("open_with_query".into())
            .spawn(move || {
                let lists: Vec<Vec<PathBuf>> = paths.iter().map(|p| candidates_for_path_cached(p)).collect();
                let intersection = intersect_candidate_lists(&lists);
                intersection
                    .into_iter()
                    .filter_map(|app_path| {
                        let bundle_id = read_bundle_identifier(&app_path)?;
                        let display_name = read_app_display_name(&app_path);
                        let icon = load_app_icon(&app_path);
                        Some(AppCandidate {
                            bundle_id,
                            display_name,
                            app_path,
                            icon,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .expect("spawn open_with worker");
        let candidates = handle.join().unwrap_or_default();
        OpenWithChoices { candidates }
    }

    /// Launches the given paths with the chosen application via
    /// `openURLs:withApplicationAtURL:configuration:completionHandler:`. Uses a single
    /// multi-URL call so apps that aren't running launch once instead of per file.
    pub fn open_paths_with(paths: &[PathBuf], app_path: &Path) -> Result<(), String> {
        autoreleasepool(|_| {
            let workspace = NSWorkspace::sharedWorkspace();
            // Build NSArray<NSURL> of the paths.
            let urls: Vec<Retained<NSURL>> = paths.iter().filter_map(|p| nsurl_from_path(p)).collect();
            if urls.is_empty() {
                return Err("No valid paths to open".to_string());
            }
            let url_refs: Vec<&NSURL> = urls.iter().map(|u| u.as_ref()).collect();
            let urls_array = NSArray::from_slice(&url_refs);

            let app_url = nsurl_from_path(app_path).ok_or_else(|| "Couldn't convert app path to URL".to_string())?;
            let config = NSWorkspaceOpenConfiguration::configuration();

            // Fire-and-forget launch: pass `None` for completion handler. Failures
            // surface as visual cues (no app launches), and we don't have a UI for
            // post-launch errors yet.
            workspace.openURLs_withApplicationAtURL_configuration_completionHandler(
                &urls_array,
                &app_url,
                &config,
                None,
            );
            Ok(())
        })
    }

    /// Shows a native `NSOpenPanel` filtered to `.app` bundles, returning the chosen
    /// app's path or `None` if the user cancelled. Must be called on the main thread.
    pub fn pick_app_via_open_panel() -> Option<PathBuf> {
        use objc2_app_kit::{NSModalResponse, NSModalResponseOK, NSOpenPanel};
        autoreleasepool(|_| {
            let mtm =
                objc2::MainThreadMarker::new().expect("pick_app_via_open_panel must be called from the main thread");
            let panel = NSOpenPanel::openPanel(mtm);
            panel.setTitle(Some(&NSString::from_str("Choose an app")));
            panel.setMessage(Some(&NSString::from_str("Pick an app to open the selected files")));
            panel.setCanChooseFiles(true);
            panel.setCanChooseDirectories(false);
            panel.setAllowsMultipleSelection(false);
            panel.setResolvesAliases(true);
            // Filter to app bundles. `setAllowedContentTypes:` is the modern
            // macOS 11+ API but adding it would pull in `objc2-uniform-type-identifiers`.
            // The legacy extension-based filter still works and avoids an extra crate.
            let exts = NSArray::from_retained_slice(&[NSString::from_str("app")]);
            #[allow(
                deprecated,
                reason = "setAllowedContentTypes needs objc2-uniform-type-identifiers; \
                          the legacy extension filter still works and avoids the extra crate"
            )]
            panel.setAllowedFileTypes(Some(&exts));
            // Default location: /Applications, like Finder's "Other..." picker.
            let apps_url = NSURL::fileURLWithPath(&NSString::from_str("/Applications"));
            panel.setDirectoryURL(Some(&apps_url));

            let response: NSModalResponse = panel.runModal();
            if response != NSModalResponseOK {
                return None;
            }
            let url = panel.URL()?;
            let path = url.path()?;
            Some(PathBuf::from(path.to_string()))
        })
    }
}

#[cfg(target_os = "macos")]
pub use imp::{compute_open_with_choices, open_paths_with, pick_app_via_open_panel, start_invalidation_observer};

#[cfg(not(target_os = "macos"))]
pub fn compute_open_with_choices(_paths: Vec<PathBuf>) -> OpenWithChoices {
    OpenWithChoices::default()
}

#[cfg(not(target_os = "macos"))]
pub fn open_paths_with(_paths: &[PathBuf], _app_path: &Path) -> Result<(), String> {
    Err("Open with is only available on macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn pick_app_via_open_panel() -> Option<PathBuf> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn start_invalidation_observer() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn intersect_single_list_passes_through() {
        let lists = vec![vec![PathBuf::from("/A.app"), PathBuf::from("/B.app")]];
        assert_eq!(
            intersect_candidate_lists(&lists),
            vec![PathBuf::from("/A.app"), PathBuf::from("/B.app")]
        );
    }

    #[test]
    fn intersect_preserves_first_list_order() {
        let lists = vec![
            vec![
                PathBuf::from("/A.app"),
                PathBuf::from("/B.app"),
                PathBuf::from("/C.app"),
            ],
            vec![
                PathBuf::from("/B.app"),
                PathBuf::from("/A.app"),
                PathBuf::from("/D.app"),
            ],
        ];
        // A and B are in both; A comes first in the first list, so A first.
        assert_eq!(
            intersect_candidate_lists(&lists),
            vec![PathBuf::from("/A.app"), PathBuf::from("/B.app")]
        );
    }

    #[test]
    fn intersect_empty_when_no_overlap() {
        let lists = vec![vec![PathBuf::from("/A.app")], vec![PathBuf::from("/B.app")]];
        assert!(intersect_candidate_lists(&lists).is_empty());
    }

    #[test]
    fn intersect_empty_input() {
        assert!(intersect_candidate_lists(&[]).is_empty());
    }

    #[test]
    fn intersect_with_one_empty_list_yields_empty() {
        let lists = vec![vec![PathBuf::from("/A.app")], vec![]];
        assert!(intersect_candidate_lists(&lists).is_empty());
    }

    #[test]
    fn ext_key_lowercases() {
        assert_eq!(extension_cache_key(Path::new("foo.PNG")).as_deref(), Some("png"));
        assert_eq!(extension_cache_key(Path::new("foo.tar.gz")).as_deref(), Some("gz"));
    }

    #[test]
    fn ext_key_none_for_extensionless() {
        assert!(extension_cache_key(Path::new("Makefile")).is_none());
        assert!(extension_cache_key(Path::new("/path/to/dir")).is_none());
    }
}
