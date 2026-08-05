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

use crate::indexing::store::IndexStore;
use crate::indexing::writer::WriteMessage;

// ── System directory exclusions ──────────────────────────────────────

/// Common system, build, and cache directory names: machine output nobody searches
/// for and nobody ranks as important.
///
/// The indexer's policy, read by three consumers so they can't drift: search
/// applies it when `SearchQuery::exclude_system_dirs` isn't `Some(false)`, the
/// importance scorer treats a match as known-unimportant, and the folder-size
/// tooltip command skips a match when it sums a directory. ❌ Match on NAME
/// EQUALITY, never a substring (`no-string-matching`): a folder called
/// `my-build-notes` is not build output.
///
/// ❌ The SCANNER is not one of them, and never should be (Decision 6 of
/// `docs/specs/unindexed-search-plan.md`): this tier is large and sits under
/// folders people search, so skipping it at walk time would stamp coverage on
/// parents whose `dir_stats` are badly short. `should_exclude` below is the
/// structural policy the walk does apply, and it shares no names with this list.
pub const SYSTEM_DIR_EXCLUDES: &[&str] = &[
    // Package managers & build tools
    "node_modules",
    ".pnpm-store",
    ".npm",
    ".yarn",
    ".cargo",
    ".m2",
    ".gradle",
    // VCS
    ".git",
    ".svn",
    ".hg",
    // Python
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    // JS/TS build output
    "build",
    "dist",
    ".next",
    ".nuxt",
    ".cache",
    ".parcel-cache",
    "target",
    // macOS system & caches
    "Caches",
    "CacheStorage",
    "Cache",
    "GPUCache",
    "ScriptCache",
    "GrShaderCache",
    "ShaderCache",
    "Logs",
    "Cookies",
    "WebKit",
    "Saved Application State",
    ".Trash",
    ".Spotlight-V100",
    ".fseventsd",
    ".DocumentRevisions-V100",
    // IDE workspace caches
    "workspaceStorage",
    "DerivedData",
];

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

/// Whether a walk runs the structural exclusion policy over the children it
/// finds.
///
/// Layered OVER an [`ExclusionScope`], never a second source of one: the scope
/// says WHICH rules this volume kind gets (derived from the kind, never from
/// `is_volume_root`), and this says whether they run at all. Which one a walk
/// takes is decided by what the walk IS — see `ScanRoot::exclusions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExclusionMode {
    /// Gate every discovered child through `should_exclude`.
    Apply,
    /// Index whatever the walk finds. For a walk pointed at a directory
    /// something else already gated.
    Off,
}

