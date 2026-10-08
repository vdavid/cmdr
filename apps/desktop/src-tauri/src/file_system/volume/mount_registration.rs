//! The one way a MOUNT becomes a registered volume.
//!
//! Path resolution mints a volume ID for ANY mount in the kernel's table
//! (`resolve_path_volume_fast` reads `statfs` / `/proc/mounts`), so registration
//! has to cover that same table. When it didn't, every mount outside `/Volumes`
//! resolved to an ID nothing served, and the pane died on
//! `VolumeError::NotFound` — which is what a cloud
//! client mounting into the home folder (pCloud's `~/pCloud Drive`) hit, along
//! with any hand-mounted share, disk image, or FUSE filesystem.
//!
//! ❗ The rule that keeps it fixed: **registration must never be cleverer than
//! resolution**. Registering a mount nothing ever resolves to costs a HashMap
//! entry; skipping one that resolution can name costs the user their folder. So
//! the sweep takes every mount this account can reach, and what the SWITCHER
//! shows is a separate, stricter question (`volumes::mounts`). The one row left
//! out is another account's own mount, which this account can't open
//! (`registrable_mount_roots`); adoption still nets a path `statfs` answers for.
//!
//! Three callers share this module: the startup sweep
//! (`file_system::register_discovered_volumes`), the mount watcher
//! (`volumes::watcher`), and the listing's last-chance adoption
//! (`listing::streaming`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::Volume;
use super::backends::LocalPosixVolume;
use super::manager::get_volume_manager;

#[cfg(target_os = "macos")]
use crate::volumes as platform;
#[cfg(target_os = "linux")]
use crate::volumes_linux as platform;

/// A live mount, as the ID funnel sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MountIdentity {
    /// Where the filesystem is mounted right now.
    root: PathBuf,
    /// The ID `volume_id_for_mount` derives for it. ❗ Always from the funnel,
    /// ❌ never rebuilt from parts.
    volume_id: String,
}

/// The display name a mount gets when it's registered from its path alone: its
/// last path component, which is what a user calls the drive ("pCloud Drive").
///
/// Deliberately NOT an NSURL volume-name lookup: this runs for every mount in
/// the table, and that call blocks on a wedged one. Discovery does the pretty
/// name for the rows it publishes; the registry only needs something readable in
/// a log line.
fn name_for_root(root: &Path) -> String {
    root.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Unknown")
        .to_string()
}

/// Registers the mount at `root` under the ID the funnel derives for it.
///
/// Keeps any incumbent ([`VolumeManager::register_if_absent`]), so a second
/// mount of an already-registered filesystem records its root as a fallback
/// instead of displacing a live backend — which is what stops this from
/// downgrading an upgraded `smb2` session back to a `LocalPosixVolume`.
///
/// Returns nothing on purpose: a caller that wants the ID has a path, and
/// `volume_id_for_mount` is the funnel that answers that question.
///
/// [`VolumeManager::register_if_absent`]: super::manager::VolumeManager::register_if_absent
pub(crate) fn register_mount_root(root: &str) {
    let volume_id = platform::volume_id_for_mount(root);
    register_mount_root_as(&volume_id, root);
}

/// Registers the mount at `root` under an ID the caller has ALREADY derived from
/// that same root.
///
/// For the one caller that derived it a moment ago and must not re-derive:
/// [`adopt_with`] proved `volume_id` is what this mount is called, and asking
/// again would both pay for a second `statfs` and open a window where a mount
/// change binds the path to a DIFFERENT volume than the one adoption approved.
/// ❌ Never call this with an ID from anywhere but `volume_id_for_mount(root)`.
fn register_mount_root_as(volume_id: &str, root: &str) {
    let volume = Arc::new(LocalPosixVolume::new(name_for_root(Path::new(root)), root));
    if get_volume_manager().register_if_absent(volume_id, volume) {
        log::debug!(target: "volume", "Registered mount {root} as {volume_id}");
    }
}

/// Registers every mount the kernel currently lists that this account can
/// reach, so no path can resolve to an ID the registry has never heard of.
///
/// Browsable or not, on purpose (see the module header). A table that wouldn't answer
/// registers nothing and says so: ❌ never treat it as an empty machine.
pub(crate) fn register_every_mount() {
    let Some(roots) = platform::registrable_mount_roots() else {
        log::warn!(
            target: "volume",
            "The mount table wouldn't answer, so no mounts were registered this pass; \
             a pane on one adopts it on first listing instead."
        );
        return;
    };
    log::debug!(target: "volume", "Registering {} mount(s) from the kernel table", roots.len());
    for root in roots {
        register_mount_root(&root);
    }
}

