//! Configuration constants and path helpers.
//!
//! These can be extracted to environment variables or a config file in the future.

use cmdr_fs::log_private::LogPrivate;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::{AppHandle, Manager, Runtime};

/// Icon size in pixels (32x32 for retina display)
pub const ICON_SIZE: u32 = 32;

/// When true (macOS only): Show the associated app's icon for document types that don't
/// have custom document icons bundled. This results in colorful app icons, and they stay
/// up to date immediately when file associations change (for example, via Finder → Get Info).
///
/// When false: Fall back to system-generated document icons (Finder-style, with a small
/// app badge). These look more consistent with Finder, but may be stale until the next system
/// restart when file associations change (due to macOS Launch Services icon cache).
/// TODO: Move this to a setting once we have a settings window in place
pub const USE_APP_ICONS_AS_DOCUMENT_ICONS: bool = true;

/// Returns the app data directory. Priority:
/// 1. `CMDR_DATA_DIR` env var (set by `tauri-wrapper.js` for dev, by test harness for E2E)
/// 2. Tauri default (production)
///
/// Creates the directory if needed.
pub fn resolved_app_data_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let dir = match data_dir_from_env(std::env::var("CMDR_DATA_DIR").ok().as_deref()) {
        Some(path) => path,
        None => app
            .path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data dir: {e}"))?,
    };

    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create app data dir: {e}"))?;

    Ok(dir)
}

/// Pure helper: pull a data dir out of the env-var value, treating empty as unset.
/// Kept private + unit-tested so the env-precedence branch of `resolved_app_data_dir`
/// is exercised without needing a Tauri mock runtime.
fn data_dir_from_env(env_value: Option<&str>) -> Option<PathBuf> {
    match env_value {
        Some(s) if !s.is_empty() => Some(PathBuf::from(s)),
        _ => None,
    }
}

/// The RELEASE bundle id, from `tauri.conf.json`. Keep in sync. Only the fallback for
/// [`app_identifier`]: a dev or capture build runs under its own identifier
/// (`com.veszelovszki.cmdr-dev`, `-dev-<slug>`, …, from `scripts/instance-id.ts`), and every
/// OS-default dir has to follow that one, or two builds share logs and a drive index (#812, #820).
pub const BUNDLE_ID: &str = "com.veszelovszki.cmdr";

/// The identifier this binary was built with, set once at the top of `run()`.
static APP_IDENTIFIER: OnceLock<String> = OnceLock::new();

/// Records the identifier this binary runs under: `tauri::generate_context!()`'s config, the
/// same value Tauri's `app_data_dir()` / `app_cache_dir()` / `app_log_dir()` join. Call once,
/// first thing in `run()`, before anything resolves a dir.
pub fn set_app_identifier(identifier: &str) {
    // A second call (a double `run`) keeps the first: same binary, same value.
    let _ = APP_IDENTIFIER.set(identifier.to_string());
}

/// The identifier the OS-default dirs hang off: the running build's own, or [`BUNDLE_ID`] when
/// `run()` hasn't recorded one (a unit test).
pub fn app_identifier() -> &'static str {
    APP_IDENTIFIER.get().map_or(BUNDLE_ID, String::as_str)
}

/// The app data dir for code that runs without an `AppHandle` (stores read before setup, or
/// behind a plain function signature): `CMDR_DATA_DIR` when set, else the OS default for
/// [`app_identifier`], the folder Tauri's `app_data_dir()` names. `None` only when the OS has
/// no data dir. Doesn't create it.
///
/// ❌ Never re-derive this at a call site: one copy that misses the test branch below is a
/// unit test writing into the developer's real data dir (`volumes::tests` once seeded the real
/// `favorites.json` that way).
pub fn standalone_app_data_dir() -> Option<PathBuf> {
    standalone_dir_from(std::env::var("CMDR_DATA_DIR").ok().as_deref())
}

fn standalone_dir_from(env_value: Option<&str>) -> Option<PathBuf> {
    if let Some(dir) = data_dir_from_env(env_value) {
        return Some(dir);
    }
    os_default_data_dir()
}

#[cfg(not(test))]
fn os_default_data_dir() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join(app_identifier()))
}

/// Under test, the OS default is a scratch dir of this process's own: a test that didn't
/// isolate a store must never read or write the developer's real one. Process-unique because
/// nextest runs tests in parallel processes; left behind on exit (a static never drops), and
/// only created by a store that writes.
#[cfg(test)]
fn os_default_data_dir() -> Option<PathBuf> {
    use std::sync::LazyLock;
    static DIR: LazyLock<PathBuf> = LazyLock::new(|| {
        std::env::temp_dir().join("cmdr-unit-test-data").join(format!(
            "{}-{:016x}",
            std::process::id(),
            rand::random::<u64>()
        ))
    });
    Some(DIR.clone())
}

