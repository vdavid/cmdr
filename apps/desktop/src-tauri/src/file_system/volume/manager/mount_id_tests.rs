//! What [`mount_id_for_path`](super::VolumeManager::mount_id_for_path) answers,
//! which is the one question the mount-root registry gets wrong in a way nothing
//! else notices: the longest-ancestor race, and the routed volumes that must not
//! enter it.
//!
//! The two shipping routed backends have their own cells beside their routes
//! (`archive_routing.rs`, `git_routing.rs`); what lives here is the rule itself.

use super::VolumeManager;
use std::sync::Arc;

#[test]
fn mount_id_for_path_returns_longest_non_root_ancestor() {
    use crate::file_system::LocalPosixVolume;

    let manager = VolumeManager::new();
    manager.register("root", Arc::new(LocalPosixVolume::new("Root", "/")));
    manager.register("ext", Arc::new(LocalPosixVolume::new("Ext", "/Volumes/X")));
    manager.register("nested", Arc::new(LocalPosixVolume::new("Nested", "/Volumes/X/Y")));

    // A path under the external mount routes to it, never to `root`.
    assert_eq!(manager.mount_id_for_path("/Volumes/X/sub").as_deref(), Some("ext"));
    // A nested mount wins over its parent (longest ancestor).
    assert_eq!(
        manager.mount_id_for_path("/Volumes/X/Y/deep").as_deref(),
        Some("nested")
    );
    // The mount root itself matches.
    assert_eq!(manager.mount_id_for_path("/Volumes/X").as_deref(), Some("ext"));
    // A component-boundary sibling is NOT a false prefix hit.
    assert_eq!(manager.mount_id_for_path("/Volumes/XY/z"), None);
    // A boot-disk path matches only `root` (skipped) → None.
    assert_eq!(manager.mount_id_for_path("/Users/me"), None);
}

/// The steal this rule prevents: a routed volume's root is a path INSIDE a
/// real mount, so it wins the longest-ancestor race for everything under it,
/// and a drive-index read or an `inspect_file` lands on a mount with no
/// index. Pinned against a STUB that only declares the capability, ❌ never
/// against `ArchiveVolume` or `GitPortalVolume`: the rule has to hold for the
/// next routed backend, which such a cell couldn't say. (The two shipping
/// backends have their own cells beside their routes.)
#[test]
fn mount_id_for_path_skips_any_volume_that_routes_over_a_parent() {
    use crate::file_system::LocalPosixVolume;
    use crate::file_system::volume::InMemoryVolume;

    let manager = VolumeManager::new();
    manager.register("ext", Arc::new(LocalPosixVolume::new("Ext", "/Volumes/X")));
    manager.register(
        "routed",
        Arc::new(
            InMemoryVolume::new("Routed")
                .with_root("/Volumes/X/thing")
                .routing_over_a_parent(),
        ),
    );

    // The deeper root would win on length alone; the declaration is what
    // keeps the path with the volume that physically holds it.
    assert_eq!(
        manager.mount_id_for_path("/Volumes/X/thing/inside").as_deref(),
        Some("ext")
    );

    // And a volume that does NOT declare it is an ordinary nested mount.
    manager.register(
        "nested",
        Arc::new(InMemoryVolume::new("Nested").with_root("/Volumes/X/mounted")),
    );
    assert_eq!(
        manager.mount_id_for_path("/Volumes/X/mounted/inside").as_deref(),
        Some("nested")
    );
}
