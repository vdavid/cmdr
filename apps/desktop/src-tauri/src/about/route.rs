//! Which facts a card gathers, decided by the item's VOLUME, ❌ never its path.
//!
//! A Finder-mounted SMB share's paths look local (`/Volumes/naspi/…`), so a path
//! prefix would send `MDItem`, `getxattr`, and a 64 KB read at a NAS. Only an item
//! the resolver places on a local volume, whose mount the mount table calls a
//! local disk, gets the local jobs. Everything else answers identity through its
//! volume's own `get_metadata`, under a deadline, and the plan leaves the rest out.

use std::path::Path;
use std::time::Duration;

use cmdr_about::card::{AboutFact, AboutPlan, OsCaps, PlanInput, VolumeCaps, plan};
use cmdr_about::fact::{AboutTarget, Fact, FactKind};
use cmdr_about::payload::Identity;
use cmdr_about::text::ItemText;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::VolumeError;

use super::event::{AboutRequest, AboutUnavailable};
use cmdr_about::owner::OwnerQuestion;

use super::history::History;
use super::index::IndexPlace;
use super::jobs::{GATHERED, Sources, local_jobs};
use super::owner::AppSource;
use super::platform;
use super::probe::{Probe, probe};
use super::session::{Gathered, PROBE_BUDGET};
use crate::location::resolve::{LocationInput, LocationMiss, VolumeFacts, resolve};
use crate::location::store_path::to_store_path;
use crate::tools::resources::volumes::VolumeKind;

/// How long a remote volume may take to describe one item. The metadata call of
/// a healthy share answers in milliseconds; a dead one would hold it for minutes.
const REMOTE_BUDGET: Duration = Duration::from_secs(2);

/// How a resolved item is reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reach {
    /// A local volume: the OS's own files, xattrs, and frameworks answer.
    Local,
    /// Anything else (a share, a server, a phone, a volume not connected): only
    /// the volume's own deadline-wrapped calls.
    ThroughTheVolume,
}

/// How an item on a volume of `kind` is reached. Live matters: a saved volume
/// that isn't registered has no local files to read.
pub(crate) fn reach(kind: VolumeKind, live: bool) -> Reach {
    match kind {
        VolumeKind::Local if live => Reach::Local,
        VolumeKind::Local
        | VolumeKind::Smb
        | VolumeKind::Sftp
        | VolumeKind::Webdav
        | VolumeKind::S3
        | VolumeKind::Mtp
        | VolumeKind::Adb
        | VolumeKind::Network
        | VolumeKind::Virtual => Reach::ThroughTheVolume,
    }
}

/// Resolves `request` and decides the card's plan and jobs.
pub(crate) async fn gather_for(request: &AboutRequest) -> Result<Gathered, AboutUnavailable> {
    let facts = VolumeFacts::now().await;
    let input = LocationInput {
        volume_id: request.volume_id.clone(),
        path: request.path.clone(),
    };
    let resolved = resolve(&facts, &input).map_err(|miss| match miss {
        LocationMiss::UnknownVolume { .. } => AboutUnavailable::UnknownVolume,
        LocationMiss::PathOutsideVolume { .. } => AboutUnavailable::PathOutsideVolume,
        LocationMiss::NoVolumeAtPath { .. } => AboutUnavailable::NoVolumeAtPath,
    })?;
    let path = resolved.location.path.clone();
    let volume_name = ItemText::new(&resolved.name);
    match reach(resolved.kind, resolved.live) {
        Reach::Local => {
            let probed = probe_on_pool(path.clone()).await?;
            let home = dirs::home_dir().map(|home| home.to_string_lossy().into_owned());
            let sources = Sources {
                history: History::of_the_app(&resolved.location.volume_id),
                index: Some(IndexPlace {
                    index_id: resolved.index.volume_id.clone(),
                    path: path.clone(),
                    importance: to_store_path(&path, &resolved.root)
                        .map(|store_path| (resolved.location.volume_id.clone(), store_path)),
                    data_dir: crate::config::standalone_app_data_dir(),
                }),
                // No home folder (a sandboxed test runner) names no Library places.
                owner: home.map_or(OwnerQuestion::Nothing, |home| OwnerQuestion::of(&path, &home)),
                apps: AppSource::of_the_os(),
            };
            Ok(local_gathered(&path, probed, volume_name, platform::os_caps(), sources))
        }
        Reach::ThroughTheVolume => {
            let volume = crate::file_system::volume::manager::get_volume_manager()
                .get(&resolved.location.volume_id)
                .ok_or(AboutUnavailable::NoVolumeAtPath)?;
            let metadata = tokio::time::timeout(REMOTE_BUDGET, volume.get_metadata(Path::new(&path)))
                .await
                .map_err(|_| AboutUnavailable::Unreachable)?
                .map_err(|error| unavailable(&error))?;
            Ok(remote_gathered(&metadata, volume_name))
        }
    }
}

