//! Linux: the download record from the `user.xdg.*` xattrs Chromium writes, the
//! installed apps from the desktop entries, and nothing that needs Spotlight,
//! code signing, a default-app lookup, or receipts (the plan leaves those rows out
//! here, so those answers are never asked).

use std::ffi::CString;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use cmdr_about::owner::KnownApp;

use crate::ignore_poison::IgnorePoison;

use cmdr_about::card::OsCaps;
use cmdr_about::fact::{AboutTarget, Fact, NotApplicableWhy, UnknownWhy};
use cmdr_about::payload::{AppRef, Signature, Usage};
use cmdr_about::provenance::{Download, MAX_XATTR_BYTES, xdg_download_fact};

use super::StatExtras;
use super::unix::{XattrSyscalls, errno, executable, hard_links, physical_size, read_capped};

pub(crate) fn os_caps() -> OsCaps {
    OsCaps {
        use_history: false,
        code_signing: false,
        default_app: false,
        install_receipts: false,
    }
}

pub(crate) fn stat_extras(metadata: &std::fs::Metadata) -> StatExtras {
    StatExtras {
        executable: executable(metadata),
        online_only: false,
        physical_size: physical_size(metadata),
        hard_links: hard_links(metadata),
    }
}

/// `lgetxattr`: the link itself, never what it points at.
struct Linux;

impl XattrSyscalls for Linux {
    const NO_ATTR: i32 = libc::ENODATA;

    fn size(path: &CString, name: &CString) -> Result<usize, i32> {
        // SAFETY: both pointers are live NUL-terminated strings for the call; a null
        // buffer with size 0 asks only for the value's size.
        let size = unsafe { libc::lgetxattr(path.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0) };
        usize::try_from(size).map_err(|_| errno())
    }

    fn read(path: &CString, name: &CString, buf: &mut [u8]) -> Result<usize, i32> {
        // SAFETY: `buf` is a live, writable slice of exactly `buf.len()` bytes, and the
        // kernel writes at most that many.
        let read = unsafe { libc::lgetxattr(path.as_ptr(), name.as_ptr(), buf.as_mut_ptr().cast(), buf.len()) };
        usize::try_from(read).map_err(|_| errno())
    }
}

pub(crate) fn download(path: &str) -> Fact<Download> {
    let origin = read_capped::<Linux>(path, "user.xdg.origin.url", MAX_XATTR_BYTES).for_decoder(MAX_XATTR_BYTES);
    let referrer = read_capped::<Linux>(path, "user.xdg.referrer.url", MAX_XATTR_BYTES).for_decoder(MAX_XATTR_BYTES);
    xdg_download_fact(origin.as_deref(), referrer.as_deref())
}

pub(crate) fn last_opened(_path: &str) -> Fact<Usage> {
    Fact::unknown(UnknownWhy::NoRecord)
}

pub(crate) fn default_app(_path: &str) -> Fact<AppRef> {
    Fact::unknown(UnknownWhy::NoRecord)
}

pub(crate) fn signature(_path: &str, _target: AboutTarget, _extension: Option<&str>) -> Fact<Signature> {
    Fact::NotApplicable(NotApplicableWhy::NoCodeSigningOnThisOs)
}

/// How long a scan of the desktop entries stands. Linux has no app-change
/// notification Cmdr listens to, so age alone renews it.
const APPS_MAX_AGE: Duration = Duration::from_secs(30 * 60);

/// The largest desktop entry read. A real one is a few KB.
const MAX_DESKTOP_ENTRY_BYTES: u64 = 64 * 1024;

/// The last scan, and when it ran.
type AppScan = Option<(Instant, Vec<KnownApp>)>;

static APPS: LazyLock<Mutex<AppScan>> = LazyLock::new(|| Mutex::new(None));

/// The folders desktop entries are installed into: the system's, the person's,
/// flatpak's exports, and snapd's.
fn desktop_entry_folders() -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = [
        "/usr/share/applications",
        "/usr/local/share/applications",
        "/var/lib/flatpak/exports/share/applications",
        "/var/lib/snapd/desktop/applications",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    if let Some(home) = dirs::home_dir() {
        folders.push(home.join(".local/share/applications"));
        folders.push(home.join(".local/share/flatpak/exports/share/applications"));
    }
    folders
}

