//! The app's half of the git portal: the `git-state-changed` event, the sink
//! that produces it, and the listing refreshes a repo's change drives.
//!
//! The watcher (`watcher.rs`) knows that a repository's state moved and what it
//! says now. Everything a USER then sees is here: the `tauri_specta` payload the
//! breadcrumb chip subscribes to, and the re-read of every open pane standing in
//! a virtual `.git` tree.
//!
//! ❗ A struct name kebab-cases to its wire event name, so renaming
//! [`GitStateChangedPayload`] silently renames the event the frontend listens
//! for. `ipc.rs` registers it in `collect_events!`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_specta::Event;

use super::path::Cat;
use super::repo::RepoInfo;
use super::state_sink::{GitStateSink, no_git_state_sink};

/// Typed `git-state-changed` Tauri event. Carries the repo root and a fresh
/// `RepoInfo` snapshot. The `…Payload` suffix wouldn't kebab-case to the existing
/// wire string, so the name is pinned via `event_name`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "git-state-changed")]
pub struct GitStateChangedPayload {
    pub repo_root: String,
    pub info: RepoInfo,
}

/// Turns a watcher report into what the app does about it: emit the event the
/// chip subscribes to, and re-read every open virtual `.git` listing.
pub struct TauriGitStateSink {
    app: AppHandle,
}

impl TauriGitStateSink {
    /// A sink reporting into `app`.
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl GitStateSink for TauriGitStateSink {
    fn repo_changed(&self, repo_root: &Path, info: RepoInfo) {
        let payload = GitStateChangedPayload {
            repo_root: repo_root.display().to_string(),
            info,
        };
        let _ = payload.emit(&self.app);
        refresh_virtual_listings(repo_root);
    }
}

/// The sink [`install_git_state_sink`] parked.
static STATE_SINK: OnceLock<Arc<dyn GitStateSink>> = OnceLock::new();

/// Parks the sink every `.git/*` watcher reports into, aimed at `app`.
///
/// Call once at startup, before a pane can subscribe. A second call keeps the
/// first sink and is ignored, so a test fixture and the app wiring can both call
/// it without fighting.
pub fn install_git_state_sink(app: &AppHandle) {
    if STATE_SINK
        .set(Arc::new(TauriGitStateSink::new(app.clone())) as Arc<dyn GitStateSink>)
        .is_err()
    {
        log::debug!(target: "git", "the git state sink was already installed; keeping the first one");
    }
}

/// The sink the app's watcher subscriptions report into.
///
/// Falls back to the detached sink so a test binary that never runs `setup()`
/// still subscribes, watches, and recomputes — it just has no window to tell.
pub fn git_state_sink() -> Arc<dyn GitStateSink> {
    Arc::clone(STATE_SINK.get_or_init(no_git_state_sink))
}

/// Re-reads any open virtual `.git/{branches,tags,commits,stash,worktrees,submodules}/…`
/// listing for `repo_root`, so a pane standing in one picks up a ref change.
pub(crate) fn refresh_virtual_listings(repo_root: &Path) {
    let dot_git = repo_root.join(".git");
    refresh_local_listings_under(&virtual_category_prefixes(&dot_git));
}

/// Builds prefixes for every virtual subtree under `<dot_git>/`. Any listing
/// path starting with any prefix is a virtual portal listing.
pub(crate) fn virtual_category_prefixes(dot_git: &Path) -> Vec<PathBuf> {
    Cat::ALL.iter().map(|c| dot_git.join(c.as_segment())).collect()
}

/// Iterates the listing cache and emits `FullRefresh` for any listing whose
/// path matches any of `prefixes` (prefix-match, including the prefix itself).
///
/// ❗ Every volume, not only the boot one. A repo lives just as happily on an
/// external disk or an OS-mounted share, and those get their own volume ids;
/// filtering to the default volume left an open portal pane on one showing
/// stale children after a `git checkout`. The prefixes are absolute host paths
/// under a real `.git`, which a protocol-only volume's paths can never match.
pub(crate) fn refresh_local_listings_under(prefixes: &[PathBuf]) {
    use crate::file_system::listing::caching::{DirectoryChange, notify_directory_changed};

    for (volume_id, listing_path) in listings_under(prefixes) {
        notify_directory_changed(&volume_id, &listing_path, DirectoryChange::FullRefresh);
    }
}

/// The `(volume_id, path)` pairs [`refresh_local_listings_under`] would refresh:
/// every cached listing whose path is one of `prefixes` or sits under one.
///
/// Split out from the refresh so the choice can be asserted on without an
/// `AppHandle` (`notify_directory_changed` is a no-op before one is registered).
pub(crate) fn listings_under(prefixes: &[PathBuf]) -> Vec<(String, PathBuf)> {
    use crate::file_system::listing::caching::{find_listings_for_path_on_volume, get_listing_path, snapshot_listings};

    if prefixes.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for entry in snapshot_listings() {
        let Some(listing_path) = get_listing_path(&entry.listing_id) else {
            continue;
        };
        let matches = prefixes
            .iter()
            .any(|prefix| listing_path.starts_with(prefix) || *prefix == listing_path);
        if !matches {
            continue;
        }
        if !find_listings_for_path_on_volume(Some(&entry.volume_id), &listing_path).is_empty() {
            out.push((entry.volume_id, listing_path));
        }
    }
    out
}

/// Refreshes every open listing the portal toggle can change: a repo's `.git/`
/// itself (whose rows gain or lose the six categories) and anything under it.
/// Called when the user flips the setting, so panes already showing one pick the
/// change up without a manual reload.
///
/// **Asks the LISTING CACHE which listings those are, ❌ never the watcher
/// registry.** A pane standing in `.git/` doesn't imply a `subscribe_git_state`
/// for that repo, so deriving the set from subscribed repos left the pane the
/// user was looking at showing six rows the portal no longer serves (caught by
/// `git-portal.spec.ts`, 2026-09-05).
///
/// Over-selecting here is a re-read and nothing more, which is why a path-shape
/// check is the right instrument: ❗ it decides what to RE-READ, not what a
/// mutation may touch. That distinction is exactly what the deleted
/// `local_posix` guards got wrong.
pub fn refresh_all_virtual_listings_after_toggle() {
    use crate::file_system::listing::caching::{DirectoryChange, notify_directory_changed};

    for (volume_id, path) in listings_inside_a_dot_git() {
        notify_directory_changed(&volume_id, &path, DirectoryChange::FullRefresh);
    }
}

/// Every cached listing whose path IS a `.git` directory or sits inside one.
pub(crate) fn listings_inside_a_dot_git() -> Vec<(String, PathBuf)> {
    use crate::file_system::listing::caching::{get_listing_path, snapshot_listings};
    use std::path::Component;

    snapshot_listings()
        .into_iter()
        .filter_map(|entry| get_listing_path(&entry.listing_id).map(|path| (entry.volume_id, path)))
        .filter(|(_, path)| {
            path.components()
                .any(|component| matches!(component, Component::Normal(segment) if segment == ".git"))
        })
        .collect()
}
