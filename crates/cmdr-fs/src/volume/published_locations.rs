//! Which discovered locations the volume switcher's list publishes.
//!
//! Both platform discovery modules gather their locations in presentation order
//! (favorites, the main volume, attached volumes, cloud drives, …) and funnel
//! the result through [`dedupe_locations`] here, because the rule is a pure list
//! transform rather than platform knowledge.
//!
//! **A favorite is a shortcut, not a volume.** It's a `fav-<uuid>` row pointing
//! at a folder that some volume holds, so it competes with other rows on its ID
//! only, never on its path. Letting it claim its path would let a favorite at
//! `/` (or at a drive's mount root) hide the volume it points into, and every
//! favorite on that volume would then resolve to a volume missing from the list.
//!
//! This only decides what the switcher shows. It runs after the canonical-root
//! collapse (`super::canonical_root`), which has already merged one filesystem
//! mounted at several paths.

use std::collections::HashSet;

/// A discovered location, as the dedupe sees it.
///
/// Implemented by each platform on its own `LocationInfo`, for the same reason
/// as `super::canonical_root::MountRootCandidate`: the two modules keep separate
/// but identically-shaped structs, so the dedupe asks for the fields it needs.
pub trait PublishedLocation {
    /// The row's ID: a volume ID, or `fav-<uuid>` for a favorite.
    fn location_id(&self) -> &str;

    /// The absolute path the row opens.
    fn location_path(&self) -> &str;

    /// Whether the row is a favorite: a shortcut into a volume, not a volume.
    fn is_favorite(&self) -> bool;
}

/// Keeps the first row per ID, and the first volume row per path.
///
/// Order is preserved, so the caller's gathering order decides which of two
/// clashing rows survives and where it sits.
pub fn dedupe_locations<T: PublishedLocation>(locations: impl IntoIterator<Item = T>) -> Vec<T> {
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut seen_paths: HashSet<String> = HashSet::new();
    let mut published = Vec::new();

    for location in locations {
        // Both inserts must run, so the sets can't drift apart on a partial hit.
        // A favorite never claims its path (see the module header).
        let new_id = seen_ids.insert(location.location_id().to_string());
        let new_path = location.is_favorite() || seen_paths.insert(location.location_path().to_string());
        if new_id && new_path {
            published.push(location);
        }
    }

    published
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three fields the dedupe needs, standing in for both platforms'
    /// `LocationInfo`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Row {
        id: &'static str,
        path: &'static str,
        favorite: bool,
    }

    impl PublishedLocation for Row {
        fn location_id(&self) -> &str {
            self.id
        }
        fn location_path(&self) -> &str {
            self.path
        }
        fn is_favorite(&self) -> bool {
            self.favorite
        }
    }

    fn volume(id: &'static str, path: &'static str) -> Row {
        Row {
            id,
            path,
            favorite: false,
        }
    }

    fn favorite(id: &'static str, path: &'static str) -> Row {
        Row {
            id,
            path,
            favorite: true,
        }
    }

    fn ids(rows: &[Row]) -> Vec<&str> {
        rows.iter().map(|row| row.id).collect()
    }

    #[test]
    fn distinct_rows_all_survive_in_order() {
        let published = dedupe_locations(vec![
            favorite("fav-1", "/Applications"),
            volume("root", "/"),
            volume("vol-usb", "/Volumes/USB"),
        ]);
        assert_eq!(ids(&published), ["fav-1", "root", "vol-usb"]);
    }

    #[test]
    fn a_repeated_id_keeps_the_first_row() {
        let published = dedupe_locations(vec![volume("root", "/"), volume("root", "/System/Volumes/Data")]);
        assert_eq!(ids(&published), ["root"]);
    }

    #[test]
    fn two_volumes_at_one_path_keep_the_first() {
        // A provider's mount sitting exactly at a cloud drive's folder: one row.
        let published = dedupe_locations(vec![
            volume("cloud-icloud", "/Users/x/Library/Mobile Documents/com~apple~CloudDocs"),
            volume("vol-fuse", "/Users/x/Library/Mobile Documents/com~apple~CloudDocs"),
        ]);
        assert_eq!(ids(&published), ["cloud-icloud"]);
    }

    #[test]
    fn a_favorite_at_the_root_keeps_the_main_volume() {
        // Issue #349: a favorite at `/` took the path slot, the "Macintosh HD"
        // row vanished, and every favorite on the boot disk then resolved to a
        // volume missing from the list.
        let published = dedupe_locations(vec![
            favorite("fav-1", "/Applications"),
            favorite("fav-2", "/"),
            volume("root", "/"),
        ]);
        assert_eq!(ids(&published), ["fav-1", "fav-2", "root"]);
    }

    #[test]
    fn a_favorite_at_a_mount_root_keeps_that_volume() {
        let published = dedupe_locations(vec![
            favorite("fav-1", "/Volumes/naspi"),
            volume("root", "/"),
            volume("smb-naspi", "/Volumes/naspi"),
        ]);
        assert_eq!(ids(&published), ["fav-1", "root", "smb-naspi"]);
    }

    #[test]
    fn a_favorite_at_a_cloud_drive_folder_keeps_the_cloud_drive() {
        let published = dedupe_locations(vec![
            favorite("fav-1", "/Users/x/Dropbox"),
            volume("root", "/"),
            volume("cloud-dropbox", "/Users/x/Dropbox"),
        ]);
        assert_eq!(ids(&published), ["fav-1", "root", "cloud-dropbox"]);
    }

    #[test]
    fn an_empty_list_stays_empty() {
        assert!(dedupe_locations(Vec::<Row>::new()).is_empty());
    }
}
