//! Predicates for "is macOS TCC what's blocking this path?"
//!
//! TCC gates access at an ANCHOR, never per subfolder. `SystemPolicyNetworkVolumes`
//! covers a whole mounted share; the per-folder services cover `~/Downloads` and its
//! siblings; a FileProvider grant covers one cloud domain's tree. Everything below an
//! anchor rides on that one grant, so an open anchor means TCC is already satisfied
//! for the entire subtree.
//!
//! Two layers, and callers should be deliberate about which one they want:
//!
//! 1. [`tcc_anchor`] (boolean shorthand: [`is_potentially_tcc_restricted`]) is a coarse
//!    FILTER answering "which gate would cover this path, if any?". USB drives, project
//!    folders, and ordinary mode-0700 directories have no anchor. Use it when there's no
//!    observed denial to reason about, for example to decide whether stat'ing a path
//!    might raise a TCC prompt.
//! 2. [`tcc_denial_is_plausible`] is the question an observed `PermissionDenied` should
//!    ask. It finds the anchor and probes whether the anchor ITSELF is shut. An anchor
//!    that opens fine means the grant is in hand and the refusal came from the
//!    filesystem's own permissions (a root-owned `lost+found` on an SMB share, say),
//!    which no amount of Full Disk Access will change.
//!
//! Classifying a denial with layer 1 alone is how a plain remote permission problem ends
//! up telling the user to open System Settings, where there is nothing to grant.
//!
//! On non-macOS platforms TCC doesn't exist, so everything here returns `false` / `None`.
//! Callers don't need to cfg-guard.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Home-relative prefixes for paths that macOS guards via TCC. A path
/// matches when, after home expansion, the input equals OR is a descendant
/// of one of these (component-wise prefix, not a string `starts_with`).
///
/// Sources:
/// - Per-folder TCC services:
///   `kTCCServiceSystemPolicy{Downloads,Documents,Desktop,Pictures,Movies,Music}Folder`
/// - FDA-gated paths: Safari/Mail/Messages history live in `~/Library/`
/// - FileProvider TCC: `~/Library/Mobile Documents/com~apple~CloudDocs` (iCloud) +
///   `~/Library/CloudStorage` (Dropbox/Drive/etc.)
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
    // SystemPolicyAppData (third-party app containers, broad, gated by the EACCES check at the call site)
    "Library/Containers",
    "Library/Group Containers",
];

/// Home-relative prefixes whose real gate sits one component DEEPER than the prefix.
/// `~/Library/CloudStorage` lists fine with no grant at all; the FileProvider gate is
/// per DOMAIN (`.../Dropbox`, `.../GoogleDrive-you@example.com`), so the domain
/// directory is the anchor. `Library/Mobile Documents/com~apple~CloudDocs` needs no
/// entry here: that prefix already names the domain.
const FILE_PROVIDER_DOMAIN_ROOTS: &[&str] = &["Library/CloudStorage"];

/// Cached `$HOME` path. Set once at first call to [`tcc_anchor`] to avoid
/// repeated `dirs::home_dir()` syscalls.
static HOME_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

fn home_dir() -> Option<&'static Path> {
    HOME_DIR.get_or_init(dirs::home_dir).as_deref()
}

/// The path where the TCC gate covering `path` actually sits, or `None` when no gate
/// covers it. `path` is its own anchor when it IS the gated volume root or folder.
///
/// See the module doc for what an anchor means. `None` on non-macOS platforms and when
/// `$HOME` is unset.
pub fn tcc_anchor(path: &Path) -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    // Network volumes are checked FIRST: a `$HOME` that itself lives on a mounted share
    // would otherwise take the home branch and miss the gate that really applies.
    if let Some(volume_root) = network_volume_root(path) {
        return Some(volume_root);
    }
    let home = home_dir()?;
    home_relative_anchor(path.strip_prefix(home).ok()?, home)
}

/// Returns `true` if some TCC gate covers `path`. A coarse filter: it says nothing about
/// whether that gate is currently shut. Use [`tcc_denial_is_plausible`] to classify an
/// observed permission denial.
pub fn is_potentially_tcc_restricted(path: &Path) -> bool {
    tcc_anchor(path).is_some()
}

/// Returns `true` when macOS TCC is a plausible cause of a permission denial on `path`.
///
/// Requires a gate to cover `path` AND that gate to be shut. A denial below an anchor
/// that opens fine came from the filesystem's own permissions or, on a share, from the
/// file server, so pointing the user at System Settings would send them nowhere.
///
/// Costs one directory read on the anchor, so call it from error paths, not hot loops.
pub fn tcc_denial_is_plausible(path: &Path) -> bool {
    tcc_denial_is_plausible_with(path, read_is_denied)
}

