//! Following a DFS link inside the share. Every call here drives smb2's
//! lock-free `Tree` API on a cloned connection, and that API doesn't follow
//! links.
//!
//! A share can hold an `msdfs:` link to a folder on another server (smb2's
//! `smb-dfs-root` fixture is one). smb2 follows a link on every `SmbClient`
//! call that takes a path (vdavid/smb2#15), and resolves one on request
//! (`SmbClient::resolve_dfs_link`, vdavid/smb2#27), but both take
//! `&mut SmbClient`, so they hold the session's client. So a path under a link
//! already known ([`LinkPaths`]) resolves from smb2's referral cache and goes
//! straight to the target; any other path runs on the `Tree` first, the way it
//! always has, and only a CREATE the server refuses with
//! `STATUS_PATH_NOT_COVERED` (the one answer that means "behind a link",
//! [`is_behind_dfs_link`]) resolves the link and runs again there, holding the
//! client for that one call (an open, for anything that streams): the answer,
//! or a handle that owns the target's connection.
//!
//! - **Main-session calls** (listings, `stat`s, the copy scan, create,
//!   delete) go through [`SmbVolume::or_through_link`], a rename through
//!   [`SmbVolume::around_link`] with `LinkRoute::ShareTree` (both ends are share paths); the scan
//!   pool's listings through their member's own client (`scan_pool.rs`).
//! - **A link a call followed is remembered** by its link folder
//!   ([`LinkPaths`]): the next call under it skips the refusal, and the
//!   share's watch never hears its target, so `link_watch.rs` watches it while
//!   a pane shows a folder there.
//!
//! - **Writes** open a `FileWriter` there (`'static`), or send the one compound
//!   frame there, and set the dates through the same client.
//! - **Reads** open smb2's `FileDownload` there (`open_file_download`, which
//!   borrows the client for the open only and owns the target's connection
//!   after), so a download through a link keeps the adaptive read-ahead window.
//! - **A server-side copy through a link** answers `NotSupported` (`copy.rs`),
//!   and the engine streams the file through the paths above.
//! - The caller's tree never moves (smb2's own rule): the next path on it still
//!   means the caller's share.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use cmdr_fs::volume::WriteMode;
use smb2::SmbClient;
use smb2::client::stream::{FileDownload, FileReader, FileWriter};
use smb2::client::tree::{FileInfo, Tree};

use cmdr_fs::ignore_poison::IgnorePoison;

use super::session::{LeaseOn, MainSession};
use super::state::{RanOn, SessionGeneration};
use super::{SmbVolume, SmbVolumeInner};

/// A call to run again through a DFS link: it gets the session's client, a
/// clone of the share's tree, and the share path, and OWNS anything else it
/// needs, so its future borrows only those three.
pub(super) type LinkCall<'c, T> = Pin<Box<dyn Future<Output = Result<T, smb2::Error>> + Send + 'c>>;

/// The link folders a call followed a link through, each with no remembered
/// ancestor, so the list stays one entry per link a person walked into.
///
/// What it's for: a call under one goes straight to the target
/// ([`SmbVolume::or_through_link`]), and the share's `CHANGE_NOTIFY` watch
/// never hears a link's target, so a listing at or below one is kept fresh
/// only by the link's own watch (`link_watch.rs`). Kept for the share's life:
/// a link that disappears only costs a resolve that finds nothing and a re-read
/// the oracle could have saved.
#[derive(Default)]
pub(super) struct LinkPaths(std::sync::Mutex<Vec<String>>);

impl LinkPaths {
    /// Remembers that `smb_path` sits behind a link, unless a path above it
    /// already says so; any remembered path below it gives way.
    pub(super) fn note(&self, smb_path: &str) {
        let mut paths = self.0.lock_ignore_poison();
        if paths.iter().any(|known| is_at_or_below(smb_path, known)) {
            return;
        }
        paths.retain(|known| !is_at_or_below(known, smb_path));
        paths.push(smb_path.to_string());
    }

    /// The remembered path `smb_path` is at or below, which is where a watch
    /// for it sits (`link_watch.rs`).
    pub(super) fn root_for(&self, smb_path: &str) -> Option<String> {
        self.0
            .lock_ignore_poison()
            .iter()
            .find(|known| is_at_or_below(smb_path, known))
            .cloned()
    }

    /// Whether `smb_path` is at or below a path that followed a link.
    pub(super) fn covers(&self, smb_path: &str) -> bool {
        self.0
            .lock_ignore_poison()
            .iter()
            .any(|known| is_at_or_below(smb_path, known))
    }
}

