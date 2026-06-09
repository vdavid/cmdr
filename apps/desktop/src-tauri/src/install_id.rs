//! Per-install random identifiers, owned by Rust.
//!
//! Two ids that never meet, by construction (see `analytics/CLAUDE.md` § "Two ids that never
//! meet"):
//!
//! - [`analytics_id`] (`anal_<uuid>`): the heartbeat key and the PostHog `distinct_id`. Never
//!   attached to a crash or error report.
//! - [`diagnostics_id`] (`diag_<uuid>`): attached only to crash and error reports so sequential
//!   reports from one install can be grouped. Never sent through the analytics pipeline.
//!
//! Both are minted on first read and persisted to one Rust-owned JSON file (`install-ids.json`,
//! keys `analId` / `diagId`) next to `settings.json`. Rust owns them (not `settings.json`) to
//! avoid a first-launch write-ownership race: the frontend owns every `settings.json` write, and
//! Rust only reads it.
//!
//! The data dir is resolved WITHOUT an `AppHandle` (mirroring `settings/loader.rs`'s
//! `early_load_*` helpers: `CMDR_DATA_DIR` env if set, else the OS default for the bundle id) so
//! the accessors stay no-arg and are callable from the panic hook, next-launch crash assembly,
//! and the analytics loop alike, none of which always have an `AppHandle` at hand.

use crate::config;
use crate::ignore_poison::IgnorePoison;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

/// Bundle id from `tauri.conf.json`. Mirrored here so the data-dir resolution works without an
/// `AppHandle`, matching `settings/loader.rs`'s early-load helpers. Keep in sync if it changes.
const BUNDLE_ID: &str = "com.veszelovszki.cmdr";

const INSTALL_IDS_FILE_NAME: &str = "install-ids.json";

const ANALYTICS_PREFIX: &str = "anal_";
const DIAGNOSTICS_PREFIX: &str = "diag_";

/// In-memory cache of the file contents, so repeated reads don't touch disk. The first
/// read-or-generate for either id loads (and lazily back-fills + persists) this.
static IDS: OnceLock<Mutex<InstallIds>> = OnceLock::new();

/// The diagnostics id snapshotted at [`init`] time into a cheap-to-read static. The crash panic
/// hook reads this rather than calling [`diagnostics_id`] (which allocates + locks), keeping the
/// hook path light.
static DIAGNOSTICS_ID_SNAPSHOT: OnceLock<String> = OnceLock::new();

/// On-disk shape of `install-ids.json`. Both fields optional so a file written by a future
/// version that adds a third id still parses, and so a partial file (one id present) back-fills
/// the other.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct InstallIds {
    #[serde(rename = "analId", skip_serializing_if = "Option::is_none")]
    analytics: Option<String>,
    #[serde(rename = "diagId", skip_serializing_if = "Option::is_none")]
    diagnostics: Option<String>,
}

/// Resolves the install-ids file path without an `AppHandle`.
fn install_ids_path() -> PathBuf {
    let data_dir: PathBuf = if let Ok(custom) = std::env::var("CMDR_DATA_DIR") {
        PathBuf::from(custom)
    } else {
        dirs::data_dir().map(|base| base.join(BUNDLE_ID)).unwrap_or_default()
    };
    data_dir.join(INSTALL_IDS_FILE_NAME)
}

fn read_from_disk(path: &Path) -> InstallIds {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn write_to_disk(path: &Path, ids: &InstallIds) {
    let Some(parent) = path.parent() else { return };
    if let Err(e) = std::fs::create_dir_all(parent) {
        log::warn!(target: "install_id", "Couldn't create data dir for install ids: {e}");
        return;
    }
    let Ok(content) = serde_json::to_string_pretty(ids) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if let Err(e) = config::durable_write_json(path, &tmp, &content) {
        log::warn!(target: "install_id", "Couldn't persist install ids: {e}");
    }
}

/// Returns the analytics id (`anal_<uuid>`), minting + persisting it on first read.
pub fn analytics_id() -> String {
    get_or_generate(IdKind::Analytics)
}

/// Returns the diagnostics id (`diag_<uuid>`), minting + persisting it on first read.
pub fn diagnostics_id() -> String {
    get_or_generate(IdKind::Diagnostics)
}

#[derive(Clone, Copy)]
enum IdKind {
    Analytics,
    Diagnostics,
}

impl IdKind {
    fn prefix(self) -> &'static str {
        match self {
            IdKind::Analytics => ANALYTICS_PREFIX,
            IdKind::Diagnostics => DIAGNOSTICS_PREFIX,
        }
    }
}

fn get_or_generate(kind: IdKind) -> String {
    let path = install_ids_path();
    let cell = IDS.get_or_init(|| Mutex::new(read_from_disk(&path)));
    let mut guard = cell.lock_ignore_poison();

    let slot = match kind {
        IdKind::Analytics => &mut guard.analytics,
        IdKind::Diagnostics => &mut guard.diagnostics,
    };
    if let Some(existing) = slot {
        return existing.clone();
    }

    let fresh = format!("{}{}", kind.prefix(), Uuid::new_v4());
    *slot = Some(fresh.clone());
    let snapshot = guard.clone();
    drop(guard);
    write_to_disk(&path, &snapshot);
    fresh
}