/// The drive index's folder inside the cache dir. Its own folder because the cache dir is
/// shared: WebKit and Core ML keep theirs there too.
const DRIVE_INDEX_DIR_NAME: &str = "drive-index";

/// Where the drive index lives: `drive-index/` in the app's cache dir, a rebuildable cache of
/// gigabytes that backups should skip (Time Machine excludes `~/Library/Caches` by default).
/// The cache dir, by priority:
/// 1. `CMDR_CACHE_DIR` (set by `tauri-wrapper.ts` for dev, mirroring Tauri's `app_cache_dir()`)
/// 2. `<CMDR_DATA_DIR>/cache`, so an E2E or capture run that names only a data dir stays wholly
///    inside it and never reaches the real cache
/// 3. the OS cache dir for [`app_identifier`], the same folder Tauri's `app_cache_dir()` names
///
/// `None` only when the OS has no cache dir. Doesn't create it. Why the index's folders are
/// split this way: `crates/cmdr-index/DETAILS.md` § "Where the stores live".
pub fn drive_index_dir() -> Option<PathBuf> {
    app_cache_dir().map(|dir| dir.join(DRIVE_INDEX_DIR_NAME))
}

/// The app's cache dir, by the priority [`drive_index_dir`] lists. Doesn't create it.
pub fn app_cache_dir() -> Option<PathBuf> {
    cache_dir_from(
        std::env::var("CMDR_CACHE_DIR").ok().as_deref(),
        std::env::var("CMDR_DATA_DIR").ok().as_deref(),
    )
}

fn cache_dir_from(cache_env: Option<&str>, data_env: Option<&str>) -> Option<PathBuf> {
    if let Some(dir) = data_dir_from_env(cache_env) {
        return Some(dir);
    }
    if let Some(data_dir) = data_dir_from_env(data_env) {
        return Some(data_dir.join("cache"));
    }
    os_default_cache_dir()
}

#[cfg(not(test))]
fn os_default_cache_dir() -> Option<PathBuf> {
    Some(os_cache_dir_for(&dirs::cache_dir()?, app_identifier()))
}

/// Pure core of the OS cache dir: one folder per identifier (Tauri's `app_cache_dir()`).
fn os_cache_dir_for(base: &Path, identifier: &str) -> PathBuf {
    base.join(identifier)
}

/// Under test, a scratch dir of this process's own, for the same reason as
/// [`os_default_data_dir`]: a test must never reach the developer's real drive index.
#[cfg(test)]
fn os_default_cache_dir() -> Option<PathBuf> {
    os_default_data_dir().map(|dir| dir.join("cache"))
}

/// Where the log files go, by priority:
/// 1. `CMDR_LOG_DIR` (explicit override)
/// 2. `<CMDR_DATA_DIR>/logs` (dev and E2E isolation; `tauri-wrapper.ts` always sets the data dir)
/// 3. the OS log dir for [`app_identifier`], the folder Tauri's `app_log_dir()` names:
///    `~/Library/Logs/<identifier>` on macOS, `<data_local_dir>/<identifier>/logs` elsewhere
///
/// Falls back to `./logs` only when the OS has no home or local data dir. Doesn't create it.
pub fn app_log_dir() -> PathBuf {
    log_dir_from(
        std::env::var("CMDR_LOG_DIR").ok().as_deref(),
        std::env::var("CMDR_DATA_DIR").ok().as_deref(),
    )
}

fn log_dir_from(log_env: Option<&str>, data_env: Option<&str>) -> PathBuf {
    if let Some(dir) = data_dir_from_env(log_env) {
        return dir;
    }
    if let Some(data_dir) = data_dir_from_env(data_env) {
        return data_dir.join("logs");
    }
    os_default_log_dir().unwrap_or_else(|| PathBuf::from("./logs"))
}

#[cfg(not(test))]
fn os_default_log_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    let base = dirs::home_dir().map(|home| home.join("Library/Logs"));
    #[cfg(not(target_os = "macos"))]
    let base = dirs::data_local_dir();
    Some(os_log_dir_for(&base?, app_identifier()))
}

/// Under test, a scratch dir of this process's own, like [`os_default_data_dir`].
#[cfg(test)]
fn os_default_log_dir() -> Option<PathBuf> {
    os_default_data_dir().map(|dir| dir.join("os-logs"))
}

/// Pure core of the OS log dir: macOS keeps one folder per identifier under `~/Library/Logs`;
/// elsewhere it's `logs/` inside the identifier's local data folder (Tauri's `app_log_dir()`).
fn os_log_dir_for(base: &Path, identifier: &str) -> PathBuf {
    if cfg!(target_os = "macos") {
        base.join(identifier)
    } else {
        base.join(identifier).join("logs")
    }
}

