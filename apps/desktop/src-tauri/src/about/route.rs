//! Which facts a card gathers, decided by the item's VOLUME, ❌ never its path.
//!
//! A Finder-mounted SMB share's paths look local (`/Volumes/naspi/…`), so a path
//! prefix would send `MDItem`, `getxattr`, and a 64 KB read at a NAS. Only an item
//! the resolver places on a local volume, whose mount the mount table calls a
//! local disk, gets the local jobs. Everything else is reached [`Way`] by way:
//! through its volume's own calls (`through.rs`), with a plan that goes out
//! before the volume is asked anything, so a dead share still gets its card at
//! once.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use cmdr_about::card::{AboutFact, OsCaps, PlanInput, VolumeCaps, VolumeLimit, plan};
use cmdr_about::fact::{AboutTarget, FactKind, UnknownWhy};
use cmdr_about::owner::OwnerQuestion;
use cmdr_about::payload::SnapshotKind;
use cmdr_about::text::ItemText;
use cmdr_fs::file_extension::is_listed_package;
use cmdr_fs::volume::{RoutedKind, Volume};

use super::event::{AboutRequest, AboutUnavailable};
use super::history::History;
use super::index::IndexPlace;
use super::jobs::{GATHERED, Sources, local_jobs};
use super::owner::AppSource;
use super::platform;
use super::probe::{Probe, probe, sits_on_a_local_disk};
use super::session::{Gathered, PROBE_BUDGET, Watch};
use super::through::{self, ArchiveRecord, Asked, DownloadFrom, STUCK, SnapshotPath, Through};
use crate::file_system::volume::manager::{get_volume_manager, path_routes_over_its_parent};
use crate::location::resolve::{LocationInput, LocationMiss, ResolvedLocation, VolumeFacts, resolve};
use crate::location::store_path::to_store_path;
use crate::tools::resources::volumes::VolumeKind;

/// The id a search-results pane gives its rows' volume. Its rows are real files on
/// whatever drive holds them, so the card places the bare path instead (the
/// frontend's `volumeKindOf`, `pane/volume-capabilities.ts`).
pub(crate) const SEARCH_RESULTS_VOLUME: &str = "search-results";

/// How long a volume has to describe one item before the card says it stopped
/// answering. A healthy share's metadata call answers in milliseconds; a dead one
/// would hold it for minutes.
pub(crate) const VOLUME_BUDGET: Duration = Duration::from_secs(2);

/// How the card reaches an item, and so which rows it can have. One row of the
/// table in [`caps_for`] per way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Way {
    /// A live local disk: the OS's own files, xattrs, and frameworks answer.
    Local,
    /// A share the OS mounted (Finder's SMB, an NFS or WebDAV mount): its
    /// volume's own metadata and xattr calls, and the index.
    OsMountedShare,
    /// A server Cmdr dials itself (SMB, SFTP, WebDAV, S3, NFS): its volume's
    /// metadata, and the index.
    Server,
    /// A phone (MTP, ADB): the name, size, and dates.
    Device,
    /// An entry inside an archive: the archive's listing, and the archive's own
    /// download record.
    Archive,
    /// A snapshot from a repo's history (`.git/commits/…`): the portal's
    /// metadata, and which part of the history the path names.
    GitHistory,
    /// Anything else a route serves (a backup snapshot): its metadata.
    Snapshot,
    /// A volume that isn't connected: nothing to ask.
    Offline,
}

/// How an item on a volume of `kind` is reached. `os_visible`: the volume's paths
/// are the OS's own (`Volume::paths_are_os_visible`), which is what tells a share
/// macOS mounted from one Cmdr dials.
pub(crate) fn reach(kind: VolumeKind, live: bool, os_visible: bool) -> Way {
    if !live {
        return Way::Offline;
    }
    match kind {
        VolumeKind::Local => Way::Local,
        VolumeKind::Smb | VolumeKind::Network if os_visible => Way::OsMountedShare,
        VolumeKind::Smb | VolumeKind::Sftp | VolumeKind::Webdav | VolumeKind::S3 | VolumeKind::Network => Way::Server,
        VolumeKind::Mtp | VolumeKind::Adb | VolumeKind::Virtual => Way::Device,
    }
}

