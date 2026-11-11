//! Paths to file handles: the walk from the export's root, the bounded cache of
//! the folders it found, and the two rules that keep a cached handle honest.
//!
//! - ❗ **Only real FOLDERS are cached, under their real path.** A file's
//!   handle is looked up fresh every time (its attributes are what the caller
//!   wanted anyway), and a path that went through a symlink is cached under the
//!   link's TARGET, ❌ never the spelling that named it: a link can be repointed
//!   without its target noticing.
//! - ❗ **`NFS3ERR_STALE` on a cached handle drops it and everything under it,
//!   and the walk runs once more uncached**; a second `STALE` is `NotFound`. A
//!   stale ROOT is the whole mount gone (the export was re-created, the server
//!   re-exported it): `DeviceDisconnected`, so the lifecycle redials, and a
//!   redial is a fresh `MNT`.
//! - ❗ **A cached handle can point at the wrong place without going stale.**
//!   Another client renames `/a/b` to `/z/b`: our `/a/b` handle still works,
//!   and now names `/z/b`. So a MUTATION asks [`NfsVolume::dir_for_mutation`],
//!   which proves every cached ancestor with one pipelined round of `GETATTR`s:
//!   each still has the `(mtime, ctime)` it had when its child was looked up
//!   (renaming a child bumps its parent's `mtime`), else the walk re-LOOKUPs
//!   from the first level that moved. Our own mutations refresh those stamps
//!   from their `wcc_data` ([`HandleCache::note_dir_changed`]), so a 10,000-file
//!   delete in one folder doesn't re-LOOKUP after each file.
//!
//! Relative links inside the export are followed client-side
//! ([`super::paths::follow_link`]); an absolute one, or one that climbs out, is
//! `NotSupported` wherever it has to be followed.

use std::collections::HashMap;
use std::sync::Mutex;

use cmdr_fs::error_detail::ErrorDetail;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::os_error::OsErrorClass;
use cmdr_fs::volume::VolumeError;
use futures_util::future::join_all;

use super::NfsVolume;
use super::paths::{components, follow_link, path_of};
use super::reconnect::NfsClient;
use crate::errors::map_nfs_error;
use crate::proto::nfs3::{Fattr3, FileHandle, FileType, NfsError, NfsStat3, NfsTime, WccData};

/// How many folders the cache remembers before it forgets the least recently
/// used half. A handle is at most 64 bytes, so this is about a megabyte.
const CACHE_CAP: usize = 8192;

/// How many links one path may follow before it's a loop (Linux's own limit).
const MAX_LINK_HOPS: u32 = 40;

/// A cached folder's handle and the parent stamp it was filed under.
type Cachedish = (FileHandle, Option<Stamp>);

/// A folder's `(mtime, ctime)`: what moves when a name inside it changes.
pub(super) type Stamp = (NfsTime, NfsTime);

fn stamp_of(attrs: &Fattr3) -> Stamp {
    (attrs.mtime, attrs.ctime)
}

/// One cached folder.
#[derive(Debug, Clone)]
struct Cached {
    handle: FileHandle,
    /// Its PARENT's stamp when it was looked up, from the LOOKUP's own
    /// `dir_attributes`. `None` when the server sent none: such an entry can
    /// never vouch for itself, so a mutation always re-LOOKUPs it.
    parent_stamp: Option<Stamp>,
    /// When it was last used, by the cache's own clock.
    used: u64,
}

/// Path → folder handle, bounded, least recently used out first.
pub(super) struct HandleCache {
    inner: Mutex<CacheMap>,
}

#[derive(Default)]
struct CacheMap {
    entries: HashMap<String, Cached>,
    clock: u64,
}

impl HandleCache {
    pub(super) fn new() -> Self {
        Self {
            inner: Mutex::new(CacheMap::default()),
        }
    }

    fn get(&self, path: &str) -> Option<Cachedish> {
        let mut map = self.inner.lock_ignore_poison();
        map.clock += 1;
        let clock = map.clock;
        let cached = map.entries.get_mut(path)?;
        cached.used = clock;
        Some((cached.handle.clone(), cached.parent_stamp))
    }

    fn put(&self, path: String, handle: FileHandle, parent_stamp: Option<Stamp>) {
        let mut map = self.inner.lock_ignore_poison();
        map.clock += 1;
        let used = map.clock;
        map.entries.insert(
            path,
            Cached {
                handle,
                parent_stamp,
                used,
            },
        );
        if map.entries.len() > CACHE_CAP {
            // Forget the older half at once, so eviction is a sort per
            // `CACHE_CAP / 2` inserts, ❌ not a scan per insert.
            let mut ages: Vec<u64> = map.entries.values().map(|cached| cached.used).collect();
            let cut = ages.len() / 2;
            let (_, median, _) = ages.select_nth_unstable(cut);
            let median = *median;
            map.entries.retain(|_, cached| cached.used > median);
        }
    }

