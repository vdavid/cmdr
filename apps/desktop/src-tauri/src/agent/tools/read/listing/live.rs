//! `list_dir`'s live source: ONE bounded readdir through the volume's provider,
//! for a connected volume whose drive index can't answer (it has none, or it
//! doesn't list the folder). The rules and why each exists:
//! `agent/tools/DETAILS.md` § The location contract.
//!
//! Three layers, each testable alone, then [`answer_live`] shapes the result:
//!
//! - [`gate_local`]: for a volume whose paths are `std::fs` paths, the target is
//!   resolved link by link and every place is checked against the Full Disk
//!   Access gate BEFORE it's stat'ed, then against the volume's root, the other
//!   live volumes, and what the index's scanner skips. Blocking; runs inside the read.
//! - [`run_bounded`]: the read runs as its own task, one per volume at a time,
//!   past a deadline first asked to stop and then ABANDONED (a thread stuck in a
//!   kernel call can't be cancelled), and raced against the turn's cancel.
//! - [`read_live`]: the production read, gate then `cmdr_listing::entries_as_stored`
//!   (the pane's own open, which also finds a typed path's stored spelling).

use std::collections::{HashSet, VecDeque};
use std::ffi::OsString;
use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::app_paths::{path_is_under, path_under, rebase};
use cmdr_fs::volume::{Volume, VolumeError};
use tokio::time::{Instant, sleep_until};
use tokio_util::sync::CancellationToken;

use cmdr_index::Freshness;

use super::{
    ChildEntry, Coverage, ListDirResult, ListOptions, ListSource, RowKind, SizeClaim, VolumeView, sort_and_page,
    to_value,
};
use crate::ignore_poison::IgnorePoison;
use crate::index_host::index;
use crate::location::Location;
use crate::location::miss::{LocationMissResult, MissToken};
use crate::location::resolve::{ResolvedLocation, VolumeFacts};
use crate::mcp::resources::indexing::{format_number, status_token};
use crate::mcp::{ToolError, ToolResult};

/// The most entries a live read keeps. A hostile folder (200,000 build
/// artifacts) must not become a 200,000-row sort for a page of 50.
pub(crate) const LIVE_ENTRY_CAP: usize = 10_000;

/// Links followed before a target counts as a loop (the kernel's `MAXSYMLINKS`).
const MAX_LINK_HOPS: usize = 32;

/// The timeout policy, as values so the tests can shrink them. The shape of
/// `inspect_file`'s `RunnerConfig`, for one read instead of many.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveReadConfig {
    /// The read's whole budget, cooperative window plus grace.
    pub timeout: Duration,
    /// How long after the read is asked to stop it may still answer before it's
    /// abandoned. Shorter than `timeout`.
    pub cancel_grace: Duration,
}

/// Production budgets: a folder of 10,000 entries over a slow share lists well
/// inside this, and a wedged mount stops costing the turn after it.
pub(crate) const LIVE_READ: LiveReadConfig = LiveReadConfig {
    timeout: Duration::from_secs(10),
    cancel_grace: Duration::from_secs(1),
};

/// Why the privacy gate refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrivacyGate {
    /// The Full Disk Access question is still open, so touching a guarded place
    /// would stack a macOS prompt on top of it. Nothing was read.
    Pending,
    /// macOS privacy refused a read Cmdr attempted.
    Refused,
}

/// Why a live read didn't answer in time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stall {
    /// The read ran out its budget and was abandoned.
    Deadline,
    /// An earlier live read on this volume is still waiting for it, so this one
    /// didn't start.
    Busy,
    /// The volume dropped out from under the read (disconnected, timed out).
    Failed,
}

/// Another live volume, for telling a link that leaves this one where it lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OtherVolume {
    pub volume_id: String,
    pub name: String,
    pub root: String,
}

