//! What arrives once the `NSFileViewer` key is ours: the cold-launch buffer, the
//! plan a set of revealed paths becomes, and the pane move that carries it out.
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

use crate::ignore_poison::IgnorePoison;

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

/// Where one reveal request puts the focused pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RevealPlan {
    /// The directory the pane navigates to.
    pub dir: PathBuf,
    /// Entries to select there, cursor on the first. Empty when the request named a
    /// folder, which is opened rather than pointed at.
    pub entries: Vec<String>,
}

/// Reveal targets that arrived before the frontend could act on them.
///
/// A cold launch delivers the event long before the webview mounts, so the arriving
/// paths are parked here and the frontend drains them once its listeners are up
/// (`commands::drain_pending_reveals`). A `Vec`, not an `Option`: two reveals can land
/// back to back during a cold launch, and the second must not erase the first.
#[derive(Default)]
pub struct PendingReveals {
    queue: Mutex<Vec<PathBuf>>,
    /// Set by the first drain. Until then nothing is delivered, because there is nobody
    /// listening for `mcp-nav-to-path` yet and the emit would go nowhere.
    frontend_ready: AtomicBool,
}

impl PendingReveals {
    /// Park `paths` and report whether they can be delivered right now.
    fn accept(&self, paths: Vec<PathBuf>) -> Vec<PathBuf> {
        let mut queue = self.queue.lock_ignore_poison();
        queue.extend(paths);
        if self.frontend_ready.load(Ordering::Acquire) {
            std::mem::take(&mut queue)
        } else {
            Vec::new()
        }
    }

    /// Mark the frontend live and take everything parked so far.
    pub(super) fn drain(&self) -> Vec<PathBuf> {
        self.frontend_ready.store(true, Ordering::Release);
        std::mem::take(&mut *self.queue.lock_ignore_poison())
    }
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
/// returns when there is no window, and the buffer holds the paths. `DETAILS.md` § "How a
/// cold-launch reveal arrives".
///
/// Called on the main thread, so it must not block: the classify probe and the pane
/// round-trip both happen on the async runtime.
pub fn on_urls_opened(app: &AppHandle<Wry>, urls: Vec<tauri::Url>) {
    let paths: Vec<PathBuf> = urls
        .iter()
        .filter(|url| url.scheme() == "file")
        .filter_map(|url| url.to_file_path().ok())
        .collect();
    if paths.is_empty() {
        log::debug!(target: "reveal", "A reveal carried no file paths; ignoring");
        return;
    }
    log::info!(target: "reveal", "Reveal: {} path(s) to show", paths.len());
    raise_main_window(app);
    let ready = PENDING.accept(paths);
    if !ready.is_empty() {
        spawn_delivery(app, ready);
    }
}

/// Bring the main window forward. The reveal was fired from another app, so without this
/// the pane lands behind whatever the user was looking at. Same three calls as the
/// go-to-latest-download hotkey (`downloads/global_shortcut.rs`).
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

/// Run a delivery on the async runtime, off whatever thread asked for it.
pub(super) fn spawn_delivery<R: Runtime>(app: &AppHandle<R>, paths: Vec<PathBuf>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { deliver(&app, paths).await });
}

/// Move the focused pane onto `paths`.
async fn deliver<R: Runtime>(app: &AppHandle<R>, paths: Vec<PathBuf>) {
    let requested = paths.len();
    let probe = paths.clone();
    let plan = crate::deadline::blocking_with_timeout(CLASSIFY_TIMEOUT, None, move || {
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
    if !plan.entries.is_empty() && plan.entries.len() < requested {
        log::info!(
            target: "reveal",
            "Showing {} of {requested} revealed path(s); the rest sit in other directories",
            plan.entries.len(),
        );
    }
    let dir = plan.dir.to_string_lossy().to_string();
    if let Err(err) = crate::mcp::go_to_in_focused_pane(app, &dir, &plan.entries).await {
        log::warn!(target: "reveal", "Couldn't show {dir}: {}", err.message);
        return;
    }
    // Announced only once the pane has actually moved: the frontend turns the first of
    // these into a once-ever notice, and a reveal that went nowhere would spend it.
    if let Err(err) = (RevealDelivered {}).emit(app) {
        log::warn!(target: "reveal", "Couldn't announce the delivered reveal: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(paths: &[&str]) -> impl Fn(&Path) -> bool + use<> {
        let dirs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        move |path: &Path| dirs.iter().any(|d| d == path)
    }

    fn p(path: &str) -> PathBuf {
        PathBuf::from(path)
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
    fn a_cold_launch_parks_paths_until_the_frontend_drains_them() {
        let pending = PendingReveals::default();
        assert!(
            pending.accept(vec![p("/tmp/one.txt")]).is_empty(),
            "nothing may be delivered before the frontend is listening"
        );
        assert!(pending.accept(vec![p("/tmp/two.txt")]).is_empty());
        assert_eq!(pending.drain(), vec![p("/tmp/one.txt"), p("/tmp/two.txt")]);
    }

    #[test]
    fn a_second_reveal_never_erases_the_first() {
        let pending = PendingReveals::default();
        pending.accept(vec![p("/tmp/one.txt")]);
        pending.accept(vec![p("/tmp/two.txt")]);
        assert_eq!(pending.drain().len(), 2);
    }

    #[test]
    fn once_drained_a_reveal_is_delivered_straight_away() {
        let pending = PendingReveals::default();
        pending.drain();
        assert_eq!(pending.accept(vec![p("/tmp/one.txt")]), vec![p("/tmp/one.txt")]);
        assert!(pending.drain().is_empty(), "an accepted path must not be queued twice");
    }
}