/// Logs the resolved data directory once at startup.
pub fn log_app_data_dir<R: Runtime>(app: &AppHandle<R>) {
    if let Ok(dir) = resolved_app_data_dir(app) {
        if std::env::var("CMDR_DATA_DIR").is_ok() {
            log::info!("Using CMDR_DATA_DIR: {}", LogPrivate::dir_path(&dir));
        } else {
            log::debug!("Using default app data dir: {}", LogPrivate::dir_path(&dir));
        }
    }
}

/// Writes `content` to `path` durably: write the bytes to `tmp`, fsync the temp file, rename
/// it over `path`, then fsync the parent directory so the rename itself survives a power loss.
///
/// `fs::write(tmp) + fs::rename(tmp, path)` alone is atomic against *process* death (the rename
/// swaps the directory entry as a unit) but NOT against a power loss / hard crash: the rename can
/// land in the filesystem journal while the temp's data blocks are still only in the page cache,
/// leaving the destination zero-length or torn. The two fsyncs close that window. This mirrors the
/// data-loss-class write discipline in `LocalPosixVolume::write_from_stream` (the
/// copy/move path that survives eject/sleep/power-loss).
///
/// The caller owns the temp-path convention (it picks `tmp`) so each store keeps its existing
/// `cleanup_tmp_file` stale-temp recovery. The parent-directory fsync is best-effort: some
/// filesystems reject opening a directory for fsync, so a failure there is logged and ignored
/// rather than failing the whole write (the file's data is already durable at that point).
pub fn durable_write_json(path: &Path, tmp: &Path, content: &str) -> std::io::Result<()> {
    {
        let mut file = std::fs::File::create(tmp)?;
        file.write_all(content.as_bytes())?;
        // fsync the temp's data + metadata before the rename so the bytes are on disk, not just
        // in the page cache, when the directory entry swaps.
        file.sync_all()?;
    }

    std::fs::rename(tmp, path)?;

    // Best-effort: fsync the parent directory so the rename (the new directory entry) is durable
    // too. Logged and ignored on failure, matching `LocalPosixVolume`'s parent-dir fsync.
    if let Some(parent) = path.parent() {
        match std::fs::File::open(parent).and_then(|dir| dir.sync_all()) {
            Ok(()) => {}
            Err(e) => log::debug!(
                target: "write_durability",
                "durable_write_json: parent dir fsync skipped for {}: {e}",
                LogPrivate::dir_path(parent)
            ),
        }
    }

    Ok(())
}

