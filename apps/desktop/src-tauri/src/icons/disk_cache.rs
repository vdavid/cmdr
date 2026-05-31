//! Persistent on-disk cache for Tier-C per-path icons (`pkg:*`, `path:*`) and the
//! Tier-B `special:*` icons.
//!
//! Folder icons rarely change, so they *should* survive restarts — but a user who
//! re-icons a folder in Finder must see the update. We key each cached entry by
//! `path + a staleness token` (the folder's own mtime): a re-icon bumps the
//! folder's mtime (Finder rewrites the icon resource / `com.apple.FinderInfo`),
//! so the stored token no longer matches and we re-fetch. This gives both
//! durability and correct invalidation without watching anything.
//!
//! Layout: a flat directory of small JSON sidecar files under
//! `<data_dir>/icon-cache/`, one per icon id, named by a hex digest of the id (so
//! arbitrary path characters never leak into a filename). The in-memory
//! `ICON_CACHE` LRU stays the hot tier; this is the warm tier consulted on a hot
//! miss, before the cold NSWorkspace fetch.
//!
//! Everything here degrades gracefully: a corrupt file, a missing directory, a
//! permission error, or a path with no resolvable mtime is just a cache miss. We
//! never panic and never block the icon path on disk-cache failure — the feature
//! is purely additive.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

/// One persisted icon entry. `token` is the folder's staleness token at fetch
/// time (its mtime as whole seconds since the epoch); a mismatch on read means
/// the folder changed and the entry is stale.
#[derive(Serialize, Deserialize)]
struct DiskEntry {
    token: u64,
    data_url: String,
}

/// Resolves the on-disk icon-cache directory, creating it on first use.
///
/// Respects `CMDR_DATA_DIR` (set by `tauri-wrapper.js` in dev and by E2E
/// harnesses) the same way the secret store and settings loader do, so dev / prod
/// / per-worktree instances stay isolated. Resolved once and memoized.
static CACHE_DIR: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
    let base = if let Ok(custom) = std::env::var("CMDR_DATA_DIR") {
        if custom.is_empty() {
            return None;
        }
        PathBuf::from(custom)
    } else {
        dirs::data_dir()?.join("com.veszelovszki.cmdr")
    };
    let dir = base.join("icon-cache");
    if let Err(e) = fs::create_dir_all(&dir) {
        log::warn!(target: "icons", "Could not create icon-cache dir {}: {e}", dir.display());
        return None;
    }
    Some(dir)
});

/// The staleness token for a folder: its mtime as whole seconds since the epoch.
/// `None` when the path is gone or its mtime is unreadable (a dead mount, a
/// permission error) — the caller then treats the entry as un-cacheable / a miss
/// rather than caching against a token that can never be reproduced.
fn staleness_token(real_path: &str) -> Option<u64> {
    let modified = fs::metadata(real_path).ok()?.modified().ok()?;
    let secs = modified.duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some(secs)
}

/// Maps an icon id to its sidecar file path: `<cache-dir>/<hex-digest>.json`. We
/// digest the id rather than using it verbatim so arbitrary path characters (`/`,
/// spaces, unicode) never produce an invalid or traversal-prone filename.
fn entry_path(dir: &Path, icon_id: &str) -> PathBuf {
    dir.join(format!("{}.json", digest_hex(icon_id)))
}

