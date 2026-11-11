//! Where a transfer's sources may land by name: the ONE check of its
//! `TransferLeaves`, run by every start before the operation is admitted
//! (`transfer/DETAILS.md` § "Named destinations"), and the refusal for a route
//! that can't name each source.

use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use cmdr_fs::name_fold::fold_name;

use super::transfer::volume::native_roots;
use super::types::{NamedLeavesRefusal, TransferLeaves, WriteOperationError};
use super::validation::{first_shared_name, validate_path_length};
use cmdr_fs::file_identity::FileId;
use cmdr_fs::volume::Volume;

/// How the start may tell whether two paths are one item: by name alone, or
/// also by `dev+ino` when every path is on this machine's disk and can be
/// statted (`may_be_same_file`, which settles a symlinked or case-differing route).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LeafIdentity {
    /// Folded names only: a remote volume's paths can't be statted here.
    Names,
    /// Folded names, and `dev+ino` wherever both paths exist.
    NamesAndInodes,
}

/// The ONE check of a transfer's leaves: the routed start runs it before the
/// operation is admitted (`routing.rs`, `local_start.rs::move_files_start`), and
/// every engine below trusts it. `sources` and `destination` are the paths the
/// engine will walk, in the same frame as the leaves' keys.
///
/// - **A single leaf** names exactly one source and never escapes its parent.
/// - **Per-source names** ([`validate_per_source_leaves`]): every key is a
///   source, every name a plain leaf, every landing name distinct ignoring case
///   and Unicode form, nothing lands on another source, and no source already
///   sits in the destination.
pub(crate) fn validate_transfer_leaves(
    leaves: &TransferLeaves,
    sources: &[PathBuf],
    destination: &Path,
    identity: LeafIdentity,
) -> Result<(), WriteOperationError> {
    if let Some(names) = leaves.per_source_names() {
        return validate_per_source_leaves(names, sources, destination, identity);
    }
    let Some(name) = leaves.single_leaf() else {
        return Ok(());
    };
    let named_path = destination.join(name);
    if sources.len() != 1
        || sources[0].file_name().is_none()
        || name.is_empty()
        || name.contains(['/', '\0'])
        || name == "."
        || name == ".."
    {
        return Err(WriteOperationError::InvalidName {
            path: named_path.display().to_string(),
            message: "Choose one item and a filename without path separators".into(),
        });
    }
    validate_path_length(&named_path)
}

/// The per-source half of [`validate_transfer_leaves`]: every key a source,
/// then [`landing_clash`]'s rules, then each landing's path length.
///
/// Folding is always [`fold_name`] (NFC, then lowercase), whatever the
/// destination's own case rule: a refusal on a case-sensitive disk costs a
/// retry, a collision missed on a case-insensitive one costs a file. Two
/// sources sharing an ORIGINAL name still refuse byte-exact in the started
/// operation (`validate_source_names_are_distinct`), because the native
/// engines key their subtree remap on `<destination>/<own name>`.
fn validate_per_source_leaves(
    names: &HashMap<PathBuf, String>,
    sources: &[PathBuf],
    destination: &Path,
    identity: LeafIdentity,
) -> Result<(), WriteOperationError> {
    let refused = |path: &Path, reason| WriteOperationError::NamedLeavesRefused {
        path: path.display().to_string(),
        reason,
    };
    // A name for nothing is a caller that lost track of its sources: refuse,
    // never ignore, or the item it meant lands under its old name unannounced.
    let known: HashSet<&PathBuf> = sources.iter().collect();
    let mut keys: Vec<&PathBuf> = names.keys().collect();
    keys.sort();
    if let Some(stray) = keys.into_iter().find(|key| !known.contains(key)) {
        return Err(refused(stray, NamedLeavesRefusal::NotASource));
    }

    let items: Vec<(&Path, Option<&str>)> = sources
        .iter()
        .map(|source| (source.as_path(), names.get(source).map(String::as_str)))
        .collect();
    landing_clash(&items, destination, identity).map_err(|clash| match clash {
        LandingClash::NotPlain { leaf, refusal } => WriteOperationError::InvalidName {
            path: destination.join(leaf).display().to_string(),
            message: format!("Not a plain file name: {refusal:?}").into(),
        },
        LandingClash::SameName { landing } => refused(&landing, NamedLeavesRefusal::SameName),
        LandingClash::OntoASource { landing } => refused(&landing, NamedLeavesRefusal::OntoASource),
        LandingClash::AlreadyInDestination { source } => refused(&source, NamedLeavesRefusal::AlreadyInDestination),
        // `landing_clash` never answers this one; it's the start's own refusal.
        LandingClash::SharedOwnName { name, first, second } => WriteOperationError::DuplicateSourceNames {
            name,
            first: first.display().to_string(),
            second: second.display().to_string(),
        },
    })?;
    for source in sources {
        let Some(leaf) = names.get(source) else { continue };
        validate_path_length(&destination.join(leaf))?;
    }
    Ok(())
}

