//! Who held the drive when an unmount was refused.
//!
//! A refusal that names nobody leaves a person with nothing to do about it, so after
//! the LAST attempt (`run_teardown`) one scan asks the kernel which processes have the
//! drive open, over every mount of the teardown that's still in the table.
//!
//! **The scan is bounded and abandonable.** `proc_listpidspath` `stat`s its path before
//! it walks the process list, and that `stat` took 142 ms to 9.4 s under load and can
//! hang for good on a wedged mount, so the walk runs on ONE plain std thread nobody
//! joins: past [`HOLDER_BUDGET`](super::deadlines::HOLDER_BUDGET) the answer is
//! "couldn't tell" and the thread is left to end on its own. ❌ Never the blocking pool:
//! a wedged scan would hold one of its threads for as long as the mount stays wedged.
//!
//! **A scan that couldn't run names nobody, which is ❌ never "nobody is holding it"**
//! ([`HolderScan`]) — the same three-answers rule as `disk_target::DiskMounts`.
//!
//! **Two stages, one budget.** The walk names each holder by its executable; then
//! [`facts`] says what KIND of holder it is, which is what picks the sentence the refusal
//! reads. Both run on the one abandonable thread, and the facts run SECOND for two
//! reasons: rule 5 needs the device of every mount of the teardown, not just the one
//! being walked, and a budget that runs out mid-facts then leaves every holder named and
//! [`HolderKind::Unclassified`] rather than dropping the ones it never reached.
//!
//! **Then one more question, inside the same budget**: the walk sees same-uid processes
//! only, and an image Finder opened is served by a `diskimagesiod` under its own uid. So
//! the attached images stored on the drive that the walk didn't name join the answer as
//! holders of their own ([`facts::images_the_walk_missed`]). Their kind comes from the
//! image list rather than from the walk, which is what lets "eject that image first" beat
//! "wait a minute" for a holder this scan could otherwise never see.

#[cfg(all(test, target_os = "macos"))]
mod detached_holder;
mod facts;
mod scan;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::ignore_poison::IgnorePoison;

/// One process that held the drive when the unmount was refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VolumeHolder {
    /// The process id, as the scan saw it.
    pub pid: u32,
    /// What to call it: an app's display name, a disk image's volume name, or the
    /// executable's own name (the last only for the log and MCP).
    pub name: String,
    /// The bundle id, when an app names this holder.
    pub bundle_id: Option<String>,
    /// What kind of holder it is, which is what picks the words.
    pub kind: HolderKind,
}

/// What kind of thing is holding the drive. It picks which sentence the refusal says,
/// so each variant is a decision, ❌ never a guess from a name or a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum HolderKind {
    /// An app the person can switch to and close.
    App,
    /// A command-line tool or a helper with no app behind it.
    Tool,
    /// A disk image stored on this drive, which has to be ejected first.
    DiskImage,
    /// macOS itself (Spotlight, LaunchServices, Time Machine).
    System,
    /// Cmdr, which is a bug worth reporting.
    Cmdr,
    /// Named, but nothing said what kind it is.
    Unclassified,
}

/// Who held the drive when the last attempt was refused.
///
/// ❗ TWO answers, ❌ never one list. A scan that couldn't run, ran out of its budget,
/// or found the mount replaced under it names nobody, and reading that as "nobody is
/// holding it" would word a plainly-held drive as free. Only [`Self::Complete`] with an
/// empty list means the scan really found nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum HolderScan {
    /// Every still-listed mount of the teardown was scanned, so `named` is the whole
    /// story. Empty means no same-uid process held the drive and no disk image is stored
    /// on it (any other holder under another uid is invisible to this scan).
    Complete {
        /// The holders, deduped by pid.
        named: Vec<VolumeHolder>,
    },
    /// The scan didn't cover every mount: it had nothing to scan, ran past its budget,
    /// or a mount's device changed under it. `named` is what it did see, ❌ never the
    /// whole story, and an empty one is ❌ never "nobody is holding it".
    Incomplete {
        /// The holders it managed to see, deduped by pid.
        named: Vec<VolumeHolder>,
    },
}

impl HolderScan {
    /// The answer before anything has scanned: nobody named, and ❌ not "nobody is
    /// holding it". What `unmount_tool::settle` builds a refusal with, until
    /// `run_teardown` runs the one scan.
    pub(super) fn not_scanned() -> Self {
        Self::Incomplete { named: Vec::new() }
    }

    /// Everyone the scan could name, however far it got.
    pub(super) fn named(&self) -> &[VolumeHolder] {
        match self {
            Self::Complete { named } | Self::Incomplete { named } => named,
        }
    }
}

