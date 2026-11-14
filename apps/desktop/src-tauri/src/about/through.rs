//! The rows of an item Cmdr reaches through its volume: a share, a server, a
//! phone, an archive's entry, or a snapshot from git's history.
//!
//! ❗ Every call against the volume is its OWN `Volume` method (the same
//! deadline-aware calls a listing makes), run to its end on an about-pool thread
//! with `Handle::block_on`. The card waits for it only as long as its budget; the
//! call itself is never dropped half-way (an abandoned MTP transaction wedges the
//! phone, `cmdr_fs::deadline` § must-knows), and the thread it holds is the
//! pool's, ❌ never an executor worker's. [`Stuck`] keeps a volume that stopped
//! answering from taking more than [`MAX_STUCK_PER_VOLUME`] of them.
//!
//! The index rows (a folder's size, what's inside, its importance) read
//! databases only, so they answer even when the volume doesn't.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use cmdr_about::card::{AboutFact, AboutPlan};
use cmdr_about::fact::{Fact, FactKind, UnknownWhy};
use cmdr_about::gather::Job;
use cmdr_about::payload::{GitSnapshot, Identity};
use cmdr_about::provenance::{Download, MAX_XATTR_BYTES};
use cmdr_about::text::ItemText;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{Volume, VolumeError, XattrReadMode};

use super::digest::folder_digest_fact;
use super::index::{IndexPlace, importance_fact, recursive_size_fact};
use super::platform;
use super::session::SubJobs;
use crate::file_system::framework_pool::note_activity;
use crate::ignore_poison::IgnorePoison;

/// How many calls one volume may have out at once from cards. A call that never
/// returns keeps its thread; past this many the volume counts as not answering,
/// and the next card says so at once instead of stranding another thread.
pub(crate) const MAX_STUCK_PER_VOLUME: usize = 2;

/// Calls out per volume id, counted from their start to their real end.
#[derive(Default)]
pub(crate) struct Stuck(Mutex<HashMap<String, usize>>);

pub(crate) static STUCK: LazyLock<Stuck> = LazyLock::new(Stuck::default);

impl Stuck {
    /// Whether `volume_id` already holds as many calls as a card may leave out.
    pub(crate) fn is_full(&self, volume_id: &str) -> bool {
        self.0.lock_ignore_poison().get(volume_id).copied().unwrap_or(0) >= MAX_STUCK_PER_VOLUME
    }

    /// Counts one call on `volume_id` until the guard drops, which is when the
    /// call really returned.
    fn hold(&'static self, volume_id: &str) -> StuckGuard {
        *self.0.lock_ignore_poison().entry(volume_id.to_string()).or_insert(0) += 1;
        StuckGuard {
            stuck: self,
            volume_id: volume_id.to_string(),
        }
    }
}

struct StuckGuard {
    stuck: &'static Stuck,
    volume_id: String,
}

impl Drop for StuckGuard {
    fn drop(&mut self) {
        let mut calls = self.stuck.0.lock_ignore_poison();
        if let Some(count) = calls.get_mut(&self.volume_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                calls.remove(&self.volume_id);
            }
        }
    }
}

/// A volume to ask, and the runtime its calls run on.
#[derive(Clone)]
pub(crate) struct Asked {
    /// The volume that serves the path (an archive's or git's view where a route
    /// swapped one in).
    pub(crate) volume: Arc<dyn Volume>,
    /// Which volume's calls [`Stuck`] counts: the drive under any view.
    pub(crate) stuck_id: String,
    pub(crate) runtime: tokio::runtime::Handle,
}

impl Asked {
    /// Runs one of the volume's own calls to its end on this thread.
    fn ask<T>(&self, call: impl Future<Output = T>) -> T {
        let _held = STUCK.hold(&self.stuck_id);
        self.runtime.block_on(call)
    }
}

/// Where an item's download record comes from, through the volume.
#[derive(Clone)]
pub(crate) enum DownloadFrom {
    /// The item's own xattrs, through its volume (an OS-mounted share).
    Volume,
    /// An archive's entry: the archive's record, named.
    Archive { archive: ItemText, record: ArchiveRecord },
}

/// Where an archive's own record is read.
#[derive(Clone)]
pub(crate) enum ArchiveRecord {
    /// The archive sits on a local disk: read its xattrs here.
    Local { archive_path: String },
    /// The archive sits on a volume that reads xattrs itself.
    Volume { volume: Asked, archive_path: PathBuf },
    /// The archive sits where no record can be read.
    None,
}

/// What a card reaching through a volume reads.
#[derive(Clone)]
pub(crate) struct Through {
    pub(crate) asked: Asked,
    pub(crate) path: PathBuf,
    /// The item's download row's source, when the plan has one.
    pub(crate) download: Option<DownloadFrom>,
    /// The item's place in the drive index, when the index covers its volume.
    pub(crate) index: Option<IndexPlace>,
    /// The snapshot a `.git` portal path names, as read off the path.
    pub(crate) snapshot: Option<SnapshotPath>,
}

