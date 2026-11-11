//! Volume and location discovery: the favorites, disks, cloud drives, and network shares the
//! volume switcher lists, plus the live mount watching that keeps the list current. Distinct from
//! `file_system/volume/` (the `Volume` trait, `VolumeManager`).
//!
//! This module owns the model every OS publishes (one `LocationInfo`, one `SmbMountInfo`), so the
//! frontend reads one shape everywhere. How each OS finds its mounts is `platform/`, a platform
//! seam (`docs/guides/platform-seams.md`).

mod model;
mod platform;

pub(crate) use model::NetworkMount;
pub use model::{DEFAULT_VOLUME_ID, LocationCategory, LocationInfo, SmbMountInfo, VolumeInfo};

/// Free and total space for the filesystem at `path`, `None` when it can't be read. The local
/// backend's reading, the same one on every OS (`crates/cmdr-local/DETAILS.md` § "Local volume
/// space").
pub use cmdr_local::space::get_volume_space;

// The seam's whole surface, OS-only extras included: `platform/mod.rs` lists and documents it.
pub(crate) use platform::*;
