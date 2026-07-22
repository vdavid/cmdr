//! Path arithmetic for indexing. Pure, lock-free helpers that map between the
//! filesystem's absolute paths and each volume's index path space.
//!
//! - [`routing`]: path -> owning-volume resolution (`volume_id_for_local_path`),
//!   the mount-relative `IndexPathSpace`, and `index_read_path` (the read-side
//!   mount/scheme strip). The seam that teaches the local pipeline a
//!   mount-rooted volume's path space.
//! - [`firmlinks`]: macOS firmlink + `/private`-symlink normalization to the
//!   canonical form the index stores and every lookup uses.
//! - [`path_prefix`]: component-aware absolute-path prefix tests (so `/a/bc` is
//!   never a child of `/a/b`), shared by rescan ancestor-collapse and
//!   removal-storm coalescing.

pub mod firmlinks;
pub(crate) mod path_prefix;
pub(crate) mod routing;
