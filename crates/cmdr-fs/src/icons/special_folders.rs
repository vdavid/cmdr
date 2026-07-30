//! Detection of the finite set of special **system** folders (Downloads,
//! Applications, the home folder, …) by their well-known canonical path.
//!
//! Each special folder maps to a bounded shared icon key `special:{name}` (for
//! example `special:downloads`). Detection is a pure path comparison against the
//! standard-location set resolved once at startup — no NSWorkspace, no
//! LaunchServices, no `getxattr`, so it never triggers a TCC popup and stays
//! cheap enough to run for every directory entry during listing.
//!
//! The set is intentionally finite and stable: `icons::get_icons` fetches each
//! `special:*` icon once from that folder's real path (FDA-gated, on the 8 MB
//! fetch thread), and the keys may persist to localStorage on the frontend.
//!
//! `~/Library/Mobile Documents`, mounted volumes, and network mounts are NOT in
//! this set. Volumes carry their own per-path icon (Tier C); a network mount is
//! never one of these standard home-relative locations.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// Prefix marking special-system-folder icon keys (`special:downloads`, …).
/// Like `dir` / `ext:*` these are an inherently bounded set, so they're never
/// LRU-capped and may persist across restarts.
pub const SPECIAL_KEY_PREFIX: &str = "special:";

/// The finite set of special system folders we detect, each with its short
/// stable name (the `special:{name}` suffix) and a resolver for its real path.
///
/// Names are lowercase ASCII so the resulting `special:{name}` key is stable
/// across locales (the on-disk folder may be localized in Finder, but its path
/// segment isn't).
struct SpecialFolderDef {
    name: &'static str,
    resolve: fn() -> Option<PathBuf>,
}

/// `/Applications` is a fixed system path on macOS, not a home-relative one.
#[cfg(target_os = "macos")]
fn applications_dir() -> Option<PathBuf> {
    Some(PathBuf::from("/Applications"))
}

/// The user Trash on macOS lives at `~/.Trash`. `dirs` has no entry for it.
#[cfg(target_os = "macos")]
fn trash_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".Trash"))
}

/// The full table of special folders. Ordering is irrelevant — paths are unique,
/// so the resolved map never has two names competing for one path. On non-macOS
/// the home-relative XDG entries still resolve via `dirs`; the macOS-only
/// `/Applications` and `~/.Trash` entries are gated out.
static SPECIAL_FOLDERS: &[SpecialFolderDef] = &[
    SpecialFolderDef {
        name: "home",
        resolve: dirs::home_dir,
    },
    SpecialFolderDef {
        name: "downloads",
        resolve: dirs::download_dir,
    },
    SpecialFolderDef {
        name: "desktop",
        resolve: dirs::desktop_dir,
    },
    SpecialFolderDef {
        name: "documents",
        resolve: dirs::document_dir,
    },
    SpecialFolderDef {
        name: "movies",
        resolve: dirs::video_dir,
    },
    SpecialFolderDef {
        name: "music",
        resolve: dirs::audio_dir,
    },
    SpecialFolderDef {
        name: "pictures",
        resolve: dirs::picture_dir,
    },
    SpecialFolderDef {
        name: "public",
        resolve: dirs::public_dir,
    },
    #[cfg(target_os = "macos")]
    SpecialFolderDef {
        name: "applications",
        resolve: applications_dir,
    },
    #[cfg(target_os = "macos")]
    SpecialFolderDef {
        name: "trash",
        resolve: trash_dir,
    },
];

/// Resolves the special-folder table into a path → name map, built once.
///
/// We normalize each resolved path with `clean_path` (lexical, no I/O — no
/// `canonicalize`, which would hit the disk and could block on a dead mount) so
/// the comparison in `classify` is a cheap `HashMap` lookup against equally
/// normalized inputs. The home folder always resolves; the rest skip silently if
/// the platform has no standard location for them.
static SPECIAL_BY_PATH: LazyLock<HashMap<PathBuf, &'static str>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for def in SPECIAL_FOLDERS {
        if let Some(path) = (def.resolve)() {
            map.insert(clean_path(&path), def.name);
        }
    }
    map
});

/// Resolves a special-folder name back to its real path, built once. Used by
/// `icons::get_icons` to fetch the icon from the folder's actual location.
static PATH_BY_NAME: LazyLock<HashMap<&'static str, PathBuf>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for def in SPECIAL_FOLDERS {
        if let Some(path) = (def.resolve)() {
            map.insert(def.name, path);
        }
    }
    map
});