/// A `should_exclude` check's scope: which [`ExclusionTier`] applies AND where the
/// volume root sits, because one rule (the root-position pseudo-filesystem skip,
/// [`is_pseudo_fs_at_volume_root`]) keys on root POSITION rather than on the path
/// string alone. Every caller has to supply one, so no path can be gated without
/// saying which volume it's being gated for.
///
/// Mirrors [`IndexPathSpace`](crate::indexing::paths::routing::IndexPathSpace)'s
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
/// [`file_provider`](super::file_provider).
fn is_file_provider_domain_root(path: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        super::file_provider::domain_id_for_dir(path).is_some()
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

// ── The policy version an index was built against ────────────────────

/// Whether this index's coverage claims predate the exclusion policy this build
/// applies, so nothing in it may be trusted as covered.
///
/// An excluded directory gets no `entries` row, so it drives no ancestor's
/// `min_subtree_epoch` to `0` and its parents read as fully covered. That answer
/// is only as true as the policy the rows were written under. REMOVE a name from
/// [`EXCLUDED_PREFIXES`], [`JUNK_BASENAMES`], or [`PSEUDO_FS_BASENAMES`] and the
/// subtrees it used to skip stay row-less while their parents keep claiming
/// coverage: permanently invisible to search, with nothing to trigger a re-walk.
/// A mismatch answers "yes", and so does an absent stamp or a read failure — a
/// redundant walk costs time, a skipped one loses files.
pub(in crate::indexing) fn index_predates_exclusion_policy(conn: &rusqlite::Connection) -> bool {
    let stored = IndexStore::get_meta(conn, crate::indexing::store::EXCLUSION_POLICY_KEY);
    !matches!(stored, Ok(Some(ref v)) if *v == exclusion_policy_fingerprint())
}

/// The message that stamps an index as built against the current exclusion policy.
///
/// ❌ Send it ONLY right after a `TruncateData`. That's the one moment the DB
/// provably holds no row beneath a directory today's scanner refuses to walk; a
/// reconcile or a scoped fill never re-lists the rest of the volume, so it can't
/// clear what an older policy let in.
pub(in crate::indexing) fn exclusion_policy_stamp_message() -> WriteMessage {
    WriteMessage::UpdateMeta {
        key: crate::indexing::store::EXCLUSION_POLICY_KEY.to_string(),
        value: exclusion_policy_fingerprint(),
    }
}

/// A stable fingerprint of the compile-time exclusion constants, persisted per
/// index under `store::EXCLUSION_POLICY_KEY`.
///
/// Content-derived, so editing any of the lists re-arms every existing index with
/// no version constant for anyone to forget to bump. FNV-1a rather than
/// `DefaultHasher` because the value goes to disk and must not shift with a
/// toolchain upgrade. Platform-specific by construction (the constants are), which
/// is fine: an index DB never moves between platforms.
///
/// `CMDR_E2E_START_PATH` is deliberately NOT folded in. It narrows the effective
/// policy at runtime, but it's a per-run fixture path rather than a shipped rule,
/// and folding it in would write a machine-specific value into every E2E index.
pub(in crate::indexing) fn exclusion_policy_fingerprint() -> String {
    // Each list is preceded by its own name, so moving a name from one list to
    // another changes the fingerprint even though the flat set of names didn't.
    let mut parts: Vec<&str> = vec!["prefixes"];
    parts.extend_from_slice(EXCLUDED_PREFIXES);
    parts.push("junk");
    parts.extend_from_slice(JUNK_BASENAMES);
    parts.push("pseudo_fs");
    parts.extend_from_slice(PSEUDO_FS_BASENAMES);
    #[cfg(target_os = "macos")]
    {
        parts.push("firmlinked");
        parts.extend_from_slice(FIRMLINKED_SYSTEM_PREFIXES);
    }
    fingerprint_of(&parts)
}

/// FNV-1a over newline-separated parts, as 16 hex digits.
///
/// Split out from [`exclusion_policy_fingerprint`] so the mixing is testable
/// against a fixed input. A fingerprint the caller feeds its own constants to can
/// only be tested symmetrically — stamp with it, read with it, agree — and that
/// agrees just as happily with a broken hash that collides two different policies
/// into one value, which would silently skip the re-walk the whole mechanism
/// exists for. `the_policy_fingerprint_mixes_its_input` pins this against a golden
/// over a test-only input, so it needs no maintenance when the real lists change.
fn fingerprint_of(parts: &[&str]) -> String {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    for part in parts {
        for byte in part.bytes().chain(std::iter::once(b'\n')) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    format!("{hash:016x}")
}

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

    // ── The exclusion-policy stamp ───────────────────────────────────

    /// A fresh temp DB carrying the index schema, for the stamp tests.
    fn temp_index() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("index.db");
        IndexStore::open(&db_path).expect("create index schema");
        (dir, db_path)
    }

    /// An index with no stamp was built under unknown rules, so nothing in it may
    /// be trusted as covered. The alternative — assuming the current policy —
    /// would quietly hide every subtree an older policy excluded.
    #[test]
    fn an_unstamped_index_predates_the_exclusion_policy() {
        let (_dir, db_path) = temp_index();
        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        assert!(index_predates_exclusion_policy(&conn));
    }

    /// The scan-start sequence stamps the index for real, through the writer. What
    /// this pins is the wiring: a message that never reaches the DB would leave
    /// every search walking its whole scope forever, with nothing failing.
    #[test]
    fn a_truncating_walk_stamps_the_policy_through_the_writer() {
        let (_dir, db_path) = temp_index();
        {
            let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
            assert!(index_predates_exclusion_policy(&conn), "test setup: an unstamped index");
        }

        // What `lifecycle/manager/start.rs` and `lifecycle/network_scan.rs` send
        // before a fresh walk.
        let writer = crate::indexing::writer::IndexWriter::spawn(&db_path, crate::NoopEventSink::shared())
            .expect("spawn writer");
        writer.send(WriteMessage::TruncateData).expect("truncate");
        writer.send(exclusion_policy_stamp_message()).expect("stamp");
        writer.flush_blocking().expect("flush");
        writer.shutdown();

        let conn = IndexStore::open_read_connection(&db_path).expect("read conn");
        assert!(
            !index_predates_exclusion_policy(&conn),
            "a walk under the current policy leaves the index trustworthy"
        );
    }

    /// Editing any of the lists re-arms every existing index, because the stamp
    /// records the policy's CONTENTS rather than a bare "done" flag. That's what
    /// makes REMOVING a name safe: the subtrees it used to hide can't stay
    /// invisible behind a stale claim of coverage.
    #[test]
    fn a_stamp_from_a_different_policy_re_arms_the_walk() {
        let (_dir, db_path) = temp_index();
        let conn = IndexStore::open_write_connection(&db_path).expect("write conn");
        IndexStore::update_meta(&conn, crate::indexing::store::EXCLUSION_POLICY_KEY, "0123456789abcdef")
            .expect("stamp an older policy");
        assert!(index_predates_exclusion_policy(&conn));
    }

    /// The fingerprint is a pure function of compile-time constants, so it can't
    /// drift between the read that decides and the write that stamps.
    #[test]
    fn the_policy_fingerprint_is_stable() {
        assert_eq!(exclusion_policy_fingerprint(), exclusion_policy_fingerprint());
        assert_eq!(exclusion_policy_fingerprint().len(), 16, "a 64-bit FNV-1a in hex");
    }

    /// The hash actually MIXES, pinned against a golden over a fixed input.
    ///
    /// Everything else about the stamp is symmetric — write it, read it, compare —
    /// and a hash that collided two different policies into one value would pass
    /// every one of those tests while silently skipping the re-walk a policy change
    /// is supposed to trigger. The input here is test-only, so the golden never
    /// needs touching when the real lists change.
    #[test]
    fn the_policy_fingerprint_mixes_its_input() {
        assert_eq!(fingerprint_of(&["a", "b"]), "78ed6781f136a14e");
        assert_ne!(
            fingerprint_of(&["a", "b"]),
            fingerprint_of(&["b", "a"]),
            "order has to matter, or moving a name between lists reads as no change"
        );
        assert_ne!(
            fingerprint_of(&["a", "b"]),
            fingerprint_of(&["ab"]),
            "the separator has to matter, or two lists concatenate ambiguously"
        );
    }
}
