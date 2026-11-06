//! What arrives through `RunEvent::Opened`: the cold-launch buffer, the plan a set of
//! revealed paths becomes, and the pane move that carries it out.
//!
//! Several senders share this door. Another app's "Show in Finder" (once the `NSFileViewer`
//! key is ours) arrives as `file://` URLs; the `cmdr` terminal command arrives as a
//! `cmdr-cli://open?…` URL (`crate::cli`). Each becomes a [`RevealRequest`] here, and only
//! its origin tells it apart from then on.
//!
//! macOS-only, like the mechanism itself. Its parent `mod.rs` is NOT gated, because
//! [`super::RevealDelivered`] has to resolve on every platform for `ipc.rs`'s
//! `collect_events!`. `DETAILS.md` § Delivery.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager, Runtime, Wry};
use tauri_specta::Event;

use crate::cli::open_url::{SCHEME as CLI_SCHEME, parse_cli_url};
use crate::ignore_poison::IgnorePoison;
use crate::tools::{GoToFailure, GoToTarget, PaneChoice};

use super::{RevealDelivered, RevealNotShown, RevealNotShownReason, RevealTabsDropped};

/// The one buffer, and process-global on purpose.
///
/// ❗ A cold-launch reveal reaches `on_urls_opened` BEFORE Tauri's `setup` runs, so
/// `app.manage`d state doesn't exist yet and `try_state` would come back empty — which is
/// exactly how the reveal used to get dropped. `DETAILS.md` § "How a cold-launch reveal
/// arrives".
pub(super) static PENDING: LazyLock<PendingReveals> = LazyLock::new(PendingReveals::default);

/// How long the "is this a folder?" probe may take. A revealed path can sit on a dead
/// network mount, where `is_dir` blocks for minutes; past this we give up on the whole
/// request rather than guess at the answer.
const CLASSIFY_TIMEOUT: Duration = Duration::from_secs(2);

/// One delivery at a time, across batches, in arrival order (Tokio's mutex is fair): two
/// moves racing for the same pane would land in whichever order they finish, and each one's
/// tabs would interleave with the other's.
static DELIVERY_TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Who asked for a reveal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RevealOrigin {
    /// Another app's "Show in Finder", routed here by the `NSFileViewer` key.
    Finder,
    /// The `cmdr` terminal command. ❗ Never announced as [`RevealDelivered`]: the
    /// frontend spends a once-ever "Show in Finder now lands here" notice on the first
    /// one, and a terminal open says nothing about Finder.
    Terminal,
    /// "Open in Cmdr" in Finder's right-click menu. Never announced, for the same reason
    /// as [`Self::Terminal`]: the person just asked for Cmdr by name.
    OpenInCmdr,
}

/// One request to show paths in a pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RevealRequest {
    pub paths: Vec<PathBuf>,
    pub target: GoToTarget,
    pub origin: RevealOrigin,
}

/// One pane move a request becomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RevealPlan {
    /// The directory the pane navigates to.
    pub dir: PathBuf,
    /// Entries to select there, cursor on the first. Empty when the request named a
    /// folder, which is opened rather than pointed at.
    pub entries: Vec<String>,
    /// Whether this move wants a tab of its own.
    pub new_tab: bool,
}

/// Reveal requests that arrived before the frontend could act on them.
///
/// A cold launch delivers the event long before the webview mounts, so the arriving
/// requests are parked here and the frontend drains them once its listeners are up
/// (`commands::drain_pending_reveals`). A `Vec`, not an `Option`: two reveals can land
/// back to back during a cold launch, and the second must not erase the first.
#[derive(Default)]
pub struct PendingReveals {
    queue: Mutex<Vec<RevealRequest>>,
    /// Set by the first drain. Until then nothing is delivered, because there is nobody
    /// listening for `mcp-nav-to-path` yet and the emit would go nowhere.
    frontend_ready: AtomicBool,
}

