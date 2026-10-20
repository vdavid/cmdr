//! A move to ANOTHER volume refuses a selected symbolic link, before anything
//! moves (#570).
//!
//! A move treats a link as one item, the link itself. Within one volume that's
//! a rename (`move_same.rs`), but across volumes it has no good shape: a link's
//! target is a path on the volume that holds it, so a link recreated on another
//! device or server points nowhere, and the streaming engines' other answer
//! (copy what it points at, then remove the link) hands the person a copy of a
//! folder they never selected. A copy keeps that second answer on purpose
//! (`../DETAILS.md` § "Symlinks are opaque to a move").

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::types::WriteOperationError;
use cmdr_fs::volume::{EntryKind, Volume};

/// The first of `sources` that is a symbolic link on `volume`, as the refusal
/// a cross-volume move returns, or `Ok` when none is.
///
/// Each distinct parent is listed ONCE (a listing reports a link as a link on
/// every backend), so a thousand selected files cost one round trip, not a
/// thousand stats. A source its parent's listing can't answer for asks
/// [`Volume::entry_kind`]; one that still can't be typed is left to the move's
/// own steps, which report what's wrong with it. A backend that holds no links
/// costs nothing.
pub(super) async fn refuse_a_selected_link(
    volume: &dyn Volume,
    sources: &[PathBuf],
) -> Result<(), WriteOperationError> {
    if !volume.backend_kind().has_symlinks() {
        return Ok(());
    }
    // In selection order, so the refusal names the first link the person picked.
    let mut by_parent: Vec<(&Path, Vec<&PathBuf>)> = Vec::new();
    for source in sources {
        let parent = source.parent().unwrap_or(Path::new(""));
        match by_parent.iter_mut().find(|(p, _)| *p == parent) {
            Some((_, in_parent)) => in_parent.push(source),
            None => by_parent.push((parent, vec![source])),
        }
    }
    for (parent, in_parent) in by_parent {
        let listing = volume.list_directory(parent, None).await.ok();
        for source in in_parent {
            let listed = listing.as_ref().and_then(|entries| {
                let name = source.file_name()?;
                entries.iter().find(|e| OsStr::new(&e.name) == name)
            });
            let is_link = match listed {
                Some(entry) => EntryKind::of(entry) == EntryKind::Symlink,
                None => matches!(volume.entry_kind(source).await, Ok(EntryKind::Symlink)),
            };
            if is_link {
                return Err(WriteOperationError::LinkCantMoveAcrossVolumes {
                    path: source.display().to_string(),
                });
            }
        }
    }
    Ok(())
}
