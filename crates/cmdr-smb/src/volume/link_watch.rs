//! Watching a DFS link's target while a pane shows a folder behind it.
//!
//! The share's `CHANGE_NOTIFY` (`watcher.rs`) runs on the share's own server,
//! and a link's target is another server (or share) that never reports to it.
//! So while a listing behind a link is open, the link gets a watch of its own:
//!
//! - **Held per listing.** The listing engine asks `Volume::watch_listing` on
//!   open and drops the answer on close; [`LinkWatches`] counts the holds per
//!   link path (the `LinkPaths` entry the listing sits under) and stops the
//!   watch when the last one goes. Nothing is watched behind a link nobody
//!   shows.
//! - **Its own session.** Each watched link opens a dedicated session to the
//!   share's server (`build_session`), resolves the link through it (smb2's
//!   `resolve_dfs_link`), and runs the `CHANGE_NOTIFY` on the target's
//!   connection, pooled inside that client. Like the share's watcher,
//!   it never multiplexes with the main session's traffic.
//! - **Same pipeline.** Events go through `watcher::pump_events` with the link
//!   path as their root, so a change lands under the path the pane keyed its
//!   listing on, and the follow-up `stat` rides the main session (through the
//!   link).
//! - **A loss is the link's alone.** ❌ It never touches the main session's
//!   state or the index's freshness (neither ever relied on it). The watch
//!   reconnects itself on the watcher's backoff and, once back, re-reads the
//!   open listings behind the link, since changes there went unheard. Until
//!   then, and after it gives up, `listing_watch_coverage` answers `None` there,
//!   so the fresh-listing oracle reads again. A listing that opens later starts
//!   a watch that gave up again.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock, Weak};
use std::time::{Duration, Instant};

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::{DirectoryChange, ListingWatch, SelfHandle, WatchCoverage};
use log::{debug, info, warn};
use smb2::SmbClient;
use tokio::sync::oneshot;

use super::dfs_links::{is_at_or_below, target_of};
use super::reconnect::WATCHER_DEATH_RECONNECT_BACKOFF;
use super::session_build::{build_session, retired_session_id};
use super::state::ConnectionState;
use super::watcher::{EventFeed, WatchEnd, pump_events, to_display_path};
use super::{MountAnchor, SmbConnectionParams, SmbVolume, SmbVolumeInner};

/// How long a link watch has to have run before its loss starts the backoff
/// over: a watch that arms and drops right away keeps backing off, so a
/// flapping target can't hold it in a fast retry loop.
const LINK_WATCH_STEADY: Duration = Duration::from_secs(60);

/// The share's watched links, by the link path each watch sits on.
#[derive(Default)]
pub(super) struct LinkWatches {
    links: std::sync::Mutex<HashMap<String, LinkWatch>>,
}

/// One watched link: how many open listings hold it, and its task.
struct LinkWatch {
    holds: usize,
    /// Dropping it ends the task (its `pump_events` hears the cancel).
    _cancel: oneshot::Sender<()>,
    status: Arc<LinkWatchStatus>,
}

/// What a link watch's task reports back.
#[derive(Default)]
struct LinkWatchStatus {
    /// The watch is armed: changes behind the link reach the panes.
    live: AtomicBool,
    /// The task is still running (arming, watching, or backing off). A task
    /// that gave up leaves its entry held but not running, and the next hold
    /// starts a new one.
    running: AtomicBool,
}

/// A listing's hold on its link's watch, released on drop.
struct LinkWatchHold {
    watches: Weak<LinkWatches>,
    root: String,
}

impl Drop for LinkWatchHold {
    fn drop(&mut self) {
        if let Some(watches) = self.watches.upgrade() {
            watches.release(&self.root);
        }
    }
}

/// What a newly held link needs a task for.
struct StartLinkWatch {
    cancel_rx: oneshot::Receiver<()>,
    status: Arc<LinkWatchStatus>,
}

impl LinkWatches {
    /// Takes a hold on the watch at `root`, and says when a task has to start:
    /// the link's first hold, or one whose task gave up.
    fn hold(self: &Arc<Self>, root: &str) -> (LinkWatchHold, Option<StartLinkWatch>) {
        let mut links = self.links.lock_ignore_poison();
        let start = match links.get_mut(root) {
            Some(watch) if watch.status.running.load(Ordering::Acquire) => {
                watch.holds += 1;
                None
            }
            existing => {
                let (cancel, cancel_rx) = oneshot::channel();
                let status = Arc::new(LinkWatchStatus::default());
                status.running.store(true, Ordering::Release);
                let holds = existing.map_or(0, |watch| watch.holds) + 1;
                let watch = LinkWatch {
                    holds,
                    _cancel: cancel,
                    status: Arc::clone(&status),
                };
                links.insert(root.to_string(), watch);
                Some(StartLinkWatch { cancel_rx, status })
            }
        };
        let hold = LinkWatchHold {
            watches: Arc::downgrade(self),
            root: root.to_string(),
        };
        (hold, start)
    }

