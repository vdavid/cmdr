//! The viewer as a value: its sessions, the windows that own them, the opens still
//! running, its media tokens, the host it reaches the app through, and the runtime its
//! async volume calls run on. Everything a session touches hangs off one [`Viewer`], so
//! two viewers (two hosts, or two tests) never share a session table.
//!
//! A [`Viewer`] is a cheap handle (`Clone` shares the same state). A background job that
//! outlives the call that started it (an index upgrade, a watcher) holds a clone.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::rt::Runtime;

use super::host::ViewerHost;
use super::media::MediaTokens;
use super::pending_open::{AbandonReason, PendingOpen};
use super::row_text::{RowText, RowView};
use super::session_store::ViewerSession;

/// The file viewer: open sessions and everything they share.
#[derive(Clone)]
pub struct Viewer(Arc<ViewerState>);

struct ViewerState {
    host: ViewerHost,
    /// Where the async volume calls run (`bridge.rs`). Handed over by the host at
    /// creation, ❌ never resolved from an ambient runtime: a reader may drop on a plain
    /// worker thread, which has none.
    runtime: Runtime,
    sessions: Mutex<HashMap<String, ViewerSession>>,
    /// A viewer window label (`viewer-<timestamp>`) to its session id. The window opens
    /// the session, so this link is what lets the window-destroyed handler free it: the
    /// titlebar X never sends `viewer_close`. Without it those sessions (and their
    /// backends, watchers, and line indexes) leaked until app quit.
    windows: Mutex<HashMap<String, String>>,
    /// The in-flight opens that belong to a window, by window label (`pending_open.rs`).
    pending_opens: Mutex<HashMap<String, Arc<PendingOpen>>>,
    tokens: Arc<MediaTokens>,
}

impl Viewer {
    /// A viewer over `host`, running its async volume calls on `runtime`, with a token
    /// table of its own.
    pub fn new(host: ViewerHost, runtime: Runtime) -> Self {
        Self::with_tokens(host, runtime, Arc::new(MediaTokens::new()))
    }

    /// A viewer that mints into `tokens`, a table something else (the app's thumbnails)
    /// mints into too, so one `cmdr-media://` handler serves both.
    pub fn with_tokens(host: ViewerHost, runtime: Runtime, tokens: Arc<MediaTokens>) -> Self {
        Self(Arc::new(ViewerState {
            host,
            runtime,
            sessions: Mutex::new(HashMap::new()),
            windows: Mutex::new(HashMap::new()),
            pending_opens: Mutex::new(HashMap::new()),
            tokens,
        }))
    }

    /// The media tokens this viewer's sessions mint, which the `cmdr-media://` handler
    /// resolves.
    pub fn tokens(&self) -> &Arc<MediaTokens> {
        &self.0.tokens
    }

    pub(super) fn host(&self) -> &ViewerHost {
        &self.0.host
    }

    pub(super) fn runtime(&self) -> &Runtime {
        &self.0.runtime
    }

    pub(super) fn sessions(&self) -> MutexGuard<'_, HashMap<String, ViewerSession>> {
        self.0.sessions.lock_ignore_poison()
    }

    /// The session table, or `None` when a panic poisoned it: for a background job
    /// that has nothing to do once the table is gone.
    pub(super) fn sessions_if_healthy(&self) -> Option<MutexGuard<'_, HashMap<String, ViewerSession>>> {
        // allowed-lock-poison: a background job's bookkeeping (an index timeout, a read's unregister) skips a poisoned table, as it always has
        self.0.sessions.lock().ok()
    }

    pub(super) fn windows(&self) -> MutexGuard<'_, HashMap<String, String>> {
        self.0.windows.lock_ignore_poison()
    }

    pub(super) fn pending_opens(&self) -> MutexGuard<'_, HashMap<String, Arc<PendingOpen>>> {
        self.0.pending_opens.lock_ignore_poison()
    }

    /// How a session asking for `rows` shows them, with this host's log transform.
    pub(super) fn row_view(&self, rows: RowText) -> RowView {
        RowView::new(rows, self.0.host.log_display)
    }

    /// Drops `token` and tells the host, so a host that minted something for it (the
    /// browser demo's `blob:` URL) can let it go.
    pub(super) fn drop_token(&self, token: &str) {
        self.0.tokens.drop_token(token);
        self.0.host.events.media_token_dropped(token);
    }

    /// Starts watching an open for `window_label`, so closing that window abandons it. A
    /// newer open for the same window replaces (and abandons) an older one. An empty label
    /// has no window to close, so its open is watched but not registered.
    pub fn begin_pending_open(&self, window_label: &str) -> Arc<PendingOpen> {
        let open = Arc::new(PendingOpen::new());
        if !window_label.is_empty()
            && let Some(older) = self.pending_opens().insert(window_label.to_string(), Arc::clone(&open))
        {
            older.abandon(AbandonReason::Cancelled);
        }
        open
    }

    /// Stops watching `open` for `window_label`, once its waiter is done with it. Leaves a
    /// newer open for the same window registered.
    pub fn end_pending_open(&self, window_label: &str, open: &Arc<PendingOpen>) {
        let mut opens = self.pending_opens();
        if opens
            .get(window_label)
            .is_some_and(|current| Arc::ptr_eq(current, open))
        {
            opens.remove(window_label);
        }
    }

    /// Abandons the in-flight open of a window that closed, if it has one.
    pub(super) fn abandon_pending_open_for_window(&self, window_label: &str) {
        let open = self.pending_opens().remove(window_label);
        if let Some(open) = open {
            open.abandon(AbandonReason::Cancelled);
        }
    }
}