impl PendingReveals {
    /// Park `request` and return whatever can be delivered right now.
    ///
    /// Back-to-back Finder reveals merge into one request: `open -R a b` sends one event
    /// per path, and on a cold launch they must still become one pane move with both
    /// selected. Terminal requests never merge, since each carries its own pane and tab.
    fn accept(&self, request: RevealRequest) -> Vec<RevealRequest> {
        let mut queue = self.queue.lock_ignore_poison();
        match queue.last_mut() {
            Some(last) if last.origin == RevealOrigin::Finder && request.origin == RevealOrigin::Finder => {
                last.paths.extend(request.paths);
            }
            _ => queue.push(request),
        }
        if self.frontend_ready.load(Ordering::Acquire) {
            std::mem::take(&mut queue)
        } else {
            Vec::new()
        }
    }

    /// Mark the frontend live and take everything parked so far, in arrival order.
    pub(super) fn drain(&self) -> Vec<RevealRequest> {
        self.frontend_ready.store(true, Ordering::Release);
        std::mem::take(&mut *self.queue.lock_ignore_poison())
    }
}

/// Sort what one `RunEvent::Opened` carried into requests: every `file://` URL into one
/// Finder reveal, every `cmdr-cli://` URL into a terminal request of its own. Anything else,
/// and any terminal URL that fails validation, is logged and dropped.
pub(crate) fn requests_from_urls(urls: &[tauri::Url]) -> Vec<RevealRequest> {
    let finder_paths: Vec<PathBuf> = urls
        .iter()
        .filter(|url| url.scheme() == "file")
        .filter_map(|url| url.to_file_path().ok())
        .collect();
    let mut requests = Vec::new();
    if !finder_paths.is_empty() {
        requests.push(RevealRequest {
            paths: finder_paths,
            target: GoToTarget::FOCUSED,
            origin: RevealOrigin::Finder,
        });
    }
    for url in urls.iter().filter(|url| url.scheme() == CLI_SCHEME) {
        match parse_cli_url(url) {
            Ok(open) => requests.push(RevealRequest {
                paths: open.paths,
                target: open.target,
                origin: RevealOrigin::Terminal,
            }),
            Err(refusal) => log::warn!(target: "reveal", "Refused a cmdr-cli URL: {refusal:?}"),
        }
    }
    requests
}

/// Turn the paths one reveal delivered into a single pane move.
///
/// A folder is opened; a file has its parent opened with the cursor on it. Extra paths
/// are honoured only when they sit in the first one's directory: a pane shows one
/// directory, and choosing a different one for the rest would be a guess. The caller
/// says so in the log when some are dropped.
///
/// `is_dir` is injected so the rule is testable without a filesystem, and so the real
/// probe can run under a timeout.
pub(crate) fn plan_reveal(paths: &[PathBuf], is_dir: &dyn Fn(&Path) -> bool) -> Option<RevealPlan> {
    let first = paths.first()?;
    if is_dir(first) {
        return Some(RevealPlan {
            dir: first.clone(),
            entries: Vec::new(),
            new_tab: false,
        });
    }
    let dir = first.parent()?.to_path_buf();
    let entries = paths
        .iter()
        .filter(|path| path.parent() == Some(dir.as_path()))
        .filter_map(|path| path.file_name().map(|name| name.to_string_lossy().to_string()))
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return None;
    }
    Some(RevealPlan {
        dir,
        entries,
        new_tab: false,
    })
}

