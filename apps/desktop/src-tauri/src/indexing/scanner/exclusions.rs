//! Scan exclusion policy in two tiers: (a) boot-disk absolute-path prefixes
//! skipped only when scanning the boot disk from `/` (platform-specific, plus the
//! firmlinked-`/System` allowlist), and (b) per-volume skips applied at any scan
//! root — junk basenames, plus a pseudo-filesystem tree sitting directly at the
//! volume root ([`is_pseudo_fs_at_volume_root`]).
//!
//! `should_exclude` is the single exclusion gate for every code path (scanner,
//! reconciler, event-loop verification, per-navigation verifier). It takes an
//! [`ExclusionScope`], which says both which tier applies (a mount-rooted scan
//! under `/Volumes/X`, SMB, or MTP applies only tier (b); the boot-disk scan
//! applies both) and WHERE the volume root sits, since the pseudo-filesystem rule
//! keys on root position. See [`ExclusionTier`] for why the tier split exists.

use std::sync::OnceLock;

/// Which exclusion tier applies to a `should_exclude` check, derived from the
/// volume being scanned (never from `is_volume_root` — the boot `/` scan is also
/// a volume root, so that bool can't tell the two apart).
///
/// The boot disk scans from `/` and must stay on the boot volume, so it skips the
/// absolute-prefix set (`/Volumes/`, `/System/...`, `/private/var/`, ...) that
/// keeps the walk off mounted volumes and system trees. A mount-rooted volume is
/// ALREADY rooted under `/Volumes/X` (or an SMB/MTP mount) and must index
/// everything beneath it: applying those same absolute prefixes there would
/// exclude EVERY child of the scan root, yield zero rows, and let the completion
/// path write `scan_completed_at` — a silent false-complete. So a mount-rooted
/// scan applies only the per-volume junk tier (`.Spotlight-V100`, `.fseventsd`,
/// ...), which is junk on any volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExclusionTier {
    /// The boot-disk scan rooted at `/`: apply the absolute-prefix set AND the
    /// per-volume tier.
    BootDisk,
    /// A scan rooted at a mount point (`/Volumes/X`, an SMB share, an MTP store):
    /// apply only the per-volume tier, so the mount's own subtree is fully indexed.
    MountRooted,
}

/// A `should_exclude` check's scope: which [`ExclusionTier`] applies AND where the
/// volume root sits, because one rule (the root-position pseudo-filesystem skip,
/// [`is_pseudo_fs_at_volume_root`]) keys on root POSITION rather than on the path
/// string alone. Every caller has to supply one, so no path can be gated without
/// saying which volume it's being gated for.
///
/// Mirrors [`IndexPathSpace`](crate::indexing::routing::IndexPathSpace)'s
/// `mount_root`, which is where it's built from for the scan / reconcile / live
/// pipeline; the boot-disk-only callers (the verifier, event-loop verification)
/// use [`ExclusionScope::boot_disk`].
#[derive(Debug, Clone)]
pub(crate) struct ExclusionScope {
    /// `None` for the `/`-rooted boot disk; `Some(root)` for a scan rooted at that
    /// mount (`/Volumes/X`, an SMB share, an MTP store). The single source of both
    /// the tier and the volume-root position.
    mount_root: Option<String>,
    /// The filesystem questions the pseudo-filesystem rule asks about a candidate's
    /// parent, injected so tests need neither a live provider domain nor a Unix root
    /// on the machine. See [`RootProbes`].
    probes: RootProbes,
}

