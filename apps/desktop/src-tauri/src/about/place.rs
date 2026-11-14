//! Where a local path really lives, decided ❌ without one call into a mount that
//! isn't a local disk.
//!
//! The resolver places a path by its text, so `~/nas/report.pdf` (with `~/nas` a
//! link to `/Volumes/naspi`) reads as the boot disk, and so does `~/nas` itself. A
//! `statfs` or `realpath` on either follows the link into the share: on a dead one it
//! blocks, and on a sleeping NAS it's the call that wakes it. So every "is this on a
//! local disk" question here walks the path one component at a time from the root:
//! before each `lstat` it places the component in the mount table by its text
//! (`getfsstat(MNT_NOWAIT)` / `/proc/mounts`, no call into any mount), and it follows
//! each link by reading it where it lives. The walk stops at the edge of the first
//! mount that isn't a local disk, so the same answer holds on macOS and Linux.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

/// The most links one walk follows before it gives up: the kernel's own limit for
/// one path (`MAXSYMLINKS`).
const MAX_LINKS: usize = 32;

/// The mount table, each mount point with whether it's a local disk.
pub(crate) struct Mounts(Vec<(PathBuf, bool)>);

impl Mounts {
    /// The OS's mount table, or `None` when it couldn't be read.
    pub(crate) fn of_the_os() -> Option<Self> {
        crate::volumes::mount_rows().map(Self::from_rows)
    }

    /// The table from `(mount point, source, fs type)` rows.
    ///
    /// On macOS the data volume's mounts are listed under `/System/Volumes/Data`,
    /// which the firmlinks show at `/`; a path is spelled the short way, so each such
    /// mount also covers its short spelling.
    pub(crate) fn from_rows(rows: Vec<(String, String, String)>) -> Self {
        let mut mounts = Vec::with_capacity(rows.len());
        for (point, source, fs_type) in rows {
            let local = cmdr_fs::filesystem_kind::mount_is_local_disk(Some(&source), Some(&fs_type));
            if let Some(short) = point
                .strip_prefix("/System/Volumes/Data")
                .filter(|rest| rest.starts_with('/'))
            {
                mounts.push((PathBuf::from(short), local));
            }
            mounts.push((PathBuf::from(point), local));
        }
        Self(mounts)
    }

    /// Whether `path` sits on a local disk, by the longest mount point its text
    /// starts with. A path no mount covers isn't one.
    fn is_local_disk(&self, path: &Path) -> bool {
        self.0
            .iter()
            .filter(|(point, _)| path.starts_with(point))
            .max_by_key(|(point, _)| point.as_os_str().len())
            .is_some_and(|(_, local)| *local)
    }
}

/// The two calls a walk makes, both on a local disk: injectable, so a test can
/// prove which paths it touched.
pub(crate) trait LocalFs {
    /// Whether the entry at `path` is a symlink (`lstat`, never following it).
    fn is_symlink(&self, path: &Path) -> std::io::Result<bool>;
    /// The link's target as written.
    fn read_link(&self, path: &Path) -> std::io::Result<PathBuf>;
}

/// The real filesystem.
pub(crate) struct Os;

impl LocalFs for Os {
    fn is_symlink(&self, path: &Path) -> std::io::Result<bool> {
        std::fs::symlink_metadata(path).map(|metadata| metadata.file_type().is_symlink())
    }

    fn read_link(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::read_link(path)
    }
}

/// Whether the walk follows the item's own link, or only its parents'.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TheItem {
    /// Asked about as itself: a link lives where its parent lives.
    AsItself,
    /// Followed like a parent: where reading through it would go.
    Followed,
}

/// Where a walk ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum End {
    /// Every component it reached is on a local disk.
    LocalDisk,
    /// A component doesn't exist (or isn't a folder): nothing past it can be read.
    Gone,
    /// The next component sits on a mount that isn't a local disk.
    OffTheDisk,
    /// A link loop, an unreadable link, a component that wouldn't answer, or a path
    /// that isn't absolute: nothing proves where it goes.
    Unproven,
}

/// What a walk found: the path with every link it followed resolved, and where it
/// stopped. When it stopped early, the components it didn't reach follow as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Walked {
    pub(crate) path: PathBuf,
    pub(crate) end: End,
}

