//! What a group's sources look like on disk, and what would stop a group from running.
//!
//! Two reads of the same thing at two moments: the CREATION SNAPSHOT a group freezes when it's
//! proposed, and the LIVE stat the review dialog shows beside it. Both go through the group's
//! SOURCE volume, so a file on a share is asked of the share, never of a guess about its mount.
//!
//! Every stat is bounded twice: by one shared [`Deadline`] per call (a dead mount costs the
//! caller one budget, not one per file), and by [`SOURCE_STATS`], which caps how many stats are
//! in flight across every caller. The dialog re-asks on scroll faster than a slow share answers,
//! and a stat that outlives its deadline keeps its permit until it really ends, so stragglers on
//! a dead mount can't pile up pool threads behind the user's back.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::deadline::{BlockingBudget, Deadline, io_budget, timeout_detached_within};
use cmdr_fs::log_error::LogError;
use futures_util::StreamExt;

use crate::agent::store::proposals::OpSnapshot;
use crate::agent::types::ProposalVerb;
use crate::file_system::volume::{Volume, VolumeError};

/// How long one batch of stats may take on a local disk or a kernel mount. A volume with a live
/// session gets [`io_budget`]'s longer wait instead, since its transport tells slow from dead.
pub const SOURCE_STAT_TIMEOUT: Duration = Duration::from_secs(2);

/// How many stats one call keeps in flight. Small on purpose: a share answers a handful of
/// concurrent stats no slower than one, and a page is 50-ish rows.
const STATS_PER_CALL: usize = 8;

/// Every source stat this module runs, from every caller (the proposal's snapshot and the
/// dialog's live facts contend for the same volumes). Sized to what a NAS can usefully serve.
static SOURCE_STATS: BlockingBudget = BlockingBudget::new(8);

/// One source as its volume answers for it right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStat {
    /// It's there. `size` is `None` for a folder, whose own stat size means nothing to a user.
    Present {
        size: Option<u64>,
        /// Unix seconds.
        modified: Option<i64>,
        inode: Option<u64>,
    },
    /// The volume says there's nothing at that path.
    Missing,
    /// The volume didn't answer in time, or refused in a way that isn't "not there".
    Unknown,
}

impl SourceStat {
    /// The creation snapshot this stat freezes into, or `None` when there was nothing to
    /// freeze. A missing or unanswered stat stays `NULL`, ❌ never a zero.
    pub fn snapshot(self) -> Option<OpSnapshot> {
        match self {
            SourceStat::Present { size, modified, inode } => Some(OpSnapshot {
                size,
                mtime: modified,
                inode,
            }),
            SourceStat::Missing | SourceStat::Unknown => None,
        }
    }
}

/// The deadline a batch of stats on `volume` shares.
pub fn stat_deadline(volume: &dyn Volume) -> Deadline {
    Deadline::new(io_budget(volume.connection_state(), SOURCE_STAT_TIMEOUT))
}

/// Stat every path on `volume` within `deadline`, answering in the order asked.
pub async fn stat_sources(volume: Arc<dyn Volume>, paths: Vec<String>, deadline: &Deadline) -> Vec<SourceStat> {
    futures_util::stream::iter(paths)
        .map(|path| stat_one(volume.clone(), path, deadline))
        .buffered(STATS_PER_CALL)
        .collect()
        .await
}

/// Why a stat gave no answer. Only ever logged.
#[derive(Debug)]
enum StatGap {
    TimedOut,
    Panicked(String),
}