/// The way a route's view reaches into a file or repo.
fn view_of(routed: RoutedKind) -> Way {
    match routed {
        RoutedKind::Archive | RoutedKind::ArchiveInSnapshot => Way::Archive,
        RoutedKind::GitPortal => Way::GitHistory,
        RoutedKind::Restic => Way::Snapshot,
    }
}

/// What the card can ask by `way`: the table every non-local plan reads.
/// `index_covers`: a live drive index holds the volume. `xattrs`: the volume
/// stores extended attributes (`Volume::supports_xattrs`).
pub(crate) fn caps_for(way: Way, name: ItemText, index_covers: bool, xattrs: bool) -> VolumeCaps {
    let none = VolumeCaps::identity_only(name.clone());
    match way {
        Way::Local => VolumeCaps::everything(name),
        // No Spotlight, code signature, or byte read: each would be a slow read over
        // the network, and Spotlight doesn't index a share.
        Way::OsMountedShare => VolumeCaps {
            download_record: xattrs,
            index: index_covers,
            ..none
        },
        Way::Server => VolumeCaps {
            index: index_covers,
            ..none
        },
        // An entry's download row always names its archive.
        Way::Archive => VolumeCaps {
            download_record: true,
            ..none
        },
        Way::Device | Way::GitHistory | Way::Snapshot | Way::Offline => none,
    }
}

/// Resolves `request` and decides the card's plan and jobs.
pub(crate) async fn gather_for(request: &AboutRequest) -> Result<Gathered, AboutUnavailable> {
    let facts = VolumeFacts::now().await;
    let input = LocationInput {
        // A search result is a real file: place its bare path.
        volume_id: request.volume_id.clone().filter(|id| id != SEARCH_RESULTS_VOLUME),
        path: request.path.clone(),
    };
    let resolved = resolve(&facts, &input).map_err(|miss| match miss {
        LocationMiss::UnknownVolume { .. } => AboutUnavailable::UnknownVolume,
        LocationMiss::PathOutsideVolume { .. } => AboutUnavailable::PathOutsideVolume,
        LocationMiss::NoVolumeAtPath { .. } => AboutUnavailable::NoVolumeAtPath,
    })?;
    let path = resolved.location.path.clone();
    let drive = get_volume_manager().get(&resolved.location.volume_id);
    let os_visible = drive.as_ref().is_some_and(|volume| volume.paths_are_os_visible());
    let drive_way = reach(resolved.kind, resolved.live && drive.is_some(), os_visible);
    let Some(drive) = drive.filter(|_| drive_way != Way::Offline) else {
        return Err(AboutUnavailable::NoVolumeAtPath);
    };
    let runtime = tokio::runtime::Handle::current();
    let asked_drive = Asked {
        volume: Arc::clone(&drive),
        stuck_id: resolved.location.volume_id.clone(),
        runtime,
    };

    // A path into an archive or git's history is a route's view, whatever drive
    // holds it. The lexical gate costs nothing; the route confirms on the pool.
    if path_routes_over_its_parent(Path::new(&path)) {
        match routed_on_pool(&asked_drive, &path).await {
            Routed::View(routed, volume) => {
                let view = Asked {
                    volume,
                    ..asked_drive.clone()
                };
                return Ok(through_gathered(
                    view_of(routed),
                    request,
                    &resolved,
                    view,
                    Some((drive_way, &asked_drive)),
                ));
            }
            Routed::Passthrough => {}
            Routed::NoAnswer if drive_way == Way::Local => return Err(AboutUnavailable::Unreachable),
            Routed::NoAnswer => {
                return Ok(through_gathered(drive_way, request, &resolved, asked_drive.clone(), None).silenced());
            }
        }
    }

    match drive_way {
        Way::Local => local_card(&resolved, path).await,
        _ => Ok(through_gathered(drive_way, request, &resolved, asked_drive, None)),
    }
}

