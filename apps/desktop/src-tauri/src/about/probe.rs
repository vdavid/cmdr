//! The one look at a local item before the card is planned: what it is, whether
//! its bytes are on the disk, and its identity row.
//!
//! Runs on the about pool under [`super::session::PROBE_BUDGET`]: `lstat` on a path
//! whose parent chain passes through a symlink into a dead mount waits as long as
//! the mount does.

use std::io::ErrorKind;
use std::path::Path;

use cmdr_about::fact::{AboutTarget, Fact, UnknownWhy};
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
    /// asked about. `None` for an archive's entry, and on a filesystem that keeps
    /// no birth time.
    pub(crate) node: Option<NodeIdentity>,
    /// The git repo around it, looked for here so the plan knows whether it has a
    /// git row. Only on a local disk, never for an archive's entry.
    pub(crate) repo: RepoAround,
}

/// Looks at `path`, which the resolver placed on a local volume.
pub(crate) fn probe(path: &str) -> Result<Probe, AboutUnavailable> {
    let fs_path = Path::new(path);
    let metadata = match std::fs::symlink_metadata(fs_path) {
        Ok(metadata) => metadata,
        Err(error) => return inside_an_archive(fs_path).ok_or_else(|| unavailable(&error)),
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

/// An entry inside an archive: the path isn't a real file, and an archive on this
/// disk holds it. Its facts come from the archive's own listing, which the card
/// doesn't read yet, so it says what it can't tell.
fn inside_an_archive(path: &Path) -> Option<Probe> {
    let (archive, _inner) = cmdr_archive::confirm_archive_boundary(path)?;
    let name = path.file_name()?.to_string_lossy().into_owned();
    Some(Probe {
        target: AboutTarget::ArchiveEntry,
        extension: extension_of(&name),
        is_executable: false,
        contents_local: false,
        on_local_disk: sits_on_a_local_disk(&archive.to_string_lossy()),
        identity: Fact::unknown(UnknownWhy::VolumeCantTell),
        node: None,
        repo: RepoAround::None,
    })
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

/// Whether the mount `path` sits on is a local disk, by the mount table's own
/// answer. A path the mount table can't place reads as not local.
fn sits_on_a_local_disk(path: &str) -> bool {
    crate::volumes::get_mount_point(path).is_some_and(|(_mount_point, fs_type, source)| {
        cmdr_fs::filesystem_kind::mount_is_local_disk(Some(&source), Some(&fs_type))
    })
}

fn identity(path: &Path, name: &str, metadata: &std::fs::Metadata, extras: platform::StatExtras) -> Identity {
    let is_dir = metadata.is_dir();
    Identity {
        name: ItemText::new(name),
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