/// The two filesystem questions [`is_pseudo_fs_at_volume_root`] asks about a
/// candidate directory's parent. Injected as a unit ([`RootProbes::REAL`] in
/// production) so the rule stays unit-testable without a real File Provider domain
/// or a real Unix root filesystem.
///
/// Plain `fn` pointers, so [`ExclusionScope`] stays `Send + Sync + Clone` for the
/// rayon walk threads that share it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RootProbes {
    /// Is this directory a File Provider domain root (a cloud provider's or
    /// MacDroid's tree grafted into the home dir)? Domain roots are volume roots for
    /// the rule, but they're discovered mid-walk rather than known up front, so this
    /// is a probe rather than a path.
    is_domain_root: fn(&str) -> bool,
    /// Does this directory hold ALL of `proc`, `sys`, and `dev` as child
    /// directories, i.e. does it actually look like a Unix root filesystem?
    is_unix_like_root: fn(&str) -> bool,
}

impl RootProbes {
    /// The production probes.
    const REAL: Self = Self {
        is_domain_root: is_file_provider_domain_root,
        is_unix_like_root: has_pseudo_fs_trio,
    };
}

/// The production domain-root probe: a File Provider domain root carries the
/// `com.apple.file-provider-domain-id` xattr (~5 µs, no XPC, no hang risk). Always
/// `false` off macOS, which has no File Provider.
///
/// It's an OPTIMIZATION, never a guarantee: the xattr is a private Apple detail, so
/// a `false` here means "not recognized", not "proven ordinary". See
/// [`file_provider`](crate::file_system::file_provider).
fn is_file_provider_domain_root(path: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        crate::file_system::file_provider::domain_id_for_dir(path).is_some()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        false
    }
}

/// Whether `dir` holds ALL of `proc`, `sys`, and `dev` as child DIRECTORIES: the
/// corroboration that makes the pseudo-filesystem rule safe (see
/// [`is_pseudo_fs_at_volume_root`]).
///
/// Three `symlink_metadata` calls, never a directory enumeration, so the cost
/// doesn't scale with how big the root is. It doesn't follow symlinks: a symlink
/// named `proc` is not the real thing (an Android root has a symlink `d` alongside
/// its real `proc`, `sys`, and `dev`).
fn has_pseudo_fs_trio(dir: &str) -> bool {
    PSEUDO_FS_BASENAMES
        .iter()
        .all(|name| std::fs::symlink_metadata(std::path::Path::new(dir).join(name)).is_ok_and(|meta| meta.is_dir()))
}

impl ExclusionScope {
    /// The `/`-rooted boot-disk scope: both tiers apply, and `/` is the volume root.
    pub(crate) fn boot_disk() -> Self {
        Self {
            mount_root: None,
            probes: RootProbes::REAL,
        }
    }

    /// A scope rooted at `mount_root` (`/Volumes/X`, an SMB share, an MTP store):
    /// the per-volume tier only, with `mount_root` as the volume root.
    pub(crate) fn mount_rooted(mount_root: impl Into<String>) -> Self {
        Self {
            mount_root: Some(mount_root.into()),
            probes: RootProbes::REAL,
        }
    }

    /// Swap the filesystem probes (tests only), so the pseudo-filesystem rule can be
    /// exercised without a real provider domain or a real Unix root on the machine.
    #[cfg(test)]
    pub(crate) fn with_probes(mut self, is_domain_root: fn(&str) -> bool, is_unix_like_root: fn(&str) -> bool) -> Self {
        self.probes = RootProbes {
            is_domain_root,
            is_unix_like_root,
        };
        self
    }

    /// Which tier applies: `BootDisk` for the `/`-rooted scan, `MountRooted` otherwise.
    pub(crate) fn tier(&self) -> ExclusionTier {
        if self.mount_root.is_some() {
            ExclusionTier::MountRooted
        } else {
            ExclusionTier::BootDisk
        }
    }

    /// The volume root this scope is rooted at: the mount root, or `/` for the boot
    /// disk.
    pub(in crate::indexing) fn volume_root(&self) -> &str {
        self.mount_root.as_deref().unwrap_or("/")
    }

    /// The mount root, or `None` for the `/`-rooted boot disk. `IndexPathSpace`
    /// stores its space AS a scope and reads the mount root back through here, so
    /// "where is this volume rooted" has exactly one home.
    pub(in crate::indexing) fn mount_root(&self) -> Option<&str> {
        self.mount_root.as_deref()
    }
}

