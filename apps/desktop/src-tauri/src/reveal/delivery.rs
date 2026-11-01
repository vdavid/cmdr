//! What arrives through `RunEvent::Opened`: the cold-launch buffer, the plan a set of
//! revealed paths becomes, and the pane move that carries it out.
//!
//! Two senders share this door. Another app's "Show in Finder" (once the `NSFileViewer` key
//! is ours) arrives as `file://` URLs; the `cmdr` terminal command arrives as a
//! `cmdr-cli://open?…` URL (`crate::cli`). Both become a [`RevealRequest`] here, and only
//! their origin tells them apart from then on.
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
use crate::tools::GoToTarget;

use super::RevealDelivered;

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

/// Who asked for a reveal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RevealOrigin {
    /// Another app's "Show in Finder", routed here by the `NSFileViewer` key.
    Finder,
    /// The `cmdr` terminal command. ❗ Never announced as [`RevealDelivered`]: the
    /// frontend spends a once-ever "Show in Finder now lands here" notice on the first
    /// one, and a terminal open says nothing about Finder.
    Terminal,
}

/// One request to show paths in a pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RevealRequest {
    pub paths: Vec<PathBuf>,
    pub target: GoToTarget,
    pub origin: RevealOrigin,
}

/// Where one reveal request puts the pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RevealPlan {
    /// The directory the pane navigates to.
    pub dir: PathBuf,
    /// Entries to select there, cursor on the first. Empty when the request named a
    /// folder, which is opened rather than pointed at.
    pub entries: Vec<String>,
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
    Some(RevealPlan { dir, entries })
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
/// other: two moves racing for the same pane would land in whichever order they finish.
pub(super) fn spawn_delivery<R: Runtime>(app: &AppHandle<R>, requests: Vec<RevealRequest>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        for request in requests {
            deliver(&app, request).await;
        }
    });
}