/// Turn one request into its pane moves, in the order they should land.
///
/// - A terminal request is one move, in the tab it asked for.
/// - Everything else gives each folder a new tab of its own.
/// - Files from Finder's right-click menu get one new tab per directory, each with its files
///   selected there.
/// - Revealed files stay in the current tab, as one move: a reveal points at something, and
///   a pane shows one directory, so files elsewhere are dropped (`deliver` logs how many).
pub(crate) fn plan_moves(request: &RevealRequest, is_dir: &dyn Fn(&Path) -> bool) -> Vec<RevealPlan> {
    if request.origin == RevealOrigin::Terminal {
        return plan_reveal(&request.paths, is_dir)
            .map(|plan| RevealPlan {
                new_tab: request.target.new_tab,
                ..plan
            })
            .into_iter()
            .collect();
    }
    let files_get_a_tab = request.origin == RevealOrigin::OpenInCmdr;
    let mut plans: Vec<RevealPlan> = Vec::new();
    for path in &request.paths {
        // A revealed package (`Foo.app`, just downloaded or unpacked) is one item to the
        // person, as in Finder, so it gets selected in its folder rather than opened up.
        let opens_as_folder = is_dir(path) && !(request.origin == RevealOrigin::Finder && is_listed_package_path(path));
        if opens_as_folder {
            plans.push(RevealPlan {
                dir: path.clone(),
                entries: Vec::new(),
                new_tab: true,
            });
            continue;
        }
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
            continue;
        };
        let name = name.to_string_lossy().to_string();
        // Only a file move holds entries, so a folder opened at the same path stays its own.
        let is_file_move = |plan: &RevealPlan| !plan.entries.is_empty();
        if let Some(plan) = plans.iter_mut().find(|plan| is_file_move(plan) && plan.dir == dir) {
            plan.entries.push(name);
        } else if files_get_a_tab || !plans.iter().any(is_file_move) {
            plans.push(RevealPlan {
                dir: dir.to_path_buf(),
                entries: vec![name],
                new_tab: files_get_a_tab,
            });
        }
    }
    plans
}

/// Whether `path`'s name is on the product's package list (`Foo.app`). Name-only, so it
/// costs no I/O inside the classify deadline; a package the list doesn't know stays a folder.
fn is_listed_package_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(cmdr_fs::file_extension::is_listed_package)
}

/// The name a person knows `path` by: its last component, or the whole path for the root.
fn display_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| path.to_string_lossy(), |name| name.to_string_lossy())
        .to_string()
}

/// What the toast names when nothing could be planned for `paths`: the first item and the
/// folder it sits in.
fn not_shown_for_paths(paths: &[PathBuf], reason: RevealNotShownReason) -> Option<RevealNotShown> {
    let first = paths.first()?;
    let folder = first.parent().map_or_else(|| display_name(first), display_name);
    Some(RevealNotShown {
        name: display_name(first),
        folder,
        reason,
    })
}

/// What the toast names when a planned move didn't land: the first entry it was to point
/// at (the folder itself for a folder move) and the folder.
fn not_shown_for_plan(plan: &RevealPlan, failure: &GoToFailure) -> RevealNotShown {
    let folder = display_name(&plan.dir);
    let reason = match failure {
        GoToFailure::NotPointed(_) => RevealNotShownReason::NotInFolder,
        failure if failure.folder_stalled() => RevealNotShownReason::NotAnswering,
        GoToFailure::NotOpened(_) => RevealNotShownReason::FolderNotOpened,
    };
    RevealNotShown {
        name: plan.entries.first().cloned().unwrap_or_else(|| folder.clone()),
        folder,
        reason,
    }
}

/// Tell the person what a reveal couldn't show. The window is already in front by now, so
/// staying quiet would leave them looking at the wrong folder with no idea why.
fn announce_not_shown<R: Runtime>(app: &AppHandle<R>, not_shown: RevealNotShown) {
    if let Err(err) = not_shown.emit(app) {
        log::warn!(target: "reveal", "Couldn't announce the reveal that didn't show: {err}");
    }
}

/// What a move does when the pane refused its new tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AtTabLimit {
    /// Nothing has landed yet, so this move takes the current tab and the rest are dropped.
    /// Somebody who asked for a folder should land somewhere, and one overwritten tab is the
    /// price of that. ❌ Never more than one, or the extras would keep overwriting it.
    UseCurrentTabThenStop,
    /// Earlier moves got their tabs. This one and the rest are dropped.
    Stop,
}

