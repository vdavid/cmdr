//! Which mount root a volume ID publishes, when one filesystem is mounted at
//! several paths.
//!
//! A volume ID is identity, and one filesystem can legitimately be reached
//! through more than one mount point: macOS mounts the same SMB share at
//! `/Volumes/naspi` and `/Volumes/naspi-1`, and on Linux a bind mount or a
//! container mount does the same thing routinely. All of them derive the ID the
//! first one did (a share keys on `(server, port, share)`, a local disk on its
//! filesystem UUID), so publishing every mount hands one identity several
//! locations, and everything downstream keys on the ID.
//!
//! Both platform discovery modules funnel their enumeration through
//! [`collapse_by_volume_id`] here, because the rule is a pure list transform
//! over `(volume id, mount root)` pairs rather than platform knowledge. What IS
//! platform-specific is deriving the ID from a mount (macOS `getfsstat` plus the
//! filesystem UUID, Linux `/proc/mounts` plus `/dev/disk/by-uuid`), and that
//! stays in each platform's module.
//!
//! **This only decides what discovery PUBLISHES.** The registry keeps every
//! mount root it learns about and promotes a survivor when the active one dies
//! (`file_system/volume/DETAILS.md` § "A volume ID owns a set of mount roots"),
//! so collapsing a row out of the switcher never makes a root unfindable and
//! never moves a pane that already sits on one.

/// A discovered mount, as the canonical-root collapse sees it: an identity, and
/// where that identity is currently reachable.
///
/// Implemented by each platform on its own location type. The two modules keep
/// separate but identically-shaped `LocationInfo` structs on purpose, so the
/// collapse asks for the two fields it needs instead of a shared type.
pub trait MountRootCandidate {
    /// The volume ID this mount derives, minted through `super::ids`.
    fn volume_id(&self) -> &str;

    /// The mount root this candidate would publish, as an absolute path.
    fn mount_root(&self) -> &str;
}

/// Collapse candidates that share a volume ID down to one, at a canonical root.
///
/// The survivor is the one [`is_more_canonical_root`] prefers; everything else
/// with that ID is dropped. Candidates that hold the only copy of their ID pass
/// through untouched, and the surviving order is first-seen, so a caller's own
/// sort still decides presentation.
///
/// Pure and order-independent: which root wins can't depend on the order the
/// kernel happened to list the mounts in.
pub fn collapse_by_volume_id<T: MountRootCandidate>(candidates: Vec<T>) -> Vec<T> {
    let mut canonical: Vec<T> = Vec::with_capacity(candidates.len());
    let mut index_of_id: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for candidate in candidates {
        match index_of_id.get(candidate.volume_id()) {
            Some(&at) => {
                if is_more_canonical_root(candidate.mount_root(), canonical[at].mount_root()) {
                    canonical[at] = candidate;
                }
            }
            None => {
                index_of_id.insert(candidate.volume_id().to_string(), canonical.len());
                canonical.push(candidate);
            }
        }
    }

    canonical
}

/// Whether `candidate` should win over `current` as a volume's published root:
/// shorter first, then lexicographic so the choice never depends on order.
///
/// The shortest path wins because the OS suffixes the LATER mount
/// (`/Volumes/naspi-1`), so the shortest is the original: the root every saved
/// path, favorite, and index row already refers to. The registry ranks its own
/// roots by the same path shape, one rank below liveness
/// (`file_system/volume/manager/roots.rs`).
pub fn is_more_canonical_root(candidate: &str, current: &str) -> bool {
    (candidate.len(), candidate) < (current.len(), current)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two fields the collapse needs, and nothing else. Standing in for both
    /// platforms' `LocationInfo`, which is the point: the rule is a list
    /// transform, so it needs no mounts, no syscalls, and no platform.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Mount {
        id: &'static str,
        root: &'static str,
    }

    impl MountRootCandidate for Mount {
        fn volume_id(&self) -> &str {
            self.id
        }
        fn mount_root(&self) -> &str {
            self.root
        }
    }

    fn mount(id: &'static str, root: &'static str) -> Mount {
        Mount { id, root }
    }

    fn roots(collapsed: &[Mount]) -> Vec<&str> {
        collapsed.iter().map(|m| m.root).collect()
    }

    #[test]
    fn two_roots_for_one_volume_collapse_to_the_shortest_path() {
        // The `/Volumes/naspi` + `/Volumes/naspi-1` shape: the OS suffixes the
        // later mount, so the shortest path is the original one every saved path
        // already refers to.
        let collapsed = collapse_by_volume_id(vec![
            mount("smb-naspi", "/Volumes/naspi-1"),
            mount("smb-naspi", "/Volumes/naspi"),
        ]);
        assert_eq!(roots(&collapsed), ["/Volumes/naspi"]);
    }

    #[test]
    fn the_winner_does_not_depend_on_the_order_mounts_arrive_in() {
        // Discovery order is the kernel's, not ours, so it must not decide
        // identity: both orders publish the same root.
        let forward = collapse_by_volume_id(vec![mount("v", "/mnt/data"), mount("v", "/srv/backup-data")]);
        let reverse = collapse_by_volume_id(vec![mount("v", "/srv/backup-data"), mount("v", "/mnt/data")]);
        assert_eq!(roots(&forward), ["/mnt/data"]);
        assert_eq!(roots(&reverse), ["/mnt/data"]);
    }

    #[test]
    fn equal_length_roots_break_ties_lexicographically() {
        let collapsed = collapse_by_volume_id(vec![mount("v", "/mnt/bbb"), mount("v", "/mnt/aaa")]);
        assert_eq!(roots(&collapsed), ["/mnt/aaa"]);
    }

    #[test]
    fn distinct_volumes_all_survive_in_first_seen_order() {
        // Collapsing is per-ID. Three filesystems stay three rows, and the order
        // they came in is preserved so the caller's own sort still decides.
        let collapsed = collapse_by_volume_id(vec![
            mount("vol-usb", "/media/user/USB"),
            mount("root", "/"),
            mount("smb-naspi", "/mnt/naspi"),
        ]);
        assert_eq!(roots(&collapsed), ["/media/user/USB", "/", "/mnt/naspi"]);
    }

    #[test]
    fn a_volume_mounted_many_times_still_publishes_once() {
        // Bind mounts are routine on Linux, so "one ID, several roots" can be
        // more than two.
        let collapsed = collapse_by_volume_id(vec![
            mount("vol-data", "/srv/containers/app/data"),
            mount("vol-data", "/mnt/data"),
            mount("vol-data", "/home/user/data"),
            mount("vol-other", "/mnt/other"),
        ]);
        assert_eq!(roots(&collapsed), ["/mnt/data", "/mnt/other"]);
    }

    #[test]
    fn an_empty_list_collapses_to_an_empty_list() {
        assert!(collapse_by_volume_id(Vec::<Mount>::new()).is_empty());
    }

    #[test]
    fn canonical_ranking_is_shortest_then_lexicographic() {
        assert!(is_more_canonical_root("/Volumes/naspi", "/Volumes/naspi-1"));
        assert!(!is_more_canonical_root("/Volumes/naspi-1", "/Volumes/naspi"));
        assert!(is_more_canonical_root("/mnt/aaa", "/mnt/bbb"));
        assert!(!is_more_canonical_root("/mnt/bbb", "/mnt/aaa"));
        // Not strictly better than itself, so a re-scan can't flip a live choice.
        assert!(!is_more_canonical_root("/mnt/data", "/mnt/data"));
    }
}
