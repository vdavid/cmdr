//! What each OS answers about a local item: the facts that ask the OS itself (the
//! download record, Spotlight, the code signature, the default app) and the few
//! `stat` fields that differ per OS. A platform seam (`docs/guides/platform-seams.md`).
//!
//! Every fn here is blocking and some reach a framework over XPC: call them from
//! the about pool's 8 MB-stack threads, ❌ never the async executor or rayon.

use cmdr_about::card::OsCaps;
use cmdr_about::fact::{AboutTarget, Fact};
use cmdr_about::payload::{AppRef, Signature, Usage};
use cmdr_about::provenance::Download;

#[cfg_attr(target_os = "macos", path = "macos/mod.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

#[cfg(unix)]
mod unix;

/// What `stat` says beyond the portable fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StatExtras {
    /// Any execute bit is set.
    pub(crate) executable: bool,
    /// A cloud drive evicted the bytes, so reading them would download the file.
    pub(crate) online_only: bool,
    /// What the item occupies on disk. `None`: the OS doesn't say.
    pub(crate) physical_size: Option<u64>,
    /// How many names point at the node. `None`: the OS doesn't count them.
    pub(crate) hard_links: Option<u64>,
}

const _: fn() -> OsCaps = os_caps;
const _: fn(&std::fs::Metadata) -> StatExtras = stat_extras;
const _: fn(&str) -> Fact<Download> = download;
const _: fn(&str) -> Fact<Usage> = last_opened;
const _: fn(&str) -> Fact<AppRef> = default_app;
const _: fn(&str, AboutTarget, Option<&str>) -> Fact<Signature> = signature;

/// What this OS keeps about any item, for the plan.
pub(crate) use imp::os_caps;

/// The `stat` fields that differ per OS.
pub(crate) use imp::stat_extras;

/// What the OS recorded about the item's download: macOS's quarantine and
/// `kMDItemWhereFroms` xattrs, Linux's `user.xdg.*` ones. Each xattr is read
/// capped, ❌ never whole (an attacker sizes it). An item with none answers
/// `Unknown(NoRecord)`.
pub(crate) use imp::download;

/// When the item was last opened and how often, from the OS's use history.
/// Nothing recorded is `Unknown(NoRecord)`, ❌ never "never opened".
pub(crate) use imp::last_opened;

/// The app the OS opens the item with.
pub(crate) use imp::default_app;

/// The code signature the item carries, and whether its main executable matches
/// it (one bounded check, resources never hashed). ❌ Never deep validation.
pub(crate) use imp::signature;