async fn stat_one(volume: Arc<dyn Volume>, path: String, deadline: &Deadline) -> SourceStat {
    // A stat that only got its permit after the caller stopped waiting is work for nobody, so it
    // doesn't start. The instant is fixed here because the detached task outlives `deadline`.
    let give_up_at = tokio::time::Instant::now() + deadline.remaining();
    let runtime = tokio::runtime::Handle::current();
    let answer = timeout_detached_within(deadline, || StatGap::TimedOut, StatGap::Panicked, async move {
        SOURCE_STATS
            .run(move || {
                if tokio::time::Instant::now() >= give_up_at {
                    return SourceStat::Unknown;
                }
                // On a blocking thread, holding the budget's permit until the stat really
                // ends: that's what keeps a dead mount's stragglers counted.
                runtime.block_on(async move { classify(volume.get_metadata(Path::new(&path)).await) })
            })
            .await
            .map_err(|e| StatGap::Panicked(e.to_string()))
    })
    .await;
    match answer {
        Ok(stat) => stat,
        Err(StatGap::TimedOut) => {
            log::debug!(
                target: "agent::suggested_ops",
                "a source stat didn't answer within {:?}",
                deadline.total()
            );
            SourceStat::Unknown
        }
        Err(StatGap::Panicked(detail)) => {
            log::warn!(target: "agent::suggested_ops", "a source stat panicked: {detail}");
            SourceStat::Unknown
        }
    }
}

fn classify(answer: Result<cmdr_fs::entry::FileEntry, VolumeError>) -> SourceStat {
    match answer {
        Ok(entry) => SourceStat::Present {
            size: if entry.is_directory { None } else { entry.size },
            modified: entry.modified_at.map(|at| at as i64),
            inode: entry.inode,
        },
        Err(VolumeError::NotFound(_)) => SourceStat::Missing,
        Err(e) => {
            log::debug!(target: "agent::suggested_ops", "a source stat was refused: {:?}", LogError(&e));
            SourceStat::Unknown
        }
    }
}

/// What an approved group would run as when that differs from its verb, known before the user
/// approves it.
///
/// A fact to DISCLOSE before the click. The review shows the action that would really run (its
/// button, its undo note), and the approval names the fallback the user saw, so the bridge runs
/// it only when that's what the user approved (`bridge::start_for`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RunsAs {
    /// A trash group whose source volume keeps no Trash (`Volume::has_trash`). Trashing there
    /// can't work: the OS refuses every item (`TrashRefusalKind::NoTrashForVolume`) and nothing
    /// happens. So an approval the user gave against a red "Delete" button deletes the files
    /// permanently (David's call, 2026-10-28): it's a user-approved action, the same answer the
    /// delete dialog gives a user on a drive with no Trash.
    PermanentDelete,
}

/// What a group of `verb` on `source` would run as when that isn't its verb, or `None` when it
/// runs as itself. Pure, so the rule is pinned by tests; [`source_runs_as`] is the registry
/// lookup both the dialog and the agent's own read go through.
///
/// An unregistered source (`None`) runs as itself: approving it already refuses with its own
/// typed reason (`SourceNotConnected`), which the dialog words.
pub fn runs_as(verb: ProposalVerb, source: Option<&dyn Volume>) -> Option<RunsAs> {
    match (verb, source) {
        (ProposalVerb::Trash, Some(volume)) if !volume.has_trash() => Some(RunsAs::PermanentDelete),
        _ => None,
    }
}

