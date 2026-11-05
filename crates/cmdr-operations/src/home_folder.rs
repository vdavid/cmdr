//! The delete nobody gets: the person's own home folder, or a folder that holds
//! it (`/Users`, `/`). Delete and trash refuse it at their start
//! (`write_start.rs`), and the delete dialog's scan preview refuses it before
//! walking (`scan_preview.rs`), so the dialog says why and offers no Delete
//! button. Every caller (a person, MCP, an AI suggestion) reaches the engine
//! through those starts, so none slips past.
//!
//! The home folder is the host's answer (`OperationsBuilder::home_folder`): the
//! app's real one, the browser demo's `/Users/you`. Only the default volume holds
//! it. Other people's home folders stay deletable.
//!
//! How a source is compared:
//! - Lexically first, with `~` expanded, `.` and `..` folded, and trailing
//!   slashes dropped. That's the whole check on a host with no local disk (the
//!   browser demo), whose volume has no symlinks.
//! - On the local disk, also by FILE IDENTITY: the source's `lstat` (dev, inode)
//!   against `stat` of the home folder and each folder above it. That catches a
//!   path through a symlinked folder, another letter case on a case-insensitive
//!   volume, and a firmlink twin (`/System/Volumes/Data/Users`). `lstat`, so a
//!   symlink POINTING AT the home folder stays deletable: removing it removes
//!   the link, never the folder.

use std::path::{Component, Path, PathBuf};

use super::Operations;
use super::local_disk::file_id;
use super::types::HomeFolderRelation;

/// A source that names the home folder, or a folder holding it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HomeFolderHit {
    /// The source as the caller sent it, `~` expanded.
    pub(crate) path: String,
    pub(crate) relation: HomeFolderRelation,
}

// ❌ No `From<HomeFolderHit>` for the wire types: `module-cycles` reads an impl
// as the module defining the type it produces, which welds this module and
// `types` into a cycle. Each caller builds its variant from the fields.

impl Operations {
    /// The first of `sources` (on `volume_id`) a delete or trash must refuse for
    /// the home folder's sake, if any.
    pub(crate) fn home_folder_hit(&self, volume_id: &str, sources: &[PathBuf]) -> Option<HomeFolderHit> {
        if volume_id != cmdr_fs::volume::DEFAULT_VOLUME_ID {
            return None;
        }
        let home = self.host().home_folder()?;
        let on_disk = self.host().volumes().local_disk(volume_id).is_some();
        home_folder_hit(home, sources, on_disk)
    }
}

/// [`Operations::home_folder_hit`]'s rule over a home folder in hand. `on_disk`
/// adds the file-identity check, for a host whose default volume is the local
/// disk.
pub(crate) fn home_folder_hit(home: &Path, sources: &[PathBuf], on_disk: bool) -> Option<HomeFolderHit> {
    let lexical_home = lexical(home);
    let identities = on_disk.then(|| HomeIdentities::of(home));

    sources.iter().find_map(|source| {
        let expanded = expand_tilde(source, home);
        let lexical_source = lexical(&expanded);
        let relation = if lexical_source == lexical_home {
            Some(HomeFolderRelation::IsHome)
        } else if lexical_home.starts_with(&lexical_source) {
            Some(HomeFolderRelation::HoldsHome)
        } else {
            identities.as_ref().and_then(|ids| ids.relation_of(&expanded))
        }?;
        Some(HomeFolderHit {
            path: expanded.display().to_string(),
            relation,
        })
    })
}

/// `~` and `~/rest` as `home` and `home/rest`; anything else unchanged. `~user`
/// isn't ours to expand, so it stays a name.
fn expand_tilde(path: &Path, home: &Path) -> PathBuf {
    let mut components = path.components();
    match components.next() {
        Some(Component::Normal(first)) if first == "~" => match components.as_path() {
            rest if rest.as_os_str().is_empty() => home.to_path_buf(),
            rest => home.join(rest),
        },
        _ => path.to_path_buf(),
    }
}

/// `path` with `.` dropped, `..` folded into its parent (never above the root),
/// and trailing slashes gone. Touches no disk.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // At the root, `..` is the root itself.
                if out.file_name().is_some() {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The (dev, inode) of the home folder and of every folder above it, under each
/// spelling the disk gives it. Empty off unix, where no file has one.
struct HomeIdentities {
    home: Vec<(u64, u64)>,
    holders: Vec<(u64, u64)>,
}

impl HomeIdentities {
    fn of(home: &Path) -> Self {
        let canonical = std::fs::canonicalize(home).unwrap_or_else(|_| lexical(home));
        let twin = data_volume_twin(&canonical);
        let spellings: Vec<PathBuf> = std::iter::once(canonical).chain(twin).collect();
        let mut home_ids = Vec::new();
        let mut holders = Vec::new();
        for spelling in &spellings {
            if let Some(id) = identity(std::fs::metadata(spelling)) {
                home_ids.push(id);
            }
            for ancestor in spelling.ancestors().skip(1) {
                if let Some(id) = identity(std::fs::metadata(ancestor)) {
                    holders.push(id);
                }
            }
        }
        Self {
            home: home_ids,
            holders,
        }
    }

    fn relation_of(&self, source: &Path) -> Option<HomeFolderRelation> {
        // `lstat`: a symlink to the home folder is its own file, and deleting it
        // removes only the link.
        let id = identity(std::fs::symlink_metadata(source))?;
        if self.home.contains(&id) {
            Some(HomeFolderRelation::IsHome)
        } else if self.holders.contains(&id) {
            Some(HomeFolderRelation::HoldsHome)
        } else {
            None
        }
    }
}

/// macOS: `/System/Volumes/Data` is the data volume's own root, and `/Users`
/// lives on it through a firmlink. Its folders above `/Users` aren't ancestors of
/// `/Users/me` by any path, yet deleting them deletes the home folder. `None`
/// elsewhere.
fn data_volume_twin(canonical: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let relative = canonical.strip_prefix("/").ok()?;
        Some(Path::new("/System/Volumes/Data").join(relative))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = canonical;
        None
    }
}

fn identity(meta: std::io::Result<std::fs::Metadata>) -> Option<(u64, u64)> {
    file_id(&meta.ok()?)
}

#[cfg(test)]
#[path = "home_folder_tests.rs"]
mod tests;