/// [`tcc_denial_is_plausible`] with the anchor probe injected, so the decision logic is
/// testable without a real gated folder or a live network mount.
fn tcc_denial_is_plausible_with(path: &Path, anchor_is_shut: impl Fn(&Path) -> bool) -> bool {
    let Some(anchor) = tcc_anchor(path) else {
        return false;
    };
    // The caller observed a denial on `path`; when `path` IS the gate, that denial is
    // the probe. Re-reading it would only repeat what we already know.
    if anchor == path {
        return true;
    }
    anchor_is_shut(&anchor)
}

/// Whether reading `dir` is refused. Only `PermissionDenied` counts: a directory that's
/// missing or otherwise unreadable isn't a grant we're lacking.
///
/// Opening a directory can succeed where READING it doesn't, so `read_dir` alone is not
/// the probe. A `lost+found` on a mounted SMB share hands back a handle and only refuses
/// at the first `readdir`; probing with `read_dir` alone calls that folder open. So pull
/// the first entry too.
pub fn read_is_denied(dir: &Path) -> bool {
    let denied = |e: &std::io::Error| e.kind() == std::io::ErrorKind::PermissionDenied;
    match std::fs::read_dir(dir) {
        Err(e) => denied(&e),
        Ok(mut entries) => matches!(entries.next(), Some(Err(e)) if denied(&e)),
    }
}

/// The anchor for a `$HOME`-relative path, given `rest` (the path with `$HOME` stripped)
/// and the `home` it was stripped from. Split out so tests can drive it with a fake home,
/// which the `OnceLock` cache otherwise makes impossible.
fn home_relative_anchor(rest: &Path, home: &Path) -> Option<PathBuf> {
    let prefix = HOME_RELATIVE_PREFIXES
        .iter()
        .find(|prefix| rest == Path::new(prefix) || rest.starts_with(prefix))?;
    let mut anchor = home.join(prefix);
    if FILE_PROVIDER_DOMAIN_ROOTS.contains(prefix) {
        // Reach one level in for the domain. A path that stops at the bare prefix has no
        // domain component, and stays its own anchor.
        if let Some(domain) = rest.strip_prefix(prefix).ok().and_then(|r| r.components().next()) {
            anchor.push(domain);
        }
    }
    Some(anchor)
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
    network_volume_root(path).is_some()
}