/// Why items can't land in a folder under the names chosen for them, for
/// [`TransferLeaves::check_landings`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LandingClash {
    /// A chosen name isn't one plain, visible leaf.
    NotPlain {
        /// The name as chosen (an empty or `..` one has no landing to name it by).
        leaf: String,
        /// Which rule the name breaks.
        refusal: cmdr_fs::validation::PlainLeafRefusal,
    },
    /// Two items would land under one name, compared ignoring case and Unicode form.
    SameName {
        /// Where the second one would land.
        landing: PathBuf,
    },
    /// An item would land where another item of the same batch is.
    OntoASource {
        /// That place.
        landing: PathBuf,
    },
    /// An item already sits in the destination folder, so naming it there is a
    /// rename, not a transfer.
    AlreadyInDestination {
        /// That item.
        source: PathBuf,
    },
    /// Two items share an ORIGINAL name (byte-exact) on a route that keys each
    /// landing on it, whatever names were chosen for them. Only
    /// [`TransferLeaves::check_own_names`] answers this.
    SharedOwnName {
        /// The name they share.
        name: String,
        /// The first item carrying it.
        first: PathBuf,
        /// The second one.
        second: PathBuf,
    },
}

impl TransferLeaves {
    /// The per-source landing rules a transfer's start enforces, by name
    /// alone, for a caller that checks a batch ahead of starting it (an AI
    /// proposal, so a refusal reaches the model while it can still fix the
    /// call). Each item is a source and the name chosen for it, `None` to keep
    /// its own; the start runs these same rules, plus `dev+ino` identity where
    /// it can stat. `transfer/DETAILS.md` § "Named destinations".
    pub fn check_landings(items: &[(&Path, Option<&str>)], destination: &Path) -> Result<(), LandingClash> {
        landing_clash(items, destination, LeafIdentity::Names)
    }

    /// The ORIGINAL-name rule a transfer's start enforces on its route, for a
    /// caller that checks a batch ahead of starting it (an AI proposal). Where
    /// both ends sit on a local path the native engines run, and they key every
    /// subtree on `<destination>/<own name>`, so two sources sharing a name
    /// (byte-exact) refuse there whatever names were chosen for them. The
    /// cross-volume engine keys each landing on the source path, so this passes
    /// there and [`Self::check_landings`] speaks for the chosen names.
    /// `transfer/DETAILS.md` § "Named destinations".
    pub fn check_own_names(
        sources: &[&Path],
        source_volume: &dyn Volume,
        dest_volume: &dyn Volume,
    ) -> Result<(), LandingClash> {
        if native_roots(source_volume, dest_volume).is_none() {
            return Ok(());
        }
        match first_shared_name(sources.iter().copied()) {
            Some((name, first, second)) => Err(LandingClash::SharedOwnName {
                name: name.to_string_lossy().into_owned(),
                first: first.to_path_buf(),
                second: second.to_path_buf(),
            }),
            None => Ok(()),
        }
    }
}

/// [`TransferLeaves::check_landings`] with the identity the caller can afford, in the
/// order a person would fix them: the names themselves, then how they meet
/// each other and the sources.
fn landing_clash(
    items: &[(&Path, Option<&str>)],
    destination: &Path,
    identity: LeafIdentity,
) -> Result<(), LandingClash> {
    landing_clash_with(items, destination, identity, file_id)
}