/// What a live read answers instead of rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LiveMiss {
    NeedsDiskAccess(PrivacyGate),
    Unreachable(Stall),
    NotFound,
    /// The target is a link that leads off this volume: refused, never followed.
    LeadsOffVolume {
        to: Option<OtherVolume>,
        path: String,
    },
    /// The index's scanner skips a folder on the way to the target on purpose.
    Excluded {
        name: String,
    },
    PermissionDenied,
    Cancelled,
}

impl LiveMiss {
    /// This miss as the location contract's one wire shape. `reconnects`: the volume is a
    /// session Cmdr owns ([`reconnects`]), so a dropped read is worth waiting out.
    pub(crate) fn into_result(self, location: &Location, volume_name: &str, reconnects: bool) -> LocationMissResult {
        let (token, detail) = match self {
            LiveMiss::NeedsDiskAccess(PrivacyGate::Pending) => (
                MissToken::NeedsDiskAccess,
                "Reading this folder would make macOS ask for permission while Cmdr's Full Disk Access question is still open, so Cmdr didn't read it. Tell the user to answer that question in Cmdr first.".to_string(),
            ),
            LiveMiss::NeedsDiskAccess(PrivacyGate::Refused) => (
                MissToken::NeedsDiskAccess,
                "macOS privacy settings refused Cmdr this folder. Tell the user they can grant Full Disk Access in System Settings › Privacy & Security.".to_string(),
            ),
            LiveMiss::Unreachable(Stall::Deadline) => (
                MissToken::Unreachable,
                format!(
                    "{volume_name} didn't answer within {} seconds, so Cmdr stopped waiting. Tell the user it seems unresponsive; retrying right away won't help.",
                    LIVE_READ.timeout.as_secs()
                ),
            ),
            LiveMiss::Unreachable(Stall::Busy) => (
                MissToken::Unreachable,
                format!(
                    "An earlier read on {volume_name} is still waiting for it to answer, so this one didn't start. Tell the user it seems unresponsive; retrying right away won't help."
                ),
            ),
            // A session Cmdr owns that drops mid-read comes back on its own (the frontend's
            // backoff loop), so the miss points at the way to wait for it rather than at the
            // user. Telling the model to "tell the user" here is how a NAS that reconnected
            // nine seconds later was written off for the whole answer.
            LiveMiss::Unreachable(Stall::Failed) if reconnects => (
                MissToken::Unreachable,
                format!(
                    "{volume_name} stopped answering while Cmdr read it, so its connection probably dropped, and Cmdr is reconnecting it. Call `connect_volume` with volumeId `{}` if you have it, which waits for the reconnect; otherwise wait a few seconds. Then read this again. If it doesn't come back, tell the user it seems disconnected.",
                    location.volume_id
                ),
            ),
            LiveMiss::Unreachable(Stall::Failed) => (
                MissToken::Unreachable,
                format!("{volume_name} stopped answering while Cmdr read it. Tell the user it may have disconnected."),
            ),
            LiveMiss::NotFound => (
                MissToken::NotFound,
                format!("Nothing is at this path on {volume_name}. List its parent folder to check the spelling."),
            ),
            LiveMiss::LeadsOffVolume { to: Some(to), path } => (
                MissToken::NotFound,
                format!(
                    "This path is a link to `{path}` on {}, so it isn't read here. List that path with volumeId `{}` instead.",
                    to.name, to.volume_id
                ),
            ),
            LiveMiss::LeadsOffVolume { to: None, path } => (
                MissToken::NotFound,
                format!("This path is a link to `{path}`, outside {volume_name}, so Cmdr doesn't follow it."),
            ),
            LiveMiss::Excluded { name } => (
                MissToken::Excluded,
                format!(
                    "`{name}` is system data the drive index leaves out on purpose, so Cmdr doesn't list it either."
                ),
            ),
            LiveMiss::PermissionDenied => (
                MissToken::PermissionDenied,
                "The folder's own permissions don't let Cmdr read it. Full Disk Access won't change that; whoever owns the folder can.".to_string(),
            ),
            LiveMiss::Cancelled => (
                MissToken::Cancelled,
                "The user stopped this turn while the folder was being read.".to_string(),
            ),
        };
        LocationMissResult::on(location, token, detail)
    }
}