// ── Exclusion prefixes ──────────────────────────────────────────────

/// macOS: absolute path prefixes to skip during scanning.
#[cfg(target_os = "macos")]
pub(in crate::indexing) const EXCLUDED_PREFIXES: &[&str] = &[
    "/System/Volumes/Data/",
    "/System/Volumes/VM/",
    "/System/Volumes/Preboot/",
    "/System/Volumes/Update/",
    "/System/Volumes/xarts/",
    "/System/Volumes/iSCPreboot/",
    "/System/Volumes/Hardware/",
    "/Volumes/", // Skip mounted volumes (network shares, external drives) -- index boot volume only
    "/private/var/",
    "/Library/Caches/",
    "/dev/",
    "/proc/",
];

/// Linux: virtual filesystems and system directories to skip during scanning.
#[cfg(target_os = "linux")]
pub(in crate::indexing) const EXCLUDED_PREFIXES: &[&str] = &[
    "/dev/",
    "/proc/",
    "/sys/",
    "/run/",
    "/snap/",
    "/lost+found/",
    "/mnt/",   // Skip manual mount points -- index the root filesystem only
    "/media/", // Skip removable media
    "/boot/",
    "/tmp/",
    "/var/tmp/",
    "/var/cache/",
    "/var/log/",
    "/var/run/",
];

/// Fallback exclusion prefixes for other platforms.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(in crate::indexing) const EXCLUDED_PREFIXES: &[&str] = &["/dev/", "/proc/"];

/// The subset of [`EXCLUDED_PREFIXES`] that marks a MOUNTED EXTERNAL VOLUME
/// (`/Volumes/` on macOS; `/mnt/`, `/media/` on Linux), as opposed to the system
/// trees and caches the boot scan also skips (`/System/…`, `/private/var/`, …).
///
/// Read routing uses this — NOT a raw `/Volumes/` literal — to decide when a path
/// belongs to a separate per-mount index rather than `root`'s: a path under one of
/// these is a subtree the boot-disk scan deliberately disowns, so its owning
/// external drive's index is the sole source of its dir-stats and status. A path
/// NOT under one of these (a boot-disk path, or a cloud-drive folder in the home
/// dir) stays on `root`, whose index owns it. Single-sourced with the scan
/// exclusions via the `external_mount_prefixes_are_excluded` test, so the two
/// can't drift.
#[cfg(target_os = "macos")]
pub(in crate::indexing) const EXTERNAL_MOUNT_PREFIXES: &[&str] = &["/Volumes/"];
#[cfg(target_os = "linux")]
pub(in crate::indexing) const EXTERNAL_MOUNT_PREFIXES: &[&str] = &["/mnt/", "/media/"];
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(in crate::indexing) const EXTERNAL_MOUNT_PREFIXES: &[&str] = &[];

/// Whether `path` sits on a mounted external volume ([`EXTERNAL_MOUNT_PREFIXES`]),
/// so it belongs to that mount's own index rather than `root`'s. Pure string work
/// (no syscall), safe on the enrichment / dir-stats hot path. A cheap fast-reject
/// for the common boot-disk / cloud-drive path: it returns `false` before routing
/// ever touches the `VolumeManager` registry.
pub(in crate::indexing) fn is_on_mounted_external_volume(path: &str) -> bool {
    EXTERNAL_MOUNT_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix) || path == prefix.trim_end_matches('/'))
}

/// Per-volume junk directory basenames skipped at ANY scan root (both the boot
/// disk and a mount-rooted volume). macOS seeds these into every volume's root;
/// they hold OS bookkeeping, not user data. On the boot disk they sit at `/`; on
/// an external drive they sit under `/Volumes/X`, so they're matched by basename
/// (not an absolute prefix) to catch both. Harmless no-op on Linux (no such dirs).
const JUNK_BASENAMES: &[&str] = &[".Spotlight-V100", ".fseventsd", ".Trashes", ".TemporaryItems"];

