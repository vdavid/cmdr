//! [`GitPortalVolume`]: a read-only [`Volume`] over one repo's virtual `.git`
//! trees.
//!
//! The same shape `ArchiveVolume` has. A path crossing `.git/<category>/` isn't
//! a hook inside the local backend: `VolumeManager::resolve` routes it here, and
//! this volume maps the whole input path to `(repo, category, ref, tree path)`
//! through [`path::classify_in`], so `ResolvedVolume.path` stays the caller's
//! path verbatim.
//!
//! ## What it serves, and what it doesn't
//!
//! Its namespace is the SIX virtual categories and everything under them.
//! `root()` is `<worktree>/.git`, and listing that root answers the six category
//! rows alone: the real `.git/*` entries (`HEAD`, `config`, `hooks/`) are the
//! parent volume's, stay writable there, and reach the pane through the listing
//! overlay rather than through here. Anything else — `.git/config`,
//! `.git/objects/…`, a `.git` that isn't a repository at all — is `NotFound`,
//! because this volume genuinely doesn't hold it.
//!
//! ## Read-only by construction
//!
//! `is_writable()` is false and every mutation keeps the trait's `NotSupported`
//! default, so nothing has to remember a guard: a git tree is a snapshot of
//! objects, and the way to change one is `git` in a terminal. It can't watch
//! either (`can_watch_listings()` false, `listing_watch_coverage` `None`): the
//! paths don't exist on disk, so `notify` has nothing to arm on. Invalidation
//! comes from the per-repo `.git/*` watcher (`watcher.rs`) instead.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use cmdr_fs::volume::scan_walk::{self, ScanSource, Walking};

use super::path::{self, Cat, VirtualGitPath};
use super::portal::GitPortal;
use super::repo::RepoHandle;
use super::{Lookup, found_or_not_found, list_ref_tree, log, stash, submodules, virtual_listing, worktrees};
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{
    BatchScanResult, CopyScanResult, DirectoryCreation, LaneKey, ListingProgress, ScanBoundary, SpaceInfo, Volume,
    VolumeError, VolumeReadStream,
};

/// A read-only [`Volume`] over one repository's virtual `.git` trees.
pub struct GitPortalVolume {
    /// The repo handles and the host this volume shares with every other portal
    /// volume in the process.
    portal: Arc<GitPortal>,
    /// `<worktree>/.git`: this volume's root, and the prefix of every path it
    /// serves.
    dot_git: PathBuf,
    /// The volume physically holding the repo. Source of the shared lane key
    /// (portal work must not run in parallel with other work on the same disk)
    /// and of the space answer.
    parent: Arc<dyn Volume>,
}

impl GitPortalVolume {
    /// Builds the portal volume for the worktree at `repo_root`, backed by
    /// `parent`. Prefer [`GitPortal::volume_for`], which is the same call with
    /// the portal already in hand.
    pub(super) fn new(portal: Arc<GitPortal>, repo_root: PathBuf, parent: Arc<dyn Volume>) -> Self {
        Self {
            portal,
            dot_git: repo_root.join(".git"),
            parent,
        }
    }

    /// Runs `work` on the app's runtime rather than the caller's thread: every
    /// `gix` call below is blocking, and a listing of a big repo would otherwise
    /// stall an async worker.
    fn blocking<T, F>(&self, work: F) -> Pin<Box<dyn Future<Output = Result<T, VolumeError>> + Send>>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, VolumeError> + Send + 'static,
    {
        let task = self.portal.host().runtime().spawn_blocking(work);
        Box::pin(async move {
            task.await
                .expect("the git portal's blocking closure doesn't panic and the task is uncancelable")
        })
    }
}

impl Volume for GitPortalVolume {
    /// The name of the thing at [`root`](Volume::root), as every backend
    /// answers it. The registry id carries which repo this is.
    fn name(&self) -> &str {
        ".git"
    }