/// Lexically normalizes a path for comparison: strips a trailing slash and
/// collapses `.` components, without touching the disk. We deliberately avoid
/// `std::fs::canonicalize` here — it resolves symlinks via syscalls and would
/// block on a dead network mount, defeating the "cheap, no I/O" contract this
/// classifier must keep to run per-entry during listing.
fn clean_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Returns the special-folder name for `path` (for example `"downloads"`), or
/// `None` when `path` isn't one of the finite special system folders.
///
/// Detection is by canonical path, NOT by folder name: a folder merely *named*
/// "Downloads" under `~/Projects/` is not the real `~/Downloads`, so it returns
/// `None` and falls back to the generic `dir` icon.
pub fn classify(path: &Path) -> Option<&'static str> {
    SPECIAL_BY_PATH.get(&clean_path(path)).copied()
}

/// Builds the `special:{name}` icon key for a special folder, or `None` when the
/// path isn't special.
pub fn icon_id_for_path(path: &Path) -> Option<String> {
    classify(path).map(|name| format!("{SPECIAL_KEY_PREFIX}{name}"))
}

/// Resolves a `special:{name}` icon id back to the real folder path the icon
/// should be fetched from. Returns `None` for non-`special:` ids or unknown
/// names.
pub fn real_path_for_icon_id(icon_id: &str) -> Option<PathBuf> {
    let name = icon_id.strip_prefix(SPECIAL_KEY_PREFIX)?;
    PATH_BY_NAME.get(name).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_each_resolved_special_folder() {
        // Every folder that resolves on this platform must classify to its name
        // when looked up by its own real path.
        for def in SPECIAL_FOLDERS {
            if let Some(path) = (def.resolve)() {
                assert_eq!(
                    classify(&path),
                    Some(def.name),
                    "real path of {} should classify to its name",
                    def.name
                );
            }
        }
    }

    // `special:*` classification is a macOS concept (Linux falls back to the XDG
    // theme path and never classifies special folders), and the standard-location
    // resolvers (`dirs::download_dir`, …) return `None` in a headless Linux CI
    // container with no XDG user-dirs configured. So the tests that assert a
    // specific `special:*` classification are macOS-only; the OS-neutral ones
    // (the "named but not the real one" negative cases, the round-trip rejection)
    // run everywhere and never depend on a `dirs::*` resolver returning `Some`.

    #[cfg(target_os = "macos")]
    #[test]
    fn home_downloads_classifies_to_downloads() {
        let downloads = dirs::download_dir().expect("download_dir resolves");
        assert_eq!(classify(&downloads), Some("downloads"));
    }

    #[test]
    fn a_folder_merely_named_downloads_elsewhere_is_not_special() {
        // `/some/where/Downloads` is named "Downloads" but isn't the real one. A
        // fixed path keeps this independent of whether `dirs::*` resolves.
        let fake = Path::new("/some/where/Projects/Downloads");
        assert_eq!(classify(fake), None);
    }

    #[test]
    fn an_arbitrary_project_folder_is_not_special() {
        let project = Path::new("/some/where/Projects/foo");
        assert_eq!(classify(project), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn trailing_slash_does_not_defeat_detection() {
        let downloads = dirs::download_dir().expect("download_dir resolves");
        let with_slash = format!("{}/", downloads.to_string_lossy());
        assert_eq!(classify(Path::new(&with_slash)), Some("downloads"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn icon_id_for_path_builds_the_special_key() {
        let downloads = dirs::download_dir().expect("download_dir resolves");
        assert_eq!(icon_id_for_path(&downloads).as_deref(), Some("special:downloads"));

        let project = Path::new("/some/where/Projects/foo");
        assert_eq!(icon_id_for_path(project), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn real_path_round_trips_from_icon_id() {
        let downloads = dirs::download_dir().expect("download_dir resolves");
        assert_eq!(real_path_for_icon_id("special:downloads"), Some(downloads));
    }

    #[test]
    fn real_path_rejects_non_special_ids() {
        assert_eq!(real_path_for_icon_id("dir"), None);
        assert_eq!(real_path_for_icon_id("ext:txt"), None);
        assert_eq!(real_path_for_icon_id("path:/tmp/foo"), None);
        assert_eq!(real_path_for_icon_id("special:nonexistent"), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn applications_and_trash_classify_on_macos() {
        assert_eq!(classify(Path::new("/Applications")), Some("applications"));
        let trash = dirs::home_dir().expect("home").join(".Trash");
        assert_eq!(classify(&trash), Some("trash"));
    }
}
