//! Scan exclusion policy: the absolute-path prefixes skipped during scanning
//! (platform-specific), the firmlinked-`/System` allowlist, the E2E scan
//! restriction, and the canonicalization-alias check. `should_exclude` is the
//! single exclusion gate for every code path (scanner, reconciler, event-loop
//! verification, per-navigation verifier). Pure code movement from the former
//! monolithic `scanner.rs`.

use std::sync::OnceLock;

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
    "/.Spotlight-V100/",
    "/.fseventsd/",
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

/// Check if a path should be excluded from scanning.
pub(in crate::indexing) fn should_exclude(path_str: &str) -> bool {
    // E2E mode: restrict scanning to only the fixture path and its ancestors.
    // Without this, the scanner traverses the entire filesystem from `/` which
    // is too slow in Docker containers (Linux E2E tests time out).
    if let Some(e2e_path) = e2e_allowlist_path() {
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