// ── The gate ──────────────────────────────────────────────────────────────────

/// The privacy rules a gate applies, injected so tests need no guarded folder.
#[derive(Clone, Copy)]
pub(crate) struct GatePolicy {
    /// The Full Disk Access question is still open.
    pub fda_pending: bool,
    /// Whether a macOS privacy gate covers this place.
    pub is_protected: fn(&Path) -> bool,
    /// Whether an observed permission denial here plausibly came from macOS
    /// privacy rather than the folder's own permissions.
    pub denial_is_privacy: fn(&Path) -> bool,
}

impl GatePolicy {
    /// The app's rules as they stand right now.
    pub(crate) fn now() -> Self {
        Self {
            fda_pending: crate::fda_gate::is_fda_pending_runtime(),
            is_protected: cmdr_fs::tcc_paths::is_potentially_tcc_restricted,
            denial_is_privacy: cmdr_fs::tcc_paths::tcc_denial_is_plausible,
        }
    }
}

/// Whether the index's scanner skips a path on this volume on purpose
/// (`Index::scan_skips`), injected so the tests need no registered volume.
pub(crate) type ScanSkips = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// What a live read is asked to read, and what it needs to judge where a link
/// lands and what the index left out.
#[derive(Clone)]
pub(crate) struct LiveTarget {
    /// The volume's root, as an app path.
    pub root: String,
    /// The folder asked for, as an app path under `root`.
    pub path: String,
    /// Every other live volume.
    pub others: Vec<OtherVolume>,
    /// The scanner's exclusion policy for this volume.
    pub skips: ScanSkips,
}

/// Gate a target on a volume whose paths are `std::fs` paths, and answer the
/// path to list: the PHYSICAL folder (every link resolved), spelled under the
/// volume's own root. Blocking.
///
/// ❗ The privacy gate runs on every place the resolution visits, BEFORE that
/// place is stat'ed. `std::fs::canonicalize` would stat a link's target to
/// resolve it, which for a link into `~/Documents` is the very prompt the gate
/// exists to prevent.
pub(crate) fn gate_local(target: &LiveTarget, policy: &GatePolicy) -> Result<String, LiveMiss> {
    let check = |place: &Path| {
        if policy.fda_pending && (policy.is_protected)(place) {
            Err(LiveMiss::NeedsDiskAccess(PrivacyGate::Pending))
        } else {
            Ok(())
        }
    };
    let root = physical_string(Path::new(&target.root), &check, policy)?;
    let physical = physical_string(Path::new(&target.path), &check, policy)?;
    let ours = depth(&root);
    // A deeper live root holding the target is another volume's ground (a link
    // from the boot disk onto a share, or out of one share into another).
    let other_owner = target
        .others
        .iter()
        .filter(|other| path_is_under(&physical, &other.root))
        .max_by_key(|other| depth(&other.root));
    let leads_off = !path_is_under(&physical, &root) || other_owner.is_some_and(|other| depth(&other.root) > ours);
    if leads_off {
        return Err(LiveMiss::LeadsOffVolume {
            to: other_owner.cloned(),
            path: physical,
        });
    }
    // Judged in the volume's own spelling, the path space its scanner walks.
    let listed = rebase(&physical, &root, &target.root).unwrap_or(physical);
    skipped_by_the_index(target, &listed)?;
    Ok(listed)
}

/// How deep a root sits: its length without a trailing slash, the same measure
/// the resolver uses to pick the deepest root.
fn depth(root: &str) -> usize {
    root.trim_end_matches('/').len()
}