// MCP Server Security Design:
// --------------------------
// The MCP (Model Context Protocol) bridge allows AI assistants to control the app.
// This requires `withGlobalTauri: true` which exposes `window.__TAURI__` to the frontend.
//
// To prevent this security risk in production:
// 1. The MCP plugin is only registered in debug builds (see lib.rs: #[cfg(debug_assertions)])
// 2. `withGlobalTauri` is only flipped to `true` by the generated `tauri.instance.json` that
//    `apps/desktop/scripts/tauri-wrapper.js` writes for non-prod instances and merges via `-c`.
// 3. Production builds skip the wrapper's instance composition, so canonical
//    `tauri.conf.json` (with `withGlobalTauri: false`) governs the bundle.
//
// See CONTRIBUTING.md for setup instructions.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_from_env_honors_set_value() {
        let got = data_dir_from_env(Some("/tmp/cmdr-p1-test"));
        assert_eq!(got, Some(PathBuf::from("/tmp/cmdr-p1-test")));
    }

    #[test]
    fn data_dir_from_env_returns_none_when_unset() {
        assert_eq!(data_dir_from_env(None), None);
    }

    #[test]
    fn data_dir_from_env_treats_empty_as_unset() {
        // An empty CMDR_DATA_DIR should not silently land us in cwd-equivalent paths.
        // Falling through to the Tauri default is the documented behavior.
        assert_eq!(data_dir_from_env(Some("")), None);
    }

    /// `volumes::tests` once seeded the developer's real `favorites.json` through
    /// `list_locations()`. With `CMDR_DATA_DIR` unset, a test must land in scratch space.
    #[test]
    fn an_unset_data_dir_under_test_is_never_the_real_one() {
        let resolved = standalone_dir_from(None).expect("a test always gets a data dir");
        let real = dirs::data_dir().map(|base| base.join(BUNDLE_ID));
        assert_ne!(Some(resolved.clone()), real, "a test resolved the real app data dir");
        // allowed-fixed-temp-dir: asserts the scratch dir sits under the temp root, writes nothing
        assert!(resolved.starts_with(std::env::temp_dir()), "{}", resolved.display());
        assert_eq!(standalone_dir_from(None), Some(resolved), "stable within one process");
    }

    #[test]
    fn a_set_cache_dir_wins_over_everything() {
        assert_eq!(
            cache_dir_from(Some("/tmp/cmdr-cache"), Some("/tmp/cmdr-data")),
            Some(PathBuf::from("/tmp/cmdr-cache"))
        );
    }

    /// An E2E or capture run names only `CMDR_DATA_DIR`. Its drive index must land inside that
    /// isolated dir, never in the real `~/Library/Caches/com.veszelovszki.cmdr`.
    #[test]
    fn a_data_dir_alone_keeps_the_cache_inside_it() {
        assert_eq!(
            cache_dir_from(None, Some("/tmp/cmdr-data")),
            Some(PathBuf::from("/tmp/cmdr-data/cache"))
        );
        assert_eq!(
            cache_dir_from(Some(""), Some("/tmp/cmdr-data")),
            Some(PathBuf::from("/tmp/cmdr-data/cache")),
            "an empty CMDR_CACHE_DIR is unset"
        );
    }

    #[test]
    fn an_unset_cache_dir_under_test_is_never_the_real_one() {
        let resolved = cache_dir_from(None, None).expect("a test always gets a cache dir");
        let real = dirs::cache_dir().map(|base| base.join(BUNDLE_ID));
        assert_ne!(Some(resolved.clone()), real, "a test resolved the real cache dir");
        // allowed-fixed-temp-dir: asserts the scratch dir sits under the temp root, writes nothing
        assert!(resolved.starts_with(std::env::temp_dir()), "{}", resolved.display());
    }

    #[test]
    fn a_set_data_dir_wins_under_test_too() {
        assert_eq!(
            standalone_dir_from(Some("/tmp/cmdr-p1-test")),
            Some(PathBuf::from("/tmp/cmdr-p1-test"))
        );
    }

    /// #812 / #820: a dev build and the release app on one Mac used to share the log folder and
    /// the drive index, because both dirs hardcoded the release identifier. Each identifier now
    /// gets its own.
    #[test]
    fn two_identifiers_get_disjoint_log_and_cache_dirs() {
        let base = Path::new("/base");
        let disjoint = |a: PathBuf, b: PathBuf| !a.starts_with(&b) && !b.starts_with(&a);
        let dev = "com.veszelovszki.cmdr-dev";
        assert!(disjoint(os_log_dir_for(base, BUNDLE_ID), os_log_dir_for(base, dev)));
        assert!(disjoint(os_cache_dir_for(base, BUNDLE_ID), os_cache_dir_for(base, dev)));
        // The release app's dirs don't move, so its logs and index stay where they are.
        assert_eq!(os_cache_dir_for(base, BUNDLE_ID), base.join("com.veszelovszki.cmdr"));
        if cfg!(target_os = "macos") {
            assert_eq!(os_log_dir_for(base, BUNDLE_ID), base.join("com.veszelovszki.cmdr"));
        }
    }

    #[test]
    fn the_log_dir_follows_env_precedence() {
        assert_eq!(
            log_dir_from(Some("/tmp/cmdr-logs"), Some("/tmp/cmdr-data")),
            PathBuf::from("/tmp/cmdr-logs")
        );
        assert_eq!(
            log_dir_from(None, Some("/tmp/cmdr-data")),
            PathBuf::from("/tmp/cmdr-data/logs")
        );
        assert_eq!(
            log_dir_from(Some(""), Some("/tmp/cmdr-data")),
            PathBuf::from("/tmp/cmdr-data/logs"),
            "an empty CMDR_LOG_DIR is unset"
        );
    }

    #[test]
    fn an_unset_log_dir_under_test_is_never_a_real_one() {
        let resolved = log_dir_from(None, None);
        // allowed-fixed-temp-dir: asserts the scratch dir sits under the temp root, writes nothing
        assert!(resolved.starts_with(std::env::temp_dir()), "{}", resolved.display());
    }

    #[test]
    fn durable_write_json_round_trips_content() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("store.json");
        let tmp = path.with_extension("json.tmp");

        durable_write_json(&path, &tmp, r#"{"a":1}"#).expect("durable write");

        let read_back = std::fs::read_to_string(&path).expect("read back");
        assert_eq!(read_back, r#"{"a":1}"#);
        // The temp file must be consumed by the rename, not left behind.
        assert!(!tmp.exists(), "temp file should be renamed away, not left behind");
    }

    #[test]
    fn durable_write_json_overwrites_existing_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("store.json");
        let tmp = path.with_extension("json.tmp");

        std::fs::write(&path, "old contents that are much longer").expect("seed file");
        durable_write_json(&path, &tmp, "new").expect("durable write");

        assert_eq!(std::fs::read_to_string(&path).expect("read back"), "new");
    }
}