async fn local_card(resolved: &ResolvedLocation, path: String) -> Result<Gathered, AboutUnavailable> {
    let probed = probe_on_pool(path.clone()).await?;
    let home = dirs::home_dir().map(|home| home.to_string_lossy().into_owned());
    let sources = Sources {
        history: History::of_the_app(&resolved.location.volume_id),
        index: Some(index_place(resolved, &path)),
        // No home folder (a sandboxed test runner) names no Library places.
        owner: home
            .as_deref()
            .map_or(OwnerQuestion::Nothing, |home| OwnerQuestion::of(&path, home)),
        home,
        apps: AppSource::of_the_os(),
    };
    Ok(local_gathered(
        &path,
        probed,
        ItemText::new(&resolved.name),
        platform::os_caps(),
        sources,
    ))
}

fn index_place(resolved: &ResolvedLocation, path: &str) -> IndexPlace {
    IndexPlace {
        index_id: resolved.index.volume_id.clone(),
        path: path.to_string(),
        importance: to_store_path(path, &resolved.root)
            .map(|store_path| (resolved.location.volume_id.clone(), store_path)),
        data_dir: crate::config::standalone_app_data_dir(),
    }
}

/// What the route answered for a path its lexical gate let through.
enum Routed {
    /// A view serves it.
    View(RoutedKind, Arc<dyn Volume>),
    /// No route took it: the drive serves it.
    Passthrough,
    /// The route's confirm didn't answer in time (it reads the drive).
    NoAnswer,
}

/// Asks the volume manager which volume serves `path`, on the about pool: a
/// route's confirm can stat the drive, and on a share that stopped answering that
/// waits as long as the share does.
async fn routed_on_pool(drive: &Asked, path: &str) -> Routed {
    let (answer, waiting) = tokio::sync::oneshot::channel();
    let runtime = drive.runtime.clone();
    let volume_id = drive.stuck_id.clone();
    let path = path.to_string();
    super::pool::submit(Box::new(move || {
        crate::file_system::framework_pool::note_activity("resolve the route", "probe");
        let resolved = runtime.block_on(get_volume_manager().resolve(&volume_id, Path::new(&path)));
        let routed = match (resolved.routed, resolved.volume) {
            (Some(kind), Some(volume)) => Routed::View(kind, volume),
            _ => Routed::Passthrough,
        };
        let _ = answer.send(routed);
    }));
    match tokio::time::timeout(PROBE_BUDGET, waiting).await {
        Ok(Ok(routed)) => routed,
        Ok(Err(_)) | Err(_) => Routed::NoAnswer,
    }
}

/// Runs the probe on the about pool and waits for it at most [`PROBE_BUDGET`].
async fn probe_on_pool(path: String) -> Result<Probe, AboutUnavailable> {
    let (answer, waiting) = tokio::sync::oneshot::channel();
    super::pool::submit(Box::new(move || {
        crate::file_system::framework_pool::note_activity("lstat", "probe");
        let _ = answer.send(probe(&path));
    }));
    match tokio::time::timeout(PROBE_BUDGET, waiting).await {
        Ok(Ok(probed)) => probed,
        // The pool dropped the job (it can't, but a lost answer is no answer).
        Ok(Err(_)) => Err(AboutUnavailable::Unreadable),
        Err(_) => Err(AboutUnavailable::Unreachable),
    }
}