impl std::fmt::Display for HolderScan {
    /// ❗ For logs and MCP replies only; the toast's words come from the typed value.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.named().is_empty() {
            return f.write_str(match self {
                Self::Complete { .. } => "held by nobody a same-uid scan can see",
                Self::Incomplete { .. } => "held by nobody anything could name",
            });
        }
        f.write_str("held by ")?;
        for (index, holder) in self.named().iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{} [{}, pid {}]", holder.name, holder.kind, holder.pid)?;
        }
        match self {
            Self::Complete { .. } => Ok(()),
            Self::Incomplete { .. } => f.write_str(", and maybe more the scan couldn't cover"),
        }
    }
}

impl std::fmt::Display for HolderKind {
    /// ❗ For logs and MCP replies only.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::App => "app",
            Self::Tool => "tool",
            Self::DiskImage => "disk image",
            Self::System => "system",
            Self::Cmdr => "Cmdr",
            Self::Unclassified => "unclassified",
        })
    }
}

/// What one path's scan found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PathScan {
    /// Every process the scan saw holding this path.
    Named(Vec<VolumeHolder>),
    /// The scan couldn't run here, or the mount changed under it, so what it saw says
    /// nothing at all.
    Unreadable,
}

impl PathScan {
    /// Whoever this path's walk named, which is nobody when it couldn't be walked.
    fn named(&self) -> &[VolumeHolder] {
        match self {
            Self::Named(holders) => holders,
            Self::Unreadable => &[],
        }
    }
}

/// Who holds `paths`, within `budget`.
///
/// `paths` are the teardown's mounts that are still in the table; an empty list is
/// [`HolderScan::Incomplete`], since "we had nothing to ask about" is ❌ not "nobody is
/// holding it".
pub(super) async fn scan(paths: Vec<PathBuf>, budget: Duration) -> HolderScan {
    let expected = paths.len();
    if expected == 0 {
        return HolderScan::not_scanned();
    }
    let found = Arc::new(Mutex::new(Vec::with_capacity(expected)));
    let kinds = Arc::new(Mutex::new(HashMap::new()));
    let missed = Arc::new(Mutex::new(Vec::new()));
    let (filled, named, added) = (Arc::clone(&found), Arc::clone(&kinds), Arc::clone(&missed));
    // ❗ A plain `Instant`, ❌ not tokio's: this deadline is read on a std thread, which a
    // paused test clock races straight past.
    let deadline = Instant::now() + budget;
    let ran = within_budget(budget, move || {
        for path in &paths {
            let one = scan::scan_path(path);
            filled.lock_ignore_poison().push(one);
        }
        // Held only to copy the pids out: the facts below take their time, and the async
        // side reads what's landed the moment the budget runs out.
        let mut holders: Vec<u32> = Vec::new();
        for holder in filled.lock_ignore_poison().iter().flat_map(PathScan::named) {
            // One process holding two of a disk's volumes is one holder, and asking
            // Security about it twice would spend the budget twice over.
            if !holders.contains(&holder.pid) {
                holders.push(holder.pid);
            }
        }
        let about = facts::Surroundings::new(facts::target_devices(&paths));
        facts::name_the_kinds(&holders, &about, deadline, |pid, what| {
            named.lock_ignore_poison().insert(pid, what);
        });
        added
            .lock_ignore_poison()
            .extend(facts::images_the_walk_missed(&holders, &about, deadline));
    })
    .await;
    if !ran {
        log::warn!(
            target: "eject",
            "The holder scan didn't finish within {} ms, so who holds the drive stays unknown",
            budget.as_millis()
        );
    }
    let found = found.lock_ignore_poison();
    let mut scanned = merge(&found, expected);
    apply_kinds(&mut scanned, &kinds.lock_ignore_poison());
    add_holders(&mut scanned, &missed.lock_ignore_poison());
    scanned
}

/// What kind of holder `pid` is, for a drive whose mounts are `paths`.
///
/// ❗ The real-image lane's one seam past [`scan`]. Everything else it pins runs through
/// the production path; this doesn't, because the walk is what a lane can't aim at a
/// single pid: `PATH_IS_VOLUME` on the volume a `.dmg` sits on names every process on it.
#[cfg(all(test, target_os = "macos"))]
pub(super) fn kind_of(pid: u32, paths: &[PathBuf]) -> HolderKind {
    let mut kind = HolderKind::Unclassified;
    let about = facts::Surroundings::new(facts::target_devices(paths));
    facts::name_the_kinds(&[pid], &about, Instant::now() + Duration::from_secs(20), |_, what| {
        kind = what.kind
    });
    kind
}

