//! The entry a folder holds under another Unicode spelling of a name.
//!
//! A byte-exact destination (an SMB share, SFTP, a phone) keeps `café` composed
//! and `café` decomposed as two entries and finds each only by its own bytes. A
//! lookup in the other spelling misses an entry a person calls the same name, and
//! a write that then believes the name is free stands a second, identical-looking
//! entry beside the user's. So every write that asks "is this name free?" asks
//! here once its own lookup has missed, and a look-alike is taken, never free.
//!
//! Only a difference in FORM counts (`cmdr_fs::name_fold::differ_only_in_form`):
//! `Report` and `report` read as two names, and a case-sensitive destination
//! keeps both on purpose. `DETAILS.md` § "Look-alike names".

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use cmdr_fs::name_fold::differ_only_in_form;

use crate::file_system::listing::FileEntry;
use crate::file_system::listing::caching::try_get_authoritative_listing;
use crate::file_system::volume::{Volume, VolumeError};

/// What a folder holds under another spelling of one name.
#[derive(Debug)]
pub(crate) enum LookAlike {
    /// Nothing: the name is free as far as spelling goes.
    None,
    /// One entry, which IS the name as far as anyone can see. A write to the
    /// name lands on this entry's own path.
    One(Box<FileEntry>),
    /// Two or more, none spelled the way the caller asked. Picking one would be
    /// a guess, and a guess that overwrites is data loss.
    Several,
}

/// `name`'s look-alike among `entries`, a folder's listing.
///
/// An entry spelled exactly `name` means the lookup that sent the caller here
/// raced a create, so the name answers for itself: [`LookAlike::None`].
pub(crate) fn among<'a>(entries: impl IntoIterator<Item = &'a FileEntry>, name: &str) -> LookAlike {
    let mut found: Option<&FileEntry> = None;
    let mut several = false;
    for entry in entries {
        if entry.name == name {
            return LookAlike::None;
        }
        if differ_only_in_form(&entry.name, name) {
            several |= found.is_some();
            found = Some(entry);
        }
    }
    match (found, several) {
        (_, true) => LookAlike::Several,
        (Some(entry), false) => LookAlike::One(Box::new(entry.clone())),
        (None, false) => LookAlike::None,
    }
}

/// Whether a clash between `source` and `destination` is a look-alike: the
/// destination entry spells the source's name another way.
///
/// Read off the two leaves, which IS the landing's own answer: every engine
/// names a destination after its source, and a look-alike landing hands the
/// resolver the stored entry's path (`transfer/volume/landing.rs`), so the
/// leaves differ in form exactly when the landing found a look-alike. Asking
/// here keeps it one rule for every engine rather than a flag threaded through
/// each of them.
pub(crate) fn is_look_alike_clash(source: &Path, destination: &Path) -> bool {
    match (
        source.file_name().and_then(|n| n.to_str()),
        destination.file_name().and_then(|n| n.to_str()),
    ) {
        (Some(source), Some(destination)) => differ_only_in_form(source, destination),
        _ => false,
    }
}

/// `name`'s look-alike in `dir` on `volume`, for a caller whose exact lookup of
/// `dir/name` just answered `NotFound`.
///
/// Free when it can't matter: an ASCII name has one spelling, and a volume that
/// finds names in any form ([`Volume::matches_names_in_any_unicode_form`]) already
/// answered with that miss. Otherwise one listing of `dir`. A `dir` that isn't
/// there holds nothing; any other listing failure is the caller's to fail the
/// item on, ❌ never "nothing is there".
pub(crate) async fn look_alike_in(volume: &dyn Volume, dir: &Path, name: &str) -> Result<LookAlike, VolumeError> {
    ListedFolders::new(volume).look_alike_in(dir, name).await
}

/// Whether `volume` holds `path`'s name under another spelling, one entry or
/// several, for a caller whose exact probe of `path` just missed. The question a
/// dialog asks before a write ("is this destination already there?"): the write
/// that follows lands on a look-alike or refuses it, so the answer the person
/// sees is "there", ❌ never "free".
pub(crate) async fn held_in_another_spelling(volume: &dyn Volume, path: &Path) -> Result<bool, VolumeError> {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else {
        return Ok(false);
    };
    Ok(!matches!(look_alike_in(volume, dir, name).await?, LookAlike::None))
}

/// Where a NEW entry a person named goes: a new folder or file, or a rename's
/// target.
#[derive(Debug)]
pub(crate) enum NewEntry {
    /// Nothing holds the name in another spelling: create it here, spelled the
    /// way the volume wants new names (`Volume::spell_new_name`).
    Free(PathBuf),
    /// This entry holds it under another spelling. A plain create or rename is
    /// refused as a taken name; a rename the user confirmed replaces THIS entry.
    Taken(Box<FileEntry>),
    /// Several do, none spelled as asked.
    Ambiguous,
}

/// Where a new entry named by `path` goes on `volume`. `renaming` is the entry a
/// rename is moving: finding IT as the look-alike means the rename respells its
/// own name, which is free (and the one way to fix a name other clients can't
/// open).
///
/// An exact name the volume holds isn't this function's to report: the create
/// or rename that follows refuses it with the backend's own `AlreadyExists`.
///
/// `volume_id` names `volume` in the registry, so a folder a pane shows is
/// answered from that pane's listing ([`ListedFolders::with_pane_listings`]).
pub(crate) async fn place_new_entry(
    volume: &dyn Volume,
    volume_id: &str,
    path: &Path,
    renaming: Option<&Path>,
) -> Result<NewEntry, VolumeError> {
    ListedFolders::with_pane_listings(volume, volume_id)
        .place_new_entry(path, renaming)
        .await
}

