//! Which dotted folders this OS presents as one item, beyond the package list. A platform seam
//! (`docs/guides/platform-seams.md`), lined up with `cmdr-fs`'s own
//! (`crates/cmdr-fs/src/file_extension/platform/`): only macOS has packages, so only its arm
//! asks the OS; the others answer from the list alone, the way that seam's
//! `FOLDER_NAMES_MAKE_PACKAGES` says they do.

use std::future::Future;

#[cfg_attr(target_os = "macos", path = "macos.rs")]
#[cfg_attr(target_os = "linux", path = "linux.rs")]
#[cfg_attr(target_os = "windows", path = "windows.rs")]
mod imp;

type Answers = Option<Vec<(String, bool)>>;

// An `async fn` doesn't coerce to a fn pointer, so the pin names its argument and the output its
// future resolves to.
const _: fn() = || {
    fn resolves_to<T, F: Future<Output = T>>(_: F) {}
    resolves_to::<Answers, _>(look_up(Vec::new()));
};

/// Each candidate with this OS's answer to "is this folder one item?", or `None` when the lookups
/// ran out of time (the pane then keeps the list's answer for this range). An OS with no package
/// concept beyond the list answers `Some` of nothing.
pub(super) use imp::look_up;

/// Whether this OS presents the folder at a path as one item beyond the list, asked of one
/// folder. Blocking (macOS: an NSURL read that can reach File Provider), so only from an
/// 8 MB-stack pool thread. An OS with no package concept beyond the list answers `false`.
pub(super) use imp::is_package_to_the_os;

const _: fn(&str) -> bool = is_package_to_the_os;

/// Whether macOS presents the folder at a path as one item (`NSURLIsPackageKey`). macOS-only by
/// nature, so only macOS code (its tests) reaches it.
#[cfg(all(target_os = "macos", test))]
pub(super) use imp::is_package_to_macos;
