//! Every backend's cells through this app's write pipeline, and the
//! backend-blind scenarios they share.
//!
//! The `network_*_test_support.rs` files hold the scenarios, written once
//! against `dyn Volume`. The `<backend>_*_test.rs` files are thin cells that dial
//! one backend's fixture and hand the live volume to a scenario. ❗ The cells stay
//! per backend because the integration lane selects them by name prefix
//! (`smb_integration_`, `sftp_integration_`, `webdav_integration_`, `s3_integration_`:
//! `scripts/check/checks/fixture-lane-coverage.go`). The ADB and MTP cells need no
//! Docker and run in the unit lane. Which scenario runs on which backend, and why
//! any is skipped: `../DETAILS.md` § "Backend suites".

// The sources the cancel scenarios hold still at a chunk boundary.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod network_gated_source_test_support;
// The transfer scenarios the WebDAV, SFTP, and ADB suites share.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod network_transfer_test_support;
// Merges, moves, safety, look-alikes, archives: every server backend drives these.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod network_archive_test_support;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod network_look_alike_test_support;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod network_move_drift_test_support;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod network_safety_test_support;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod network_semantics_test_support;

// "A copy keeps the source's date", onto a server and off it, for every backend.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod network_dates_test_support;

// The date scenarios against the in-memory double: the engine's half of
// "a copy keeps the source's date", green whatever the network backends do.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod in_memory_dates_test;

// Real copies, a move, a delete, and a mkdir between local disk and a phone over
// ADB, against the crate's fake server, through the app's own write operations.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod adb_transfer_test;
// A phone's drive index end to end: the scan over the fake server, a Cmdr copy
// and delete patching it, an unplug mid-scan, and another phone's path kept out.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod adb_index_test;

// Archive browsing and remote editing over a virtual MTP device: the archive
// routing is this app's, so the cell sits with the pipeline it asserts on rather
// than with the backend. The MTP twin of `smb_archive_integration_test`.
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "virtual-mtp"))]
mod mtp_archive_test;
// "A copy keeps the source's date" onto and off a virtual MTP device.
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "virtual-mtp"))]
mod mtp_dates_test;

// SFTP: gated on the Docker fixture and named for the `sftp_integration_` lane.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod sftp_archive_integration_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod sftp_look_alike_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod sftp_test_support;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod sftp_transfer_integration_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod sftp_transfer_safety_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod sftp_transfer_semantics_test;

// SMB: every cell whose other half is this pipeline rather than the protocol.
// The backend's own white-box suites live in `cmdr-smb`. All Docker-gated and
// named for the `smb_integration_` lane, except the soak and stress loops, which
// are opt-in by hand.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod smb_archive_integration_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod smb_full_concurrency_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod smb_look_alike_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod smb_soak_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod smb_stream_write_integration_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod smb_stress_test;
// The fixture wiring the SMB suites share, plus the three outside this
// directory (`volume::smb_media_fetch_integration_test`,
// `listing::smb_pane_close_watch_integration_test`, and
// `network::smb_upgrade_respell_test`), which reach it by path.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) mod smb_test_support;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod smb_transfer_safety_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod smb_transfer_semantics_test;

// S3: gated on the Docker fixture and named for the `s3_integration_` lane.
// Copies onto, off, and between buckets; renames that run as moves, and
// copies inside one account; the shared semantics and safety scenarios. Every
// scenario takes an `S3Target`, so `s3_live_engine_test` runs the same bodies
// against real accounts (skipped unless `CMDR_S3_LIVE=1`).
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod s3_engine_integration_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod s3_live_engine_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod s3_rename_integration_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod s3_transfer_integration_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod s3_transfer_safety_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod s3_transfer_semantics_test;

// WebDAV: gated on the Docker fixture and named for the `webdav_integration_` lane.
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod webdav_archive_integration_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod webdav_look_alike_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod webdav_refusal_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod webdav_refusing_proxy_test_support;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod webdav_test_support;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod webdav_transfer_integration_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod webdav_transfer_safety_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod webdav_transfer_semantics_test;
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod webdav_wire_cost_test;