/// The first folder from the volume's root down to `path` (the leaf included)
/// that the index's scanner skips on purpose, as a miss: a live read never shows
/// ground the index deliberately left out, or one tool would answer two ways for
/// the same folder. Each step is asked separately because the scanner judges one
/// path at a time and never descends below a skipped one. Can stat (see
/// `Index::scan_skips`), so it runs on the read's blocking thread.
fn skipped_by_the_index(target: &LiveTarget, path: &str) -> Result<(), LiveMiss> {
    let Some(rest) = path_under(path, &target.root) else {
        return Ok(());
    };
    let mut prefix = target.root.trim_end_matches('/').to_string();
    for part in rest.split('/').filter(|part| !part.is_empty()) {
        prefix.push('/');
        prefix.push_str(part);
        if (target.skips)(&prefix) {
            return Err(LiveMiss::Excluded { name: part.to_string() });
        }
    }
    Ok(())
}

fn physical_string(
    path: &Path,
    check: &dyn Fn(&Path) -> Result<(), LiveMiss>,
    policy: &GatePolicy,
) -> Result<String, LiveMiss> {
    let physical = resolve_physical(path, check, policy)?;
    // App paths are UTF-8; a resolution that leaves UTF-8 left through a link
    // nothing else could name either.
    physical.into_os_string().into_string().map_err(|_| LiveMiss::NotFound)
}

/// One step of a path still to resolve.
enum Step {
    Up,
    Name(OsString),
}

fn steps_of(path: &Path) -> impl Iterator<Item = Step> + '_ {
    path.components().filter_map(|component| match component {
        Component::ParentDir => Some(Step::Up),
        Component::Normal(name) => Some(Step::Name(name.to_os_string())),
        Component::RootDir | Component::CurDir | Component::Prefix(_) => None,
    })
}

/// Resolve `path` to the folder it physically is, one component at a time,
/// asking `check` about each place before `lstat` touches it.
fn resolve_physical(
    path: &Path,
    check: &dyn Fn(&Path) -> Result<(), LiveMiss>,
    policy: &GatePolicy,
) -> Result<PathBuf, LiveMiss> {
    if !path.is_absolute() {
        return Err(LiveMiss::NotFound);
    }
    let mut pending: VecDeque<Step> = steps_of(path).collect();
    let mut resolved = PathBuf::from("/");
    let mut hops = 0;
    while let Some(step) = pending.pop_front() {
        let name = match step {
            // `resolved` holds no links, so its parent is the physical one.
            Step::Up => {
                resolved.pop();
                continue;
            }
            Step::Name(name) => name,
        };
        let place = resolved.join(&name);
        check(&place)?;
        let metadata = std::fs::symlink_metadata(&place).map_err(|e| io_miss(&e, &place, policy))?;
        if !metadata.file_type().is_symlink() {
            resolved = place;
            continue;
        }
        hops += 1;
        if hops > MAX_LINK_HOPS {
            return Err(LiveMiss::NotFound);
        }
        let link = std::fs::read_link(&place).map_err(|e| io_miss(&e, &place, policy))?;
        if link.is_absolute() {
            resolved = PathBuf::from("/");
        }
        for step in steps_of(&link).collect::<Vec<_>>().into_iter().rev() {
            pending.push_front(step);
        }
    }
    Ok(resolved)
}

fn io_miss(error: &std::io::Error, place: &Path, policy: &GatePolicy) -> LiveMiss {
    use std::io::ErrorKind;
    match error.kind() {
        ErrorKind::NotFound | ErrorKind::NotADirectory => LiveMiss::NotFound,
        ErrorKind::PermissionDenied if (policy.denial_is_privacy)(place) => {
            LiveMiss::NeedsDiskAccess(PrivacyGate::Refused)
        }
        ErrorKind::PermissionDenied => LiveMiss::PermissionDenied,
        _ => LiveMiss::Unreachable(Stall::Failed),
    }
}

/// A listing's error as a live miss. Typed on the variant, never its message.
fn volume_miss(error: &VolumeError, path: &Path, policy: &GatePolicy, local_fs: bool) -> LiveMiss {
    match error {
        VolumeError::NotFound(_) | VolumeError::NotADirectory(_) => LiveMiss::NotFound,
        VolumeError::PermissionDenied { .. } if local_fs && (policy.denial_is_privacy)(path) => {
            LiveMiss::NeedsDiskAccess(PrivacyGate::Refused)
        }
        VolumeError::PermissionDenied { .. } => LiveMiss::PermissionDenied,
        VolumeError::Cancelled(_) => LiveMiss::Cancelled,
        // Every other failure (a dropped session, a timeout, an I/O fault) is the
        // volume not answering, which is all the model can act on.
        _ => LiveMiss::Unreachable(Stall::Failed),
    }
}