    /// Gives back one hold on `root`; the last one stops its watch.
    fn release(&self, root: &str) {
        let mut links = self.links.lock_ignore_poison();
        let Some(watch) = links.get_mut(root) else {
            return; // `stop_all` already took it
        };
        watch.holds = watch.holds.saturating_sub(1);
        if watch.holds == 0 {
            let stopped = links.remove(root);
            drop(links);
            drop(stopped); // outside the lock: drops the cancel sender
        }
    }

    /// Whether a live watch covers share path `smb_path`.
    pub(super) fn is_live_for(&self, smb_path: &str) -> bool {
        self.links
            .lock_ignore_poison()
            .iter()
            .any(|(root, watch)| watch.status.live.load(Ordering::Acquire) && is_at_or_below(smb_path, root))
    }

    /// Stops every link watch, for a share that's unmounted or superseded. The
    /// holds still out there release nothing.
    pub(super) fn stop_all(&self) {
        let stopped = std::mem::take(&mut *self.links.lock_ignore_poison());
        drop(stopped);
    }
}

impl SmbVolume {
    /// `Volume::listing_watch_coverage` for a path behind a DFS link, or
    /// `None` when it sits behind none. The link's target never reports to the
    /// share's watch, so only the link's own covers it, while it's live.
    pub(super) fn link_watch_coverage(&self, path: &Path) -> Option<WatchCoverage> {
        let smb_path = self.to_smb_path(path).ok()?;
        if !self.inner.link_paths.covers(&smb_path) {
            return None;
        }
        let live = self.inner.link_watches.is_live_for(&smb_path);
        Some(match live && self.session_state() == ConnectionState::Direct {
            true => WatchCoverage::EveryWriter,
            false => WatchCoverage::None,
        })
    }

    /// `Volume::watch_listing`: a hold on the watch of the DFS link `path` sits
    /// behind, or `None` when it sits behind none (the share's own watch covers
    /// it). Cheap: the watch arms on a task of its own.
    pub(super) fn hold_link_watch(&self, path: &Path) -> Option<ListingWatch> {
        let inner = &self.inner;
        if inner.is_retired() || inner.unmounted.load(Ordering::Relaxed) {
            return None;
        }
        let smb_path = self.to_smb_path(path).ok()?;
        let root = inner.link_paths.root_for(&smb_path)?;
        let (hold, start) = inner.link_watches.hold(&root);
        if let Some(StartLinkWatch { cancel_rx, status }) = start {
            let link = WatchedLink {
                share: inner.self_handle(),
                host: inner.host.clone(),
                volume_id: inner.volume_id.clone(),
                mount_path: Arc::clone(&inner.active_mount_path),
                params: Arc::clone(&inner.params),
                label: inner.share_name.clone(),
                root,
                status,
            };
            // The app's runtime, never `tokio::spawn`: the listing engine may
            // ask from a thread with no reactor.
            inner.host.runtime().spawn(run_link_watch(link, cancel_rx));
        }
        Some(ListingWatch::new(hold))
    }
}

/// Everything a link watch's task carries.
struct WatchedLink {
    share: SelfHandle<SmbVolumeInner>,
    host: VolumeHost,
    volume_id: String,
    mount_path: Arc<RwLock<PathBuf>>,
    params: Arc<tokio::sync::RwLock<SmbConnectionParams>>,
    label: String,
    /// The share path the watch sits on (a `LinkPaths` entry).
    root: String,
    status: Arc<LinkWatchStatus>,
}

impl WatchedLink {
    /// The share, while a watch is still worth keeping for it.
    fn share_is_live(&self) -> bool {
        self.share
            .live()
            .is_some_and(|inner| !inner.unmounted.load(Ordering::Relaxed))
    }

    /// Re-reads every open listing at or below the link, whose changes may
    /// have gone unheard.
    fn refresh_open_listings(&self, anchor: &MountAnchor) {
        let Some(root) = to_display_path(anchor, &self.root) else {
            return;
        };
        let listings = self.host.listings();
        for path in listings.open_listing_paths(&self.volume_id) {
            if path.starts_with(&root) {
                listings.directory_changed(&self.volume_id, &path, DirectoryChange::FullRefresh);
            }
        }
    }