/// Puts what the facts found onto each holder the walk named.
///
/// Pure. A pid with no entry keeps its executable name and stays `Unclassified`: the
/// budget ran out before the facts reached it, and that's ❌ never a reason to drop it.
fn apply_kinds(scanned: &mut HolderScan, kinds: &HashMap<u32, facts::Classified>) {
    let named = match scanned {
        HolderScan::Complete { named } | HolderScan::Incomplete { named } => named,
    };
    for holder in named.iter_mut() {
        if let Some(what) = kinds.get(&holder.pid) {
            facts::rename(holder, what);
        }
    }
}

/// Adds holders the walk couldn't see to what it did, after everything it named.
///
/// Pure, and it doesn't touch which arm the answer is: adding a name says nothing about
/// whether the walk covered every mount.
fn add_holders(scanned: &mut HolderScan, unseen: &[VolumeHolder]) {
    let named = match scanned {
        HolderScan::Complete { named } | HolderScan::Incomplete { named } => named,
    };
    named.extend_from_slice(unseen);
}

/// Runs `work` on ONE std thread the budget can walk away from, answering whether it
/// finished in time.
///
/// ❗ A plain thread, ❌ never `spawn_blocking`: the scan's `stat` can hang on a wedged
/// mount for minutes, and a detached pool thread would be one fewer for everything else
/// in the app. Nobody joins it; it ends when its `stat` does.
async fn within_budget(budget: Duration, work: impl FnOnce() + Send + 'static) -> bool {
    let (done, finished) = tokio::sync::oneshot::channel();
    let spawned = std::thread::Builder::new()
        .name("cmdr-holder-scan".to_string())
        .spawn(move || {
            work();
            // A closed receiver IS the budget having run out, which the caller already knows.
            let _ = done.send(());
        });
    if let Err(spawn_error) = spawned {
        log::warn!(target: "eject", "No thread was free to scan for holders: {spawn_error}");
        return false;
    }
    tokio::time::timeout(budget, finished)
        .await
        .is_ok_and(|sent| sent.is_ok())
}

/// One answer out of what each path's scan found, deduped by pid in first-seen order.
///
/// Pure. `expected` is how many paths were asked about: a scan that produced fewer
/// answers than that ran out of its budget, and one that answered `Unreadable` couldn't
/// trust what it saw. Either way the answer is [`HolderScan::Incomplete`], ❌ never a
/// short list passed off as the whole story.
fn merge(found: &[PathScan], expected: usize) -> HolderScan {
    let mut complete = expected > 0 && found.len() == expected;
    let mut named: Vec<VolumeHolder> = Vec::new();
    for path in found {
        match path {
            PathScan::Named(holders) => {
                for holder in holders {
                    if !named.iter().any(|seen| seen.pid == holder.pid) {
                        named.push(holder.clone());
                    }
                }
            }
            PathScan::Unreadable => complete = false,
        }
    }
    if complete {
        HolderScan::Complete { named }
    } else {
        HolderScan::Incomplete { named }
    }
}

/// One path's scan, with its reads as parameters: the root device before, the process
/// walk, the root device after, then a name per pid.
///
/// ❗ The device is read on BOTH sides. A mount that went away and a different volume
/// that took its place mid-scan would have the scan name processes holding somebody
/// else's drive, so a device that moved discards the whole answer. ❌ Never `f_fsid`:
/// on the boot volume it names the sealed system snapshot, not the mount
/// (§ "Spike results" 8).
fn scan_path_with(
    mut root_device: impl FnMut() -> Option<u64>,
    pids_holding: impl FnOnce() -> Option<Vec<u32>>,
    name: impl Fn(u32) -> Option<VolumeHolder>,
) -> PathScan {
    let Some(before) = root_device() else {
        return PathScan::Unreadable;
    };
    let Some(pids) = pids_holding() else {
        return PathScan::Unreadable;
    };
    let Some(after) = root_device() else {
        return PathScan::Unreadable;
    };
    if before != after {
        return PathScan::Unreadable;
    }
    // A pid with no name is gone (ESRCH) or invisible, and naming it "" helps nobody.
    PathScan::Named(pids.into_iter().filter_map(name).collect())
}

/// The device id of the mount root, from its own `stat`.
///
/// ❗ This IS a probe of a mount root, which everywhere else in the app is forbidden (it
/// blocks 30–120 s on a wedged mount). It's sound only here, because the whole scan runs
/// on the thread the budget abandons, and `proc_listpidspath` `stat`s the same path
/// anyway. ❌ Never lift it out of that thread.
fn root_device(path: &Path) -> Option<u64> {
    let metadata = std::fs::metadata(path).ok()?;
    cmdr_fs::file_identity::device_of(&metadata).map(cmdr_fs::file_identity::DeviceId::raw)
}

#[cfg(test)]
mod tests;