// ── The bounded run ───────────────────────────────────────────────────────────

/// Volumes with a live read in flight. A read leaves only when its task really
/// returns, so a hung mount holds its slot and the next call answers `Busy`
/// instead of parking another thread behind it.
static IN_FLIGHT: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Mutex::default);

/// A claimed slot in [`IN_FLIGHT`], freed on drop.
struct Slot(String);

impl Slot {
    fn claim(volume_id: &str) -> Option<Self> {
        // The lock is released before a `Slot` exists: its drop takes the lock too.
        let claimed = IN_FLIGHT.lock_ignore_poison().insert(volume_id.to_string());
        claimed.then(|| Self(volume_id.to_string()))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        IN_FLIGHT.lock_ignore_poison().remove(&self.0);
    }
}

/// Run `read` for `volume_id` under `cfg`, one at a time per volume, raced
/// against `turn_cancel`.
///
/// The read gets its own token: the deadline flips it (a backend that lists in
/// round trips, SMB or MTP, stops between them), then waits the grace, then
/// abandons the task. So does the turn's cancel, which answers `Cancelled` at
/// once. Dropping this future (an AI client hanging up) flips it too. The read
/// runs on the app's shared runtime, not the caller's: a turn's runtime is
/// single-threaded and ends with the turn, and an abandoned read must still
/// finish to free its slot.
pub(crate) async fn run_bounded<F>(
    volume_id: &str,
    cfg: LiveReadConfig,
    turn_cancel: &CancellationToken,
    read: impl FnOnce(CancellationToken) -> F,
) -> Result<Vec<FileEntry>, LiveMiss>
where
    F: Future<Output = Result<Vec<FileEntry>, LiveMiss>> + Send + 'static,
{
    if turn_cancel.is_cancelled() {
        return Err(LiveMiss::Cancelled);
    }
    let Some(slot) = Slot::claim(volume_id) else {
        return Err(LiveMiss::Unreachable(Stall::Busy));
    };
    let stop = CancellationToken::new();
    let _stop_on_drop = stop.clone().drop_guard();
    let reading = read(stop.clone());
    let mut task = tauri::async_runtime::spawn(async move {
        let _slot = slot;
        reading.await
    });

    let cooperative_until = Instant::now() + cfg.timeout.saturating_sub(cfg.cancel_grace);
    tokio::select! {
        done = &mut task => return finished(done, false),
        () = turn_cancel.cancelled() => return Err(LiveMiss::Cancelled),
        () = sleep_until(cooperative_until) => {}
    }
    // Past the window: ask the read to stop, and give it the grace to answer.
    stop.cancel();
    tokio::select! {
        done = &mut task => finished(done, true),
        () = turn_cancel.cancelled() => Err(LiveMiss::Cancelled),
        // Dropping `task` detaches it: abandoned, not stopped.
        () = sleep_until(Instant::now() + cfg.cancel_grace) => Err(LiveMiss::Unreachable(Stall::Deadline)),
    }
}

/// A finished read's answer. `stopped_by_deadline` turns the read's own
/// `Cancelled` (the deadline asked it to stop) into the stall it really is.
fn finished(
    done: Result<Result<Vec<FileEntry>, LiveMiss>, tauri::Error>,
    stopped_by_deadline: bool,
) -> Result<Vec<FileEntry>, LiveMiss> {
    match done {
        Ok(Err(LiveMiss::Cancelled)) if stopped_by_deadline => Err(LiveMiss::Unreachable(Stall::Deadline)),
        Ok(answer) => answer,
        Err(e) => {
            log::warn!(target: "agent::tools::listing", "list_dir: a live read panicked: {e}");
            Err(LiveMiss::Unreachable(Stall::Failed))
        }
    }
}