    /// A folder a LISTING found, cached only when nothing better is: a listing
    /// carries no parent stamp, so this entry can't vouch for itself to a
    /// mutation, and ❌ must never replace one a LOOKUP filed with its stamp.
    pub(super) fn offer(&self, path: String, handle: FileHandle) {
        if self.inner.lock_ignore_poison().entries.contains_key(&path) {
            return;
        }
        self.put(path, handle, None);
    }

    /// Drops `path` and everything cached under it.
    pub(super) fn forget_subtree(&self, path: &str) {
        let mut map = self.inner.lock_ignore_poison();
        if path == "/" {
            map.entries.clear();
            return;
        }
        map.entries.retain(|cached, _| {
            !(cached == path || cached.strip_prefix(path).is_some_and(|rest| rest.starts_with('/')))
        });
    }

    /// Forgets everything: a new mount may hand out a new root.
    pub(super) fn clear(&self) {
        self.inner.lock_ignore_poison().entries.clear();
    }

    /// Our own mutation in folder `dir` answered `wcc`: the folder's stamp
    /// moved from `before` to `after`, so the children cached under it still
    /// vouch for themselves if, and only if, they held `before`. ❗ A child
    /// holding anything else saw ANOTHER change too, and keeps its old stamp so
    /// the next mutation re-LOOKUPs it.
    pub(super) fn note_dir_changed(&self, dir: &str, wcc: &WccData) {
        let (Some(before), Some(after)) = (wcc.before, wcc.after) else {
            return;
        };
        let was = (before.mtime, before.ctime);
        let now = stamp_of(&after);
        let mut map = self.inner.lock_ignore_poison();
        for (path, cached) in &mut map.entries {
            if parent_of(path) == dir && cached.parent_stamp == Some(was) {
                cached.parent_stamp = Some(now);
            }
        }
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.inner.lock_ignore_poison().entries.len()
    }
}

/// The folder `path` sits in (`/a/b` → `/a`, `/a` → `/`).
fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) | None => "/",
        Some(cut) => &path[..cut],
    }
}

/// Which symlink at the END of a path the walk follows. Links in the middle
/// are always followed, the way a path through a folder link reads anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Follow {
    /// Never: the link itself (`entry_kind`, `lstat`).
    Never,
    /// When it can be followed, else the link itself (`get_metadata`, `stat`
    /// with grace for a link we may not follow).
    IfInside,
    /// Always; a link that can't be followed is `NotSupported` (opening,
    /// listing).
    Always,
}

/// What a walk found.
#[derive(Debug, Clone)]
pub(super) struct Found {
    /// Its handle.
    pub(super) handle: FileHandle,
    /// Its attributes, fresh from this walk, or `None` when the handle came
    /// from the cache and the caller didn't ask for them.
    pub(super) attributes: Option<Fattr3>,
    /// The path it was found at with every link followed: what its cache
    /// entries are filed under.
    pub(super) real: String,
}

/// The folder a mutation acts in, proven current by
/// [`NfsVolume::dir_for_mutation`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MutationDir {
    /// Its handle, the `dir` half of every `dir handle + name` call.
    pub(super) handle: FileHandle,
    /// Its path with every link followed, what the cache files it under.
    pub(super) real: String,
}

/// Why a walk stopped.
enum WalkError {
    /// A CACHED handle answered `STALE`: the walk may run again uncached.
    Stale,
    /// Anything else, already in the `Volume` vocabulary.
    Volume(VolumeError),
}

impl From<VolumeError> for WalkError {
    fn from(err: VolumeError) -> Self {
        Self::Volume(err)
    }
}

impl NfsVolume {
    /// The handle (and, when `fresh_leaf`, the fresh attributes) of what
    /// `remote` names, following links as `follow` says at the end and always
    /// in the middle. `fresh_leaf` false lets a cached FOLDER answer for the
    /// last component too: a listing needs only the handle.
    pub(super) async fn resolve(
        &self,
        client: &NfsClient,
        remote: &str,
        follow: Follow,
        fresh_leaf: bool,
    ) -> Result<Found, VolumeError> {
        match self.walk(client, remote, follow, fresh_leaf, true, 0).await {
            Ok(found) => Ok(found),
            Err(WalkError::Stale) => match self.walk(client, remote, follow, fresh_leaf, false, 0).await {
                Ok(found) => Ok(found),
                Err(WalkError::Stale) => Err(VolumeError::NotFound(remote.to_string())),
                Err(WalkError::Volume(err)) => Err(err),
            },
            Err(WalkError::Volume(err)) => Err(err),
        }
    }

