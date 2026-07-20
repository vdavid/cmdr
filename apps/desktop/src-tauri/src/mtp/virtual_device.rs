//! Virtual MTP device setup for E2E testing and manual dev sessions.
//!
//! Registers a virtual MTP device backed by local filesystem directories so that
//! Playwright E2E tests (and `CMDR_VIRTUAL_MTP=1 pnpm dev` dev sessions) can
//! exercise the full MTP UI flow without real hardware.
//!
//! Gated behind `--features virtual-mtp`. Never compiled into production builds.
//! See `mtp/CLAUDE.md` § "Virtual MTP device" and `docs/tooling/virtual-mtp.md`.

use crate::ignore_poison::IgnorePoison;
use log::info;
use mtp_rs::{VirtualDeviceConfig, VirtualStorageConfig, WatcherGuard};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// Env var that opts a feature-compiled binary into registering the virtual MTP
/// device at startup. `1` (or `true`) uses the default fixture root; any other
/// non-empty value is treated as a custom backing-dir path. Unset means "don't
/// register" (so a binary compiled with `virtual-mtp` but launched without the
/// var behaves like a plain build). The wrapper (`tauri-wrapper.js`) also reads
/// this var to append `--features virtual-mtp` to the dev build.
pub const VIRTUAL_MTP_ENV: &str = "CMDR_VIRTUAL_MTP";

/// Holds the watcher guard while the watcher is paused. Protected by a mutex
/// so it can be accessed from Tauri commands (which run on arbitrary threads).
static WATCHER_GUARD: Mutex<Option<WatcherGuard>> = Mutex::new(None);

/// Root directory for the virtual device's backing files.
/// The TypeScript E2E fixture helper references the same path; see
/// `test/e2e-shared/mtp-fixtures.ts`.
pub const MTP_FIXTURE_ROOT: &str = "/tmp/cmdr-mtp-e2e-fixtures";

/// Decides whether a feature-compiled binary should register the virtual MTP
/// device at startup, and which fixture root to back it with.
///
/// Returns `Some(root)` to register at `root`, or `None` to skip. The decision
/// is pure (takes the relevant env values as args) so it's unit-testable:
///
/// - The skip override (`CMDR_E2E_SKIP_VIRTUAL_MTP_SETUP`, any non-empty value)
///   always wins → `None`. Parallel E2E shards set this on the non-MTP lanes so
///   they don't race the MTP lane's wipe-and-recreate of the shared backing dir.
/// - Otherwise we register when **either** we're under an E2E run (`e2e_mode`)
///   **or** `CMDR_VIRTUAL_MTP` is set to a non-empty value (the dev opt-in).
/// - The root is the default `MTP_FIXTURE_ROOT` unless `CMDR_VIRTUAL_MTP` names
///   a custom path (anything other than the `1` / `true` "use the default"
///   sentinels).
fn decide_startup_root(skip_override: bool, e2e_mode: bool, virtual_mtp_var: Option<&str>) -> Option<PathBuf> {
    if skip_override {
        return None;
    }
    let var = virtual_mtp_var.map(str::trim).filter(|s| !s.is_empty());
    if !e2e_mode && var.is_none() {
        return None;
    }
    let root = match var {
        Some(value) if !is_default_sentinel(value) => PathBuf::from(value),
        _ => PathBuf::from(MTP_FIXTURE_ROOT),
    };
    Some(root)
}