/// [`landing_clash`] with its stat as a seam. `Names` mode never calls `stat`:
/// it runs for remote routes (whose paths mean nothing on this disk) and on a
/// tokio worker for an AI proposal, where one stat of a hung mount stalls it.
fn landing_clash_with<Id: Copy + Eq + std::hash::Hash>(
    items: &[(&Path, Option<&str>)],
    destination: &Path,
    identity: LeafIdentity,
    stat: impl Fn(&Path) -> Option<Id>,
) -> Result<(), LandingClash> {
    for (_, leaf) in items {
        let Some(leaf) = leaf else { continue };
        if let Err(refusal) = cmdr_fs::validation::validate_plain_leaf(leaf) {
            return Err(LandingClash::NotPlain {
                leaf: (*leaf).to_string(),
                refusal,
            });
        }
    }

    // Where each source lands, its own name included: a renamed item can
    // collide with one that kept its name as easily as with another rename.
    let mut landings: Vec<(&Path, PathBuf)> = Vec::with_capacity(items.len());
    let mut taken: HashSet<String> = HashSet::with_capacity(items.len());
    for (source, chosen) in items {
        let Some(leaf) = chosen.map(OsStr::new).or_else(|| source.file_name()) else {
            continue;
        };
        let landing = destination.join(leaf);
        if !taken.insert(folded(Path::new(leaf))) {
            return Err(LandingClash::SameName { landing });
        }
        landings.push((source, landing));
    }

    // A landing on another source replaces it before it's read (a copy) or
    // turns "a to b, b to c" into a lost file when the order runs wrong (a
    // move). Compared folded, and by file id where both have one, which catches
    // the same item reached through another spelling or a symlinked folder.
    let by_name: HashMap<String, &Path> = items.iter().map(|(s, _)| (folded(s), *s)).collect();
    let by_inode: HashMap<Id, &Path> = match identity {
        LeafIdentity::NamesAndInodes => items.iter().filter_map(|(s, _)| Some((stat(s)?, *s))).collect(),
        LeafIdentity::Names => HashMap::new(),
    };
    for (source, landing) in landings {
        let onto = by_name.get(&folded(&landing)).or_else(|| match identity {
            LeafIdentity::NamesAndInodes => stat(&landing).and_then(|id| by_inode.get(&id)),
            LeafIdentity::Names => None,
        });
        // Landing on ITSELF is the next rule's, not a chain.
        if onto.is_some_and(|other| *other != source) {
            return Err(LandingClash::OntoASource { landing });
        }
    }

    // Names go to OTHER folders. One already in the destination is a rename,
    // and the same-volume move's identity guard would drop a case-only one.
    let destination_key = folded(destination);
    let destination_id = (identity == LeafIdentity::NamesAndInodes)
        .then(|| stat(destination))
        .flatten();
    for (source, _) in items {
        let Some(parent) = source.parent() else { continue };
        let already_there =
            folded(parent) == destination_key || destination_id.is_some_and(|id| stat(parent) == Some(id));
        if already_there {
            return Err(LandingClash::AlreadyInDestination {
                source: source.to_path_buf(),
            });
        }
    }
    Ok(())
}

/// `path` under [`fold_name`], whole, for the per-source comparisons. Its
/// components first, so a trailing `/` or a `.` can't make one folder two.
fn folded(path: &Path) -> String {
    let normal: PathBuf = path.components().collect();
    fold_name(&normal.to_string_lossy()).into_owned()
}

/// `path`'s file id, following symlinks the way [`crate::validation::may_be_same_file`] does;
/// `None` when it doesn't exist or the OS gives no id, which leaves the name rules alone.
fn file_id(path: &Path) -> Option<FileId> {
    fs::metadata(path).ok().as_ref().and_then(FileId::of)
}

/// Refuses per-source names on a route that can't honour them: an archive
/// changeset (copy or move into a zip, move out of one), whose plan never takes
/// more than one leaf. Every other engine names each source. A single leaf passes.
pub(crate) fn refuse_per_source_leaves_here(
    leaves: &TransferLeaves,
    destination: &Path,
) -> Result<(), WriteOperationError> {
    if leaves.is_per_source() {
        return Err(WriteOperationError::NamedLeavesRefused {
            path: destination.display().to_string(),
            reason: NamedLeavesRefusal::RouteCannotName,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name-only check runs for remote routes (whose paths mean nothing on
    /// this disk) and on a tokio worker for an AI proposal, so one stat of a
    /// hung mount would stall it: `Names` mode must never touch the disk.
    #[test]
    fn names_mode_never_stats() {
        let items: Vec<(&Path, Option<&str>)> = vec![
            (Path::new("/Volumes/hung/a.txt"), Some("b.txt")),
            (Path::new("/Volumes/hung/c.txt"), None),
        ];
        let stat = |path: &Path| -> Option<(u64, u64)> { panic!("statted {}", path.display()) };
        let result = landing_clash_with(&items, Path::new("/Volumes/hung/dest"), LeafIdentity::Names, stat);
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn names_and_inodes_mode_stats_landings_and_destination() {
        let items: Vec<(&Path, Option<&str>)> = vec![(Path::new("/src/a.txt"), Some("b.txt"))];
        let statted = std::cell::RefCell::new(Vec::new());
        let stat = |path: &Path| {
            statted.borrow_mut().push(path.to_path_buf());
            None::<(u64, u64)>
        };
        let result = landing_clash_with(&items, Path::new("/dest"), LeafIdentity::NamesAndInodes, stat);
        assert_eq!(result, Ok(()));
        let statted = statted.into_inner();
        assert!(statted.contains(&PathBuf::from("/dest/b.txt")), "{statted:?}");
        assert!(statted.contains(&PathBuf::from("/dest")), "{statted:?}");
    }
}