fn at_tab_limit(index: usize) -> AtTabLimit {
    if index == 0 {
        AtTabLimit::UseCurrentTabThenStop
    } else {
        AtTabLimit::Stop
    }
}

/// Handle a `RunEvent::Opened`: the one door every arriving reveal comes through.
///
/// ⚠️ On a cold launch this runs before `setup`, before the main window exists, and before
/// the logger is initialised, so anything it says there goes nowhere and the delivery has
/// to wait for the frontend's drain. Both calls below tolerate that: `raise_main_window`
/// returns when there is no window, and the buffer holds the requests. `DETAILS.md` § "How a
/// cold-launch reveal arrives".
///
/// Called on the main thread, so it must not block: the classify probe and the pane
/// round-trip both happen on the async runtime.
pub fn on_urls_opened(app: &AppHandle<Wry>, urls: Vec<tauri::Url>) {
    let requests = requests_from_urls(&urls);
    if requests.is_empty() {
        log::debug!(target: "reveal", "An open event carried nothing to show; ignoring");
        return;
    }
    take_requests(app, requests);
}

/// Raise the window and park or deliver `requests`. Every sender ends up here.
///
/// The raise comes FIRST, before the pane has moved, on purpose: macOS already activates
/// Cmdr when the open event arrives, so waiting wouldn't keep it behind, and a covered
/// window's webview is a hidden page that WebKit throttles, which would slow the very
/// round trip the move waits on. A move that then doesn't land says so in a toast
/// (`RevealNotShown`), so being in front never goes unexplained. `DETAILS.md` § Delivery.
pub(super) fn take_requests(app: &AppHandle<Wry>, requests: Vec<RevealRequest>) {
    raise_main_window(app);
    let ready: Vec<RevealRequest> = requests
        .into_iter()
        .flat_map(|request| PENDING.accept(request))
        .collect();
    if !ready.is_empty() {
        spawn_delivery(app, ready);
    }
}

/// Bring the main window forward. The reveal was fired from another app (or a terminal),
/// so without this the pane lands behind whatever the user was looking at. Same three
/// calls as the go-to-latest-download hotkey (`downloads/global_shortcut.rs`).
fn raise_main_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.unminimize();
    let _ = window.show();
    if let Err(err) = window.set_focus() {
        log::warn!(target: "reveal", "Couldn't focus the main window for a reveal: {err}");
    }
}

/// Run deliveries on the async runtime, off whatever thread asked for them, one after the
/// other ([`DELIVERY_TURN`]).
pub(super) fn spawn_delivery<R: Runtime>(app: &AppHandle<R>, requests: Vec<RevealRequest>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _turn = DELIVERY_TURN.lock().await;
        for request in requests {
            deliver(&app, request).await;
        }
    });
}