/// Basenames of kernel pseudo-filesystems, skipped when they sit DIRECTLY at a
/// volume root (see [`is_pseudo_fs_at_volume_root`]). These trees are synthesized
/// per-read, are effectively infinite, and hold no user data.
const PSEUDO_FS_BASENAMES: &[&str] = &["proc", "sys", "dev"];

/// macOS: `/System/` paths reachable via firmlinks (from `/usr/share/firmlinks`).
/// These are the ONLY `/System/` subdirectories we allow through the exclusion filter.
#[cfg(target_os = "macos")]
pub(in crate::indexing) const FIRMLINKED_SYSTEM_PREFIXES: &[&str] = &[
    "/System/Library/Caches",
    "/System/Library/Assets",
    "/System/Library/PreinstalledAssets",
    "/System/Library/AssetsV2",
    "/System/Library/PreinstalledAssetsV2",
    "/System/Library/CoreServices/CoreTypes.bundle/Contents/Library",
    "/System/Library/Speech",
];

// ── Helpers ──────────────────────────────────────────────────────────

/// Whether the path's final component is a per-volume junk directory
/// ([`JUNK_BASENAMES`]). Matched on the basename so it catches the dir at the
/// boot root (`/.Spotlight-V100`) and under a mount (`/Volumes/X/.Spotlight-V100`)
/// alike. A user folder that merely contains a junk name as a substring is not
/// matched.
fn is_junk_basename(path_str: &str) -> bool {
    std::path::Path::new(path_str)
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| JUNK_BASENAMES.contains(&name))
}

/// Whether `path_str` is a kernel pseudo-filesystem tree sitting DIRECTLY at the
/// root of a Unix-like filesystem, so it's skipped in EVERY [`ExclusionTier`].
///
/// Both halves are load-bearing, and either one alone would be wrong:
///
/// - **Root POSITION**, so a user's `~/projects/myapp/proc` (somebody's source
///   directory) stays indexed and only `<volume root>/proc` goes. A volume root is
///   `/`, a `/Volumes/X` mount, an SMB or MTP scan root (all of them
///   [`ExclusionScope::volume_root`]), or a File Provider domain root, which is
///   grafted into the home dir mid-walk and so needs a probe.
/// - **Corroboration that the root really is a Unix filesystem**: all three of
///   `proc`, `sys`, and `dev` present as sibling directories ([`has_pseudo_fs_trio`]).
///   The name alone is far too loose, because `dev` is an extremely ordinary name for
///   a real folder: without this, a developer's `~/Library/CloudStorage/Dropbox/dev`
///   (whose parent IS a domain root) or a `dev` at the top of a USB stick would
///   vanish from the index and from folder sizes with no error at all, and a wrong
///   size nobody is told about is worse than a slow walk. All three co-occurring is
///   diagnostic; any one alone is just a folder name.
///
/// **The name test runs FIRST, before any probe**, so the syscalls fire only for
/// directories actually called `proc`, `sys`, or `dev` (at most three per volume
/// root), never per scanned directory.
///
/// Why it matters: MacDroid mounts an Android phone as a File Provider domain, and
/// that phone's Linux `proc/<pid>/task/<tid>/{attr,ns,fd,net,map_files}` tree cost
/// ~454 s of a measured 21m49s reconcile walk (~35%). Its root lists `proc`, `sys`,
/// and `dev` among `bin`, `etc`, `sdcard`, …, so it corroborates; a cloud drive's
/// root never does. Only the boot volume's `/proc` was caught before, as an
/// absolute prefix.
fn is_pseudo_fs_at_volume_root(path_str: &str, scope: &ExclusionScope) -> bool {
    let path = std::path::Path::new(path_str);
    let is_pseudo_fs_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| PSEUDO_FS_BASENAMES.contains(&name));
    if !is_pseudo_fs_name {
        return false;
    }
    let Some(parent) = path.parent().and_then(|p| p.to_str()) else {
        return false;
    };
    let sits_at_a_volume_root = trim_trailing_slash(parent) == trim_trailing_slash(scope.volume_root())
        // The domain probe is a syscall, and a mount-rooted scope can sit on a
        // network mount where any syscall blocks indefinitely. It's also pointless
        // there: providers register their domains in the home dir, on the boot disk.
        // So probe under the boot-disk tier only, where the path is local by
        // construction.
        || (scope.tier() == ExclusionTier::BootDisk && (scope.probes.is_domain_root)(parent));

    sits_at_a_volume_root && (scope.probes.is_unix_like_root)(parent)
}

