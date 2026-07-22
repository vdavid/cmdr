//! Per-transport enable + live-watch wiring. Each transport builds on the
//! shared machinery (the `network_scanner` trait BFS for SMB/MTP, the local
//! `scanner` + `watch` pipeline for local-external) and differs only in how a
//! volume is enabled and how live changes arrive.
//!
//! - [`smb`]: SMB shares (direct-smb2 gate + `CHANGE_NOTIFY` live watch).
//! - [`mtp`]: MTP storages (USB/PTP; no gate, PTP-event live watch).
//! - [`local_external`]: plain local external drives (mount-rooted, but scanned
//!   and watched by the LOCAL pipeline).

pub(crate) mod smb;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) mod local_external;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) mod mtp;