/// What a `.git` portal path names, read off the path alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SnapshotPath {
    pub(crate) repo_root: PathBuf,
    pub(crate) kind: cmdr_about::payload::SnapshotKind,
    /// The path's components after the category: the ref's name, then the path
    /// inside its snapshot. Where one ends and the other starts is the portal's
    /// to say for a branch or tag (`feature/foo` holds a slash).
    pub(crate) after: Vec<String>,
}

impl SnapshotPath {
    /// The ref's name. A commit id or a stash entry is one component; a branch's
    /// or tag's is the shortest run of components the portal knows as a ref
    /// (git forbids a ref whose name is another ref's folder, so the shortest is
    /// the only one).
    fn ref_name(&self, asked: &Asked) -> Option<String> {
        use cmdr_about::payload::SnapshotKind;
        match self.kind {
            SnapshotKind::Commit | SnapshotKind::Stash => self.after.first().cloned(),
            SnapshotKind::Branch | SnapshotKind::Tag => {
                let category = self.repo_root.join(".git").join(category_segment(self.kind));
                (1..=self.after.len()).find_map(|taken| {
                    let name = self.after[..taken].join("/");
                    let candidate = category.join(&name);
                    asked.ask(asked.volume.get_metadata(&candidate)).ok().map(|_| name)
                })
            }
        }
    }
}

/// The portal's word for a category (`cmdr_git`'s `Cat::as_segment`, pinned by a
/// test against a real portal).
pub(crate) fn category_segment(kind: cmdr_about::payload::SnapshotKind) -> &'static str {
    use cmdr_about::payload::SnapshotKind;
    match kind {
        SnapshotKind::Branch => "branches",
        SnapshotKind::Tag => "tags",
        SnapshotKind::Commit => "commits",
        SnapshotKind::Stash => "stash",
    }
}

/// The kinds [`jobs`] answers for `through`: a planned row outside it has no job,
/// so the route trims it from the plan.
pub(crate) fn answers(through: &Through, kind: FactKind) -> bool {
    match kind {
        FactKind::Identity => true,
        FactKind::Download => through.download.is_some(),
        FactKind::RecursiveSize | FactKind::FolderDigest => through.index.is_some(),
        FactKind::Importance => through.index.as_ref().is_some_and(IndexPlace::scores),
        FactKind::GitSnapshot => through.snapshot.is_some(),
        FactKind::RealType
        | FactKind::Bundle
        | FactKind::Signature
        | FactKind::CmdrOperation
        | FactKind::LastOpened
        | FactKind::DefaultApp
        | FactKind::Owner
        | FactKind::Git
        | FactKind::AppData => false,
    }
}

/// Whether a row asks the volume (rather than a database). When the volume is
/// known not to answer, those rows land at once and only the rest run.
pub(crate) fn asks_the_volume(kind: FactKind) -> bool {
    matches!(kind, FactKind::Identity | FactKind::Download | FactKind::GitSnapshot)
}

