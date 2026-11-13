//! The one look at a local item before the card is planned: what it is, whether
//! its bytes are on the disk, and its identity row.
//!
//! Runs on the about pool under [`super::session::PROBE_BUDGET`]: `lstat` on a path
//! whose parent chain passes through a symlink into a dead mount waits as long as
//! the mount does.

use std::io::ErrorKind;
use std::path::Path;

use cmdr_about::fact::{AboutTarget, Fact};
use cmdr_about::payload::Identity;
use cmdr_about::text::ItemText;
use cmdr_fs::file_extension::{is_listed_package, needs_package_lookup};
use cmdr_fs::volume::NodeIdentity;

use super::event::AboutUnavailable;
use super::git::{RepoAround, repo_around};
use super::platform;

/// What the probe learned.
#[derive(Debug, Clone)]
pub(crate) struct Probe {
    pub(crate) target: AboutTarget,
    /// The name's last extension, without its dot. `None` for a folder, whose
    /// dot is part of its name.
    pub(crate) extension: Option<String>,
    pub(crate) is_executable: bool,
    /// The bytes are on the disk (not an online-only cloud file).
    pub(crate) contents_local: bool,
    /// The mount the item sits on is a local disk. `false` for a path whose parent
    /// chain reaches a network mount through a symlink: its volume said local, and
    /// it isn't.
    pub(crate) on_local_disk: bool,
    pub(crate) identity: Fact<Identity>,
    /// Which node the item is, off the same `lstat`: what Cmdr's own history is
    /// asked about. `None` on a filesystem that keeps no birth time.
    pub(crate) node: Option<NodeIdentity>,
    /// The git repo around it, looked for here so the plan knows whether it has a
    /// git row. Only on a local disk.
    pub(crate) repo: RepoAround,
}

/// Looks at `path`, which the resolver placed on a local volume.
pub(crate) fn probe(path: &str) -> Result<Probe, AboutUnavailable> {
    let fs_path = Path::new(path);
    let metadata = match std::fs::symlink_metadata(fs_path) {
        Ok(metadata) => metadata,
        Err(error) => return Err(unavailable(&error)),
    };
    let name = fs_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    let target = target_of(fs_path, &name, &metadata);
    let extras = platform::stat_extras(&metadata);
    let on_local_disk = sits_on_a_local_disk(path);
    Ok(Probe {
        target,
        extension: (target != AboutTarget::Folder).then(|| extension_of(&name)).flatten(),
        is_executable: metadata.is_file() && extras.executable,
        contents_local: !extras.online_only,
        on_local_disk,
        identity: Fact::Known(identity(fs_path, &name, &metadata, extras)),
        node: NodeIdentity::of(&metadata),
        repo: if on_local_disk {
            repo_around(fs_path, metadata.is_dir())
        } else {
            RepoAround::None
        },
    })
}

fn extension_of(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned())
}

fn unavailable(error: &std::io::Error) -> AboutUnavailable {
    match error.kind() {
        ErrorKind::NotFound | ErrorKind::NotADirectory => AboutUnavailable::NotFound,
        ErrorKind::PermissionDenied => AboutUnavailable::PermissionDenied,
        _ => AboutUnavailable::Unreadable,
    }
}

fn target_of(path: &Path, name: &str, metadata: &std::fs::Metadata) -> AboutTarget {
    if metadata.file_type().is_symlink() {
        return AboutTarget::Symlink;
    }
    if !metadata.is_dir() {
        return AboutTarget::File;
    }
    let is_package = is_listed_package(name)
        || (needs_package_lookup(name)
            && path
                .to_str()
                .is_some_and(crate::file_system::packages::is_package_to_the_os));
    if !is_package {
        return AboutTarget::Folder;
    }
    let is_app = extension_of(name).is_some_and(|extension| extension.eq_ignore_ascii_case("app"));
    if is_app {
        AboutTarget::AppBundle
    } else {
        AboutTarget::Package
    }
}

/// The most links [`reaches_a_share`] follows before it gives up and answers yes:
/// the kernel's own limit for one path (`MAXSYMLINKS`).
const MAX_LINKS: usize = 32;