/// `path` spelled the way `volume` wants a NEW name (`Volume::spell_new_name`).
/// Only for a name being created; one that addresses an existing entry keeps its
/// stored bytes.
pub(crate) fn spelled_new_path(volume: &dyn Volume, path: &Path) -> PathBuf {
    match (path.parent(), path.file_name().and_then(|n| n.to_str())) {
        (Some(dir), Some(name)) => dir.join(volume.spell_new_name(name).as_ref()),
        _ => path.to_path_buf(),
    }
}

/// Look-alike answers for many names on one volume: each folder is listed at
/// most once, however many names ask about it. For a caller whose folders hold
/// still while it asks, like a bulk rename settling its plan before it writes.
pub(crate) struct ListedFolders<'v> {
    volume: &'v dyn Volume,
    /// `volume`'s registry id, when the caller trusts a pane's listing over a
    /// read. See [`Self::with_pane_listings`].
    pane_listings_of: Option<&'v str>,
    listed: HashMap<PathBuf, Vec<FileEntry>>,
}

impl<'v> ListedFolders<'v> {
    /// Lists every folder it's asked about.
    pub(crate) fn new(volume: &'v dyn Volume) -> Self {
        Self {
            volume,
            pane_listings_of: None,
            listed: HashMap::new(),
        }
    }

    /// Answers a folder a pane shows from that pane's listing, when the volume's
    /// watch keeps it current for every writer
    /// (`listing::caching::try_get_authoritative_listing`); lists the rest. For
    /// a person waiting on a new name: re-reading a 3,000-entry share folder the
    /// pane already holds costs a second or more on a busy NAS.
    pub(crate) fn with_pane_listings(volume: &'v dyn Volume, volume_id: &'v str) -> Self {
        Self {
            pane_listings_of: Some(volume_id),
            ..Self::new(volume)
        }
    }

    /// [`look_alike_in`], answered from `dir`'s one listing.
    pub(crate) async fn look_alike_in(&mut self, dir: &Path, name: &str) -> Result<LookAlike, VolumeError> {
        if name.is_ascii() || self.volume.matches_names_in_any_unicode_form() {
            return Ok(LookAlike::None);
        }
        if !self.listed.contains_key(dir) {
            let entries = match self.pane_listing(dir) {
                Some(entries) => entries,
                None => self.read(dir).await?,
            };
            self.listed.insert(dir.to_path_buf(), entries);
        }
        Ok(self
            .listed
            .get(dir)
            .map_or(LookAlike::None, |entries| among(entries, name)))
    }

    /// `dir`'s entries as a pane's listing holds them, when the caller trusts
    /// one ([`Self::with_pane_listings`]) and a pane shows `dir`.
    ///
    /// A watch's debounce can leave it a moment behind another client, which
    /// risks only a missed twin: a second, look-alike entry, never an
    /// overwrite, since the volume holds the two spellings as two names.
    fn pane_listing(&self, dir: &Path) -> Option<Vec<FileEntry>> {
        let entries = try_get_authoritative_listing(self.pane_listings_of?, dir)?;
        log::debug!(
            target: "look_alike",
            "answered from a pane's listing of {} ({} entries)",
            dir.display(),
            entries.len()
        );
        Some(entries)
    }

    /// `dir`'s entries, read off the volume. A `dir` that isn't there holds
    /// nothing.
    async fn read(&self, dir: &Path) -> Result<Vec<FileEntry>, VolumeError> {
        let start = std::time::Instant::now();
        let entries = match self.volume.list_directory(dir, None).await {
            Ok(entries) => entries,
            Err(VolumeError::NotFound(_)) => Vec::new(),
            Err(e) => return Err(e),
        };
        log::debug!(
            target: "look_alike",
            "listed {} ({} entries) in {} ms",
            dir.display(),
            entries.len(),
            start.elapsed().as_millis()
        );
        Ok(entries)
    }

    /// [`place_new_entry`], answered from the target folder's one listing.
    pub(crate) async fn place_new_entry(
        &mut self,
        path: &Path,
        renaming: Option<&Path>,
    ) -> Result<NewEntry, VolumeError> {
        let spelled_path = spelled_new_path(self.volume, path);
        let (Some(dir), Some(spelled)) = (spelled_path.parent(), spelled_path.file_name().and_then(|n| n.to_str()))
        else {
            return Ok(NewEntry::Free(spelled_path));
        };
        match self.look_alike_in(dir, spelled).await? {
            LookAlike::None => Ok(NewEntry::Free(spelled_path)),
            LookAlike::One(entry) if renaming == Some(dir.join(&entry.name).as_path()) => {
                Ok(NewEntry::Free(spelled_path))
            }
            LookAlike::One(entry) => Ok(NewEntry::Taken(entry)),
            LookAlike::Several => Ok(NewEntry::Ambiguous),
        }
    }
}

#[cfg(test)]
#[path = "look_alike_tests.rs"]
mod tests;