/// Move a pane onto one request's paths.
async fn deliver<R: Runtime>(app: &AppHandle<R>, request: RevealRequest) {
    let RevealRequest { paths, target, origin } = request;
    let requested = paths.len();
    log::info!(target: "reveal", "{origin:?} reveal: {requested} path(s) to show");
    let probe = paths.clone();
    let plan = cmdr_fs::deadline::blocking_with_timeout(CLASSIFY_TIMEOUT, None, move || {
        plan_reveal(&probe, &|path| path.is_dir())
    })
    .await;
    let Some(plan) = plan else {
        log::warn!(
            target: "reveal",
            "Couldn't work out what to show for {requested} path(s) (unreadable, parentless, or a stalled mount)",
        );
        return;
    };
    // A folder plan shows one path (the folder) with no entries to select.
    let shown = plan.entries.len().max(1);
    if shown < requested {
        log::info!(
            target: "reveal",
            "Showing {shown} of {requested} revealed path(s); the rest sit in other directories",
        );
    }
    let dir = plan.dir.to_string_lossy().to_string();
    if let Err(err) = crate::tools::go_to_in_pane(app, target, &dir, &plan.entries).await {
        log::warn!(
            target: "reveal",
            "Couldn't show {dir}: {}",
            err.message,
            dir = cmdr_fs::log_private::LogPrivate::dir_path(&dir)
        );
        return;
    }
    // Announced only once the pane has actually moved, and only for Finder reveals: the
    // frontend turns the first of these into a once-ever notice about "Show in Finder",
    // which neither a reveal that went nowhere nor a terminal open may spend.
    if origin == RevealOrigin::Finder
        && let Err(err) = (RevealDelivered {}).emit(app)
    {
        log::warn!(target: "reveal", "Couldn't announce the delivered reveal: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::PaneChoice;

    fn dirs(paths: &[&str]) -> impl Fn(&Path) -> bool + use<> {
        let dirs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        move |path: &Path| dirs.iter().any(|d| d == path)
    }

    fn p(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    fn finder(paths: &[&str]) -> RevealRequest {
        RevealRequest {
            paths: paths.iter().map(PathBuf::from).collect(),
            target: GoToTarget::FOCUSED,
            origin: RevealOrigin::Finder,
        }
    }

    fn terminal(paths: &[&str], pane: PaneChoice) -> RevealRequest {
        RevealRequest {
            paths: paths.iter().map(PathBuf::from).collect(),
            target: GoToTarget { pane, new_tab: false },
            origin: RevealOrigin::Terminal,
        }
    }

    fn urls(raw: &[&str]) -> Vec<tauri::Url> {
        raw.iter().map(|url| tauri::Url::parse(url).expect("a URL")).collect()
    }

    #[test]
    fn a_file_opens_its_parent_with_the_cursor_on_it() {
        let plan = plan_reveal(&[p("/Users/dave/notes/todo.md")], &dirs(&[])).expect("a plan");
        assert_eq!(plan.dir, p("/Users/dave/notes"));
        assert_eq!(plan.entries, vec!["todo.md"]);
    }

    #[test]
    fn a_folder_is_opened_rather_than_pointed_at() {
        // A reveal OF a folder becomes an open of it. Finder parity isn't the goal here;
        // landing inside the folder the user asked about is.
        let plan = plan_reveal(&[p("/Users/dave/notes")], &dirs(&["/Users/dave/notes"])).expect("a plan");
        assert_eq!(plan.dir, p("/Users/dave/notes"));
        assert!(plan.entries.is_empty());
    }

    #[test]
    fn siblings_are_all_selected() {
        let plan =
            plan_reveal(&[p("/tmp/one.txt"), p("/tmp/two.txt"), p("/tmp/three.txt")], &dirs(&[])).expect("a plan");
        assert_eq!(plan.dir, p("/tmp"));
        assert_eq!(plan.entries, vec!["one.txt", "two.txt", "three.txt"]);
    }

    #[test]
    fn paths_outside_the_first_ones_directory_are_left_out() {
        // One pane shows one directory. Anything elsewhere is dropped, and `deliver`
        // logs how many.
        let plan = plan_reveal(&[p("/tmp/one.txt"), p("/var/other.txt")], &dirs(&[])).expect("a plan");
        assert_eq!(plan.entries, vec!["one.txt"]);
    }

    #[test]
    fn a_folder_first_ignores_the_rest() {
        let plan = plan_reveal(&[p("/tmp/folder"), p("/tmp/one.txt")], &dirs(&["/tmp/folder"])).expect("a plan");
        assert_eq!(plan.dir, p("/tmp/folder"));
        assert!(plan.entries.is_empty());
    }

    #[test]
    fn nothing_to_show_yields_no_plan() {
        assert!(plan_reveal(&[], &dirs(&[])).is_none());
        // A parentless path (the root itself, reported as a file) can't be revealed.
        assert!(plan_reveal(&[p("/")], &dirs(&[])).is_none());
    }

    #[test]
    fn a_cold_launch_parks_reveals_until_the_frontend_drains_them() {
        let pending = PendingReveals::default();
        assert!(
            pending.accept(finder(&["/tmp/one.txt"])).is_empty(),
            "nothing may be delivered before the frontend is listening"
        );
        assert!(pending.accept(finder(&["/tmp/two.txt"])).is_empty());
        // Back-to-back Finder reveals become one move, so both end up selected.
        assert_eq!(pending.drain(), vec![finder(&["/tmp/one.txt", "/tmp/two.txt"])]);
    }

    #[test]
    fn a_second_reveal_never_erases_the_first() {
        let pending = PendingReveals::default();
        pending.accept(finder(&["/tmp/one.txt"]));
        pending.accept(terminal(&["/tmp/two.txt"], PaneChoice::Right));
        assert_eq!(pending.drain().len(), 2);
    }

    #[test]
    fn terminal_requests_keep_their_own_pane_and_never_merge() {
        let pending = PendingReveals::default();
        pending.accept(terminal(&["/tmp/a"], PaneChoice::Left));
        pending.accept(terminal(&["/tmp/b"], PaneChoice::Right));
        pending.accept(finder(&["/tmp/c"]));
        assert_eq!(
            pending.drain(),
            vec![
                terminal(&["/tmp/a"], PaneChoice::Left),
                terminal(&["/tmp/b"], PaneChoice::Right),
                finder(&["/tmp/c"]),
            ],
            "arrival order, each with its own target"
        );
    }

    #[test]
    fn once_drained_a_reveal_is_delivered_straight_away() {
        let pending = PendingReveals::default();
        pending.drain();
        assert_eq!(
            pending.accept(finder(&["/tmp/one.txt"])),
            vec![finder(&["/tmp/one.txt"])]
        );
        assert!(
            pending.drain().is_empty(),
            "an accepted request must not be queued twice"
        );
    }

    #[test]
    fn file_urls_become_one_finder_reveal() {
        let requests = requests_from_urls(&urls(&["file:///tmp/a.txt", "file:///tmp/b%20c.txt"]));
        assert_eq!(requests, vec![finder(&["/tmp/a.txt", "/tmp/b c.txt"])]);
    }

    #[test]
    fn a_cli_url_becomes_a_terminal_request_with_its_target() {
        let requests = requests_from_urls(&urls(&["cmdr-cli://open?path=%2Ftmp%2Fa&pane=right&tab=new"]));
        assert_eq!(
            requests,
            vec![RevealRequest {
                paths: vec![p("/tmp/a")],
                target: GoToTarget {
                    pane: PaneChoice::Right,
                    new_tab: true
                },
                origin: RevealOrigin::Terminal,
            }]
        );
    }

    #[test]
    fn invalid_cli_urls_and_other_schemes_are_dropped() {
        let requests = requests_from_urls(&urls(&[
            "cmdr-cli://open?path=relative",
            "cmdr-cli://delete?path=%2Ftmp",
            "https://example.com/",
        ]));
        assert!(requests.is_empty());
    }
}
