//! Where one source name lands in a destination folder: on the entry that's
//! already there, in THAT entry's own spelling, or as a new name spelled the way
//! the destination wants new names.
//!
//! Every engine asks this for every name it's about to write, top level and deep,
//! so the two rules live here once:
//!
//! - **A look-alike is taken, never free.** A byte-exact destination misses a
//!   name held in another Unicode form, and a write that believed it free would
//!   stand a second, identical-looking entry beside the user's
//!   (`../../look_alike.rs`). So it's a conflict like any other, and the path
//!   handed back is the stored entry's: an Overwrite replaces THAT entry, a merge
//!   walks into THAT folder, and the share ends with one.
//! - **Only a free name gets respelled** ([`Volume::spell_new_name`]): an entry
//!   that exists is only ever addressed by its stored bytes.
//!
//! `DETAILS.md` § "Look-alike names and new-name spelling".

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::super::look_alike::{LookAlike, look_alike_in};
use super::super::super::types::WriteOperationError;
use super::super::dest_name_index::{DestLookup, DestNameIndex};
use super::super::transfer_driver::{FetchFut, NameAtDest};
use super::super::transfer_probe::{DriverPhase, OperationProbe};
use super::transfer_error::{PathRole, map_volume_error};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{Volume, VolumeError};

/// Where a name lands.
pub(super) enum Landing {
    /// Nothing holds the name: write a new entry at this path, already spelled
    /// for the destination.
    Free(PathBuf),
    /// `entry` holds the name, at `path`, its OWN spelling, which may differ
    /// from the source's.
    Taken { path: PathBuf, entry: Box<FileEntry> },
}

impl Landing {
    /// The path to write, and what sits there if anything.
    pub(super) fn into_parts(self) -> (PathBuf, Option<FileEntry>) {
        match self {
            Self::Free(path) => (path, None),
            Self::Taken { path, entry } => (path, Some(*entry)),
        }
    }
}

/// What the caller already knows about the destination folder.
#[derive(Clone, Copy)]
pub(super) enum DestFolder<'a> {
    /// This operation created the folder, so nothing the user had can be in it:
    /// every name is free, and nothing is asked.
    CreatedByUs,
    /// One listing of the folder, indexed. A byte-exact hit and a look-alike are
    /// settled in memory; only a name the listing can't settle is probed.
    Listed(&'a DestNameIndex),
    /// No listing: one probe per name, and a listing only when that probe misses
    /// a non-ASCII name on a byte-exact volume.
    Unlisted,
}

/// Whether a free name takes the destination's spelling.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NewName {
    /// A copy makes a new entry, so its name is spelled for the destination.
    Respell,
    /// A same-volume move keeps the entry it moves, bytes and all.
    Keep,
}

/// The conflict pre-check every serial engine (`copy_serial.rs`,
/// `move_cross.rs`, `move_same.rs`) hands the async driver as its
/// `dest_meta_fetcher`: [`name_at_destination`] for each top-level name, with the
/// step named on the operation's probe BEFORE the await. A `get_metadata` on a
/// wedged share returns to nobody, so a dump has to be able to name it as the
/// step in progress rather than leave the driver reading `starting`.
pub(super) fn top_level_precheck(
    dest_volume: &Arc<dyn Volume>,
    probe: Option<Arc<OperationProbe>>,
    new_name: NewName,
    destination_name: Option<&str>,
) -> impl for<'a> FnMut(&'a Path) -> FetchFut<'a> + use<> {
    let dest_volume = Arc::clone(dest_volume);
    let destination_name = destination_name.map(str::to_owned);
    move |dest: &Path| -> FetchFut<'_> {
        let dest_volume = Arc::clone(&dest_volume);
        let probe = probe.clone();
        let dest = destination_name
            .as_deref()
            .map_or_else(|| dest.to_path_buf(), |name| dest.with_file_name(name));
        Box::pin(async move {
            if let Some(probe) = probe {
                probe.set_driver_phase(DriverPhase::PreparingNext, &dest.display().to_string());
            }
            name_at_destination(&dest_volume, &dest, new_name).await
        })
    }
}