/// Resolves the diagnostics id once into a cheap static so the crash panic hook can read it
/// without allocating or locking. Call once at startup, before anything that might crash. The
/// signal handler is async-signal-safe and must NOT call this; it attaches the diag id at
/// next-launch report assembly instead.
pub fn init() {
    let _ = DIAGNOSTICS_ID_SNAPSHOT.set(diagnostics_id());
}

/// The diagnostics id snapshotted at [`init`], or `None` if `init` hasn't run yet. The panic hook
/// reads this cheap copy rather than calling [`diagnostics_id`].
pub fn diagnostics_id_snapshot() -> Option<String> {
    DIAGNOSTICS_ID_SNAPSHOT.get().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex as StdMutex, MutexGuard};

    /// `install_ids_path` reads a process-global env var and `IDS` is a process-global cache, so
    /// these tests can't run concurrently. This lock serializes them; each test also resets the
    /// cache so a prior test's mint doesn't leak in.
    static TEST_LOCK: StdMutex<()> = StdMutex::new(());

    struct TestEnv {
        _guard: MutexGuard<'static, ()>,
        _dir: tempfile::TempDir,
        prev: Option<String>,
    }

    fn setup() -> TestEnv {
        let guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempfile::tempdir().expect("temp dir");
        let prev = std::env::var("CMDR_DATA_DIR").ok();
        // SAFETY: tests are serialized by TEST_LOCK, so no other thread reads the env concurrently.
        unsafe {
            std::env::set_var("CMDR_DATA_DIR", dir.path());
        }
        clear_cache();
        TestEnv {
            _guard: guard,
            _dir: dir,
            prev,
        }
    }

    /// Simulates a fresh process: re-reads the persisted file into the in-memory cache. `IDS` is a
    /// `OnceLock`, so we can't unset it to force `get_or_init` to re-run; overwriting the inner
    /// value with a fresh disk read is the equivalent of a cold start reading `install-ids.json`.
    fn reload_cache_from_disk() {
        if let Some(cell) = IDS.get() {
            *cell.lock_ignore_poison() = read_from_disk(&install_ids_path());
        }
    }

    /// Empties the in-memory cache without reading disk: models "no file on disk yet."
    fn clear_cache() {
        if let Some(cell) = IDS.get() {
            *cell.lock_ignore_poison() = InstallIds::default();
        }
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            // SAFETY: serialized by TEST_LOCK.
            unsafe {
                match &self.prev {
                    Some(v) => std::env::set_var("CMDR_DATA_DIR", v),
                    None => std::env::remove_var("CMDR_DATA_DIR"),
                }
            }
        }
    }

    #[test]
    fn analytics_id_has_anal_prefix_and_matches_wire_shape() {
        let _env = setup();
        let id = analytics_id();
        assert!(id.starts_with("anal_"), "got {id}");
        // The heartbeat contract: `anal_` + a 36-char lowercase hyphenated v4 UUID.
        let uuid_part = id.strip_prefix("anal_").expect("prefix");
        assert_eq!(uuid_part.len(), 36, "uuid part should be 36 chars: {uuid_part}");
        assert!(
            uuid_part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "uuid part should be lowercase hex + hyphens: {uuid_part}"
        );
    }

    #[test]
    fn diagnostics_id_has_diag_prefix() {
        let _env = setup();
        let id = diagnostics_id();
        assert!(id.starts_with("diag_"), "got {id}");
    }

    #[test]
    fn ids_are_stable_across_calls() {
        let _env = setup();
        assert_eq!(analytics_id(), analytics_id());
        assert_eq!(diagnostics_id(), diagnostics_id());
    }

    #[test]
    fn analytics_and_diagnostics_ids_differ() {
        let _env = setup();
        assert_ne!(analytics_id(), diagnostics_id());
    }

    #[test]
    fn id_survives_a_reload_from_disk() {
        let _env = setup();
        let first = analytics_id();
        // Re-read the file the first call persisted, modeling a cold process start.
        reload_cache_from_disk();
        let second = analytics_id();
        assert_eq!(first, second, "id should survive a reload from disk");
    }

    #[test]
    fn id_regenerates_if_file_missing() {
        let _env = setup();
        let first = analytics_id();
        // Remove the file AND the cache: nothing left to read, so a fresh id is minted.
        let _ = std::fs::remove_file(install_ids_path());
        clear_cache();
        let second = analytics_id();
        assert_ne!(first, second, "a fresh id should be minted when the file is gone");
    }

    #[test]
    fn both_ids_persist_to_one_file() {
        let _env = setup();
        let anal = analytics_id();
        let diag = diagnostics_id();
        let contents = std::fs::read_to_string(install_ids_path()).expect("file written");
        assert!(contents.contains(&anal), "analId missing from file: {contents}");
        assert!(contents.contains(&diag), "diagId missing from file: {contents}");
        assert!(contents.contains("analId"), "analId key missing: {contents}");
        assert!(contents.contains("diagId"), "diagId key missing: {contents}");
    }
}
