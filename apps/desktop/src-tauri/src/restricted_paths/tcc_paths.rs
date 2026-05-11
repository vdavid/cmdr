//! Pure predicates for "this path is *possibly* macOS-TCC-restricted."
//!
//! These match a hard-coded list of paths under `$HOME` (plus `/Volumes/*`
//! for network shares). The predicate is intentionally a coarse FILTER —
//! it rules out USB drives, ordinary mode-0700 dirs, regular project
//! folders, etc. The actual restricted-state UI only kicks in when both:
//!
//! 1. `is_potentially_tcc_restricted(path)` returns `true`, AND
//! 2. We've observed a `PermissionDenied` accessing that path.
//!
//! The double check keeps false-positives out: a USB drive with weird
//! permissions never gets the "TCC-restricted" treatment because it's not
//! on the hard-coded list.
//!
//! On non-macOS platforms TCC doesn't exist, so both predicates always
//! return `false`. Callers don't need to cfg-guard.

use std::path::Path;
use std::sync::OnceLock;

/// Home-relative prefixes for paths that macOS guards via TCC. A path
/// matches when, after home expansion, the input equals OR is a descendant
/// of one of these (component-wise prefix — not a string `starts_with`).
///
/// Sources:
/// - Per-folder TCC services: `kTCCServiceSystemPolicy{Downloads,Documents,Desktop,Pictures,Movies,Music}Folder`
/// - FDA-gated paths: Safari/Mail/Messages history live in `~/Library/`
/// - FileProvider TCC: `~/Library/Mobile Documents/com~apple~CloudDocs` (iCloud) + `~/Library/CloudStorage` (Dropbox/Drive/etc.)
/// - SystemPolicyAppData: `~/Library/Containers` + `~/Library/Group Containers`
const HOME_RELATIVE_PREFIXES: &[&str] = &[
    // Per-folder TCC services
    "Downloads",
    "Documents",
    "Desktop",
    "Pictures",
    "Movies",
    "Music",
    // Require FDA
    "Library/Safari",
    "Library/Mail",
    "Library/Messages",
    // iCloud Drive (FileProvider)
    "Library/Mobile Documents/com~apple~CloudDocs",
    // Third-party cloud storage (FileProvider)
    "Library/CloudStorage",
    // SystemPolicyAppData (third-party app containers — broad, gated by the EACCES check at the call site)
    "Library/Containers",
    "Library/Group Containers",
];

/// Cached `$HOME` path. Set once at first call to `is_potentially_tcc_restricted`
/// to avoid repeated `dirs::home_dir()` syscalls.
static HOME_DIR: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();

fn home_dir() -> Option<&'static Path> {
    HOME_DIR.get_or_init(dirs::home_dir).as_deref()
}

/// Returns `true` if `path` is on the hard-coded list of paths macOS *may*
/// restrict via TCC. See module-level doc for the policy.
///
/// `false` on non-macOS platforms and when `$HOME` is unset.
pub fn is_potentially_tcc_restricted(path: &Path) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    let Some(home) = home_dir() else {
        return false;
    };
    let Ok(rest) = path.strip_prefix(home) else {
        // Path isn't under $HOME — check the network-volume branch.
        return is_network_volume_path(path);
    };
    HOME_RELATIVE_PREFIXES
        .iter()
        .any(|prefix| rest == Path::new(prefix) || rest.starts_with(prefix))
}

/// Returns `true` for `/Volumes/<share>` (or descendants) where the
/// underlying filesystem is one of the network types macOS gates via the
/// `SystemPolicyNetworkVolumes` TCC service: `smbfs`, `afpfs`, `nfs`.
///
/// Uses `libc::statfs` to read the fs type. Cheap (one syscall) but does
/// touch the filesystem, so don't call in tight loops without dedup.
///
/// `false` on non-macOS platforms.
pub fn is_network_volume_path(path: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        let s = path.to_string_lossy();
        if !s.starts_with("/Volumes/") {
            return false;
        }
        // We need to statfs the *root* of the volume (`/Volumes/<share>`),
        // not arbitrary descendants — statfs walks parents on its own, but
        // doing it at the volume root is cleanest and avoids triggering
        // anything inside the share.
        let mut comps = path.components();
        // skip leading `/`
        let _ = comps.next();
        // `/Volumes`
        let _ = comps.next();
        // `<share>`
        let share = match comps.next() {
            Some(c) => c.as_os_str(),
            None => return false,
        };
        let volume_root = std::path::PathBuf::from("/Volumes").join(share);
        fs_type_is_network(&volume_root)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        false
    }
}