// ── The production read ───────────────────────────────────────────────────────

/// Gate `target` (when the volume's paths are `std::fs` paths), then list it
/// through the provider the way a pane opens a folder. `stop` reaches the
/// listing; the gate is a few `lstat`s, abandoned with the read if one hangs.
pub(crate) async fn read_live(
    volume: Arc<dyn Volume>,
    target: LiveTarget,
    policy: GatePolicy,
    stop: CancellationToken,
) -> Result<Vec<FileEntry>, LiveMiss> {
    let local_fs = volume.supports_local_fs_access();
    let path = if local_fs {
        let gate_target = target.clone();
        tauri::async_runtime::spawn_blocking(move || gate_local(&gate_target, &policy))
            .await
            .map_err(|_| LiveMiss::Unreachable(Stall::Failed))??
    } else {
        // A provider-addressed volume (direct SMB, MTP) reaches no local folder,
        // so only the scanner's policy applies: links are the server's to resolve.
        let path = target.path.clone();
        tauri::async_runtime::spawn_blocking(move || skipped_by_the_index(&target, &target.path).map(|()| path))
            .await
            .map_err(|_| LiveMiss::Unreachable(Stall::Failed))??
    };
    if stop.is_cancelled() {
        return Err(LiveMiss::Cancelled);
    }
    let path = PathBuf::from(path);
    let error = match cmdr_listing::entries_as_stored(volume.as_ref(), &path, Some(&stop)).await {
        Ok(entries) => return Ok(entries),
        Err(error) => error,
    };
    if !(local_fs && matches!(error, VolumeError::PermissionDenied { .. })) {
        return Err(volume_miss(&error, &path, &policy, local_fs));
    }
    // Telling a privacy refusal from the folder's own permissions reads the
    // gate's anchor: a blocking call.
    let miss = tauri::async_runtime::spawn_blocking(move || volume_miss(&error, &path, &policy, true))
        .await
        .unwrap_or(LiveMiss::Unreachable(Stall::Failed));
    Err(miss)
}

// ── The answer ────────────────────────────────────────────────────────────────

/// Everything a live read needs, resolved by the caller so the tests can inject
/// the volume, the budgets, and the privacy rules.
pub(crate) struct LiveAsk<'a> {
    pub resolved: &'a ResolvedLocation,
    pub facts: &'a VolumeFacts,
    pub opts: &'a ListOptions,
    pub view: &'a VolumeView,
    /// The registered volume, `None` when it left between the resolve and now.
    pub volume: Option<Arc<dyn Volume>>,
    pub cancel: &'a CancellationToken,
    pub cfg: LiveReadConfig,
    pub policy: GatePolicy,
}

/// Answer from one bounded live read of the resolved folder.
pub(crate) async fn answer_live(ask: LiveAsk<'_>) -> ToolResult {
    let location = &ask.resolved.location;
    // The cap stops what's kept, so a page past it would be a page of nothing.
    if ask.opts.offset >= LIVE_ENTRY_CAP {
        return Err(ToolError::invalid_params(format!(
            "This folder is read live, which keeps its first {} entries, so offset {} is past them. List a subfolder, or search it with a name pattern.",
            format_number(LIVE_ENTRY_CAP as u64),
            ask.opts.offset
        )));
    }
    let reconnects = reconnects(ask.facts, &location.volume_id);
    let Some(volume) = ask.volume else {
        return to_value(&LiveMiss::Unreachable(Stall::Failed).into_result(location, &ask.resolved.name, reconnects));
    };
    let target = LiveTarget {
        root: ask.resolved.root.clone(),
        path: location.path.clone(),
        others: ask
            .facts
            .volumes
            .iter()
            .filter(|v| v.live && v.id != location.volume_id)
            .map(|v| OtherVolume {
                volume_id: v.id.clone(),
                name: v.name.clone(),
                root: v.root.clone(),
            })
            .collect(),
        skips: {
            let volume_id = location.volume_id.clone();
            Arc::new(move |path: &str| index().scan_skips(&volume_id, path))
        },
    };
    let policy = ask.policy;
    let read = move |stop| read_live(volume, target, policy, stop);
    match run_bounded(&location.volume_id, ask.cfg, ask.cancel, read).await {
        Ok(entries) => to_value(&build_live_list_dir(location, entries, ask.view, ask.opts)),
        Err(miss) => to_value(&miss.into_result(location, &ask.resolved.name, reconnects)),
    }
}

