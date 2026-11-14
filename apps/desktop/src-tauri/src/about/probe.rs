//! The one look at a local item before the card is planned: what it is, whether
//! its bytes are on the disk, and its identity row.
//!
//! Runs on the about pool under [`super::session::PROBE_BUDGET`]: `lstat` on a path
//! whose parent chain passes through a symlink into a dead mount waits as long as
//! the mount does.

use std::io::ErrorKind;
use std::path::Path;

use cmdr_about::fact::{AboutTarget, Fact};
use cmdr_about::owner::{OwnerPlace, OwnerQuestion};
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
    /// The mount the item sits on is a local disk, decided by `place` without a call
    /// into any other mount. `false` for a path whose parent chain reaches a network
    /// mount through a symlink: its volume said local, and it isn't. A link itself
    /// lives where its parent lives, wherever it points.
    pub(crate) on_local_disk: bool,
    pub(crate) identity: Fact<Identity>,
    /// Which node the item is, off the same `lstat`: what Cmdr's own history is
    /// asked about. `None` on a filesystem that keeps no birth time.
    pub(crate) node: Option<NodeIdentity>,
    /// The git repo around it, looked for here so the plan knows whether it has a
    /// git row. Only on a local disk.
    pub(crate) repo: RepoAround,
}

/// Looks at `path`, which the resolver placed on a local volume. `home`: the
/// person's home folder, whose app containers the probe never looks inside.
pub(crate) fn probe(path: &str, home: Option<&str>) -> Result<Probe, AboutUnavailable> {
    let fs_path = Path::new(path);
    let metadata = match std::fs::symlink_metadata(fs_path) {
        Ok(metadata) => metadata,
        Err(error) => return Err(unavailable(&error)),
    };
    let name = fs_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string());
    let sealed = home.is_some_and(|home| sealed(path, home));
    let target = target_of(fs_path, &name, &metadata, sealed);
    let extras = platform::stat_extras(&metadata);
    // ❌ Never a `statfs` on the item: it follows the item's own link, so a link to a
    // dead share would hold the probe past its budget, and a live one wakes the NAS.
    let on_local_disk = super::place::sits_on_a_local_disk(path);
    Ok(Probe {
        target,
        extension: (target != AboutTarget::Folder).then(|| extension_of(&name)).flatten(),
        is_executable: metadata.is_file() && extras.executable,
        contents_local: !extras.online_only,
        on_local_disk,
        identity: Fact::Known(identity(fs_path, &name, &metadata, extras)),
        node: NodeIdentity::of(&metadata),
        // Git's discovery stats `<folder>/.git` and walks up: inside a container, that's
        // a look at another app's data.
        repo: if on_local_disk && !sealed {
            repo_around(fs_path, metadata.is_dir())
        } else {
            RepoAround::None
        },
    })
}

/// Whether `path` is an app's container or sits inside one
/// (`~/Library/Containers/<id>`, `~/Library/Group Containers/<id>`), by its name alone.
///
/// ❌ Nothing looks inside one: that access is what can raise macOS's "would like to
/// access data from other apps" prompt, so git's discovery (a `stat` of `<it>/.git`)
/// and the package lookup (every container is named like `com.vendor.App`) are
/// skipped. The container's own `lstat` is a look at its entry in the Library, which
/// the pane already listed.
pub(crate) fn sealed(path: &str, home: &str) -> bool {
    let is_a_container = |place: OwnerPlace| matches!(place, OwnerPlace::Containers | OwnerPlace::GroupContainers);
    let itself = match OwnerQuestion::of(path, home) {
        OwnerQuestion::BundleId { place, .. } | OwnerQuestion::Name { place, .. } => is_a_container(place),
        OwnerQuestion::Receipt | OwnerQuestion::Answered(_) | OwnerQuestion::Nothing => false,
    };
    let library = format!("{}/Library", home.trim_end_matches('/'));
    itself
        || OwnerQuestion::app_folder_above(path, home).is_some_and(|above| {
            ["Containers", "Group Containers"]
                .iter()
                .any(|folder| Path::new(&above).parent() == Some(Path::new(&format!("{library}/{folder}"))))
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

/// What the item is. `sealed`: an app's container ([`sealed`]), never asked about.
fn target_of(path: &Path, name: &str, metadata: &std::fs::Metadata, sealed: bool) -> AboutTarget {
    if metadata.file_type().is_symlink() {
        return AboutTarget::Symlink;
    }
    if !metadata.is_dir() {
        return AboutTarget::File;
    }
    let is_package = is_listed_package(name)
        || (needs_package_lookup(name)
            && !sealed
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