    fn root(&self) -> &Path {
        &self.dot_git
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// The **parent's** lane key: reading a snapshot still reads the repo's
    /// object database off the parent's disk, so the two must not be scheduled
    /// as independent resources.
    fn lane_key(&self) -> LaneKey {
        self.parent.lane_key()
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        let owned = path.to_path_buf();
        let portal = Arc::clone(&self.portal);
        let listing = self.blocking(move || {
            let Some((virt, handle, root)) = path::classify_in(portal.repos(), &owned) else {
                return Err(VolumeError::NotFound(owned.display().to_string()));
            };
            found_or_not_found(listing_for(&virt, &handle, &root), &owned)
        });
        Box::pin(async move {
            let entries = listing.await?;
            // One cumulative tick, as the trait asks: a snapshot listing is
            // atomic, so there's nothing incremental to report.
            if let Some(callback) = on_progress {
                callback(ListingProgress::of(&entries));
            }
            Ok(entries)
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        let owned = path.to_path_buf();
        let portal = Arc::clone(&self.portal);
        self.blocking(move || {
            let Some((virt, handle, root)) = path::classify_in(portal.repos(), &owned) else {
                return Err(VolumeError::NotFound(owned.display().to_string()));
            };
            found_or_not_found(virtual_listing::get_metadata_for(&root, &virt, &handle), &owned)
        })
    }

    // ---- Read-only: a snapshot is changed with `git`, not with a file manager

    /// ❌ Never flip this to track "the user can write to this repo". They can,
    /// through the parent volume, which is what serves every real file under
    /// `.git`. This volume holds objects that already exist, addressed by
    /// content; there is nothing here to write to.
    fn is_writable(&self) -> bool {
        false
    }

    /// `create_file`, `create_directory`, `delete`, `rename`, and
    /// `write_from_stream` all inherit the trait's `NotSupported` default. Only
    /// this one is overridden: the default walks `exists()` and answers
    /// `Ok(AlreadyExisted)` for a directory that's already there, which would
    /// have a read-only volume claim it created something.
    fn create_directory_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<DirectoryCreation, VolumeError>> + Send + 'a>> {
        let _ = path;
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    // ---- Copy out: streaming reads and the scan that plans them ------------

    fn supports_export(&self) -> bool {
        true
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let owned = path.to_path_buf();
        let portal = Arc::clone(&self.portal);
        self.blocking(move || {
            let Some((virt, handle, _root)) = path::classify_in(portal.repos(), &owned) else {
                return Err(VolumeError::NotFound(owned.display().to_string()));
            };
            let opened = match &virt {
                VirtualGitPath::RefTree(cat, name, sub) if cat.browses_commit_tree() => {
                    super::open_blob(&handle, *cat, name, sub)
                }
                // A category, a ref, or a leaf row: a directory has no bytes.
                _ => return Err(VolumeError::NotSupported),
            };
            found_or_not_found(opened, &owned)
        })
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        scan_walk::scan_one(self, path)
    }

    fn scan_for_copy_batch_with_boundary<'a>(
        &'a self,
        paths: &'a [PathBuf],
        boundary: &'a ScanBoundary,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        scan_walk::scan_trees(self, paths, boundary)
    }

    // ---- Capability flags: answered explicitly, never inherited ------------

    /// `None`: a virtual path has no counterpart on disk, so there's no
    /// `copyfile(2)` fast path to advertise.
    fn local_path(&self) -> Option<PathBuf> {
        None
    }

    /// `false`: `.git/branches/main/README.md` can't be `stat`ed or read with
    /// `std::fs`. It's an object in a pack file.
    fn supports_local_fs_access(&self) -> bool {
        false
    }

    /// `None`: a snapshot's size doesn't move, so polling it would be pure
    /// churn (the trait default is every two seconds).
    fn space_poll_interval(&self) -> Option<std::time::Duration> {
        None
    }

    /// The parent's. A repo's objects live on the parent's disk, and a copy OUT
    /// of a snapshot lands wherever the destination is, so the parent's free
    /// space is the honest answer; reporting zero would read as "disk full" to
    /// the pre-copy space check.
    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        Box::pin(async move { self.parent.get_space_info().await })
    }

    /// `false`, and `listing_watch_coverage` stays at the trait's `None`: these
    /// paths don't exist on disk, so `notify` has nothing to arm on (it answers "No
    /// path was found" and spams the warn log). Invalidation arrives from the
    /// per-repo `.git/*` watcher instead (`watcher.rs`).
    fn can_watch_listings(&self) -> bool {
        false
    }
}

/// What a classified path lists, for the portal volume AND for the `.git/` root
/// hook in `mod.rs`, which delegates every arm but `Root` here so the two can't
/// answer differently. A free function so the blocking closure needs only the
/// classified triple, never the volume.
pub(super) fn listing_for(virt: &VirtualGitPath, handle: &RepoHandle, root: &Path) -> Lookup<Vec<FileEntry>> {
    match virt {
        VirtualGitPath::Root => Ok(Some(virtual_listing::list_categories(handle, root))),
        VirtualGitPath::Category(Cat::Branches) => virtual_listing::list_branches(handle, root).map(Some),
        VirtualGitPath::Category(Cat::Tags) => virtual_listing::list_tags(handle, root).map(Some),
        VirtualGitPath::Category(Cat::Commits) => log::list_commits(handle, root).map(Some),
        VirtualGitPath::Category(Cat::Stash) => stash::list_stashes(root).map(Some),
        VirtualGitPath::Category(Cat::Worktrees) => worktrees::list_worktrees(handle, root).map(Some),
        VirtualGitPath::Category(Cat::Submodules) => submodules::list_submodules(handle, root).map(Some),
        VirtualGitPath::Ref(cat, name) if cat.browses_commit_tree() => list_ref_tree(handle, root, *cat, name, ""),
        VirtualGitPath::RefTree(cat, name, sub) if cat.browses_commit_tree() => {
            list_ref_tree(handle, root, *cat, name, sub)
        }
        // A worktree or submodule row is a leaf carrying `redirectToPath`; the
        // frontend redirects on Enter, so listing one is empty, not missing.
        VirtualGitPath::Ref(_, _) | VirtualGitPath::RefTree(_, _, _) => Ok(Some(Vec::new())),
    }
}

/// The two reads `scan_walk` needs, so `scan_for_copy` and the batch scan behind
/// every transfer come for free — that's what makes copying a whole branch tree
/// out to another volume work at all.
impl ScanSource for GitPortalVolume {
    fn scan_stat<'a>(&'a self, path: &'a Path) -> Walking<'a, FileEntry> {
        self.get_metadata(path)
    }

    fn scan_list<'a>(&'a self, path: &'a Path) -> Walking<'a, Vec<FileEntry>> {
        self.list_directory(path, None)
    }
}
