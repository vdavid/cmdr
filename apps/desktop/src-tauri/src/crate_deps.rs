//! Markers that keep `unused_crate_dependencies` quiet.
//!
//! The lint is `#![warn(...)]` at the crate root because it's judged per
//! compilation unit: as a package-wide flag every bin, integration test, and
//! bench would report ~100 "unused extern crate" errors for deps only the lib
//! uses. What it buys is catching platform-specific `cfg` mismatches, and the
//! price is one `use foo as _;` per dependency whose only uses sit behind a
//! `cfg` this build didn't take.
//!
//! A marker satisfies the lint from anywhere in the crate, so they live here
//! rather than in `lib.rs`: they're lint artifacts, and `lib.rs`'s preamble is
//! the crate's module map, which is a different thing to read.

//noinspection RsUnusedImport
// Silence false positives for dev dependencies (used only in benches/, not lib)
// and transitive dependencies (notify is used by notify-debouncer-full)
#[cfg(test)]
use criterion as _;
//noinspection RsUnusedImport
// Property-based testing. Used in module-local `mod proptests` blocks; the
// marker keeps `unused_crate_dependencies` quiet for builds that happen to
// compile a subset of test modules.
#[cfg(test)]
use proptest as _;
//noinspection RsUnusedImport
// Dev-only log-routing shim. Used by the phase4 bench's optional
// `env_logger::try_init()` (commented-in when collecting wire traces) and by
// ad-hoc debug-logging in tests. Harmless otherwise.
#[cfg(test)]
use env_logger as _;
//noinspection RsUnusedImport
// We dev-depend on ourselves so the `testing` feature is on for dev targets and
// off for the shipped binary (see `Cargo.toml`). That makes `cmdr_lib` an extern
// crate of its own test target, which `unused_crate_dependencies` then reports.
#[cfg(test)]
use cmdr_lib as _;
//noinspection RsUnusedImport
// Scratch dirs for tests and fixtures, an optional dependency the `testing`
// feature turns on. Its only LIB use is the virtual-MTP fixture, which also
// needs `virtual-mtp`, so a `testing`-without-`virtual-mtp` build has the crate
// and no use for it.
#[cfg(feature = "testing")]
use tempfile as _;
//noinspection RsUnusedImport
use mimalloc as _;
//noinspection ALL
// smb2 crate is used in network/smb_client module (macOS + Linux)
#[cfg(any(target_os = "macos", target_os = "linux"))]
use smb2 as _;

//noinspection ALL
// trash crate is used in write_operations/trash.rs (Linux only)
#[cfg(target_os = "linux")]
use trash as _;

//noinspection ALL
// keyring-core + the zbus secret-service backend are used in secrets/keyring_linux.rs
// for credential storage (Linux only).
#[cfg(target_os = "linux")]
use keyring_core as _;
#[cfg(target_os = "linux")]
use zbus_secret_service_keyring_store as _;
//noinspection ALL
// MCP Bridge is only used in debug builds, so silence the warning in release builds
#[cfg(not(debug_assertions))]
use tauri_plugin_mcp_bridge as _;
//noinspection ALL
// tauri_plugin_updater is only registered on non-macOS (custom updater handles macOS)
#[cfg(target_os = "macos")]
use tauri_plugin_updater as _;
// cmdr-adb is used in the adb/ module for Android-over-ADB support (macOS + Linux)
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use cmdr_adb as _;
//noinspection ALL
// `bytes` is a dev-dependency the MTP upload cells build their fake source streams
// out of, and every one of them is behind `virtual-mtp`. The lanes that don't pass
// that feature still LINK it into the lib test target, so without this the extern
// reads as unused there. The production upload path lives in `cmdr-mtp`, which
// declares its own copy.
#[cfg(test)]
use bytes as _;
