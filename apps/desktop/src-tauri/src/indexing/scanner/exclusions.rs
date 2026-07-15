//! Scan exclusion policy in two tiers: (a) boot-disk absolute-path prefixes
//! skipped only when scanning the boot disk from `/` (platform-specific, plus the
//! firmlinked-`/System` allowlist), and (b) per-volume junk basenames skipped at
//! any scan root. `should_exclude` is the single exclusion gate for every code
//! path (scanner, reconciler, event-loop verification, per-navigation verifier);
//! it takes an [`ExclusionScope`] so a mount-rooted scan (an external drive under
//! `/Volumes/X`, SMB, MTP) applies only tier (b), while the boot-disk scan applies
//! both. See [`ExclusionScope`] for why the split exists.

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
pub(crate) enum ExclusionScope {
    /// The boot-disk scan rooted at `/`: apply the absolute-prefix set AND the
    /// junk basenames.
    BootDisk,
    /// A scan rooted at a mount point (`/Volumes/X`, an SMB share, an MTP store):
    /// apply only the junk basenames, so the mount's own subtree is fully indexed.
    MountRooted,
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

/// Per-volume junk directory basenames skipped at ANY scan root (both the boot
/// disk and a mount-rooted volume). macOS seeds these into every volume's root;
/// they hold OS bookkeeping, not user data. On the boot disk they sit at `/`; on
/// an external drive they sit under `/Volumes/X`, so they're matched by basename
/// (not an absolute prefix) to catch both. Harmless no-op on Linux (no such dirs).
const JUNK_BASENAMES: &[&str] = &[".Spotlight-V100", ".fseventsd", ".Trashes", ".TemporaryItems"];

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
/// tier (a) absolute prefixes only under [`ExclusionScope::BootDisk`].
pub(in crate::indexing) fn should_exclude(path_str: &str, scope: ExclusionScope) -> bool {
    // E2E mode: restrict scanning to only the fixture path and its ancestors.
    // Without this, the scanner traverses the entire filesystem from `/` which
    // is too slow in Docker containers (Linux E2E tests time out). This bounds
    // the otherwise-unbounded boot-disk `/` scan; a mount-rooted scan is already
    // bounded to its mount, so the restriction is a boot-disk concept only.
    if scope == ExclusionScope::BootDisk
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

    // Tier (b): per-volume junk basenames — skipped at any scan root.
    if is_junk_basename(path_str) {
        return true;
    }

    // Tier (a): boot-disk absolute-prefix exclusions apply ONLY to the `/`-rooted
    // boot scan. A mount-rooted scan sits under `/Volumes/X` and must index its
    // whole subtree, so these prefixes would exclude EVERY child of the scan root
    // → zero rows → a silent false-complete (`scan_completed_at` written on an
    // empty tree). See `ExclusionScope`.
    if scope == ExclusionScope::MountRooted {
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