    /// Runs `op` on what `remote` resolves to, and once more on a fresh walk
    /// when `op` finds the handle `STALE` (the object was replaced under its
    /// name). A second `STALE` is `NotFound`.
    pub(super) async fn on_resolved<T, F, Fut>(
        &self,
        client: &NfsClient,
        remote: &str,
        follow: Follow,
        fresh_leaf: bool,
        op: F,
    ) -> Result<T, VolumeError>
    where
        F: Fn(Found) -> Fut,
        Fut: Future<Output = Result<T, NfsError>>,
    {
        let found = self.resolve(client, remote, follow, fresh_leaf).await?;
        match op(found).await {
            Err(NfsError::Status(NfsStat3::Stale)) => {
                self.inner.handles.forget_subtree(remote);
                let found = self.resolve(client, remote, follow, fresh_leaf).await?;
                op(found).await.map_err(|err| map_nfs_error(&err, remote))
            }
            other => other.map_err(|err| map_nfs_error(&err, remote)),
        }
    }

    /// The folder `dir` names NOW, for a mutation in it: its handle and its
    /// real path (what its cache entries, and `note_dir_changed`, are filed
    /// under).
    ///
    /// ❗ Resolves, then proves the cached chain to it: one pipelined round of
    /// `GETATTR`s on the root and every ancestor, each checked against the
    /// stamp its child was looked up under. A level that moved is forgotten
    /// with everything under it, and the walk runs again, re-LOOKUPing from
    /// there. A `dir` that leads to anything but a folder is `NotADirectory`,
    /// carrying `dir`: the thing in the way.
    pub(super) async fn dir_for_mutation(&self, client: &NfsClient, dir: &str) -> Result<MutationDir, VolumeError> {
        let found = self.resolve(client, dir, Follow::Always, false).await?;
        if found
            .attributes
            .as_ref()
            .is_some_and(|attrs| attrs.file_type != FileType::Directory)
        {
            return Err(VolumeError::NotADirectory(dir.to_string()));
        }
        let real = found.real.clone();
        let parts: Vec<&str> = components(&real).collect();
        let chain: Vec<(String, Option<Cachedish>)> = (1..=parts.len())
            .map(|depth| {
                let path = path_of(&parts[..depth]);
                let cached = self.inner.handles.get(&path);
                (path, cached)
            })
            .collect();
        // The handle each level's stamp is read from: the root, then each
        // cached ancestor in turn. An uncached level ends the chain; the walk
        // just looked it up, so it's as fresh as anything this round could say.
        let mut askers = vec![client.root.clone()];
        for (_, cached) in &chain {
            match cached {
                Some((handle, _)) => askers.push(handle.clone()),
                None => break,
            }
        }
        askers.truncate(chain.len());
        let stamps = join_all(askers.iter().map(|handle| client.nfs.getattr(handle))).await;
        for (level, answer) in stamps.into_iter().enumerate() {
            let (child_path, child) = &chain[level];
            let moved = match answer {
                Ok(attrs) => child.as_ref().and_then(|(_, stamp)| *stamp) != Some(stamp_of(&attrs)),
                Err(NfsError::Status(NfsStat3::Stale)) if level == 0 => {
                    return Err(VolumeError::DeviceDisconnected(self.inner.volume_id.clone().into()));
                }
                Err(NfsError::Status(NfsStat3::Stale | NfsStat3::NoEnt)) => true,
                Err(err) => return Err(map_nfs_error(&err, dir)),
            };
            if moved {
                self.inner.handles.forget_subtree(child_path);
                let again = self.resolve(client, dir, Follow::Always, false).await?;
                if again
                    .attributes
                    .as_ref()
                    .is_some_and(|attrs| attrs.file_type != FileType::Directory)
                {
                    return Err(VolumeError::NotADirectory(dir.to_string()));
                }
                return Ok(MutationDir {
                    handle: again.handle,
                    real: again.real,
                });
            }
        }
        Ok(MutationDir {
            handle: found.handle,
            real,
        })
    }