/// Whether share path `path` is `ancestor` or inside it, by whole components
/// (`data2` isn't inside `data`). Byte-exact: SMB names are opaque.
pub(super) fn is_at_or_below(path: &str, ancestor: &str) -> bool {
    path.strip_prefix(ancestor)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

impl SmbVolume {
    /// A main-session call on `smb_path`, followed through the DFS link it
    /// sits behind: `first` (the call on the share's tree), or `op` on the
    /// link's target.
    ///
    /// - **A path under a known link** ([`LinkPaths`]) skips `first`: smb2's
    ///   resolver answers from its referral cache, and `op` goes straight to
    ///   the target, with no refused round trip. A link that's gone runs
    ///   `first` after all.
    /// - **Any other path** runs `first`; only a refusal as behind a link
    ///   ([`is_behind_dfs_link`]) resolves the link (one referral round trip,
    ///   cached) and runs `op` there, noting the link folder.
    ///
    /// `op` gets the main client, the TARGET's tree, and the path on it. Holds
    /// the session's client for the resolve and `op`'s round trips only (an
    /// open, for a listing or a reader, whose handle then owns the target's
    /// connection). `generation` is the lease's: once a reconnect replaced that
    /// session, its tree means nothing on the new client, so the call stands as
    /// it came. Whatever `op` answers funnels as the main session's
    /// (`RanOn::Main`), and a target that's gone arrives as smb2's
    /// `DfsTargetUnreachable`, which doesn't end it: smb2 lets the dead target
    /// go and the next call through the link dials it again.
    pub(super) async fn or_through_link<T, F>(
        &self,
        generation: SessionGeneration,
        smb_path: &str,
        first: impl Future<Output = Result<T, smb2::Error>>,
        op: F,
    ) -> Result<T, smb2::Error>
    where
        F: for<'c> FnOnce(&'c mut SmbClient, &'c mut Tree, &'c str) -> LinkCall<'c, T>,
    {
        self.around_link(LinkRoute::Target, generation, smb_path, first, op)
            .await
    }

    /// The decision behind [`or_through_link`](Self::or_through_link), with
    /// where `op` runs as `route`. A rename takes [`LinkRoute::ShareTree`]: its
    /// destination is a share path too, so smb2 follows the link with both ends
    /// (a move between folders inside one link included, smb2 0.36.2).
    pub(super) async fn around_link<T, F>(
        &self,
        route: LinkRoute,
        generation: SessionGeneration,
        smb_path: &str,
        first: impl Future<Output = Result<T, smb2::Error>>,
        op: F,
    ) -> Result<T, smb2::Error>
    where
        F: for<'c> FnOnce(&'c mut SmbClient, &'c mut Tree, &'c str) -> LinkCall<'c, T>,
    {
        if self.inner.link_paths.covers(smb_path) {
            return match self.on_main_session(route, generation, smb_path, op).await {
                Some(result) => result,
                None => first.await,
            };
        }
        let refusal = match first.await {
            Err(e) if is_behind_dfs_link(&e) => e,
            answered => return answered,
        };
        self.on_main_session(route, generation, smb_path, op)
            .await
            .unwrap_or(Err(refusal))
    }

    /// Runs `op` through the main client where `route` says, or `None` once a
    /// reconnect replaced the session `generation` names. For
    /// [`LinkRoute::Target`] it's also `None` when there's no link to follow
    /// (none there, or the referral failed); a link with no reachable target
    /// is that error, as smb2 answers it.
    async fn on_main_session<T, F>(
        &self,
        route: LinkRoute,
        generation: SessionGeneration,
        smb_path: &str,
        op: F,
    ) -> Option<Result<T, smb2::Error>>
    where
        F: for<'c> FnOnce(&'c mut SmbClient, &'c mut Tree, &'c str) -> LinkCall<'c, T>,
    {
        let mut guard = self.inner.session.lock().await;
        let session = guard.as_mut().filter(|s| s.generation == generation)?;
        if route == LinkRoute::ShareTree {
            let mut tree = Tree::clone(&session.tree);
            return Some(op(&mut session.client, &mut tree, smb_path).await);
        }
        let link = match resolve_link(&mut session.client, &session.tree, smb_path).await {
            Ok(link) => link?,
            Err(e) => return Some(Err(e)),
        };
        let mut target = link.tree;
        let result = op(&mut session.client, &mut target, &link.path).await;
        drop(guard);
        // Only a call that got through says where a link is: a refusal from
        // the link's far side may be its target refusing the referral's path.
        if result.is_ok() {
            self.inner.link_paths.note(&link.link_path);
        }
        Some(result)
    }
}

/// Where a call through a DFS link runs on the main client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LinkRoute {
    /// On the link's target tree, at the path behind the link.
    Target,
    /// On the share's own tree and path, for a call smb2 follows the link for
    /// itself (a rename, whose destination is a share path too).
    ShareTree,
}

