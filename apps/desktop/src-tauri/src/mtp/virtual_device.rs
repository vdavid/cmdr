//! Virtual MTP device setup for E2E testing.
//!
//! Registers a virtual MTP device backed by local filesystem directories so that
//! Playwright E2E tests can exercise the full MTP UI flow without real hardware.
//!
//! Gated behind `--features virtual-mtp`. Never enable in production builds.

use log::info;
use mtp_rs::{VirtualDeviceConfig, VirtualStorageConfig};
use std::fs;
use std::path::Path;
use std::time::Duration;

/// Root directory for the virtual device's backing files.
/// The TypeScript E2E fixture helper references the same path — see
/// `tests/e2e/fixtures/mtp-fixtures.ts` (added in Milestone 2).
pub const MTP_FIXTURE_ROOT: &str = "/tmp/cmdr-mtp-e2e-fixtures";

/// Registers a virtual MTP device with two storages and pre-populated test files.
///
/// Must be called **before** `start_mtp_watcher()` so the device appears in the
/// initial device snapshot.
pub fn setup_virtual_mtp_device() -> u64 {
    let root = Path::new(MTP_FIXTURE_ROOT);
    let internal = root.join("internal");
    let readonly = root.join("readonly");

    // Clean slate — remove any leftover fixtures from a previous run
    if root.exists() {
        fs::remove_dir_all(root).expect("failed to clean MTP fixture root");
    }

    // Create directory structure
    fs::create_dir_all(internal.join("Documents")).expect("failed to create Documents dir");
    fs::create_dir_all(internal.join("DCIM")).expect("failed to create DCIM dir");
    fs::create_dir_all(internal.join("Music")).expect("failed to create Music dir");
    fs::create_dir_all(readonly.join("photos")).expect("failed to create photos dir");

    // Populate writable storage
    fs::write(
        internal.join("Documents/report.txt"),
        "Quarterly report — Q4 2025 placeholder content.\n",
    )
    .expect("failed to write report.txt");
    fs::write(
        internal.join("Documents/notes.txt"),
        "Meeting notes: discuss MTP E2E test strategy.\n",
    )
    .expect("failed to write notes.txt");
    fs::write(internal.join("DCIM/photo-001.jpg"), b"\xFF\xD8\xFF\xE0dummy-jpeg-bytes")
        .expect("failed to write photo-001.jpg");

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
        supports_rename: true,
        event_poll_interval: Duration::from_millis(100),
    };

    let device_info = mtp_rs::register_virtual_device(&config);
    let location_id = device_info.location_id;

    info!(
        "Registered virtual MTP device: {} {} (serial={}, location_id={})",
        config.manufacturer, config.model, config.serial, location_id,
    );

    location_id
}