/// Whether resolving `path` would pass into a mount the mount table doesn't call a
/// local disk, decided ❌ without one call into such a mount.
///
/// The resolver places a path by its text, so `~/nas/report.pdf`, with `~/nas` a link
/// to `/Volumes/naspi`, reads as the boot disk; its `lstat` then reaches the share
/// (the kernel follows the parent's link). For a card Cmdr started on its own, that
/// call is what wakes a sleeping NAS. So this walks the path one component at a time
/// from the root: before each `lstat` it checks the component against the mount
/// table (`getfsstat(MNT_NOWAIT)` / `/proc/mounts`, no call to any mount), and follows
/// each link by reading it where it lives, on the local disk. An unreadable mount
/// table, a link loop, or a component that won't answer reads as yes: a background
/// card that can't prove the path local asks nothing.
pub(crate) fn reaches_a_share(path: &str) -> bool {
    let Some(rows) = crate::volumes::mount_rows() else {
        return true;
    };
    let shares: Vec<std::path::PathBuf> = rows
        .into_iter()
        .filter(|(_point, source, fs_type)| !cmdr_fs::filesystem_kind::mount_is_local_disk(Some(source), Some(fs_type)))
        .map(|(point, _source, _fs_type)| std::path::PathBuf::from(point))
        .collect();
    walk_reaches(Path::new(path), &shares, &|step| std::fs::symlink_metadata(step))
}

/// [`reaches_a_share`] over a given mount list and `lstat`, so a test can prove which
/// paths it touched.
fn walk_reaches(
    path: &Path,
    shares: &[std::path::PathBuf],
    lstat: &dyn Fn(&Path) -> std::io::Result<std::fs::Metadata>,
) -> bool {
    use std::collections::VecDeque;
    use std::path::Component;
    if !path.is_absolute() {
        return true;
    }
    let under_a_share = |candidate: &Path| shares.iter().any(|share| candidate.starts_with(share));
    let mut todo: VecDeque<std::ffi::OsString> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name.to_os_string()),
            Component::ParentDir => Some("..".into()),
            Component::RootDir | Component::CurDir | Component::Prefix(_) => None,
        })
        .collect();
    let mut current = std::path::PathBuf::from("/");
    let mut links = 0;
    while let Some(name) = todo.pop_front() {
        if name == ".." {
            current.pop();
            continue;
        }
        let candidate = current.join(&name);
        if under_a_share(&candidate) {
            return true;
        }
        // The item itself is `lstat`ed as itself, never followed: only its parents'
        // links decide where it lives.
        if todo.is_empty() {
            return false;
        }
        let metadata = match lstat(&candidate) {
            Ok(metadata) => metadata,
            // Gone: the probe will say so, without the share.
            Err(error) if error.kind() == ErrorKind::NotFound => return false,
            Err(_) => return true,
        };
        if !metadata.file_type().is_symlink() {
            current = candidate;
            continue;
        }
        links += 1;
        if links > MAX_LINKS {
            return true;
        }
        let Ok(target) = std::fs::read_link(&candidate) else {
            return true;
        };
        if target.is_absolute() {
            current = std::path::PathBuf::from("/");
        }
        for component in target.components().rev() {
            match component {
                Component::Normal(part) => todo.push_front(part.to_os_string()),
                Component::ParentDir => todo.push_front("..".into()),
                Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
            }
        }
    }
    false
}

/// Whether the mount `path` sits on is a local disk, by the mount table's own
/// answer. A path the mount table can't place reads as not local.
pub(crate) fn sits_on_a_local_disk(path: &str) -> bool {
    crate::volumes::get_mount_point(path).is_some_and(|(_mount_point, fs_type, source)| {
        cmdr_fs::filesystem_kind::mount_is_local_disk(Some(&source), Some(&fs_type))
    })
}

fn identity(path: &Path, name: &str, metadata: &std::fs::Metadata, extras: platform::StatExtras) -> Identity {
    let is_dir = metadata.is_dir();
    Identity {
        name: ItemText::new(name),
        name_hid_reordering: cmdr_about::text::hides_reordering(name),
        // A folder's own entry size says nothing about what it holds.
        logical_size: (!is_dir).then_some(metadata.len()),
        physical_size: if is_dir { None } else { extras.physical_size },
        created: metadata.created().ok().and_then(unix_seconds),
        modified: metadata.modified().ok().and_then(unix_seconds),
        hard_links: extras.hard_links,
        symlink_target: metadata
            .file_type()
            .is_symlink()
            .then(|| std::fs::read_link(path).ok())
            .flatten()
            .map(|target| ItemText::new(&target.to_string_lossy())),
    }
}

