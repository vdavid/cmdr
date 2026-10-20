//! Is the entry occupying a restore target the very entry being restored?
//!
//! The one exception to the pinned non-destructive restore (`DETAILS.md` § "The two
//! data-safety guards"): undoing `dog.JPG` → `dog.jpg` on a case-insensitive volume
//! finds the old spelling "taken" by the renamed file itself. Getting this wrong
//! either way costs: calling a different file "self" renames over it with force, and
//! calling self "different" leaves every case-only rename un-undoable. So it's
//! decided by identity (a node id, or exact names in a fresh listing), never by
//! folding names.

use std::path::{Component, Path, PathBuf};

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::Volume;

use crate::store::fold_name;

/// One place where a restore target's spelling differs from the landed entry's only
/// by case or Unicode normalization: the folder holding both spellings, and the two
/// names. `parent` is spelled the way the landed entry's path spells it, so it exists.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SpellingTwin {
    pub(super) parent: PathBuf,
    pub(super) from_name: String,
    pub(super) to_name: String,
}

/// Where `from` and `to` differ, component by component, if they differ ONLY by case
/// or normalization (each pair of names exactly equal or [`fold_name`]-equal). `None`
/// when any pair genuinely differs, or the depths differ: then the occupant is some
/// other entry. An empty list means the two paths are spelled identically.
///
/// Each `parent` is an ancestor of `from` sliced off its own text, never rebuilt from
/// components: rebuilding would collapse a remote root's `scheme://` into `scheme:/`.
fn spelling_twins(from: &Path, to: &Path) -> Option<Vec<SpellingTwin>> {
    let from_parts: Vec<_> = from.components().collect();
    let to_parts: Vec<_> = to.components().collect();
    if from_parts.len() != to_parts.len() {
        return None;
    }
    let mut twins = Vec::new();
    for (depth, (f, t)) in from_parts.iter().zip(&to_parts).enumerate() {
        if f == t {
            continue;
        }
        let (Component::Normal(f_name), Component::Normal(t_name)) = (f, t) else {
            return None;
        };
        let (f_name, t_name) = (f_name.to_string_lossy(), t_name.to_string_lossy());
        if fold_name(&f_name) != fold_name(&t_name) {
            return None;
        }
        twins.push(SpellingTwin {
            parent: from.ancestors().nth(from_parts.len() - depth)?.to_path_buf(),
            from_name: f_name.into_owned(),
            to_name: t_name.into_owned(),
        });
    }
    Some(twins)
}

/// What the cheap checks say about an entry occupying a restore target.
#[derive(Debug, PartialEq)]
pub(super) enum SelfCollision {
    /// The occupant IS the entry being restored.
    Same,
    /// A different entry: restoring would overwrite it.
    Different,
    /// The two paths differ only by case or normalization, and nothing proves
    /// identity: the parent listings decide ([`listing_confirms_twin`]).
    AskTheListings(Vec<SpellingTwin>),
}

/// Classify an occupied restore target without touching the volume.
///
/// Never folds names into an identity: the log can't know a volume's case or
/// normalization rules, and on a case-sensitive one (SFTP to Linux, S3, Samba,
/// case-sensitive APFS) `IMG.JPG` and `img.jpg` are two files (#511). So:
/// - A cross-volume restore is never self: the occupant lives on another device.
/// - Paths that differ by more than case or normalization are never self, even on
///   one inode (a hard link under another name is still a name we'd replace).
/// - Both node ids known decides it. `FileEntry::inode` is set only where a backend
///   reports one (`LocalPosixVolume`, and only for a file with more than one link),
///   so this is the rare case.
/// - Paths spelled identically are self: it's the very path the entry sits at.
/// - Otherwise, ask the listings.
pub(super) fn classify_self_collision(
    same_volume: bool,
    from: &Path,
    to: &Path,
    from_entry: &FileEntry,
    occupant: &FileEntry,
) -> SelfCollision {
    if !same_volume {
        return SelfCollision::Different;
    }
    let Some(twins) = spelling_twins(from, to) else {
        return SelfCollision::Different;
    };
    if let (Some(a), Some(b)) = (from_entry.inode, occupant.inode) {
        return if a == b {
            SelfCollision::Same
        } else {
            SelfCollision::Different
        };
    }
    if twins.is_empty() {
        return SelfCollision::Same;
    }
    SelfCollision::AskTheListings(twins)
}

/// Does a listing of `twin.parent` prove the two spellings name ONE entry? Only when
/// the landed spelling is listed and the target spelling is not: the volume answered
/// for the target spelling (the occupant) without storing it, so it folded it onto
/// the landed one. Both spellings listed is two entries on a case-sensitive volume;
/// neither, or only the target, is a state we can't prove, so it isn't self either.
pub(super) fn listing_confirms_twin(listed: &[FileEntry], twin: &SpellingTwin) -> bool {
    let has = |name: &str| listed.iter().any(|entry| entry.name == name);
    has(&twin.from_name) && !has(&twin.to_name)
}

/// Is the entry occupying the restore target actually the SAME entry we're
/// restoring (a case-only rename on a case-insensitive volume), not a real
/// collision? [`classify_self_collision`] decides what it can for free; the rest
/// re-lists each parent where the spellings differ and compares EXACT names. A
/// listing that fails is not proof, so it's not self.
pub(super) async fn is_self_collision(
    volume: &dyn Volume,
    same_volume: bool,
    from: &Path,
    to: &Path,
    from_entry: &FileEntry,
    occupant: &FileEntry,
) -> bool {
    let twins = match classify_self_collision(same_volume, from, to, from_entry, occupant) {
        SelfCollision::Same => return true,
        SelfCollision::Different => return false,
        SelfCollision::AskTheListings(twins) => twins,
    };
    for twin in &twins {
        match volume.list_directory(&twin.parent, None).await {
            Ok(listed) if listing_confirms_twin(&listed, twin) => {}
            _ => return false,
        }
    }
    true
}