/// The installed apps, by their desktop entries: the entry's `Name=` and its
/// desktop-file id. Cached; the first card pays for the scan.
pub(crate) fn installed_apps() -> Vec<KnownApp> {
    if let Some((at, apps)) = APPS.lock_ignore_poison().as_ref()
        && at.elapsed() < APPS_MAX_AGE
    {
        return apps.clone();
    }
    let apps = scan_desktop_entries(&desktop_entry_folders());
    *APPS.lock_ignore_poison() = Some((Instant::now(), apps.clone()));
    apps
}

/// Every `.desktop` file directly in `folders`.
pub(crate) fn scan_desktop_entries(folders: &[PathBuf]) -> Vec<KnownApp> {
    let mut apps = Vec::new();
    for folder in folders {
        let Ok(entries) = std::fs::read_dir(folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "desktop") {
                continue;
            }
            let Some(id) = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()) else {
                continue;
            };
            let small = entry.metadata().is_ok_and(|meta| meta.len() <= MAX_DESKTOP_ENTRY_BYTES);
            let name = small
                .then(|| std::fs::read_to_string(&path).ok())
                .flatten()
                .and_then(|text| desktop_entry_name(&text));
            apps.push(known_app(id, name));
        }
    }
    apps
}

/// An app from its desktop-file id and `Name=`. snapd names its entries
/// `<snap>_<app>`, so the snap's own name is one the app answers to too.
fn known_app(id: String, name: Option<String>) -> KnownApp {
    let mut other_names = vec![id.clone()];
    if let Some((snap, _app)) = id.split_once('_') {
        other_names.push(snap.to_string());
    }
    KnownApp {
        name: name.unwrap_or_else(|| id.clone()),
        id: Some(id),
        other_names,
    }
}

/// The unlocalized `Name=` of a desktop entry's `[Desktop Entry]` group.
pub(crate) fn desktop_entry_name(text: &str) -> Option<String> {
    let mut in_entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if in_entry && let Some(name) = line.strip_prefix("Name=") {
            let name = name.trim();
            return (!name.is_empty()).then(|| name.to_string());
        }
    }
    None
}

/// A flatpak or snap with this id installed on this disk, by its install folder.
/// A desktop entry is optional for both, so the scan can miss one.
pub(crate) fn app_with_id(id: &str) -> Option<KnownApp> {
    // The id comes from a folder name; one that could climb out of the install
    // folder names nothing.
    if id.is_empty() || id.contains('/') || id == "." || id == ".." {
        return None;
    }
    let mut places = vec![
        PathBuf::from("/var/lib/flatpak/app").join(id),
        PathBuf::from("/snap").join(id),
    ];
    if let Some(home) = dirs::home_dir() {
        places.push(home.join(".local/share/flatpak/app").join(id));
    }
    places.iter().any(|place| place.is_dir()).then(|| KnownApp {
        name: id.to_string(),
        id: Some(id.to_string()),
        other_names: Vec::new(),
    })
}

/// Linux keeps no receipts Cmdr reads (`OsCaps::install_receipts` is off), so
/// the plan never asks.
pub(crate) fn receipts(_path: &str) -> Result<Vec<String>, UnknownWhy> {
    Err(UnknownWhy::NoRecord)
}

#[cfg(test)]
mod apps_tests {
    use super::*;

    #[test]
    fn a_desktop_entry_is_named_by_its_unlocalized_name_in_its_own_group() {
        let entry = "[Desktop Action new]\nName=New window\n\n[Desktop Entry]\nName[de]=Feuerfuchs\nName=Firefox\n";
        assert_eq!(desktop_entry_name(entry), Some("Firefox".to_string()));
        assert_eq!(desktop_entry_name("[Desktop Entry]\nExec=x\n"), None);
    }

    #[test]
    fn a_snaps_entry_answers_to_the_snaps_name() {
        let app = known_app("firefox_firefox".to_string(), Some("Firefox".to_string()));
        assert!(app.other_names.contains(&"firefox".to_string()));
    }

    #[test]
    fn an_id_that_climbs_out_names_no_app() {
        assert_eq!(app_with_id(".."), None);
        assert_eq!(app_with_id("a/b"), None);
    }
}