/// A path without its trailing slash, except for bare `/` (which IS its root).
/// Volume roots and scanned paths reach us in both forms.
fn trim_trailing_slash(path: &str) -> &str {
    match path.trim_end_matches('/') {
        "" => "/",
        trimmed => trimmed,
    }
}

/// Returns the E2E allowlist path from `CMDR_E2E_START_PATH`, if set.
///
/// When running E2E tests, the fixture directory may be under an excluded prefix
/// (for example, `/tmp/cmdr-e2e-*` on Linux where `/tmp/` is excluded). This allowlist
/// ensures the scanner, reconciler, verifier, and event loop all include the fixture path.
pub(in crate::indexing) fn e2e_allowlist_path() -> Option<&'static str> {
    static E2E_PATH: OnceLock<Option<String>> = OnceLock::new();
    E2E_PATH
        .get_or_init(|| {
            let raw = std::env::var("CMDR_E2E_START_PATH").ok()?;
            // Canonicalize to resolve symlinks (macOS: /tmp → /private/tmp).
            // The process_read_dir callback sees raw filesystem paths BEFORE
            // firmlink normalization, so the E2E path must match the canonical
            // form. Falls back to raw if canonicalize fails (path not yet created).
            let path = std::fs::canonicalize(&raw)
                .ok()
                .and_then(|p| p.to_str().map(String::from))
                .unwrap_or_else(|| raw.clone());
            log::debug!("E2E scan restriction: only indexing under {path}");
            Some(path)
        })
        .as_deref()
}

/// Check if a path should be excluded from scanning, given the scan's
/// [`ExclusionScope`]. Tier (b) junk basenames are skipped under both scopes;
/// tier (a) absolute prefixes only under [`ExclusionTier::BootDisk`].
pub(in crate::indexing) fn should_exclude(path_str: &str, scope: &ExclusionScope) -> bool {
    // E2E mode: restrict scanning to only the fixture path and its ancestors.
    // Without this, the scanner traverses the entire filesystem from `/` which
    // is too slow in Docker containers (Linux E2E tests time out). This bounds
    // the otherwise-unbounded boot-disk `/` scan; a mount-rooted scan is already
    // bounded to its mount, so the restriction is a boot-disk concept only.
    if scope.tier() == ExclusionTier::BootDisk
        && let Some(e2e_path) = e2e_allowlist_path()
    {
        // Allow the fixture path and its children
        if path_str.starts_with(e2e_path) {
            return false;
        }
        // Allow ancestors of the fixture path (so the scanner descends into them)
        if e2e_path.starts_with(path_str) {
            return false;
        }
        // Exclude everything else: we only care about the fixture subtree
        return true;
    }

    // Tier (b): per-volume skips, applied at any scan root — junk basenames, and a
    // pseudo-filesystem tree sitting directly at the volume root (the boot disk's,
    // a mount's, or a File Provider domain's).
    if is_junk_basename(path_str) {
        return true;
    }
    if is_pseudo_fs_at_volume_root(path_str, scope) {
        return true;
    }

    // Tier (a): boot-disk absolute-prefix exclusions apply ONLY to the `/`-rooted
    // boot scan. A mount-rooted scan sits under `/Volumes/X` and must index its
    // whole subtree, so these prefixes would exclude EVERY child of the scan root
    // → zero rows → a silent false-complete (`scan_completed_at` written on an
    // empty tree). See `ExclusionScope`.
    if scope.tier() == ExclusionTier::MountRooted {
        return false;
    }

    // Check explicit exclusion prefixes
    for prefix in EXCLUDED_PREFIXES {
        if path_str.starts_with(prefix) {
            return true;
        }
        // Also match exact prefix without trailing slash (for example, "/dev" matches "/dev/")
        let prefix_no_slash = prefix.trim_end_matches('/');
        if path_str == prefix_no_slash {
            return true;
        }
    }

    // macOS: special handling for /System/ -- skip everything except firmlinked paths
    #[cfg(target_os = "macos")]
    if path_str.starts_with("/System/") || path_str == "/System" {
        // Already covered by EXCLUDED_PREFIXES above for /System/Volumes/*
        // For remaining /System/ paths, allow only firmlinked ones
        for allowed in FIRMLINKED_SYSTEM_PREFIXES {
            if path_str.starts_with(allowed) {
                return false;
            }
        }
        return true;
    }

    false
}