#[cfg(target_os = "macos")]
fn fs_type_is_network(path: &Path) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let Ok(cpath) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    let mut buf: std::mem::MaybeUninit<libc::statfs> = std::mem::MaybeUninit::uninit();
    let rc = unsafe { libc::statfs(cpath.as_ptr(), buf.as_mut_ptr()) };
    if rc != 0 {
        return false;
    }
    let s = unsafe { buf.assume_init() };
    let name_bytes: Vec<u8> = s
        .f_fstypename
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    let Ok(name) = std::str::from_utf8(&name_bytes) else {
        return false;
    };
    matches!(name, "smbfs" | "afpfs" | "nfs" | "cifs")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn home() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("/Users/test"))
    }

    /// Bypass the OnceLock cache for tests by calling the predicate directly
    /// with a known home. Required because OnceLock only initializes once
    /// per process.
    fn match_under_home(path: &Path, home: &Path) -> bool {
        let Ok(rest) = path.strip_prefix(home) else {
            return false;
        };
        HOME_RELATIVE_PREFIXES
            .iter()
            .any(|prefix| rest == Path::new(prefix) || rest.starts_with(prefix))
    }

    #[test]
    fn matches_known_per_folder_tcc_paths() {
        let h = home();
        for name in ["Downloads", "Documents", "Desktop", "Pictures", "Movies", "Music"] {
            assert!(match_under_home(&h.join(name), &h), "{name}");
            assert!(match_under_home(&h.join(name).join("sub/file.txt"), &h), "{name}/sub");
        }
    }

    #[test]
    fn matches_fda_paths() {
        let h = home();
        assert!(match_under_home(&h.join("Library/Safari"), &h));
        assert!(match_under_home(&h.join("Library/Safari/History.db"), &h));
        assert!(match_under_home(&h.join("Library/Mail/V10/MailData"), &h));
        assert!(match_under_home(&h.join("Library/Messages/chat.db"), &h));
    }

    #[test]
    fn matches_cloud_paths() {
        let h = home();
        assert!(match_under_home(
            &h.join("Library/Mobile Documents/com~apple~CloudDocs"),
            &h
        ));
        assert!(match_under_home(
            &h.join("Library/Mobile Documents/com~apple~CloudDocs/Photos/IMG_0001.HEIC"),
            &h
        ));
        assert!(match_under_home(&h.join("Library/CloudStorage"), &h));
        assert!(match_under_home(&h.join("Library/CloudStorage/Dropbox/file.txt"), &h));
        assert!(match_under_home(
            &h.join("Library/CloudStorage/GoogleDrive-foo@bar/x"),
            &h
        ));
    }

    #[test]
    fn matches_app_data_paths() {
        let h = home();
        assert!(match_under_home(
            &h.join("Library/Containers/com.apple.Safari/Data"),
            &h
        ));
        assert!(match_under_home(
            &h.join("Library/Group Containers/group.com.example"),
            &h
        ));
    }

    #[test]
    fn rejects_partial_name_siblings() {
        let h = home();
        // Path with a sibling-like prefix should NOT match (it's not a real subpath of `Downloads`).
        assert!(!match_under_home(&h.join("DownloadsDecoy"), &h));
        assert!(!match_under_home(&h.join("DocumentsBackup"), &h));
        assert!(!match_under_home(&h.join("Library/SafariBackup"), &h));
    }

    #[test]
    fn rejects_unrelated_paths() {
        let h = home();
        assert!(!match_under_home(&h.join("Projects"), &h));
        assert!(!match_under_home(&h.join("Code/foo.rs"), &h));
        assert!(!match_under_home(&h.join(".config"), &h));
        assert!(!match_under_home(&h.join("Library/Caches"), &h));
        assert!(!match_under_home(&h.join("Library/Logs"), &h));
        assert!(!match_under_home(
            &h.join("Library/Application Support/com.example.app"),
            &h
        ));
    }

    #[test]
    fn rejects_paths_outside_home() {
        let h = home();
        assert!(!match_under_home(Path::new("/"), &h));
        assert!(!match_under_home(Path::new("/tmp"), &h));
        assert!(!match_under_home(Path::new("/Applications"), &h));
        assert!(!match_under_home(Path::new("/System/Library/Mail"), &h));
        // Non-network /Volumes paths (USB drives) don't match per-home rules
        assert!(!match_under_home(Path::new("/Volumes/USB-stick/Downloads"), &h));
    }

    #[test]
    fn empty_path_doesnt_match() {
        let h = home();
        assert!(!match_under_home(Path::new(""), &h));
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn non_macos_always_false() {
        // The public predicate short-circuits to false on non-macOS regardless of path.
        let h = home();
        assert!(!is_potentially_tcc_restricted(&h.join("Downloads")));
        assert!(!is_network_volume_path(Path::new("/Volumes/share")));
    }

    #[test]
    fn network_volume_path_form_checks() {
        // We can't easily mount a real network share in unit tests, so just
        // verify the path-form rejection (statfs is short-circuited).
        // The positive case is covered by manual + integration testing.
        assert!(!is_network_volume_path(Path::new("/")));
        assert!(!is_network_volume_path(Path::new("/Users/test")));
        assert!(!is_network_volume_path(Path::new("/Volumes")));
        // Real path may or may not exist on the test machine; we only verify
        // it doesn't panic and returns a bool.
        let _: bool = is_network_volume_path(Path::new("/Volumes/Macintosh HD"));
    }
}