/// Whether `volume_id` is a session Cmdr owns, which the frontend's backoff loop brings back
/// after a drop and `connect_volume` can wait out. An OS mount or a local disk isn't: Cmdr has
/// nothing of its own to reconnect there.
fn reconnects(facts: &VolumeFacts, volume_id: &str) -> bool {
    use cmdr_fs::volume::ConnectionState;
    facts.get(volume_id).is_some_and(|v| {
        matches!(
            v.connection_state,
            Some(ConnectionState::Direct | ConnectionState::Disconnected)
        )
    })
}

/// The coverage block for a live read. A complete read is authoritative about
/// what's in the folder right now; one the entry cap cut isn't. The index's own
/// token rides along, so `off` still says the volume has no index.
pub(crate) fn live_coverage(enabled: bool, freshness: Option<Freshness>, capped: bool) -> Coverage {
    let note = if capped {
        format!(
            "Read live just now, and cut at the first {} entries, so the total is a lower bound. Folder sizes need the drive index, so they're absent.",
            format_number(LIVE_ENTRY_CAP as u64)
        )
    } else {
        "Read live just now, because the drive index doesn't list this folder. Folder sizes need the index, so they're absent.".to_string()
    };
    Coverage {
        source: ListSource::Live,
        index_status: status_token(enabled, freshness).to_string(),
        authoritative: !capped,
        as_of: None,
        note: Some(note),
    }
}

/// Shape a live read's entries. Pure, so the cap and the unknown folder sizes are
/// testable without a volume.
///
/// The cap keeps the FIRST [`LIVE_ENTRY_CAP`] entries in the provider's own order
/// (folders first, then names), so a capped folder pages over a stable set; the
/// requested order applies within it.
pub(crate) fn build_live_list_dir(
    location: &Location,
    mut entries: Vec<FileEntry>,
    volume: &VolumeView,
    opts: &ListOptions,
) -> ListDirResult {
    let capped = entries.len() > LIVE_ENTRY_CAP;
    entries.truncate(LIVE_ENTRY_CAP);
    let children: Vec<ChildEntry> = entries.into_iter().map(child_from_entry).collect();
    let page = sort_and_page(children, opts);
    let total = page.total;
    let fitted = crate::mcp::fit_to_result_budget(page.rows);
    let returned = fitted.items.len();
    ListDirResult {
        location: location.clone(),
        coverage: live_coverage(volume.enabled, volume.freshness, capped),
        volume: volume.block.clone(),
        // A live read has no recursive total to state: absent, never a zero.
        size: None,
        total: Some(total),
        total_is_lower_bound: capped,
        returned: Some(returned),
        offset: opts.offset,
        truncated: capped || opts.offset.saturating_add(returned) < total,
        remainder: None,
        sorted_by: opts.sort_by.token(),
        children: Some(fitted.items),
    }
}

/// Map a live listing's entry into the model-facing child shape. A file's size is
/// its own; a folder's is unknown, since a recursive total needs the index.
fn child_from_entry(entry: FileEntry) -> ChildEntry {
    let size = if entry.is_directory {
        SizeClaim::unknown()
    } else {
        SizeClaim::exact(entry.size)
    };
    ChildEntry::new(
        entry.name,
        RowKind::of(entry.is_directory, entry.is_symlink),
        size,
        entry.modified_at,
    )
}

#[cfg(test)]
#[path = "live_tests.rs"]
mod tests;