/// smb2's resolver: where `smb_path` on `tree` really lives, `Ok(None)` when
/// it sits behind no link or the referral couldn't be had (the caller's own
/// answer stands, as smb2's follow keeps it), and an error only for a link
/// whose targets all failed (`DfsTargetUnreachable`) or that leads to a server no
/// one allowed signing in to (`DfsTargetNotAllowed`), each of which refuses that
/// path and leaves the session alone.
pub(super) async fn resolve_link(
    client: &mut SmbClient,
    tree: &Tree,
    smb_path: &str,
) -> Result<Option<smb2::DfsLink>, smb2::Error> {
    match client.resolve_dfs_link(tree, smb_path).await {
        Ok(link) => Ok(link),
        Err(e @ (smb2::Error::DfsTargetUnreachable { .. } | smb2::Error::DfsTargetNotAllowed { .. })) => Err(e),
        Err(e) => {
            log::debug!(
                "dfs: couldn't resolve the link, keeping the server's answer ({:?})",
                e.kind()
            );
            Ok(None)
        }
    }
}

/// The tree and path a client call on `smb_path` should go to: the link's
/// target when it sits behind one, else `tree` and `smb_path` as given (and
/// smb2's own follow takes over).
pub(super) async fn target_of(
    client: &mut SmbClient,
    tree: &Tree,
    smb_path: &str,
) -> Result<(Tree, String), smb2::Error> {
    Ok(match resolve_link(client, tree, smb_path).await? {
        Some(link) => (link.tree, link.path),
        None => (tree.clone(), smb_path.to_string()),
    })
}

/// [`SmbVolume::or_through_link`]'s `op` for a bare `stat`: the entry itself,
/// a symlink as the link.
pub(super) fn link_bare_stat<'c>(
    client: &'c mut SmbClient,
    tree: &'c mut Tree,
    path: &'c str,
) -> LinkCall<'c, FileInfo> {
    Box::pin(client.stat(tree, path))
}

/// [`SmbVolume::or_through_link`]'s `op` for opening a `FileReader`, which
/// owns the target's connection once open.
pub(super) fn link_open_reader<'c>(
    client: &'c mut SmbClient,
    tree: &'c mut Tree,
    path: &'c str,
) -> LinkCall<'c, FileReader> {
    Box::pin(client.open_file_reader(tree, path))
}

/// [`SmbVolume::or_through_link`]'s `op` for the one-frame delete
/// (`Tree::delete_link`): a file, an empty folder, or a link as the link.
pub(super) fn link_delete<'c>(client: &'c mut SmbClient, tree: &'c mut Tree, path: &'c str) -> LinkCall<'c, ()> {
    Box::pin(client.delete_link(tree, path))
}

/// [`SmbVolume::or_through_link`]'s `op` for `links::stat_through_link`.
pub(super) fn link_stat<'c>(client: &'c mut SmbClient, tree: &'c mut Tree, path: &'c str) -> LinkCall<'c, FileInfo> {
    Box::pin(link_stat_through_symlink(client, tree, path))
}

/// `stat` through a DFS link, with a symlink behind it resolved to its
/// target's kind and size the way `links::stat_through_link` does on the tree.
async fn link_stat_through_symlink(
    client: &mut SmbClient,
    tree: &mut Tree,
    path: &str,
) -> Result<FileInfo, smb2::Error> {
    let mut info = client.stat(tree, path).await?;
    if info.is_link() && !info.is_directory {
        if let Ok(reader) = client.open_directory_reader(tree, path).await {
            let _ = reader.close().await;
            info.is_directory = true;
            info.size = 0;
        } else if let Ok(reader) = client.open_file_reader(tree, path).await {
            info.size = reader.size();
            let _ = reader.close().await;
        }
    }
    Ok(info)
}

/// Whether `err` is a server saying the path sits behind a DFS link: a CREATE
/// refused with `STATUS_PATH_NOT_COVERED`, as smb2's own trigger reads it. A
/// refused CREATE wrote nothing, so running the call again through the link is
/// safe.
pub(super) fn is_behind_dfs_link(err: &smb2::Error) -> bool {
    matches!(
        err,
        smb2::Error::Protocol {
            status: smb2::types::status::NtStatus::PATH_NOT_COVERED,
            command: smb2::types::Command::Create,
        }
    )
}

/// The client that owns a lease's session, locked to follow a link.
pub(super) enum LinkClient<'a> {
    Main(tokio::sync::MutexGuard<'a, Option<MainSession>>),
    Transfer(tokio::sync::OwnedMutexGuard<SmbClient>),
}

impl LinkClient<'_> {
    pub(super) fn client(&mut self) -> &mut SmbClient {
        match self {
            Self::Main(guard) => &mut guard.as_mut().expect("checked at lock").client,
            Self::Transfer(guard) => guard,
        }
    }
}

