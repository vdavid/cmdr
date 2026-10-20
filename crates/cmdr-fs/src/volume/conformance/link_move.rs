//! What a move leans on about a symbolic link to a folder: the parent's
//! listing reports it as a link, and [`Volume::rename`] moves the link itself.

use std::path::Path;

use crate::volume::{EntryKind, Volume, VolumeError};

/// A symbolic link to a folder is ONE item to a move: its parent's listing
/// reports it as a link, and [`Volume::rename`] moves the LINK to `moved_to`,
/// still pointing where it did, with the folder it points at untouched.
///
/// `link` must already exist on `volume` as a symbolic link to the directory
/// `target`, which holds `child_name` directly inside it. `moved_to` must be
/// free, in an existing folder.
///
/// **Why this one is worth a shared assertion.** A same-volume move is a rename
/// per selected item, and a move to another volume refuses a selected link by
/// reading its parent's listing (#570). A backend whose listing typed the link
/// as what it points at would let that move copy a folder the person never
/// selected and then remove the link; one whose rename followed the link would
/// move the TARGET out from under every other link to it.
///
/// Backends with no links (MTP, S3, WebDAV, SMB) have nothing to run this
/// against.
pub async fn assert_a_link_to_a_folder_moves_as_the_link(
    volume: &dyn Volume,
    link: &Path,
    moved_to: &Path,
    target: &Path,
    child_name: &str,
) {
    let parent = link.parent().unwrap_or(Path::new(""));
    let link_name = link.file_name().and_then(|n| n.to_str()).expect("the link has a name");
    let listing = volume.list_directory(parent, None).await.unwrap_or_else(|e| {
        panic!(
            "fixture precondition: listing {} must work, got {e:?}",
            parent.display()
        )
    });
    let listed = listing
        .iter()
        .find(|e| e.name == link_name)
        .unwrap_or_else(|| panic!("fixture precondition: {} must list {link_name}", parent.display()));
    assert_eq!(
        EntryKind::of(listed),
        EntryKind::Symlink,
        "listing {} on {} must report {link_name} AS a link, never what it points at",
        parent.display(),
        volume.name(),
    );

    let outcome = volume.rename(link, moved_to, false).await;
    assert!(
        outcome.is_ok(),
        "rename({} -> {}) on {} must move the link, got {outcome:?}",
        link.display(),
        moved_to.display(),
        volume.name(),
    );
    assert!(
        matches!(volume.entry_kind(link).await, Err(VolumeError::NotFound(_))),
        "the link {} must be gone from its old place",
        link.display(),
    );
    assert_eq!(
        volume.entry_kind(moved_to).await.ok(),
        Some(EntryKind::Symlink),
        "{} on {} must be the moved LINK, never a moved folder",
        moved_to.display(),
        volume.name(),
    );
    let in_target = volume
        .list_directory(target, None)
        .await
        .unwrap_or_else(|e| panic!("the link's target {} must still be there, got {e:?}", target.display()));
    assert!(
        in_target.iter().any(|e| e.name == child_name),
        "moving a link must leave its target alone, but {child_name} is gone from {}",
        target.display(),
    );
}