/// [`runs_as`] for a stored group, against the volume registered under its source id.
/// No I/O: a registry lookup and a predicate, so every read can ask it per group.
pub fn source_runs_as(verb: ProposalVerb, source_volume_id: &str) -> Option<RunsAs> {
    let source = crate::file_system::volume::manager::get_volume_manager().get(source_volume_id);
    runs_as(verb, source.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmdr_fs::volume::InMemoryVolume;

    /// A volume that keeps a Trash, as `LocalPosixVolume` on a local disk does.
    struct LocalDisk(InMemoryVolume);

    impl Volume for LocalDisk {
        fn name(&self) -> &str {
            self.0.name()
        }
        fn root(&self) -> &Path {
            self.0.root()
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
        fn list_directory<'a>(
            &'a self,
            path: &'a Path,
            on_progress: Option<&'a (dyn Fn(cmdr_fs::volume::ListingProgress) + Sync)>,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<Vec<cmdr_fs::entry::FileEntry>, VolumeError>> + Send + 'a>>
        {
            self.0.list_directory(path, on_progress)
        }
        fn get_metadata<'a>(
            &'a self,
            path: &'a Path,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<cmdr_fs::entry::FileEntry, VolumeError>> + Send + 'a>>
        {
            self.0.get_metadata(path)
        }
        fn has_trash(&self) -> bool {
            true
        }
    }

    /// Trash on a volume with no Trash can't trash anything: the OS refuses each item and
    /// deletes nothing (QA on an SMB Downloads cleanup, #747). So the review offers it as the
    /// permanent delete it would have to be.
    #[test]
    fn a_trash_group_on_a_volume_without_a_trash_runs_as_a_permanent_delete() {
        let share = InMemoryVolume::new("naspi");
        let disk = LocalDisk(InMemoryVolume::new("Macintosh HD"));

        assert_eq!(
            runs_as(ProposalVerb::Trash, Some(&share)),
            Some(RunsAs::PermanentDelete)
        );
        assert_eq!(runs_as(ProposalVerb::Trash, Some(&disk)), None);
        assert_eq!(
            runs_as(ProposalVerb::Trash, None),
            None,
            "an unconnected source refuses with its own typed reason at approval"
        );
    }

    /// Only trash depends on a Trash: every other verb runs the same on either volume.
    #[test]
    fn other_verbs_run_as_themselves() {
        let share = InMemoryVolume::new("naspi");
        for verb in [
            ProposalVerb::Move,
            ProposalVerb::Copy,
            ProposalVerb::Delete,
            ProposalVerb::Rename,
            ProposalVerb::Compress,
            ProposalVerb::Extract,
            ProposalVerb::AddServer,
            ProposalVerb::EditServer,
        ] {
            assert_eq!(runs_as(verb, Some(&share)), None, "{verb:?}");
        }
    }

    /// Present files carry their facts, a missing one says so, and a stat the volume couldn't
    /// answer is unknown: three answers the dialog words differently, so none may collapse.
    #[tokio::test(flavor = "multi_thread")]
    async fn each_source_gets_the_answer_its_volume_gives() {
        let volume = InMemoryVolume::new("Test");
        volume
            .create_file(Path::new("/here.txt"), b"twelve bytes")
            .await
            .expect("seed file");
        volume.set_modified_at(Path::new("/here.txt"), Some(1_700_000_000));
        volume
            .create_file(Path::new("/unanswerable.txt"), b"x")
            .await
            .expect("seed file");
        volume.set_stat_failing(Path::new("/unanswerable.txt"));
        let volume: Arc<dyn Volume> = Arc::new(volume);

        let deadline = stat_deadline(volume.as_ref());
        let stats = stat_sources(
            volume,
            vec!["/here.txt".into(), "/gone.txt".into(), "/unanswerable.txt".into()],
            &deadline,
        )
        .await;

        assert!(
            matches!(
                stats[0],
                SourceStat::Present {
                    size: Some(12),
                    modified: Some(1_700_000_000),
                    ..
                }
            ),
            "{stats:?}"
        );
        assert_eq!(stats[1], SourceStat::Missing);
        assert_eq!(stats[2], SourceStat::Unknown);
        assert_eq!(
            stats[1].snapshot(),
            None,
            "a missing file freezes as NULL, never a zero"
        );
    }

    /// A spent deadline answers at once rather than queuing work nobody will read.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_spent_deadline_answers_unknown_without_asking() {
        let volume = InMemoryVolume::new("Test");
        volume.create_file(Path::new("/a.txt"), b"a").await.expect("seed file");
        let volume: Arc<dyn Volume> = Arc::new(volume);

        let stats = stat_sources(volume, vec!["/a.txt".into()], &Deadline::new(Duration::ZERO)).await;

        assert_eq!(stats, vec![SourceStat::Unknown]);
    }
}