/// The plan and jobs for an item the probe looked at.
///
/// An item whose mount isn't a local disk after all (a symlink in its parent
/// chain leads to a share) answers identity only: nothing reads its bytes,
/// xattrs, or frameworks from here.
///
/// `sources` says where the stores are. One that isn't there (the operation log
/// didn't open, the index has no place for the path) leaves its row out of the
/// plan: a row with no job would never settle.
pub(crate) fn local_gathered(
    path: &str,
    probed: Probe,
    volume_name: ItemText,
    os: OsCaps,
    sources: Sources,
) -> Gathered {
    let caps = if probed.on_local_disk {
        VolumeCaps::everything(volume_name)
    } else {
        VolumeCaps::identity_only(volume_name)
    };
    let mut plan = plan(PlanInput {
        target: probed.target,
        extension: probed.extension.as_deref(),
        is_executable: probed.is_executable,
        contents_local: probed.contents_local,
        caps: &caps,
        os: &os,
        owner: &sources.owner,
        in_a_repo: probed.repo.is_some(),
        git_snapshot: false,
    });
    plan.retain_kinds(|kind| {
        GATHERED.contains(&kind)
            && match kind {
                FactKind::CmdrOperation => sources.history.is_some(),
                FactKind::RecursiveSize | FactKind::FolderDigest => sources.index.is_some(),
                FactKind::AppData => sources.index.is_some() && sources.home.is_some(),
                FactKind::Importance => sources.index.as_ref().is_some_and(IndexPlace::scores),
                _ => true,
            }
    });
    let jobs = if probed.on_local_disk {
        local_jobs(&plan, path, &probed, &sources)
    } else {
        Vec::new()
    };
    Gathered {
        plan,
        ready: vec![AboutFact::Identity(probed.identity)],
        jobs,
        reach: Watch::EachRow,
    }
}

/// What the item is, from what the pane listed: the plan of an item reached
/// through its volume goes out before the volume answers anything.
pub(crate) fn hinted_target(name: &str, is_directory: bool) -> AboutTarget {
    if !is_directory {
        AboutTarget::File
    } else if !is_listed_package(name) {
        AboutTarget::Folder
    } else if name.to_ascii_lowercase().ends_with(".app") {
        AboutTarget::AppBundle
    } else {
        AboutTarget::Package
    }
}

/// The plan and jobs for an item reached `way`, through `asked`'s own calls.
/// `drive`: for a view, how the drive under it is reached and asked (an archive's
/// own download record lives there).
pub(crate) fn through_gathered(
    way: Way,
    request: &AboutRequest,
    resolved: &ResolvedLocation,
    asked: Asked,
    drive: Option<(Way, &Asked)>,
) -> Gathered {
    let path = resolved.location.path.clone();
    let name = Path::new(&path)
        .file_name()
        .map_or_else(|| path.clone(), |name| name.to_string_lossy().into_owned());
    let index_covers = resolved.index.live && matches!(way, Way::OsMountedShare | Way::Server);
    let download = match way {
        Way::OsMountedShare if asked.volume.supports_xattrs() => Some(DownloadFrom::Volume),
        Way::Archive => archive_download(&path, drive),
        _ => None,
    };
    let through = Through {
        asked: asked.clone(),
        path: path.clone().into(),
        download,
        index: index_covers.then(|| index_place(resolved, &path)),
        snapshot: (way == Way::GitHistory)
            .then(|| snapshot_of(Path::new(&path)))
            .flatten(),
    };
    let caps = caps_for(
        way,
        ItemText::new(&resolved.name),
        index_covers,
        asked.volume.supports_xattrs(),
    );
    through_plan(way, &name, request.is_directory, &caps, through)
}