    fn feed(&self) -> EventFeed<'_> {
        EventFeed {
            host: &self.host,
            volume_id: &self.volume_id,
            share: &self.share,
            mount_path: &self.mount_path,
            root: &self.root,
            label: &self.label,
        }
    }
}

/// Clears the task's status as it ends, however it ends.
struct Running(Arc<LinkWatchStatus>);

impl Drop for Running {
    fn drop(&mut self) {
        self.0.live.store(false, Ordering::Release);
        self.0.running.store(false, Ordering::Release);
    }
}

/// Opens a session for the watch, resolves the link `root` sits behind
/// (`dfs_links::target_of`), and watches its target recursively. The watch
/// runs on the target's connection, pooled inside the returned client, which
/// has to outlive it.
async fn arm(
    params: &SmbConnectionParams,
    replaces: Option<smb2::types::SessionId>,
    root: &str,
) -> Result<(SmbClient, smb2::Watcher), smb2::Error> {
    let (mut client, share) = build_session(params, replaces).await?;
    let (tree, path) = target_of(&mut client, &share, root).await?;
    let watcher = client.watch(&tree, &path, true).await?;
    Ok((client, watcher))
}

/// Watches one link until its last hold goes, the share goes, or the backoff
/// runs out. See the module docs.
async fn run_link_watch(link: WatchedLink, mut cancel_rx: oneshot::Receiver<()>) {
    let _running = Running(Arc::clone(&link.status));
    let mut replaces = None;
    let mut armed_before = false;
    let mut failures = 0usize;
    loop {
        if !link.share_is_live() {
            return;
        }
        let params = link.params.read().await.clone();
        // A listing closing mid-arm drops the half-built session with it.
        let armed = tokio::select! {
            armed = arm(&params, replaces.take(), &link.root) => armed,
            _ = &mut cancel_rx => return,
        };
        match armed {
            Ok((client, watcher)) => {
                info!(
                    "smb_link_watch share={:?} link={:?}: watching",
                    LogPrivate::volume(&link.label),
                    LogPrivate::dir_path(Path::new(&link.root))
                );
                link.status.live.store(true, Ordering::Release);
                if armed_before {
                    link.refresh_open_listings(&link.feed().anchor());
                }
                armed_before = true;
                let armed_at = Instant::now();
                let end = pump_events(watcher, &mut cancel_rx, &link.feed(), |anchor| {
                    if let Some(inner) = link.share.live() {
                        inner.spellings.forget_all();
                    }
                    link.refresh_open_listings(anchor);
                })
                .await;
                link.status.live.store(false, Ordering::Release);
                let WatchEnd::Lost { error, watcher } = end else {
                    return; // the last hold went
                };
                warn!(
                    "smb_link_watch share={:?} link={:?}: lost ({:?}); the share's own session is unaffected",
                    LogPrivate::volume(&link.label),
                    LogPrivate::dir_path(Path::new(&link.root)),
                    error.kind()
                );
                let _ = watcher.close().await;
                replaces = retired_session_id(&client);
                if armed_at.elapsed() >= LINK_WATCH_STEADY {
                    failures = 0;
                }
            }
            // Nobody allowed signing in there, so trying again can't help until
            // someone does, and then the next listing opens a new watch.
            Err(error)
                if crate::errors::is_auth_error(&error) || matches!(error, smb2::Error::DfsTargetNotAllowed { .. }) =>
            {
                // The main session owns the credential dance; a refused watch
                // just leaves the listings behind the link re-reading.
                info!(
                    "smb_link_watch share={:?} link={:?}: refused ({:?}); not watching",
                    LogPrivate::volume(&link.label),
                    LogPrivate::dir_path(Path::new(&link.root)),
                    error.kind()
                );
                return;
            }
            Err(error) => {
                debug!(
                    "smb_link_watch share={:?} link={:?}: couldn't arm ({:?})",
                    LogPrivate::volume(&link.label),
                    LogPrivate::dir_path(Path::new(&link.root)),
                    error.kind()
                );
            }
        }
        let Some(delay) = WATCHER_DEATH_RECONNECT_BACKOFF.get(failures) else {
            info!(
                "smb_link_watch share={:?} link={:?}: still down, giving up until a listing opens there again",
                LogPrivate::volume(&link.label),
                LogPrivate::dir_path(Path::new(&link.root))
            );
            return;
        };
        failures += 1;
        tokio::select! {
            _ = tokio::time::sleep(*delay) => {}
            _ = &mut cancel_rx => return,
        }
    }
}

#[cfg(test)]
#[path = "link_watch_test.rs"]
mod link_watch_test;