/// One job per planned row. With `volume_answers` false (it's holding calls that
/// never came back), the rows that ask it are left out: the route lands them.
/// `sub_jobs`: where a row's own pool work goes, under the card's cancel.
pub(crate) fn jobs(
    plan: &AboutPlan,
    through: &Through,
    volume_answers: bool,
    sub_jobs: &SubJobs,
) -> Vec<(FactKind, Job<AboutFact>)> {
    plan.kinds()
        .into_iter()
        .filter(|kind| volume_answers || !asks_the_volume(*kind))
        .filter_map(|kind| job(kind, through.clone(), sub_jobs.clone()).map(|job| (kind, job)))
        .collect()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn job(kind: FactKind, through: Through, sub_jobs: SubJobs) -> Option<Job<AboutFact>> {
    let job: Job<AboutFact> = match kind {
        FactKind::Identity => Box::new(move || {
            note_activity("the volume's metadata", "identity");
            let answer = through.asked.ask(through.asked.volume.get_metadata(&through.path));
            AboutFact::Identity(identity_answer(answer))
        }),
        FactKind::Download => {
            let from = through.download.clone()?;
            Box::new(move || {
                note_activity("the volume's xattrs", "download");
                AboutFact::Download(match from {
                    DownloadFrom::Volume => download_through(&through.asked, &through.path),
                    DownloadFrom::Archive { archive, record } => {
                        let record = match record {
                            ArchiveRecord::Local { archive_path } => platform::download(&archive_path),
                            ArchiveRecord::Volume { volume, archive_path } => download_through(&volume, &archive_path),
                            ArchiveRecord::None => Fact::unknown(UnknownWhy::VolumeCantTell),
                        };
                        Download::of_archive(archive, record)
                    }
                })
            })
        }
        FactKind::GitSnapshot => {
            let snapshot = through.snapshot.clone()?;
            Box::new(move || {
                note_activity("the portal's metadata", "gitSnapshot");
                let entry = through.asked.ask(through.asked.volume.get_metadata(&through.path));
                AboutFact::GitSnapshot(match entry {
                    Err(error) => Fact::unknown(why_of(&error)),
                    Ok(entry) => match snapshot.ref_name(&through.asked) {
                        Some(name) => Fact::Known(GitSnapshot {
                            repo_root: ItemText::new(&snapshot.repo_root.to_string_lossy()),
                            kind: snapshot.kind,
                            name: ItemText::new(&name),
                            last_changed: entry.modified_at.and_then(|at| i64::try_from(at).ok()),
                        }),
                        // The item answered and no ref around it did: the portal
                        // moved under the card.
                        None => Fact::unknown(UnknownWhy::NotFound),
                    },
                })
            })
        }
        FactKind::RecursiveSize => {
            let place = through.index?;
            Box::new(move || {
                note_activity("read the index's folder size", "recursiveSize");
                AboutFact::RecursiveSize(recursive_size_fact(&place))
            })
        }
        FactKind::FolderDigest => {
            let place = through.index?;
            Box::new(move || {
                note_activity("read the index's folder listing", "folderDigest");
                AboutFact::FolderDigest(folder_digest_fact(&place, now_secs(), &sub_jobs))
            })
        }
        FactKind::Importance => {
            let place = through.index.filter(IndexPlace::scores)?;
            Box::new(move || {
                note_activity("read the importance store", "importance");
                AboutFact::Importance(importance_fact(&place, now_secs()))
            })
        }
        FactKind::RealType
        | FactKind::Bundle
        | FactKind::Signature
        | FactKind::CmdrOperation
        | FactKind::LastOpened
        | FactKind::DefaultApp
        | FactKind::Owner
        | FactKind::Git
        | FactKind::AppData => return None,
    };
    Some(job)
}

/// The OS's download record, read through `asked`'s own xattr call, capped as a
/// local read is: a value past the cap reaches the decoder as an oversized
/// buffer, which it reads as unparseable.
fn download_through(asked: &Asked, path: &Path) -> Fact<Download> {
    let paths = [path.to_path_buf()];
    let mut failed = None;
    let fact = platform::download_record(&mut |name| {
        let answers = asked.ask(
            asked
                .volume
                .read_xattr_many(&paths, name, XattrReadMode::AssumePathsExist),
        );
        match answers.into_iter().next() {
            Some(Ok(Some(value))) if value.len() > MAX_XATTR_BYTES => Some(vec![0; MAX_XATTR_BYTES + 1]),
            Some(Ok(Some(value))) => Some(value),
            Some(Ok(None)) => None,
            Some(Err(error)) => {
                failed.get_or_insert(why_of(&error));
                None
            }
            None => None,
        }
    });
    // A read that failed isn't "no record": say why, unless the other one answered.
    match (failed, &fact) {
        (Some(why), Fact::Unknown(_)) => Fact::unknown(why),
        _ => fact,
    }
}

/// The identity row from the volume's metadata answer.
pub(crate) fn identity_answer(answer: Result<FileEntry, VolumeError>) -> Fact<Identity> {
    match answer {
        Ok(entry) => Fact::Known(identity_of(&entry)),
        Err(error) => Fact::unknown(why_of(&error)),
    }
}

/// What an item's metadata says, as the identity row. A symlink reads as what the
/// volume resolved it to, since no remote metadata carries a link target.
pub(crate) fn identity_of(entry: &FileEntry) -> Identity {
    let seconds = |at: Option<u64>| at.and_then(|at| i64::try_from(at).ok());
    let is_dir = entry.is_directory;
    Identity {
        name: ItemText::new(&entry.name),
        name_hid: cmdr_about::text::hidden_in_name(&entry.name),
        logical_size: if is_dir { None } else { entry.size },
        physical_size: if is_dir { None } else { entry.physical_size },
        created: seconds(entry.created_at),
        modified: seconds(entry.modified_at),
        // No remote protocol Cmdr speaks counts other names for a node.
        hard_links: None,
        symlink_target: None,
    }
}

/// Why a volume call came back without an answer. ❌ Never by its message.
fn why_of(error: &VolumeError) -> UnknownWhy {
    match error {
        VolumeError::NotFound(_) | VolumeError::NotADirectory(_) => UnknownWhy::NotFound,
        VolumeError::PermissionDenied { .. } => UnknownWhy::PermissionDenied,
        VolumeError::DeviceDisconnected(_)
        | VolumeError::NotConnected(_)
        | VolumeError::ConnectionTimeout(_)
        | VolumeError::DeviceSessionReset(_) => UnknownWhy::Unreachable,
        VolumeError::NotSupported => UnknownWhy::VolumeCantTell,
        _ => UnknownWhy::Unparseable,
    }
}
