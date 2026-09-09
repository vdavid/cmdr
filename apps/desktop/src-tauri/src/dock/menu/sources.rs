//! Where the Dock menu's two lists come from, and why reading them can't stall.
//!
//! Both are read LIVE, at the moment the Dock asks, so the menu is never stale. That
//! is only safe because neither read touches a disk, a network, or a lock a slow
//! thread can hold:
//!
//! - **Bookmarks** come out of the favorites store's in-memory cache through
//!   `list_cached`, which `try_lock`s and answers `None` rather than waiting. The
//!   ordinary `list` is ❌ off limits here: on a cold cache it reads the file, seeds
//!   it, and holds the store's disk lock while it does.
//! - **Tabs** come out of the backend's pane mirror (`mcp::pane_state`), which the
//!   frontend pushes on every tab change. `tabs_focused_first` `try_read`s all three
//!   locks and answers `None` rather than waiting; the writers only ever move a value
//!   in memory, so a contended read is already vanishingly unlikely.
//!
//! An unavailable source contributes no rows. That is deliberate: a Dock menu missing
//! its bookmarks for one right-click is a nuisance, and every alternative (waiting,
//! reading a file, keeping a snapshot that drifts) is worse.

use tauri::{AppHandle, Manager};

use super::rows::Candidate;
use crate::mcp::pane_state::PaneStateStore;

/// The favorites, in the order the user keeps them.
pub fn bookmarks() -> Vec<Candidate> {
    crate::favorites::store::list_cached()
        .unwrap_or_default()
        .into_iter()
        .map(|favorite| Candidate {
            name: Some(favorite.name),
            path: favorite.path,
        })
        .collect()
}

/// The tabs both panes have open, the focused pane's first.
///
/// Focused-first because that is the half of the app the user was last looking at,
/// which is the same order `priority::roots` ranks them in for the index.
pub fn tabs(app: &AppHandle) -> Vec<Candidate> {
    let Some(store) = app.try_state::<PaneStateStore>() else {
        return Vec::new();
    };
    store
        .tabs_focused_first()
        .unwrap_or_default()
        .into_iter()
        .map(|tab| Candidate {
            name: None,
            path: tab.path,
        })
        .collect()
}