/// [`through_gathered`] past resolving: the plan from the hint, then a job per
/// row it keeps. Apart so a test can hand it any volume.
pub(crate) fn through_plan(way: Way, name: &str, is_directory: bool, caps: &VolumeCaps, through: Through) -> Gathered {
    let target = if way == Way::Archive {
        AboutTarget::ArchiveEntry
    } else {
        hinted_target(name, is_directory)
    };
    let extension = (target != AboutTarget::Folder)
        .then(|| Path::new(name).extension().map(|e| e.to_string_lossy().into_owned()))
        .flatten();
    let mut plan = plan(PlanInput {
        target,
        extension: extension.as_deref(),
        is_executable: false,
        contents_local: false,
        caps,
        os: &platform::os_caps(),
        // Nothing reads the OS's owner records or a repo's status through a volume.
        owner: &OwnerQuestion::Nothing,
        in_a_repo: false,
        git_snapshot: through.snapshot.is_some(),
    });
    plan.retain_kinds(|kind| GATHERED.contains(&kind) && through::answers(&through, kind));
    let is_view = matches!(way, Way::Archive | Way::GitHistory | Way::Snapshot);
    let volume_answers = !STUCK.is_full(&through.asked.stuck_id);
    let jobs = through::jobs(&plan, &through, volume_answers);
    let mut gathered = Gathered {
        plan,
        ready: Vec::new(),
        jobs,
        // A view's rows read one file or repo on the drive: one that's slow is
        // slow by itself. A volume's identity is its health check.
        reach: if is_view { Watch::EachRow } else { Watch::TheVolume },
    };
    if !volume_answers {
        log::debug!(target: "about", "the item's volume is still holding earlier calls; not asking it again");
        gathered = gathered.silenced();
    }
    gathered
}

impl Gathered {
    /// The card for a volume already known not to answer: every row that would
    /// ask it lands `VolumeStoppedAnswering` at once and the note says so; rows
    /// the index answers still run.
    pub(crate) fn silenced(mut self) -> Self {
        let silent: Vec<FactKind> = self
            .plan
            .kinds()
            .into_iter()
            .filter(|kind| through::asks_the_volume(*kind))
            .collect();
        self.jobs.retain(|(kind, _)| !silent.contains(kind));
        self.ready = silent
            .into_iter()
            .map(|kind| AboutFact::unknown(kind, UnknownWhy::VolumeStoppedAnswering))
            .collect();
        if let Some(note) = &mut self.plan.volume_note {
            note.limit = VolumeLimit::Unreachable;
        }
        self
    }
}

/// An archive entry's download row: the archive's name, and where its own record
/// can be read.
fn archive_download(path: &str, drive: Option<(Way, &Asked)>) -> Option<DownloadFrom> {
    let (archive_path, _inner) = cmdr_fs::archive_format::archive_boundary_candidate(Path::new(path))?;
    let archive = ItemText::new(&archive_path.file_name()?.to_string_lossy());
    let record = match drive {
        // The mount table, no call to the mount: a symlink into a share isn't local.
        Some((Way::Local, _)) if sits_on_a_local_disk(&archive_path.to_string_lossy()) => ArchiveRecord::Local {
            archive_path: archive_path.to_string_lossy().into_owned(),
        },
        Some((Way::OsMountedShare, asked)) if asked.volume.supports_xattrs() => ArchiveRecord::Volume {
            volume: asked.clone(),
            archive_path,
        },
        _ => ArchiveRecord::None,
    };
    Some(DownloadFrom::Archive { archive, record })
}

/// Which part of git's history a portal path names, off the path alone:
/// `<repo>/.git/<category>/<name>/…`. The category words are the portal's own
/// (`cmdr_git`'s `Cat::as_segment`); a test pins them against a real portal.
pub(crate) fn snapshot_of(path: &Path) -> Option<SnapshotPath> {
    let repo_root = cmdr_git::portal_route(path)?;
    let mut rest = path.strip_prefix(repo_root.join(".git")).ok()?.components();
    let category = rest.next()?.as_os_str().to_str()?.to_string();
    let kind = [
        SnapshotKind::Branch,
        SnapshotKind::Tag,
        SnapshotKind::Commit,
        SnapshotKind::Stash,
    ]
    .into_iter()
    .find(|kind| through::category_segment(*kind) == category)?;
    let after: Vec<String> = rest
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    // The category's own folder names no snapshot.
    (!after.is_empty()).then_some(SnapshotPath { repo_root, kind, after })
}