/// A scanned path is a "canonicalization alias" when its firmlink/symlink-normalized form
/// (`firmlinks::normalize_path`) differs from the path itself. On macOS the root symlinks
/// `/tmp`, `/var`, and `/etc` resolve to `/private/tmp`, etc.: two distinct filesystem objects
/// (the symlink and the real directory) that canonicalize to the same key. The real directory
/// owns the canonical `(parent_id, name_folded)` slot (it carries the size and children), so the
/// scanner skips the alias. Storing it would collide on `INSERT OR IGNORE` and risks an
/// order-dependent race where the symlink wins and the real directory's subtree size is lost.
///
/// Takes the already-computed `normalized` so the scan loop doesn't normalize twice per entry.
pub(in crate::indexing) fn is_canonicalization_alias(real_path: &str, normalized: &str) -> bool {
    real_path != normalized
}

/// Build the default exclusion list for tests.
#[cfg(test)]
pub(in crate::indexing) fn default_exclusions() -> Vec<String> {
    EXCLUDED_PREFIXES.iter().map(|s| (*s).to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every external-mount prefix MUST also be a boot-disk exclusion prefix.
    /// That's the invariant read routing rests on: a path under one of these is a
    /// subtree `root`'s scan skips, so the external drive's own index is its sole
    /// owner. If someone drops `/Volumes/` from `EXCLUDED_PREFIXES`, `root` would
    /// start indexing external drives AND routing would still divert them — this
    /// test fails loudly before that ships.
    #[test]
    fn external_mount_prefixes_are_excluded() {
        for prefix in EXTERNAL_MOUNT_PREFIXES {
            assert!(
                EXCLUDED_PREFIXES.contains(prefix),
                "{prefix} must be in EXCLUDED_PREFIXES so root's scan disowns the mount",
            );
        }
    }

    /// Nothing on this machine is a File Provider domain root or a Unix root.
    fn no_probe(_path: &str) -> bool {
        false
    }

    /// Everything is a Unix root (paired with a specific domain probe, this isolates
    /// the root-POSITION half of the rule).
    fn every_dir_is_a_unix_root(_path: &str) -> bool {
        true
    }

    /// A directory named after a Linux pseudo-filesystem is skipped when it sits
    /// DIRECTLY at the volume root of a Unix-like filesystem, in every scope: the
    /// boot disk's `/proc`, an external drive's `/Volumes/X/proc`, an MTP-style scan
    /// root's. This is what keeps an Android phone's `proc/<pid>/task/<tid>/…` tree
    /// out of the index; before it, only the boot volume's absolute `/proc` prefix
    /// was caught.
    #[test]
    fn pseudo_fs_at_a_unix_like_volume_root_is_skipped_in_every_scope() {
        let unix_root = |scope: ExclusionScope| scope.with_probes(no_probe, every_dir_is_a_unix_root);
        for name in PSEUDO_FS_BASENAMES {
            assert!(
                should_exclude(&format!("/{name}"), &unix_root(ExclusionScope::boot_disk())),
                "{name} at the boot root",
            );
            assert!(
                should_exclude(
                    &format!("/Volumes/USB/{name}"),
                    &unix_root(ExclusionScope::mount_rooted("/Volumes/USB")),
                ),
                "{name} at a mount root",
            );
            assert!(
                should_exclude(
                    &format!("mtp://mtp-PIXEL9/65537/{name}"),
                    &unix_root(ExclusionScope::mount_rooted("mtp://mtp-PIXEL9/65537")),
                ),
                "{name} at an MTP scan root",
            );
        }
    }

    /// The name alone is NOT enough. Someone's Dropbox with a top-level `dev` folder
    /// (a very ordinary name for a real folder) must keep being indexed: excluding it
    /// would drop it from sizes with no error at all, which is worse than a slow walk.
    ///
    /// So the rule also demands corroboration that the root really is a Unix-like
    /// filesystem: all three of `proc`, `sys`, and `dev` present as siblings. A cloud
    /// folder has none of the other two, an Android root has all three.
    #[test]
    fn a_cloud_folder_named_dev_is_not_mistaken_for_a_pseudo_filesystem() {
        const DROPBOX: &str = "/Users/me/Library/CloudStorage/Dropbox";
        fn dropbox_is_a_domain_root(path: &str) -> bool {
            path == DROPBOX
        }
        // A real domain root, but its only pseudo-fs-shaped child is `dev`.
        let scope = ExclusionScope::boot_disk().with_probes(dropbox_is_a_domain_root, no_probe);

        for name in PSEUDO_FS_BASENAMES {
            assert!(
                !should_exclude(&format!("{DROPBOX}/{name}"), &scope),
                "{name} in a cloud drive is a user folder, not a pseudo-filesystem",
            );
        }
    }

    /// Same corroboration on a `/Volumes/X` mount root: a `dev` folder at the top of
    /// someone's USB stick or backup drive stays indexed.
    #[test]
    fn a_folder_named_dev_at_a_mount_root_is_not_mistaken_for_a_pseudo_filesystem() {
        let scope = ExclusionScope::mount_rooted("/Volumes/Backup").with_probes(no_probe, no_probe);

        for name in PSEUDO_FS_BASENAMES {
            assert!(
                !should_exclude(&format!("/Volumes/Backup/{name}"), &scope),
                "{name} at the root of a plain drive is a user folder",
            );
        }
    }

    /// The corroboration probe itself, against real directories: a temp dir holding
    /// all three of `proc`, `sys`, and `dev` reads as a Unix-like root; the same dir
    /// with only `dev` does not, and neither does a symlink standing in for `proc`
    /// (an Android root has a symlink `d` alongside its real `proc`/`sys`/`dev`).
    #[test]
    fn the_unix_root_probe_needs_all_three_real_directories() {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().to_string_lossy().into_owned();

        std::fs::create_dir(dir.path().join("dev")).expect("create dev");
        assert!(!has_pseudo_fs_trio(&root), "`dev` alone is just a folder name");

        std::fs::create_dir(dir.path().join("sys")).expect("create sys");
        assert!(!has_pseudo_fs_trio(&root), "two of three is still not a Unix root");

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path().join("sys"), dir.path().join("proc")).expect("symlink proc");
            assert!(!has_pseudo_fs_trio(&root), "a symlink named proc is not the real thing");
            std::fs::remove_file(dir.path().join("proc")).expect("remove the symlink");
        }

        std::fs::create_dir(dir.path().join("proc")).expect("create proc");
        assert!(has_pseudo_fs_trio(&root), "all three present reads as a Unix-like root");
    }

    /// The rule keys on root POSITION, not on the name: an ordinary folder that
    /// happens to be called `proc` (or `dev`, or `sys`) deeper in the tree stays
    /// indexed. `~/projects/myapp/proc` is somebody's source directory.
    #[test]
    fn pseudo_fs_below_the_volume_root_stays_indexed() {
        for name in PSEUDO_FS_BASENAMES {
            assert!(
                !should_exclude(
                    &format!("/Users/me/projects/myapp/{name}"),
                    &ExclusionScope::boot_disk()
                ),
                "{name} deep on the boot disk is an ordinary folder",
            );
            assert!(
                !should_exclude(
                    &format!("/Volumes/USB/a/{name}"),
                    &ExclusionScope::mount_rooted("/Volumes/USB"),
                ),
                "{name} one level below a mount root is an ordinary folder",
            );
            // A child INSIDE the skipped tree isn't matched by this rule either
            // (the scanner never descends into a skipped dir, so nothing else needs it).
            assert!(
                !should_exclude(
                    &format!("/{name}/1/task"),
                    &ExclusionScope::mount_rooted("/Volumes/USB")
                ),
                "{name}'s children aren't matched by the root-position rule",
            );
        }
    }

    /// A File Provider domain root (Dropbox, Google Drive, iCloud Drive, MacDroid)
    /// counts as a volume root, so the phone's `proc` tree MacDroid grafts under
    /// `~/Library/CloudStorage/MacDroid-…` is skipped: the phone's root really is a
    /// Unix root (its listing carries `proc`, `sys`, AND `dev` among `bin`, `etc`,
    /// `sdcard`, …). Both probes are injected, so this needs neither a real provider
    /// domain nor a phone attached.
    #[test]
    fn pseudo_fs_at_a_file_provider_domain_root_is_skipped() {
        const DOMAIN: &str = "/Users/me/Library/CloudStorage/MacDroid-pixel";
        fn fake_domain_probe(path: &str) -> bool {
            path == DOMAIN
        }
        let scope = ExclusionScope::boot_disk().with_probes(fake_domain_probe, every_dir_is_a_unix_root);

        assert!(
            should_exclude(&format!("{DOMAIN}/proc"), &scope),
            "a domain root's proc tree is a volume-root pseudo-filesystem",
        );
        // Same shape one level deeper is an ordinary folder: the parent isn't a domain root.
        assert!(
            !should_exclude(&format!("{DOMAIN}/sdcard/proc"), &scope),
            "only the domain root itself is a volume root",
        );
        // And with the real (macOS xattr) probe, an ordinary folder is never a domain root.
        assert!(
            !should_exclude(&format!("{DOMAIN}/proc"), &ExclusionScope::boot_disk()),
            "an unmarked parent is not a volume root",
        );
    }

    /// `is_on_mounted_external_volume` accepts a mounted-external path (mount root
    /// and anything beneath it) and rejects boot-disk and cloud-drive paths.
    #[test]
    fn mounted_external_volume_detection() {
        #[cfg(target_os = "macos")]
        {
            assert!(is_on_mounted_external_volume("/Volumes/NONAME"));
            assert!(is_on_mounted_external_volume("/Volumes/NONAME/sub/deep"));
        }
        #[cfg(target_os = "linux")]
        {
            assert!(is_on_mounted_external_volume("/media/usb"));
            assert!(is_on_mounted_external_volume("/mnt/data/sub"));
        }
        // Boot-disk and cloud-drive paths are NOT on an external mount.
        assert!(!is_on_mounted_external_volume("/Users/me/project"));
        assert!(!is_on_mounted_external_volume(
            "/Users/me/Library/CloudStorage/Dropbox/x"
        ));
        assert!(!is_on_mounted_external_volume("/"));
    }
}