fn unix_seconds(time: std::time::SystemTime) -> Option<i64> {
    match time.duration_since(std::time::UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_secs()).ok(),
        Err(before) => i64::try_from(before.duration().as_secs()).ok().map(|secs| -secs),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::cell::RefCell;
    use std::path::PathBuf;

    use super::*;
    use crate::test_support::TestDir;

    /// A folder the test calls a share, and every path the walk `lstat`ed.
    struct World {
        _dir: TestDir,
        root: PathBuf,
        share: PathBuf,
        touched: RefCell<Vec<PathBuf>>,
    }

    impl World {
        fn new() -> Self {
            let dir = TestDir::new("about-reaches-share");
            // Canonical, so the test's own temp-folder link (`/var` → `/private/var`)
            // isn't what the walk follows.
            let root = std::fs::canonicalize(dir.as_ref()).expect("canonical");
            let share = root.join("share");
            std::fs::create_dir(&share).expect("make the share");
            std::fs::write(share.join("report.pdf"), "x").expect("write into it");
            Self {
                _dir: dir,
                root,
                share,
                touched: RefCell::new(Vec::new()),
            }
        }

        fn reaches(&self, path: &Path) -> bool {
            walk_reaches(path, std::slice::from_ref(&self.share), &|step| {
                self.touched.borrow_mut().push(step.to_path_buf());
                std::fs::symlink_metadata(step)
            })
        }

        fn touched_the_share(&self) -> bool {
            self.touched.borrow().iter().any(|path| path.starts_with(&self.share))
        }
    }

    /// ❗ A path that reaches a share through a linked folder is found out without one
    /// call into the share, absolute link or relative.
    #[test]
    fn a_link_into_a_share_is_found_without_a_call_into_it() {
        let world = World::new();
        let absolute = world.root.join("nas");
        std::os::unix::fs::symlink(&world.share, &absolute).expect("link to it");
        assert!(world.reaches(&absolute.join("report.pdf")));
        assert!(!world.touched_the_share(), "{:?}", world.touched.borrow());

        std::fs::create_dir(world.root.join("deeper")).expect("make a folder");
        let relative = world.root.join("deeper").join("nas");
        std::os::unix::fs::symlink("../share", &relative).expect("link to it");
        assert!(world.reaches(&relative.join("report.pdf")));
        assert!(!world.touched_the_share(), "{:?}", world.touched.borrow());
        assert!(world.reaches(&world.share.join("report.pdf")), "the share itself");
    }

    /// A path that stays on the local disk, links and all, reads as local; the item
    /// itself is never followed.
    #[test]
    fn a_local_path_reads_local_links_included() {
        let world = World::new();
        let docs = world.root.join("docs");
        std::fs::create_dir(&docs).expect("make docs");
        std::fs::write(docs.join("notes.txt"), "x").expect("write");
        let alias = world.root.join("alias");
        std::os::unix::fs::symlink(&docs, &alias).expect("link to docs");
        assert!(!world.reaches(&alias.join("notes.txt")));
        let item_link = world.root.join("item-into-share");
        std::os::unix::fs::symlink(&world.share, &item_link).expect("link");
        assert!(!world.reaches(&item_link), "the item is asked about as itself");
        assert!(
            !world.reaches(&world.root.join("gone").join("x")),
            "gone is the probe's to say"
        );
    }

    /// A link loop answers yes: a background card that can't prove the path local
    /// asks nothing.
    #[test]
    fn a_link_loop_reads_as_a_share() {
        let world = World::new();
        let a = world.root.join("a");
        let b = world.root.join("b");
        std::os::unix::fs::symlink(&b, &a).expect("a → b");
        std::os::unix::fs::symlink(&a, &b).expect("b → a");
        assert!(world.reaches(&a.join("x")));
    }
}