/// The mounted network share `path` sits on, as `/Volumes/<share>`. See
/// [`is_network_volume_path`] for the filesystem types this recognizes.
fn network_volume_root(path: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let s = path.to_string_lossy();
        if !s.starts_with("/Volumes/") {
            return None;
        }
        // We need to statfs the *root* of the volume (`/Volumes/<share>`),
        // not arbitrary descendants; statfs walks parents on its own, but
        // doing it at the volume root is cleanest and avoids triggering
        // anything inside the share.
        let mut comps = path.components();
        // skip leading `/`
        let _ = comps.next();
        // `/Volumes`
        let _ = comps.next();
        // `<share>`
        let share = comps.next()?.as_os_str();
        let volume_root = PathBuf::from("/Volumes").join(share);
        fs_type_is_network(&volume_root).then_some(volume_root)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        None
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
    // SAFETY: `cpath` is a valid null-terminated C string, and `buf` is a writable `statfs` slot.
    let rc = unsafe { libc::statfs(cpath.as_ptr(), buf.as_mut_ptr()) };
    if rc != 0 {
        return false;
    }
    // SAFETY: statfs succeeded (`rc == 0`), so `buf` is initialized.
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
    use crate::testing::TestDir;
    use std::path::PathBuf;

    fn home() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("/Users/test"))
    }

    /// Bypass the OnceLock cache for tests by calling the anchor lookup directly
    /// with a known home. Required because OnceLock only initializes once
    /// per process.
    fn anchor_under_home(path: &Path, home: &Path) -> Option<PathBuf> {
        home_relative_anchor(path.strip_prefix(home).ok()?, home)
    }

    fn match_under_home(path: &Path, home: &Path) -> bool {
        anchor_under_home(path, home).is_some()
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

    // ── anchors ──────────────────────────────────────────────────────────

    #[test]
    fn per_folder_anchor_is_the_gated_folder_itself() {
        let h = home();
        assert_eq!(anchor_under_home(&h.join("Downloads"), &h), Some(h.join("Downloads")));
        assert_eq!(
            anchor_under_home(&h.join("Documents/taxes/2025/receipt.pdf"), &h),
            Some(h.join("Documents")),
            "a descendant anchors on the gated folder, not on itself"
        );
    }

    #[test]
    fn cloud_storage_anchor_reaches_the_provider_domain() {
        let h = home();
        // `~/Library/CloudStorage` itself needs no grant; each FileProvider domain
        // under it does, so the domain directory is the anchor.
        assert_eq!(
            anchor_under_home(&h.join("Library/CloudStorage/Dropbox/notes/todo.md"), &h),
            Some(h.join("Library/CloudStorage/Dropbox"))
        );
        assert_eq!(
            anchor_under_home(&h.join("Library/CloudStorage/GoogleDrive-x@y.com/f"), &h),
            Some(h.join("Library/CloudStorage/GoogleDrive-x@y.com"))
        );
        assert_eq!(
            anchor_under_home(&h.join("Library/CloudStorage"), &h),
            Some(h.join("Library/CloudStorage")),
            "the bare prefix has no domain to reach into"
        );
        // iCloud's prefix already names the domain, so it anchors on the prefix.
        assert_eq!(
            anchor_under_home(
                &h.join("Library/Mobile Documents/com~apple~CloudDocs/Photos/x.heic"),
                &h
            ),
            Some(h.join("Library/Mobile Documents/com~apple~CloudDocs"))
        );
    }

    // ── the anchor probe ─────────────────────────────────────────────────

    /// A directory that reads fine, and one that isn't there at all, are both "not
    /// refused". Only an actual refusal counts as a gate we're locked out of.
    ///
    /// The refusal case worth guarding can't be built here: a `lost+found` on an SMB
    /// share opens fine and only refuses at the first `readdir`, which is why
    /// `read_is_denied` pulls an entry. Reproducing it needs a live mount, so it's
    /// covered by `scripts/soak-smb.sh` and by the mode-0000 case below (which refuses
    /// at open, the other half of the same contract).
    #[test]
    fn read_is_denied_only_on_a_real_refusal() {
        let scratch = TestDir::new("tcc_probe_open");
        assert!(!read_is_denied(&scratch), "a readable dir is not refused");
        assert!(
            !read_is_denied(&scratch.join("no-such-child")),
            "a missing dir is not a grant we're lacking"
        );
    }

    #[cfg(unix)]
    #[test]
    fn read_is_denied_on_a_mode_0000_dir() {
        use std::os::unix::fs::PermissionsExt as _;

        let scratch = TestDir::new("tcc_probe_shut");
        let dir = scratch.join("shut");
        std::fs::create_dir(&dir).expect("create probe dir");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).expect("chmod 000");

        // Root bypasses mode bits entirely, and some filesystems ignore them outright,
        // so there'd be nothing to assert. Probing the dir directly answers "can this
        // process still read it?" for both cases at once, where an euid test only covers
        // the first (and would drag `libc`, a macOS-only dependency here, onto Linux).
        if std::fs::read_dir(&dir).is_ok() {
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("restore mode");
            return;
        }

        let denied = read_is_denied(&dir);

        // Restore before asserting: `TestDir`'s Drop can't remove a mode-0000 child, and
        // a failed assertion would otherwise leak the scratch tree.
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("restore mode");
        assert!(denied, "a mode-0000 dir must read as refused");
    }

    // ── denial plausibility ──────────────────────────────────────────────

    /// The bug this guards: TCC gates a whole tree, so a denial below an anchor that
    /// opens fine came from the filesystem, not from macOS. Telling the user to grant
    /// Full Disk Access would send them after a permission that's already theirs.
    // `tcc_anchor` short-circuits to `None` off macOS, where TCC doesn't exist.
    #[cfg(target_os = "macos")]
    #[test]
    fn denial_below_an_open_anchor_is_not_tcc() {
        let h = home();
        let path = h.join("Documents/root-owned-dir");
        assert!(
            match_under_home(&path, &h),
            "the path must be TCC-classified for this test to be meaningful"
        );
        assert!(!tcc_denial_is_plausible_with(&path, |_| false));
    }

    // `tcc_anchor` short-circuits to `None` off macOS, where TCC doesn't exist.
    #[cfg(target_os = "macos")]
    #[test]
    fn denial_below_a_shut_anchor_is_tcc() {
        let h = home();
        assert!(tcc_denial_is_plausible_with(&h.join("Documents/taxes"), |_| true));
    }

    /// A denial ON the gate needs no probe: the denial we're classifying IS the probe.
    // `tcc_anchor` short-circuits to `None` off macOS, where TCC doesn't exist.
    #[cfg(target_os = "macos")]
    #[test]
    fn denial_on_the_anchor_itself_skips_the_probe() {
        let h = home();
        assert!(tcc_denial_is_plausible_with(&h.join("Documents"), |_| {
            panic!("the anchor must not be re-probed when it is the denied path")
        }));
    }

    // `tcc_anchor` short-circuits to `None` off macOS, where TCC doesn't exist.
    #[cfg(target_os = "macos")]
    #[test]
    fn denial_on_an_ungated_path_is_never_tcc() {
        // No anchor, so the probe never runs however permissive it is.
        assert!(!tcc_denial_is_plausible_with(Path::new("/tmp/cmdr-plain/x"), |_| true));
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