    /// One walk from the root. `use_cache` false is the retry after a cached
    /// handle went `STALE`.
    fn walk<'a>(
        &'a self,
        client: &'a NfsClient,
        remote: &'a str,
        follow: Follow,
        fresh_leaf: bool,
        use_cache: bool,
        hops: u32,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Found, WalkError>> + Send + 'a>> {
        Box::pin(async move {
            let parts: Vec<&str> = components(remote).collect();
            if parts.is_empty() {
                let attributes = if fresh_leaf {
                    Some(
                        client
                            .nfs
                            .getattr(&client.root)
                            .await
                            .map_err(|err| self.root_failure(&err, remote))?,
                    )
                } else {
                    None
                };
                return Ok(Found {
                    handle: client.root.clone(),
                    attributes,
                    real: "/".to_string(),
                });
            }
            let mut dir = client.root.clone();
            let mut dir_is_cached = false;
            for (index, name) in parts.iter().enumerate() {
                let is_leaf = index + 1 == parts.len();
                let here = path_of(&parts[..=index]);
                if use_cache
                    && !(is_leaf && fresh_leaf)
                    && let Some((handle, _)) = self.inner.handles.get(&here)
                {
                    dir = handle;
                    dir_is_cached = true;
                    if is_leaf {
                        return Ok(Found {
                            handle: dir,
                            attributes: None,
                            real: here,
                        });
                    }
                    continue;
                }
                let looked = client.nfs.lookup(&dir, name.as_bytes()).await;
                let found = match looked {
                    Ok(found) => found,
                    Err(NfsError::Status(NfsStat3::Stale)) if index == 0 => {
                        return Err(WalkError::Volume(
                            self.root_failure(&NfsError::Status(NfsStat3::Stale), remote),
                        ));
                    }
                    Err(NfsError::Status(NfsStat3::Stale)) if dir_is_cached => {
                        self.inner.handles.forget_subtree(&path_of(&parts[..index]));
                        return Err(WalkError::Stale);
                    }
                    Err(err) => return Err(WalkError::Volume(map_nfs_error(&err, remote))),
                };
                let attrs = match found.attributes {
                    Some(attrs) => attrs,
                    None => client
                        .nfs
                        .getattr(&found.handle)
                        .await
                        .map_err(|err| map_nfs_error(&err, remote))?,
                };
                let follows = match follow {
                    _ if !is_leaf => true,
                    Follow::Never => false,
                    Follow::IfInside | Follow::Always => true,
                };
                if attrs.file_type == FileType::Symlink && follows {
                    if hops >= MAX_LINK_HOPS {
                        return Err(WalkError::Volume(too_many_links(remote)));
                    }
                    let target = client
                        .nfs
                        .readlink(&found.handle)
                        .await
                        .map_err(|err| map_nfs_error(&err, remote))?;
                    let next = follow_link(&parts[..index], &target, &parts[index + 1..]);
                    let link_itself = Found {
                        handle: found.handle.clone(),
                        attributes: Some(attrs),
                        real: here.clone(),
                    };
                    return match (next, follow, is_leaf) {
                        (Some(next), Follow::IfInside, true) => {
                            match self.walk(client, &next, follow, fresh_leaf, use_cache, hops + 1).await {
                                // A dangling link, or one whose target leads
                                // somewhere we won't go: the link itself.
                                Err(WalkError::Volume(VolumeError::NotFound(_) | VolumeError::NotSupported)) => {
                                    Ok(link_itself)
                                }
                                other => other,
                            }
                        }
                        (Some(next), _, _) => self.walk(client, &next, follow, fresh_leaf, use_cache, hops + 1).await,
                        (None, Follow::IfInside, true) => Ok(link_itself),
                        (None, _, _) => Err(WalkError::Volume(VolumeError::NotSupported)),
                    };
                }
                if attrs.file_type == FileType::Directory {
                    self.inner.handles.put(
                        here.clone(),
                        found.handle.clone(),
                        found.dir_attributes.as_ref().map(stamp_of),
                    );
                }
                if is_leaf {
                    return Ok(Found {
                        handle: found.handle,
                        attributes: Some(attrs),
                        real: here,
                    });
                }
                // ❗ The thing IN THE WAY, ❌ not the path asked for: the frontend
                // renders this as the file to move aside.
                if attrs.file_type != FileType::Directory {
                    return Err(WalkError::Volume(VolumeError::NotADirectory(here)));
                }
                dir = found.handle;
                dir_is_cached = false;
            }
            unreachable!("a non-empty path returns at its last component")
        })
    }

    /// A failure on the ROOT handle. `STALE` there is the mount gone, which
    /// only a fresh `MNT` mends: `DeviceDisconnected`, so the lifecycle
    /// redials.
    fn root_failure(&self, err: &NfsError, remote: &str) -> VolumeError {
        match err {
            NfsError::Status(NfsStat3::Stale | NfsStat3::BadHandle) => {
                VolumeError::DeviceDisconnected(self.inner.volume_id.clone().into())
            }
            other => map_nfs_error(other, remote),
        }
    }
}

fn too_many_links(remote: &str) -> VolumeError {
    VolumeError::io_of_class(
        // allowed-pluralize-noun: `MAX_LINK_HOPS` is 40, never 1.
        ErrorDetail::path_then(remote, format!(" follows more than {MAX_LINK_HOPS} links")),
        OsErrorClass::LinkLoop,
    )
}

#[cfg(test)]
#[path = "handles_test.rs"]
mod handles_test;