/// Walks `path` from the root, as [`TheItem`] says, ❌ never calling into a mount
/// `mounts` doesn't call a local disk.
pub(crate) fn walk(path: &Path, item: TheItem, mounts: &Mounts, fs: &dyn LocalFs) -> Walked {
    if !path.is_absolute() {
        return Walked {
            path: path.to_path_buf(),
            end: End::Unproven,
        };
    }
    let mut todo: VecDeque<OsString> = components(path).collect();
    let mut current = PathBuf::from("/");
    let mut links = 0;
    let rest =
        |current: &Path, todo: &VecDeque<OsString>| todo.iter().fold(current.to_path_buf(), |at, part| at.join(part));
    while let Some(name) = todo.pop_front() {
        if name == ".." {
            current.pop();
            continue;
        }
        let candidate = current.join(&name);
        if !mounts.is_local_disk(&candidate) {
            todo.push_front(name);
            return Walked {
                path: rest(&current, &todo),
                end: End::OffTheDisk,
            };
        }
        if todo.is_empty() && item == TheItem::AsItself {
            return Walked {
                path: candidate,
                end: End::LocalDisk,
            };
        }
        let is_link = match fs.is_symlink(&candidate) {
            Ok(is_link) => is_link,
            Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
                return Walked {
                    path: rest(&candidate, &todo),
                    end: End::Gone,
                };
            }
            Err(_) => {
                return Walked {
                    path: rest(&candidate, &todo),
                    end: End::Unproven,
                };
            }
        };
        if !is_link {
            current = candidate;
            continue;
        }
        links += 1;
        let target = match fs.read_link(&candidate) {
            Ok(target) if links <= MAX_LINKS => target,
            _ => {
                return Walked {
                    path: rest(&candidate, &todo),
                    end: End::Unproven,
                };
            }
        };
        if target.is_absolute() {
            current = PathBuf::from("/");
        }
        for part in components(&target).collect::<Vec<_>>().into_iter().rev() {
            todo.push_front(part);
        }
    }
    Walked {
        path: current,
        end: End::LocalDisk,
    }
}

/// `path`'s named components, `..` kept as a step back.
fn components(path: &Path) -> impl Iterator<Item = OsString> + '_ {
    path.components().filter_map(|component| match component {
        Component::Normal(name) => Some(name.to_os_string()),
        Component::ParentDir => Some("..".into()),
        Component::RootDir | Component::CurDir | Component::Prefix(_) => None,
    })
}

/// Whether the item at `path` sits on a local disk: its parents' links followed, its
/// own (if it's one) not, since a link lives where its parent lives. An unreadable
/// mount table, or a path nothing proves, reads as no. Blocking (the walk `lstat`s
/// each local ancestor): on the about pool.
pub(crate) fn sits_on_a_local_disk(path: &str) -> bool {
    let Some(mounts) = Mounts::of_the_os() else {
        return false;
    };
    on_a_local_disk(Path::new(path), &mounts, &Os)
}

/// [`sits_on_a_local_disk`] over a given table and filesystem.
pub(crate) fn on_a_local_disk(path: &Path, mounts: &Mounts, fs: &dyn LocalFs) -> bool {
    // Gone: nothing is there to read, and the probe says so without a share.
    matches!(
        walk(path, TheItem::AsItself, mounts, fs).end,
        End::LocalDisk | End::Gone
    )
}

/// Whether reading the item at `path` would pass into a mount that isn't a local
/// disk, through its parents' links OR its own. For a card Cmdr started on its own,
/// that call is what wakes a sleeping NAS: an unreadable mount table, a link loop,
/// or a component that won't answer reads as yes, so a background card that can't
/// prove the path local asks nothing. Blocking: off the executor.
pub(crate) fn reaches_a_share(path: &str) -> bool {
    let Some(mounts) = Mounts::of_the_os() else {
        return true;
    };
    reaches_off_the_disk(Path::new(path), &mounts, &Os)
}

/// [`reaches_a_share`] over a given table and filesystem.
pub(crate) fn reaches_off_the_disk(path: &Path, mounts: &Mounts, fs: &dyn LocalFs) -> bool {
    matches!(
        walk(path, TheItem::Followed, mounts, fs).end,
        End::OffTheDisk | End::Unproven
    )
}

/// `path` with every link on a local disk followed, the item's own included, for the
/// sensitive gate: past the edge of a share (or a part that's gone) the rest stays as
/// written. `None` when nothing proves where it goes (a loop, an unreadable link or
/// mount table): the gate fails closed then.
pub(crate) fn resolved_on_local_disks(path: &str) -> Option<PathBuf> {
    let mounts = Mounts::of_the_os()?;
    resolved_within(Path::new(path), &mounts, &Os)
}

/// [`resolved_on_local_disks`] over a given table and filesystem.
pub(crate) fn resolved_within(path: &Path, mounts: &Mounts, fs: &dyn LocalFs) -> Option<PathBuf> {
    let walked = walk(path, TheItem::Followed, mounts, fs);
    (walked.end != End::Unproven).then_some(walked.path)
}

#[cfg(all(test, unix))]
#[path = "place_tests.rs"]
mod tests;