fn unavailable(error: &VolumeError) -> AboutUnavailable {
    match error {
        VolumeError::NotFound(_) => AboutUnavailable::NotFound,
        VolumeError::PermissionDenied { .. } => AboutUnavailable::PermissionDenied,
        _ => AboutUnavailable::Unreadable,
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
/// An archive entry, and an item whose mount isn't a local disk after all (a
/// symlink in its parent chain leads to a share), answer identity only: nothing
/// reads their bytes, xattrs, or frameworks from here.
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
    let reads_locally = probed.on_local_disk && probed.target != AboutTarget::ArchiveEntry;
    let caps = if reads_locally {
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
    });
    plan.retain_kinds(|kind| {
        GATHERED.contains(&kind)
            && match kind {
                FactKind::CmdrOperation => sources.history.is_some(),
                FactKind::RecursiveSize => sources.index.is_some(),
                FactKind::Importance => sources.index.as_ref().is_some_and(IndexPlace::scores),
                _ => true,
            }
    });
    let jobs = if reads_locally {
        local_jobs(&plan, path, &probed, &sources)
    } else {
        Vec::new()
    };
    Gathered {
        plan,
        ready: vec![AboutFact::Identity(probed.identity)],
        jobs,
    }
}

/// The plan for an item only its volume can describe: identity, from the
/// metadata it answered. A symlink there reads as what the volume resolved it to.
pub(crate) fn remote_gathered(entry: &FileEntry, volume_name: ItemText) -> Gathered {
    let target = match (entry.is_directory, entry.is_package) {
        (true, true) if entry.name.to_ascii_lowercase().ends_with(".app") => AboutTarget::AppBundle,
        (true, true) => AboutTarget::Package,
        (true, false) => AboutTarget::Folder,
        (false, _) => AboutTarget::File,
    };
    let extension = (target != AboutTarget::Folder)
        .then(|| {
            Path::new(&entry.name)
                .extension()
                .map(|e| e.to_string_lossy().into_owned())
        })
        .flatten();
    let caps = VolumeCaps::identity_only(volume_name);
    let mut plan: AboutPlan = plan(PlanInput {
        target,
        extension: extension.as_deref(),
        is_executable: false,
        contents_local: false,
        caps: &caps,
        os: &platform::os_caps(),
        // Nothing reads the OS or a repo through another volume yet.
        owner: &OwnerQuestion::Nothing,
        in_a_repo: false,
    });
    plan.retain_kinds(|kind| GATHERED.contains(&kind));
    let seconds = |at: Option<u64>| at.and_then(|at| i64::try_from(at).ok());
    let is_dir = entry.is_directory;
    let identity = Identity {
        name: ItemText::new(&entry.name),
        logical_size: if is_dir { None } else { entry.size },
        physical_size: if is_dir { None } else { entry.physical_size },
        created: seconds(entry.created_at),
        modified: seconds(entry.modified_at),
        // No remote protocol Cmdr speaks counts other names for a node.
        hard_links: None,
        symlink_target: None,
    };
    Gathered {
        plan,
        ready: vec![AboutFact::Identity(Fact::Known(identity))],
        jobs: Vec::new(),
    }
}