/// Move a pane onto one request's paths.
async fn deliver<R: Runtime>(app: &AppHandle<R>, request: RevealRequest) {
    let requested = request.paths.len();
    let (pane, origin) = (request.target.pane, request.origin);
    log::info!(target: "reveal", "{origin:?} reveal: {requested} path(s) to show");
    let paths = request.paths.clone();
    // `None` only when the probe outlived its deadline: a stalled mount, told apart from a
    // request that planned to nothing, since the person hears a different reason for each.
    let plans = cmdr_fs::deadline::blocking_with_timeout(CLASSIFY_TIMEOUT, None, move || {
        Some(plan_moves(&request, &|path| path.is_dir()))
    })
    .await;
    let plans = match plans {
        Some(plans) if !plans.is_empty() => plans,
        unplanned => {
            let reason = if unplanned.is_none() {
                RevealNotShownReason::NotAnswering
            } else {
                RevealNotShownReason::FolderNotOpened
            };
            log::warn!(
                target: "reveal",
                "Couldn't work out what to show for {requested} path(s): {reason:?}",
            );
            if let Some(not_shown) = not_shown_for_paths(&paths, reason) {
                announce_not_shown(app, not_shown);
            }
            return;
        }
    };
    // A folder move shows one path (the folder) with no entries to select.
    let shown: usize = plans.iter().map(|plan| plan.entries.len().max(1)).sum();
    if shown < requested {
        log::info!(
            target: "reveal",
            "Showing {shown} of {requested} revealed path(s); the rest sit in other directories",
        );
    }
    let mut outcome = run_moves(app, pane, &plans).await;
    if let Some(not_shown) = outcome.not_shown.take() {
        announce_not_shown(app, not_shown);
    }
    if outcome.dropped > 0 {
        log::info!(
            target: "reveal",
            "The pane is at its tab limit, so {} of {} move(s) won't open",
            outcome.dropped,
            plans.len()
        );
        let dropped = RevealTabsDropped {
            opened: outcome.landed,
            dropped: outcome.dropped,
        };
        if let Err(err) = dropped.emit(app) {
            log::warn!(target: "reveal", "Couldn't announce the dropped tabs: {err}");
        }
    }
    // Announced only once a pane has actually moved, and only for a reveal the `NSFileViewer`
    // key sent here: the frontend turns the first of these into a once-ever notice about
    // "Show in Finder", which neither a reveal that went nowhere, a terminal open, nor a
    // folder opened through the folder default may spend.
    if origin == RevealOrigin::Finder
        && outcome.landed > 0
        && super::registration::key_is_ours()
        && let Err(err) = (RevealDelivered {}).emit(app)
    {
        log::warn!(target: "reveal", "Couldn't announce the delivered reveal: {err}");
    }
}

/// What a run of pane moves came to.
struct MovesOutcome {
    landed: u32,
    /// Moves the pane's tab limit turned away.
    dropped: u32,
    /// The first move that didn't show what it was asked for, worded for the person.
    not_shown: Option<RevealNotShown>,
}

/// Make `plans`' pane moves in order, stopping at the pane's tab limit.
async fn run_moves<R: Runtime>(app: &AppHandle<R>, pane: PaneChoice, plans: &[RevealPlan]) -> MovesOutcome {
    let mut outcome = MovesOutcome {
        landed: 0,
        dropped: 0,
        not_shown: None,
    };
    for (index, plan) in plans.iter().enumerate() {
        let tab_opened = !plan.new_tab
            || crate::tools::open_tab_in_pane(app, pane).await.unwrap_or_else(|err| {
                log::warn!(target: "reveal", "Couldn't open a new tab: {}", err.message);
                false
            });
        let limit = (!tab_opened).then(|| at_tab_limit(index));
        if limit == Some(AtTabLimit::Stop) {
            outcome.dropped = u32::try_from(plans.len() - index).unwrap_or(u32::MAX);
            return outcome;
        }
        let dir = plan.dir.to_string_lossy().to_string();
        match crate::tools::go_to_in_pane(app, pane, &dir, &plan.entries).await {
            Ok(()) => outcome.landed += 1,
            Err(failure) => {
                // A move that opened the folder but missed the item still counts as landed:
                // the pane did move, and that's what `RevealDelivered` is about.
                if matches!(failure, GoToFailure::NotPointed(_)) {
                    outcome.landed += 1;
                }
                log::warn!(
                    target: "reveal",
                    "Couldn't show {dir}: {}",
                    failure.error().message,
                    dir = cmdr_fs::log_private::LogPrivate::dir_path(&dir)
                );
                // One toast per batch: the first miss is the one worth naming, and a stack
                // of them for one click would bury it.
                if outcome.not_shown.is_none() {
                    outcome.not_shown = Some(not_shown_for_plan(plan, &failure));
                }
            }
        }
        if limit == Some(AtTabLimit::UseCurrentTabThenStop) {
            outcome.dropped = u32::try_from(plans.len() - index - 1).unwrap_or(u32::MAX);
            return outcome;
        }
    }
    outcome
}

#[cfg(test)]
#[path = "delivery_test.rs"]
mod tests;