impl SmbVolumeInner {
    /// Locks the client a lease's session belongs to, or `None` when that
    /// session is no longer installed (a reconnect replaced it: the lease's tree
    /// would be refused on the new client, so the caller keeps its error).
    pub(super) async fn link_client(&self, ran_on: RanOn, on: &LeaseOn) -> Option<LinkClient<'_>> {
        match (on, ran_on) {
            (LeaseOn::Main, RanOn::Main(generation)) => {
                let guard = self.session.lock().await;
                guard
                    .as_ref()
                    .is_some_and(|s| s.generation == generation)
                    .then_some(LinkClient::Main(guard))
            }
            (LeaseOn::Transfer { member, .. }, RanOn::Transfer(_)) => {
                Some(LinkClient::Transfer(Arc::clone(&member.client).lock_owned().await))
            }
            _ => None,
        }
    }

    /// Opens a streaming download of `smb_path` through the DFS link it sits
    /// behind (smb2 0.36.1's `open_file_download`). The client is held for the
    /// open only; the download owns the target's connection and tree, so it
    /// keeps smb2's adaptive read-ahead window.
    pub(super) async fn open_link_download(
        &self,
        ran_on: RanOn,
        on: &LeaseOn,
        tree: &Tree,
        smb_path: &str,
    ) -> Option<Result<FileDownload<'static>, smb2::Error>> {
        let mut client = self.link_client(ran_on, on).await?;
        let client = client.client();
        Some(match target_of(client, tree, smb_path).await {
            Ok((tree, path)) => client.open_file_download(&tree, &path).await,
            Err(e) => Err(e),
        })
    }

    /// Opens the streaming writer `mode` asks for through the DFS link
    /// `smb_path` sits behind. The writer owns the target's connection, so the
    /// client is held for the open only.
    pub(super) async fn open_link_writer(
        &self,
        ran_on: RanOn,
        on: &LeaseOn,
        tree: &Tree,
        smb_path: &str,
        mode: WriteMode,
    ) -> Option<Result<FileWriter, smb2::Error>> {
        let mut client = self.link_client(ran_on, on).await?;
        let client = client.client();
        let (tree, path) = match target_of(client, tree, smb_path).await {
            Ok(target) => target,
            Err(e) => return Some(Err(e)),
        };
        Some(match mode {
            WriteMode::CreateNew | WriteMode::CreateNewInFreshFolder => {
                client.create_file_writer_exclusive(&tree, &path).await
            }
            WriteMode::CreateOrReplace => client.create_file_writer(&tree, &path).await,
        })
    }

    /// The one-frame write `mode` asks for, through the DFS link `smb_path`
    /// sits behind. Holds the client for the frame's two round trips.
    pub(super) async fn link_write_compound(
        &self,
        ran_on: RanOn,
        on: &LeaseOn,
        tree: &Tree,
        smb_path: &str,
        data: &[u8],
        mode: WriteMode,
    ) -> Option<Result<u64, smb2::Error>> {
        let mut client = self.link_client(ran_on, on).await?;
        let client = client.client();
        let (tree, path) = match target_of(client, tree, smb_path).await {
            Ok(target) => target,
            Err(e) => return Some(Err(e)),
        };
        Some(match mode {
            WriteMode::CreateNew | WriteMode::CreateNewInFreshFolder => {
                client.write_file_compound_exclusive(&tree, &path, data).await
            }
            WriteMode::CreateOrReplace => client.write_file_compound(&tree, &path, data).await,
        })
    }

    /// Sets `times` by path through the DFS link `smb_path` sits behind.
    pub(super) async fn link_set_times(
        &self,
        ran_on: RanOn,
        on: &LeaseOn,
        tree: &Tree,
        smb_path: &str,
        times: smb2::FileTimes,
    ) -> Option<Result<(), smb2::Error>> {
        let mut client = self.link_client(ran_on, on).await?;
        let client = client.client();
        Some(match target_of(client, tree, smb_path).await {
            Ok((tree, path)) => client.set_times(&tree, &path, times).await,
            Err(e) => Err(e),
        })
    }

    /// Deletes `smb_path` through the DFS link it sits behind, for a failed
    /// write's partial file.
    pub(super) async fn link_delete_file(
        &self,
        ran_on: RanOn,
        on: &LeaseOn,
        tree: &Tree,
        smb_path: &str,
    ) -> Option<Result<(), smb2::Error>> {
        let mut client = self.link_client(ran_on, on).await?;
        let client = client.client();
        Some(match target_of(client, tree, smb_path).await {
            Ok((tree, path)) => client.delete_file(&tree, &path).await,
            Err(e) => Err(e),
        })
    }
}

#[cfg(test)]
#[path = "dfs_links_test.rs"]
mod dfs_links_test;