/// What the destination holds at one TOP-LEVEL name: [`where_it_lands`] with no
/// listing, for the path the driver would write.
///
/// ❗ **Only `NotFound` means free.** A `ConnectionTimeout`, a
/// `DeviceSessionReset`, a `PermissionDenied` (anything else) is the
/// destination refusing to answer, and reading that as "nothing is there" is
/// the whole bug: the item skips the resolver, the Skip/Stop policy is never
/// consulted, and the landing then clears whatever the probe was asked about.
/// So it fails THAT item, at the destination path. A merge child keeps the same
/// discipline one level down, through the same [`where_it_lands`].
///
/// ❌ No retry here. Per-file retry belongs to `retry.rs`, inside
/// `stream_pipe_file`, and a second layer above it would multiply the wait a
/// user sits through on a dead link (`transfer/CLAUDE.md`).
async fn name_at_destination(
    dest_volume: &Arc<dyn Volume>,
    dest: &Path,
    new_name: NewName,
) -> Result<NameAtDest, WriteOperationError> {
    let refused = |e| map_volume_error(&dest.display().to_string(), PathRole::Destination, e);
    let landing = match (dest.parent(), dest.file_name()) {
        (Some(dir), Some(name)) => where_it_lands(dest_volume, dir, name, DestFolder::Unlisted, new_name).await,
        // A source with no name of its own lands on the destination folder
        // itself, which no listing of a parent describes.
        _ => match dest_volume.get_metadata(dest).await {
            Ok(entry) => Ok(Landing::Taken {
                path: dest.to_path_buf(),
                entry: Box::new(entry),
            }),
            Err(VolumeError::NotFound(_)) => Ok(Landing::Free(dest.to_path_buf())),
            Err(e) => Err(e),
        },
    };
    match landing.map_err(refused)? {
        Landing::Free(path) => Ok(NameAtDest::Free(path)),
        Landing::Taken { path, entry } => Ok(NameAtDest::Taken {
            path,
            size: entry.size.unwrap_or(0),
        }),
    }
}

/// Where `name` lands in `dest_dir`.
///
/// `Err(AmbiguousName)` when the folder holds two look-alikes and neither is
/// spelled as asked: which one the write means is a guess, and a guess that
/// overwrites is data loss. Any other `Err` is a probe or listing that couldn't
/// answer, and fails the item: ❌ never "nothing is there", which hands the
/// name to a fresh write whose landing clears what the probe was asked about.
pub(super) async fn where_it_lands(
    dest_volume: &Arc<dyn Volume>,
    dest_dir: &Path,
    name: &OsStr,
    folder: DestFolder<'_>,
    new_name: NewName,
) -> Result<Landing, VolumeError> {
    let asked = dest_dir.join(name);
    let free = || {
        let spelled = match (new_name, name.to_str()) {
            (NewName::Respell, Some(name)) => dest_dir.join(dest_volume.spell_new_name(name).as_ref()),
            _ => asked.clone(),
        };
        Landing::Free(spelled)
    };
    let look_alike_at = |entry: Box<FileEntry>| Landing::Taken {
        path: dest_dir.join(&entry.name),
        entry,
    };
    let ambiguous = || VolumeError::AmbiguousName(asked.display().to_string());

    let may_be_a_look_alike = match folder {
        DestFolder::CreatedByUs => return Ok(free()),
        DestFolder::Listed(index) => match index.lookup(Some(name)) {
            DestLookup::Absent => return Ok(free()),
            DestLookup::Present(entry) => return Ok(Landing::Taken { path: asked, entry }),
            DestLookup::LookAlike(entry) => return Ok(look_alike_at(entry)),
            DestLookup::Ambiguous => return Err(ambiguous()),
            // The listing already ruled out a look-alike; only a case-only match
            // (or an alias it can't enumerate) is left for the backend to call.
            DestLookup::Unknown => false,
        },
        DestFolder::Unlisted => true,
    };

    match dest_volume.get_metadata(&asked).await {
        Ok(entry) => Ok(Landing::Taken {
            path: asked,
            entry: Box::new(entry),
        }),
        Err(VolumeError::NotFound(_)) => {
            let Some(text) = name.to_str().filter(|_| may_be_a_look_alike) else {
                return Ok(free());
            };
            match look_alike_in(dest_volume.as_ref(), dest_dir, text).await? {
                LookAlike::None => Ok(free()),
                LookAlike::One(entry) => Ok(look_alike_at(entry)),
                LookAlike::Several => Err(ambiguous()),
            }
        }
        Err(e) => Err(e),
    }
}
