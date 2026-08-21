//! The SMB backend, as this app sees it.
//!
//! The backend itself is the `cmdr-smb` crate: a `Volume` over a live smb2
//! session, with its own watcher, reconnect loop, and scan pool, and no `tauri`
//! anywhere in its dependency tree (enforced by the `index-crate-isolation`
//! check). This module re-exports it under its original
//! `crate::file_system::volume::backends::smb::…` path, the way
//! `file_system::volume` re-exports `cmdr_fs::volume`, so every call site here
//! reads the same as before the crate boundary existed.
//!
//! What stays on this side, and why it isn't in the crate:
//!
//! - **Finding a share and mounting it.** `network/` owns mDNS discovery, the
//!   keychain, the kernel mount, the OS-mount-to-smb2 upgrade passes, and the
//!   once-per-server fallback notice. None of it can be answered from the
//!   protocol alone.
//! - **Registration and routing.** `network/smb_upgrade.rs` mints an
//!   `SmbVolume`, hands it the app's `VolumeHost`, and registers it. A backend
//!   never registers itself.
//! - **Driving transfers.** `write_operations/` runs every copy, move, and
//!   delete with the real event sink, pause gate, and conflict resolution; the
//!   backend only ever moves bytes.
//!
//! Editing the backend? Everything is in `crates/cmdr-smb/` — start at its
//! `CLAUDE.md`. `cargo check -p cmdr-smb` is a complete verification loop there,
//! with none of this app in it.

pub use cmdr_smb::volume::*;

// The app's half of the SMB suites: the cells whose other side is this app's own
// machinery rather than the protocol. The backend's own white-box suites live
// with it, in `cmdr-smb`. `#[path]` on a module declared here resolves relative
// to `backends/`, which is where the files sit.
#[cfg(test)]
#[path = "smb_test_support.rs"]
mod smb_test_support;

#[cfg(test)]
#[path = "smb_app_integration_test.rs"]
mod smb_app_integration_test;
#[cfg(test)]
#[path = "smb_archive_integration_test.rs"]
mod smb_archive_integration_test;
#[cfg(test)]
#[path = "smb_full_concurrency_test.rs"]
mod smb_full_concurrency_test;
#[cfg(test)]
#[path = "smb_media_fetch_integration_test.rs"]
mod smb_media_fetch_integration_test;
#[cfg(test)]
#[path = "smb_soak_test.rs"]
mod smb_soak_test;
#[cfg(test)]
#[path = "smb_stress_test.rs"]
mod smb_stress_test;
#[cfg(test)]
#[path = "smb_transfer_safety_test.rs"]
mod smb_transfer_safety_test;
#[cfg(test)]
#[path = "smb_transfer_semantics_test.rs"]
mod smb_transfer_semantics_test;