/// A small, dependency-free FNV-1a 64-bit hash, rendered as zero-padded hex. Not
/// cryptographic — collision resistance only needs to be good enough that two
/// distinct icon ids don't share a sidecar file in practice, and the stored entry
/// is self-describing enough (token-checked) that a stray collision is just a
/// miss, never wrong data.
fn digest_hex(s: &str) -> String {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    for byte in s.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

/// Loads a cached icon for `icon_id` whose folder is at `real_path`, if present
/// AND still fresh (stored token == the folder's current mtime). Returns `None`
/// on any miss: no file, unreadable file, malformed JSON, or a stale token. Never
/// panics.
pub fn load(icon_id: &str, real_path: &str) -> Option<String> {
    load_in(CACHE_DIR.as_ref()?, icon_id, real_path)
}

/// Persists `data_url` for `icon_id` under the folder's current mtime token. A
/// best-effort write — any failure (no cache dir, unresolvable mtime, write
/// error) is silently dropped; the in-memory cache still has the icon for this
/// session, and the next session just re-fetches.
pub fn store(icon_id: &str, real_path: &str, data_url: &str) {
    let Some(dir) = CACHE_DIR.as_ref() else {
        return;
    };
    store_in(dir, icon_id, real_path, data_url);
}

/// Pure `load` against an explicit cache dir. Public-in-module so tests can run
/// hermetically against a temp dir instead of the process-wide `CACHE_DIR` (a
/// `LazyLock` whose first-touch ordering across tests isn't controllable).
fn load_in(dir: &Path, icon_id: &str, real_path: &str) -> Option<String> {
    let token = staleness_token(real_path)?;
    let raw = fs::read(entry_path(dir, icon_id)).ok()?;
    let entry: DiskEntry = serde_json::from_slice(&raw).ok()?;
    if entry.token == token {
        Some(entry.data_url)
    } else {
        // Stale: the folder changed since we cached this icon (likely a re-icon).
        None
    }
}

/// Pure `store` against an explicit cache dir. See `load_in` for the test seam.
fn store_in(dir: &Path, icon_id: &str, real_path: &str, data_url: &str) {
    let Some(token) = staleness_token(real_path) else {
        return;
    };
    let entry = DiskEntry {
        token,
        data_url: data_url.to_string(),
    };
    if let Ok(bytes) = serde_json::to_vec(&entry) {
        // Recreate the dir if a `clear_all` (theme change) removed it since the
        // `CACHE_DIR` LazyLock first built it. Cheap and idempotent.
        let _ = fs::create_dir_all(dir);
        let path = entry_path(dir, icon_id);
        if let Err(e) = write_atomic(&path, &bytes) {
            log::debug!(target: "icons", "icon-cache write failed for {icon_id}: {e}");
        }
    }
}

/// Drops the entire on-disk cache. Called on a theme/accent change, since macOS
/// tints folder icons (including `special:*` glyphs) by the current appearance:
/// the mtime token can't catch that (the folder didn't change, the *system* did),
/// so we wipe wholesale and let the icons re-fetch lazily. Removing the directory
/// is enough; it's recreated on the next `store`. Best-effort, never panics.
pub fn clear_all() {
    if let Some(dir) = CACHE_DIR.as_ref()
        && let Err(e) = fs::remove_dir_all(dir) {
            // ENOENT (already gone) is fine; anything else is logged and ignored.
            if e.kind() != std::io::ErrorKind::NotFound {
                log::debug!(target: "icons", "icon-cache clear failed: {e}");
            }
        }
}

/// Writes bytes to `path` via a temp-file + rename so a crash mid-write can't
/// leave a half-written sidecar that would later parse as garbage (it'd just be a
/// miss, but the temp+rename keeps the on-disk set always-valid, matching the
/// project's safe-write convention).
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

/// Test-only seam: the current epoch seconds, used by tests to bump a folder's
/// mtime deterministically without sleeping.
#[cfg(test)]
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hermetic test fixture: an isolated cache dir plus a target "folder" whose
    /// mtime stands in for a real browsed folder. Both are unique per test and
    /// cleaned up on drop, so tests never touch the real data dir or each other.
    struct Fixture {
        cache_dir: PathBuf,
        folder: String,
    }

    impl Fixture {
        fn new(tag: &str) -> Self {
            let base = std::env::temp_dir().join(format!("cmdr_icon_disk_{tag}_{}", std::process::id()));
            let cache_dir = base.join("cache");
            let folder = base.join("folder");
            let _ = fs::remove_dir_all(&base);
            fs::create_dir_all(&cache_dir).expect("create cache dir");
            fs::create_dir_all(&folder).expect("create target folder");
            let folder = folder.to_string_lossy().into_owned();
            let me = Self { cache_dir, folder };
            me.set_folder_mtime(now_secs());
            me
        }

        /// Sets the target folder's mtime to a specific epoch-second value so the
        /// staleness token is deterministic (no sleeping).
        fn set_folder_mtime(&self, secs: u64) {
            let t = filetime::FileTime::from_unix_time(secs as i64, 0);
            filetime::set_file_mtime(&self.folder, t).expect("set mtime");
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            // Both dirs share the same `..` base; remove it wholesale.
            if let Some(base) = self.cache_dir.parent() {
                let _ = fs::remove_dir_all(base);
            }
        }
    }

    #[test]
    fn digest_is_stable_and_filename_safe() {
        // Same input → same digest; arbitrary path chars don't leak.
        let a = digest_hex("pkg:/Applications/My App.app");
        let b = digest_hex("pkg:/Applications/My App.app");
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        // Distinct inputs → distinct digests (FNV is fine for this).
        assert_ne!(digest_hex("path:/a"), digest_hex("path:/b"));
    }

    #[test]
    fn write_then_read_back_hits() {
        let fx = Fixture::new("rw");
        let id = format!("path:{}", fx.folder);

        assert_eq!(load_in(&fx.cache_dir, &id, &fx.folder), None, "cold miss before store");
        store_in(&fx.cache_dir, &id, &fx.folder, "data:image/webp;base64,AAAA");
        assert_eq!(
            load_in(&fx.cache_dir, &id, &fx.folder).as_deref(),
            Some("data:image/webp;base64,AAAA")
        );
    }

    #[test]
    fn bumping_the_mtime_token_misses() {
        let fx = Fixture::new("stale");
        let base = now_secs();
        fx.set_folder_mtime(base);
        let id = format!("path:{}", fx.folder);

        store_in(&fx.cache_dir, &id, &fx.folder, "old-icon");
        assert_eq!(load_in(&fx.cache_dir, &id, &fx.folder).as_deref(), Some("old-icon"));

        // Re-icon simulation: the folder's mtime moves forward.
        fx.set_folder_mtime(base + 100);
        assert_eq!(
            load_in(&fx.cache_dir, &id, &fx.folder),
            None,
            "a changed folder mtime invalidates the entry"
        );
    }

    #[test]
    fn corrupt_sidecar_is_a_graceful_miss() {
        let fx = Fixture::new("corrupt");
        let id = format!("path:{}", fx.folder);

        // Hand-write garbage where the sidecar would live.
        fs::write(entry_path(&fx.cache_dir, &id), b"not json at all").expect("write garbage");

        assert_eq!(
            load_in(&fx.cache_dir, &id, &fx.folder),
            None,
            "malformed JSON must be a miss, not a panic"
        );
    }

    #[test]
    fn missing_folder_never_caches_and_reads_miss() {
        let fx = Fixture::new("gone");
        let id = format!("path:{}", fx.folder);
        // Remove the target folder so its mtime is unresolvable.
        let _ = fs::remove_dir_all(&fx.folder);

        // Store is a no-op (no token), load is a miss.
        store_in(&fx.cache_dir, &id, &fx.folder, "should-not-persist");
        assert_eq!(load_in(&fx.cache_dir, &id, &fx.folder), None);
    }

    #[test]
    fn write_is_atomic_no_tmp_left_behind() {
        let fx = Fixture::new("atomic");
        let id = format!("pkg:{}", fx.folder);
        store_in(&fx.cache_dir, &id, &fx.folder, "icon");

        let tmp = entry_path(&fx.cache_dir, &id).with_extension("json.tmp");
        assert!(!tmp.exists(), "temp file must be renamed away after an atomic write");
        assert_eq!(load_in(&fx.cache_dir, &id, &fx.folder).as_deref(), Some("icon"));
    }
}