/// Last-chance registration for a path whose volume ID nothing serves: if the
/// mount that actually owns `path` derives exactly `volume_id`, register it and
/// hand it back.
///
/// The safety net under the startup sweep and the mount watcher, for a
/// filesystem that arrived without an `NSWorkspace` notification (a FUSE or
/// NFS-backed cloud drive mounted while Cmdr was running posts none) or before
/// the sweep finished (discovery runs off the main thread, and a restored tab
/// can resolve first).
///
/// # The equality check is the whole safety argument
///
/// Adoption binds a path to a backend, so it may only ever confirm what the
/// caller already believes. Asking the SAME funnel what the live mount under
/// `path` is called and requiring it to equal `volume_id` means adoption can't
/// invent a volume, can't answer for a phone or a saved server that simply isn't
/// connected (those IDs derive from no mount, so they never match, and stay
/// `NotConnected`), and can't hand a path to a drive it isn't on. ❌ Never
/// weaken it to a prefix or a path-shape test.
///
/// Blocking: reads the mount table and, for a local mount, one NSURL resource.
/// Call it from a blocking context.
pub(crate) fn adopt_mount_serving(volume_id: &str, path: &Path) -> Option<Arc<dyn Volume>> {
    adopt_with(volume_id, path, live_mount_under)
}

/// The live mount that owns `path`, named by the ID funnel.
fn live_mount_under(path: &Path) -> Option<MountIdentity> {
    let (root, _fs_type) = platform::get_mount_point(&path.to_string_lossy())?;
    let volume_id = platform::volume_id_for_mount(&root);
    Some(MountIdentity {
        root: PathBuf::from(root),
        volume_id,
    })
}

/// [`adopt_mount_serving`]'s body with the mount probe injected, so the rule is
/// testable without a real mount.
fn adopt_with(
    volume_id: &str,
    path: &Path,
    probe: impl FnOnce(&Path) -> Option<MountIdentity>,
) -> Option<Arc<dyn Volume>> {
    let found = probe(path)?;
    if found.volume_id != volume_id {
        return None;
    }
    let root = found.root.to_string_lossy().to_string();
    log::info!(
        target: "volume",
        "Adopting the live mount at {root} for volume {volume_id}, which nothing had registered.",
    );
    // With the ID the probe proved, ❌ never a fresh derivation: see
    // `register_mount_root_as`.
    register_mount_root_as(volume_id, &root);
    get_volume_manager().get(volume_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::listing::caching_test_support::unique_test_id;

    fn identity(root: &str, volume_id: &str) -> MountIdentity {
        MountIdentity {
            root: PathBuf::from(root),
            volume_id: volume_id.to_string(),
        }
    }

    /// The pCloud case: a mount in the home folder, resolving to an ID the
    /// registry never heard of, is adopted on the spot.
    #[test]
    fn a_mount_whose_id_matches_is_adopted() {
        let volume_id = unique_test_id("vol-adopt-match");
        let root = format!("/Users/someone/{}", unique_test_id("pCloud Drive"));
        let path = PathBuf::from(&root).join("Documents/report.pdf");
        assert!(
            get_volume_manager().get(&volume_id).is_none(),
            "precondition: nothing serves this ID yet"
        );

        let adopted = adopt_with(&volume_id, &path, |_| Some(identity(&root, &volume_id)));

        let adopted = adopted.expect("the live mount derives this exact ID, so it is adoptable");
        assert_eq!(adopted.root(), Path::new(&root));
        assert!(
            get_volume_manager().get(&volume_id).is_some(),
            "adoption registers, so every later caller finds it too"
        );
        get_volume_manager().unregister(&volume_id);
    }

    /// The safety argument: an ID the live mount doesn't derive is never adopted,
    /// so a disconnected phone or saved server keeps its own answer and no path
    /// is ever bound to a drive it isn't on.
    #[test]
    fn a_mount_whose_id_differs_is_left_alone() {
        let asked_for = unique_test_id("vol-adopt-asked");
        let derived = unique_test_id("vol-adopt-derived");
        let root = format!("/Volumes/{}", unique_test_id("Other"));

        let adopted = adopt_with(&asked_for, Path::new("/Volumes/Other/file.txt"), |_| {
            Some(identity(&root, &derived))
        });

        assert!(adopted.is_none(), "the IDs disagree, so nothing may be adopted");
        assert!(
            get_volume_manager().get(&asked_for).is_none(),
            "a refused adoption registers nothing under the asked-for ID"
        );
        assert!(
            get_volume_manager().get(&derived).is_none(),
            "and nothing under the derived one either: adoption is not a discovery pass"
        );
    }

    /// A path on no live mount (the drive really did leave) stays an error.
    #[test]
    fn a_path_on_no_live_mount_is_not_adopted() {
        let volume_id = unique_test_id("vol-adopt-gone");

        let adopted = adopt_with(&volume_id, Path::new("/Volumes/Gone/file.txt"), |_| None);

        assert!(adopted.is_none());
        assert!(get_volume_manager().get(&volume_id).is_none());
    }

    #[test]
    fn a_root_with_no_last_component_still_gets_a_name() {
        assert_eq!(name_for_root(Path::new("/Users/sven/pCloud Drive")), "pCloud Drive");
        assert_eq!(name_for_root(Path::new("/")), "Unknown");
    }
}