/// Whether a `CMDR_VIRTUAL_MTP` value means "just use the default fixture root"
/// (as opposed to naming a custom backing dir).
fn is_default_sentinel(value: &str) -> bool {
    matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

/// Reads the environment and, if a virtual MTP device is requested, registers it.
///
/// Called once at startup (before `start_mtp_watcher()` so the device lands in
/// the initial snapshot). Returns the registered device's `location_id`, or
/// `None` when no device was requested. See [`decide_startup_root`] for the
/// gating logic and `docs/tooling/virtual-mtp.md` for the dev workflow.
pub fn activate_from_env_if_requested() -> Option<u64> {
    let skip_override = std::env::var("CMDR_E2E_SKIP_VIRTUAL_MTP_SETUP")
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    let e2e_mode = crate::test_mode::is_e2e_mode();
    let virtual_mtp_var = std::env::var(VIRTUAL_MTP_ENV).ok();
    let root = decide_startup_root(skip_override, e2e_mode, virtual_mtp_var.as_deref())?;
    Some(setup_virtual_mtp_device_at(&root))
}

/// A registered virtual MTP device plus the temp dir backing it. Holding it
/// keeps the backing dir alive; dropping it deletes the dir.
#[cfg(test)]
pub(crate) struct VirtualDeviceFixture {
    pub(crate) location_id: u64,
    root: tempfile::TempDir,
}

#[cfg(test)]
impl VirtualDeviceFixture {
    /// The storage backing dir, for tests that seed files directly on disk.
    pub(crate) fn root(&self) -> &Path {
        self.root.path()
    }
}

/// Registers a virtual MTP device with two storages and pre-populated test files
/// in a **fresh temp dir per call**.
///
/// ❌ Don't point this at the shared [`MTP_FIXTURE_ROOT`]: `setup_virtual_mtp_device_at`
/// wipes its root, so two tests sharing one root delete each other's fixtures
/// mid-run. Under `cargo nextest` (process per test) a shared root is the ONLY
/// thing that can make these tests collide, since every other piece of state
/// (mtp-rs registry, `connection_manager()`) is process-local. Under plain
/// `cargo test` they additionally share the virtual device's serial, hence one
/// Cmdr device id — [`virtual_device_test_lock`] covers that.
///
/// Used by Rust tests that drive the device directly. The startup path goes
/// through [`activate_from_env_if_requested`] instead (it derives the root from
/// the environment, which IS `MTP_FIXTURE_ROOT` for E2E).
///
/// Registers with the backing-dir WATCHER OFF. No Rust test needs it (they sync
/// the object tree explicitly with [`rescan_virtual_device`]), and each watcher
/// is a real FSEvents/inotify watch: under a saturated full-suite run, a handful
/// of concurrent test processes each holding one starve each other's delivery
/// and push these tests past nextest's 8 s cap. Only the E2E/dev startup path
/// ([`setup_virtual_mtp_device_at`]) arms it, and E2E is the one consumer that
/// exercises live watching.
#[cfg(test)]
pub(crate) fn setup_virtual_mtp_device() -> VirtualDeviceFixture {
    let root = tempfile::tempdir().expect("failed to create a virtual-device fixture root");
    let location_id = register_virtual_mtp_device_at(root.path(), false);
    VirtualDeviceFixture { location_id, root }
}

/// Registers a virtual MTP device backed by `root`, with two storages and
/// pre-populated files so drag-and-drop / transfers are testable immediately.
///
/// Must be called **before** `start_mtp_watcher()` so the device appears in the
/// initial device snapshot.
pub fn setup_virtual_mtp_device_at(root: &Path) -> u64 {
    register_virtual_mtp_device_at(root, true)
}

/// The shared body of [`setup_virtual_mtp_device_at`] and (test-only)
/// [`setup_virtual_mtp_device`]. `watch_backing_dirs` arms the device's
/// filesystem watcher, which turns out-of-band disk writes into PTP events.
fn register_virtual_mtp_device_at(root: &Path, watch_backing_dirs: bool) -> u64 {
    let internal = root.join("internal");
    let readonly = root.join("readonly");

    // Clean slate: remove any leftover fixtures from a previous run
    if root.exists() {
        fs::remove_dir_all(root).expect("failed to clean MTP fixture root");
    }

    // Create directory structure. Mirrors `test/e2e-shared/mtp-fixtures.ts`'s
    // `recreateMtpFixtures()` so the dev opt-in and the E2E recreate seed the
    // same tree (DCIM/Burst included).
    fs::create_dir_all(internal.join("Documents")).expect("failed to create Documents dir");
    fs::create_dir_all(internal.join("DCIM/Burst")).expect("failed to create DCIM/Burst dir");
    fs::create_dir_all(internal.join("Music")).expect("failed to create Music dir");
    fs::create_dir_all(readonly.join("photos")).expect("failed to create photos dir");

    // Populate writable storage
    fs::write(
        internal.join("Documents/report.txt"),
        "Quarterly report: Q4 2025 placeholder content.\n",
    )
    .expect("failed to write report.txt");
    fs::write(
        internal.join("Documents/notes.txt"),
        "Meeting notes: discuss MTP E2E test strategy.\n",
    )
    .expect("failed to write notes.txt");
    fs::write(internal.join("DCIM/photo-001.jpg"), b"\xFF\xD8\xFF\xE0dummy-jpeg-bytes")
        .expect("failed to write photo-001.jpg");
    fs::write(
        internal.join("DCIM/Burst/burst-001.jpg"),
        b"\xFF\xD8\xFF\xE0dummy-burst-bytes",
    )
    .expect("failed to write burst-001.jpg");

    // Populate read-only storage
    fs::write(
        readonly.join("photos/sunset.jpg"),
        b"\xFF\xD8\xFF\xE0dummy-sunset-bytes",
    )
    .expect("failed to write sunset.jpg");

    let config = VirtualDeviceConfig {
        manufacturer: "Google".into(),
        model: "Virtual Pixel 9".into(),
        serial: "cmdr-e2e-virtual".into(),
        storages: vec![
            VirtualStorageConfig {
                description: "Internal Storage".into(),
                capacity: 64 * 1024 * 1024 * 1024, // 64 GB
                backing_dir: internal,
                read_only: false,
            },
            VirtualStorageConfig {
                description: "SD Card".into(),
                capacity: 16 * 1024 * 1024 * 1024, // 16 GB
                backing_dir: readonly,
                read_only: true,
            },
        ],
        // `supports_rename` and `supports_partial_object_64` come from the default
        // (both true), which models a modern Android device: the virtual device
        // stands in for a Pixel 9. Setting the latter false would exercise mtp-rs's
        // 32-bit GetPartialObject fallback instead (cameras like the Lumix TZ61).
        event_poll_interval: Duration::from_millis(100),
        watch_backing_dirs,
        ..Default::default()
    };

    let device_info = mtp_rs::register_virtual_device(&config);
    let location_id = device_info.location_id;

    info!(
        "Registered virtual MTP device: {} {} (serial={}, location_id={})",
        config.manufacturer, config.model, config.serial, location_id,
    );

    location_id
}

/// Serial number of the virtual device, used to look it up for rescan.
pub const VIRTUAL_DEVICE_SERIAL: &str = "cmdr-e2e-virtual";

/// Forces the virtual MTP device to rescan its backing directories, syncing
/// its in-memory object tree with the actual filesystem state.
///
/// Call this after recreating test fixtures to avoid waiting for the file watcher.
/// Returns the number of objects added and removed, or None if the device wasn't found.
pub fn rescan_virtual_device() -> Option<(usize, usize)> {
    let summary = mtp_rs::rescan_virtual_device(VIRTUAL_DEVICE_SERIAL)?;
    info!(
        "Virtual MTP device rescan: {} added, {} removed",
        summary.added, summary.removed
    );
    Some((summary.added as usize, summary.removed as usize))
}

/// Pauses the virtual device's filesystem watcher. While paused, all OS-level
/// filesystem events are silently dropped. Call before manipulating backing dir
/// files externally, then sync the object tree with [`rescan_virtual_device`]
/// (which reads the backing dir directly). In E2E the watcher deliberately
/// stays paused for the whole test body so late FSEvents can't race the test;
/// see `mtp/DETAILS.md` § "Virtual device watcher in E2E".
pub fn pause_virtual_watcher() -> bool {
    let guard = mtp_rs::pause_watcher(VIRTUAL_DEVICE_SERIAL);
    let paused = guard.is_some();
    if paused {
        *WATCHER_GUARD.lock_ignore_poison() = guard;
        info!("Virtual MTP watcher paused");
    }
    paused
}

/// Resumes the virtual device's filesystem watcher by dropping the guard. The
/// underlying mtp-rs pause is refcounted so this only flips the watcher back on
/// when no other concurrent pause still holds a guard. In E2E only the one test
/// that verifies the live-watch pipeline calls this.
pub fn resume_virtual_watcher() {
    let had_guard = WATCHER_GUARD.lock_ignore_poison().take().is_some();
    if had_guard {
        info!("Virtual MTP watcher resumed");
    }
}

/// Serializes tests that stand up their own virtual MTP device.
///
/// Every virtual device registers under the SAME serial, so Cmdr's device id
/// (`mtp-{serial}`) is identical for all of them: `resolve_device_location_id`
/// matches the FIRST registration with that id, `connect()` is idempotent per
/// device id, and `rescan_virtual_device` resolves by serial too. Two tests
/// running at once would silently share one connection pointed at whichever
/// backing dir registered first. Hold this guard across the whole
/// register → connect → use → disconnect → unregister span.
#[cfg(test)]
pub(crate) fn virtual_device_test_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Removes a virtual device registration (test teardown).
///
/// Required, not hygiene: a leftover registration keeps answering to the shared
/// device id, so the next test's `connect()` would open the previous test's
/// backing dir. Pairs with [`setup_virtual_mtp_device_at`].
#[cfg(test)]
pub(crate) fn unregister_virtual_mtp_device(location_id: u64) {
    mtp_rs::unregister_virtual_device(location_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_override_always_wins() {
        // Even under E2E mode or with the dev var set, the skip override (set by
        // non-MTP E2E shards) forces "don't register" so they don't race the
        // shared backing dir.
        assert_eq!(decide_startup_root(true, true, Some("1")), None);
        assert_eq!(decide_startup_root(true, false, None), None);
        assert_eq!(decide_startup_root(true, true, None), None);
    }

    #[test]
    fn plain_dev_does_not_register() {
        // No E2E mode, no dev var → a feature-compiled binary stays inert.
        assert_eq!(decide_startup_root(false, false, None), None);
        assert_eq!(decide_startup_root(false, false, Some("")), None);
        assert_eq!(decide_startup_root(false, false, Some("   ")), None);
    }

    #[test]
    fn e2e_mode_registers_at_default_root() {
        // The E2E path: feature compiled, E2E mode on, no dev var.
        assert_eq!(
            decide_startup_root(false, true, None),
            Some(PathBuf::from(MTP_FIXTURE_ROOT))
        );
    }

    #[test]
    fn dev_var_sentinels_register_at_default_root() {
        for sentinel in ["1", "true", "TRUE", "yes", "on"] {
            assert_eq!(
                decide_startup_root(false, false, Some(sentinel)),
                Some(PathBuf::from(MTP_FIXTURE_ROOT)),
                "sentinel {sentinel:?} should use the default root",
            );
        }
    }

    #[test]
    fn dev_var_path_registers_at_custom_root() {
        assert_eq!(
            decide_startup_root(false, false, Some("/tmp/my-mtp-fixtures")),
            Some(PathBuf::from("/tmp/my-mtp-fixtures")),
        );
        // Surrounding whitespace is trimmed.
        assert_eq!(
            decide_startup_root(false, false, Some("  /tmp/spaced  ")),
            Some(PathBuf::from("/tmp/spaced")),
        );
    }

    #[test]
    fn is_default_sentinel_matrix() {
        assert!(is_default_sentinel("1"));
        assert!(is_default_sentinel("true"));
        assert!(is_default_sentinel("Yes"));
        assert!(!is_default_sentinel("/tmp/foo"));
        assert!(!is_default_sentinel("0"));
        assert!(!is_default_sentinel("off"));
    }
}
