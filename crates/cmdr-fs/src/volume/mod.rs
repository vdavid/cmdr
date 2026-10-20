//! The `Volume` trait: the one abstraction every storage backend implements.
//!
//! Every filesystem operation in Cmdr goes through a `Volume`, with **paths
//! relative to the volume root**. The data types the trait exchanges live in
//! `types` (`VolumeError`, `SpaceInfo`, `CopyScanResult`, `ScanConflict`,
//! `MutationEvent`, …) and the volume ID funnel in `ids` (`local_volume_id`,
//! `smb_volume_id`, …); both are re-exported here.
//!
//! Real-storage backends (local POSIX, SMB, MTP, archive) live in the app, where
//! their `smb2` / `mtp-rs` / git / mount-detection dependencies belong. The one
//! implementation here is [`InMemoryVolume`], which needs no host at all and is
//! what a test reaches for.

use crate::entry::FileEntry;
use crate::metadata::{CarriedMetadata, MetadataClasses, MetadataReport, Unblock};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Default volume ID for the root filesystem.
pub const DEFAULT_VOLUME_ID: &str = "root";

/// Async trait for volume file system operations.
///
/// Implementations provide access to different storage backends:
/// - `LocalPosixVolume`: Real local file system (async via `spawn_blocking`)
/// - `InMemoryVolume`: In-memory file system for testing
/// - `MtpVolume`: MTP device storage (natively async)
/// - `SmbVolume`: SMB share storage (natively async via smb2)
///
/// All path parameters are relative to the volume root. The volume handles
/// translating these to actual storage locations.
///
/// Methods are split into two categories:
/// - **Sync**: Identity accessors and capability flags that return struct fields. No I/O.
/// - **Async**: Methods that perform I/O. Return `Pin<Box<dyn Future<Output = T> + Send + '_>>` for
///   object safety (`dyn Volume`). Implementors wrap bodies in `Box::pin(async { ... })`.
pub trait Volume: Send + Sync {
    /// Returns the display name for this volume (like "Macintosh HD", "Dropbox").
    fn name(&self) -> &str;

    /// Returns the root path of this volume: the ACTIVE mount root of the
    /// registry entry it sits in, when it sits in one.
    fn root(&self) -> &Path;

    /// Builds an equivalent volume rooted at `new_root`, or `None` when this
    /// backend can't change root without rebuilding its transport.
    ///
    /// One filesystem can be reached through several mount points (macOS mounts
    /// the same SMB share at `/Volumes/naspi` AND `/Volumes/naspi-1`), and they
    /// all derive one volume ID. The registry therefore tracks the SET of roots
    /// carrying an ID and keeps one of them active; when the active one dies it
    /// promotes a survivor, and this is how the promotion is carried out. See
    /// `apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "A volume ID
    /// owns a set of mount roots".
    ///
    /// Implement it wherever the root is just an addressing prefix (that's every
    /// path-addressed backend). The conservative default `None` means "leave me
    /// where I am": a backend whose session is anchored to the old root would
    /// otherwise be handed a root its transport can't serve.
    ///
    /// ❌ Never call this speculatively, to ask whether a backend CAN re-root.
    /// An implementation is allowed to commit share-scoped state to `new_root` as
    /// it builds the answer, and `SmbVolume` does exactly that: it re-points the
    /// watcher that the whole share (not any one mount) owns. Call it once, at the
    /// moment of promotion, and install what it returns.
    fn rerooted(&self, _new_root: &Path) -> Option<Arc<dyn Volume>> {
        None
    }

    /// Tells this volume that the mount root it is anchored to is PROVEN gone and
    /// the registry had nothing live to promote it to.
    ///
    /// The counterpart to [`rerooted`](Self::rerooted): when a survivor exists the
    /// registry moves the ID there, and when none does it says so here. A backend
    /// whose transport doesn't ride the mount keeps serving either way (a direct
    /// SMB volume browses over smb2), but everything it publishes as a real
    /// filesystem path stops being openable, which is what
    /// [`paths_are_os_visible`](Self::paths_are_os_visible) answers.
    ///
    /// One-way, and the volume can't work it out for itself: nothing may PROBE a
    /// mount for liveness (a `statfs` on a wedged network mount blocks 30–120 s),
    /// so the evidence lands in the registry and is pushed from there. A mount
    /// that comes back re-registers the volume from scratch.
    ///
    /// Default no-op: a backend that reaches its storage THROUGH the mount has
    /// nothing left to answer for anyway.
    fn note_root_mount_gone(&self) {}

    /// Whether this volume was minted by a ROUTE rather than mounted: its root
    /// is a path INSIDE another volume's storage, and it maps a namespace onto
    /// it (an archive's root is the `.zip` file, a git portal's is
    /// `<worktree>/.git`).
    ///
    /// ❗ A routed volume is not a mount, and a host that treats it as one hands
    /// every path inside it to the wrong place. The host's mount lookup picks
    /// the registered volume whose root is the LONGEST ancestor of a path, so a
    /// routed volume would win that race for every path under it, and reads that
    /// belong to the disk holding the `.zip` or the repo (a drive index, an
    /// agent's file inspection) would be routed to a mount that has no index.
    ///
    /// Default `false`, so an ordinary backend says nothing. ❗ A new routed
    /// backend MUST override it to `true`: nothing else can tell, and the
    /// symptom is silent.
    ///
    /// ❌ Not the same question as a host's "could this PATH route?" predicate,
    /// and the two must not be collapsed. This one is about a volume that
    /// already exists, and answering it needs the volume. That one is lexical,
    /// asked before any volume has been resolved, so that a hot path can skip
    /// the resolve entirely. Merging them would force either a `stat` on that
    /// hot path or a downcast where only a path is in hand.
    fn routes_over_a_parent(&self) -> bool {
        false
    }

    /// Returns this volume as `&dyn Any` for downcasting to a concrete
    /// backend type. Used by debug/IPC paths (for example, the SMB
    /// diagnostics dashboard) that need backend-specific state. Most
    /// implementations are one line: `fn as_any(&self) -> &dyn std::any::Any { self }`.
    fn as_any(&self) -> &dyn std::any::Any;

    /// Identifies the shared physical resource this volume contends for, so the
    /// operation manager can serialize transfers that would thrash the same
    /// device or saturate the same single transport. See [`LaneKey`].
    ///
    /// Default: the volume root. Backends override with a per-resource id so
    /// two volumes on the SAME device share a lane: `LocalPosixVolume` →
    /// mount root, `MtpVolume` → device serial (one USB pipe), `SmbVolume` →
    /// `server+port+share` id. Never parse a `volume_id` string here.
    fn lane_key(&self) -> LaneKey {
        LaneKey::new(self.root().to_string_lossy().into_owned())
    }

    // ========================================
    // Required: All volumes must implement
    // ========================================

    /// Lists directory contents at the given path (relative to volume root).
    ///
    /// Returns entries sorted with directories first, then files, both alphabetically.
    /// Pass `on_progress` to receive incremental `ListingProgress` updates during the stat
    /// loop (used by the streaming listing UI and by the scan-preview/scan-for-copy paths
    /// to surface running bytes + dirs in the dialog). Pass `None` when progress isn't
    /// needed. Backends should call `on_progress` periodically, not per-entry, to avoid
    /// flooding the IPC layer.
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>>;

    /// Cancel-aware version of [`list_directory`](Self::list_directory).
    ///
    /// `cancel`, when `Some`, is consulted by backends that issue many small
    /// USB or network roundtrips inside one listing (MTP, where a 950-entry
    /// folder is 950 `GetObjectInfo` calls, and SMB, which lists a server batch
    /// of a few hundred entries per round trip). Once the token is cancelled,
    /// the backend bails between roundtrips with `VolumeError::Cancelled`
    /// instead of running to completion.
    ///
    /// Local and in-memory backends ignore the token (their listings are
    /// effectively atomic from the caller's perspective).
    ///
    /// Default impl delegates to `list_directory`, dropping the token.
    fn list_directory_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        let _ = cancel;
        self.list_directory(path, on_progress)
    }

    /// Lists `path` the way a transfer reads it as its SOURCE: the plain listing
    /// minus entries the filesystem keeps FOR a sibling, which travel as that
    /// sibling's metadata rather than as items of their own. Today that's the
    /// AppleDouble `._name` sidecars a local FAT or exFAT volume holds on macOS
    /// (`crate::apple_double`).
    ///
    /// Every walk that copies, stamps, or sweeps a source lists through this, so
    /// they agree on what the source holds. The pane's listing doesn't: it shows
    /// what's on disk. Default: the plain listing, since no other backend keeps
    /// such entries.
    fn list_directory_as_source<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.list_directory(path, None)
    }

    /// List a directory for the BACKGROUND index scan.
    ///
    /// Same result as `list_directory_with_cancel` with no progress callback, but
    /// backends that hold a scarce serialized resource across a listing (currently
    /// MTP — one USB pipe shared with foreground nav/copy/delete) override this to
    /// release that resource between bounded units and YIELD to any pending
    /// foreground op, so a long scan of a huge folder can't starve interactive use.
    /// Backends with no such contention (local, SMB, in-memory) use the default,
    /// which is just `list_directory_with_cancel`. The scanner
    /// (`indexing::network_scanner`) calls THIS, not `list_directory`, for every
    /// directory it walks.
    fn list_directory_for_scan<'a>(
        &'a self,
        path: &'a Path,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        self.list_directory_with_cancel(path, None, cancel)
    }

    /// The most directory listings a background index walk may keep in flight on
    /// this volume at once, whatever budget the walk itself would allow.
    ///
    /// The walk paces itself (a quiet volume gets a wide budget, one the user is
    /// browsing or copying to gets one listing at a time), and this is the
    /// backend's own ceiling on top: a volume whose every listing costs something
    /// scarce on the far end says how many it can take. Read once per walk.
    ///
    /// Default: no ceiling of the backend's own (`usize::MAX`). A walk never keeps
    /// fewer than one in flight, so an answer of 0 still means one.
    fn max_concurrent_scan_listings(&self) -> usize {
        usize::MAX
    }

    /// The spelling this volume stores for `path`, when `path` names something
    /// under a spelling it doesn't store.
    ///
    /// For a path that came from somewhere other than this volume's own listings:
    /// typed, restored, a favorite, or carried over from another view of the same
    /// storage (the macOS kernel mount decomposes every name). A backend whose
    /// lookups match names byte-for-byte (SMB) misses such a path even though the
    /// thing it names is there.
    ///
    /// `Ok(Some(p))`: `p` is where `path` is, spelled the way this volume stores
    /// it, matched against real listings. `Ok(None)`: no other spelling was found,
    /// so the caller's own answer for `path` stands. `Err(AmbiguousName)`: more
    /// than one stored entry matches and none exactly, and this refuses rather than
    /// guess.
    ///
    /// ❗ **Only where a foreign path ENTERS**, ❌ never inside an operation's own
    /// calls. A resolve can't tell "another spelling of this file" from "a
    /// different file sharing its folded name, the named one having vanished",
    /// so every `Volume` call keeps meaning the exact path it's given, and a
    /// delete or overwrite never lands on a twin. Directory opens use it
    /// (`cmdr_listing::foreign_path`).
    ///
    /// Default: `Ok(None)`, for a backend with one spelling per entry, or whose
    /// own lookups already fold.
    fn find_stored_spelling<'a>(
        &'a self,
        path: &'a Path,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<Option<PathBuf>, VolumeError>> + Send + 'a>> {
        let _ = (path, cancel);
        Box::pin(async { Ok(None) })
    }

    /// How a background index walk treats `dir`, a directory it met in a listing,
    /// with `is_symlink` set when the entry is a link to one.
    ///
    /// Default: [`IndexWalk::unless_link`], so a real directory is walked and a link
    /// never is. A backend overrides this to keep trees out of its walks (a phone's
    /// `/proc`), or to name the link that IS its storage (a phone's `/sdcard`).
    fn index_walk(&self, dir: &Path, is_symlink: bool) -> IndexWalk {
        let _ = dir;
        IndexWalk::unless_link(is_symlink)
    }

    /// Called by the index-scan lifecycle right before a background scan/reconcile
    /// walk starts. Lets a backend spin up scan-scoped resources that only make
    /// sense for the duration of a walk. SMB opens a small pool of extra TCP
    /// sessions here so its latency-bound listing walk isn't serialized on the one
    /// session the pane also browses through (the cold-scan bottleneck is
    /// per-connection serialization in the server, not the disks — see
    /// `smb/DETAILS.md` § "Scan-connection pool"). Paired with
    /// [`end_scan_session`](Self::end_scan_session); the pool is invisible to the
    /// scanner, which keeps calling `list_directory_for_scan`.
    ///
    /// Default no-op: most backends scan fine on their single session (MTP's one
    /// USB pipe can't parallelize; local has no session at all).
    fn begin_scan_session<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }

    /// Called by the index-scan lifecycle right after a background scan/reconcile
    /// walk ends (any outcome: clean, cancel, disconnect, error). Tears down
    /// whatever [`begin_scan_session`](Self::begin_scan_session) opened, so the
    /// steady-state footprint is unchanged between scans. Idempotent. Default
    /// no-op.
    fn end_scan_session<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }

    /// Gets metadata for a single path (relative to volume root).
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>>;

    /// Checks if a path exists (relative to volume root).
    ///
    /// Defaults to "[`get_metadata`](Self::get_metadata) answered", which is the
    /// right answer for any backend whose only existence primitive is a stat.
    /// Override when the backend can answer more cheaply (a bare protocol
    /// `stat`, an in-memory index) or when its truth differs:
    /// `LocalPosixVolume` uses `symlink_metadata`, because a BROKEN symlink is
    /// still a thing on disk the user can see and delete.
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { self.get_metadata(path).await.is_ok() })
    }

    /// Checks if a path is a directory.
    /// Returns Ok(true) if directory, Ok(false) if file, Err if path doesn't exist.
    ///
    /// Defaults to the directory bit off [`get_metadata`](Self::get_metadata).
    /// Same reasons to override as [`exists`](Self::exists).
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async move { self.get_metadata(path).await.map(|entry| entry.is_directory) })
    }

    /// What the entry at `path` is, with a link reported AS a link.
    ///
    /// Ask this, ❌ never [`is_directory`](Self::is_directory), before
    /// descending into, merging with, or recursively deleting a path: whether
    /// `is_directory` follows a link differs by backend, and following one walks
    /// out of the tree the user selected.
    ///
    /// Defaults to [`EntryKind::of`] on [`get_metadata`](Self::get_metadata),
    /// which is right for any backend whose metadata sets `is_symlink`, and for
    /// every backend with no links at all. Override when the stat underneath
    /// follows links, or when a bare `lstat` is cheaper than full metadata.
    fn entry_kind<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<EntryKind, VolumeError>> + Send + 'a>> {
        Box::pin(async move { self.get_metadata(path).await.map(|entry| EntryKind::of(&entry)) })
    }

    // ========================================
    // E2E test support (feature-gated)
    // ========================================

    /// Injects an error that will be returned by the next `list_directory` call.
    /// After the error is returned once, subsequent calls work normally (enables testing retry).
    /// Only available in E2E builds. Default is no-op.
    #[cfg(feature = "playwright-e2e")]
    fn inject_error(&self, _errno: i32) {
        // No-op for volumes that don't support error injection
    }

    // ========================================
    // Optional: Default to NotSupported
    // ========================================

    /// Creates a file with the given content.
    ///
    /// **Strict contract: must NOT clobber.** If `path` already exists, return
    /// `VolumeError::AlreadyExists` and leave the existing file untouched; ❌
    /// never truncate it. The New File command hands a user-typed name straight
    /// here and renders the refusal as "that name is taken", so a backend that
    /// overwrote instead would silently empty a file the user only meant to
    /// name, and the command would report success. Reach for the atomic
    /// primitive rather than a stat-then-write (`create_new(true)`, SMB's
    /// `FileCreate` disposition), so there's no TOCTOU window either.
    ///
    /// **A shared conformance assertion enforces this**, not the wording above:
    /// every backend that implements `create_file` runs
    /// `conformance::assert_create_file_refuses_to_clobber` (test builds only).
    ///
    /// Default: `NotSupported`.
    fn create_file<'a>(
        &'a self,
        path: &'a Path,
        content: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let _ = (path, content);
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Creates a directory.
    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let _ = path;
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Recursively creates `path` and any missing ancestor directories, like
    /// `mkdir -p`. Idempotent: a path (or ancestor) that already exists is a
    /// no-op, so re-running against an existing destination succeeds.
    ///
    /// This is the volume-aware transfer pipelines' destination gate: a copy or
    /// move into a not-yet-existing folder creates it on EVERY backend (local,
    /// SMB, MTP, in-memory), matching the local-FS `ensure_destination_dir`.
    ///
    /// The default walks `path`'s ancestors leaf→root, stopping at the first one
    /// that leads to a directory (or at the volume root), then creates the
    /// missing ones shallowest-first via `create_directory`. Probing per ancestor
    /// before creating means it never calls `create_directory` on a dir that's
    /// already there, so backends whose `create_directory` can't signal a
    /// collision (`MtpVolume`, `create_directory_errors_on_existing_dir() ==
    /// false`) never make a duplicate sibling. An `AlreadyExists` from
    /// `create_directory` (a concurrent op won a race) is also treated as
    /// success. These are network/IPC round-trips, so the leaf-first walk keeps
    /// them minimal: when the parent already exists (the common "new folder name
    /// under an existing dir" case), it's one probe plus one `create_directory`.
    ///
    /// Backends override only if they have a cheaper native recursive mkdir;
    /// SMB and MTP don't, so the per-component loop is the right shape there.
    ///
    /// **A file in the way is refused, by name.** A path that exists and doesn't
    /// lead to a directory (a file, or a link to anything but a folder), at
    /// `path` itself or at an ancestor, answers [`VolumeError::NotADirectory`]
    /// carrying the path of the thing in the way, and nothing is created. ❌
    /// Never `Ok(AlreadyExisted)` for it, and ❌ never the `NotFound` of the
    /// level below it. A link that leads to a folder IS a folder here, and the
    /// walk creates through it. Every mutable backend runs
    /// `conformance::assert_create_directory_all_refuses_a_file_in_the_way`, and
    /// the ones with links run
    /// `conformance::assert_create_directory_all_goes_through_a_link_to_a_folder`.
    /// Why a link is fine here and not in a merge: `mkdir_all.rs` § "A link to a
    /// folder is a folder".
    ///
    /// Reports whether the LEAF was created here or was already there
    /// ([`DirectoryCreation`]). An overriding backend MUST answer that honestly:
    /// the transfer driver skips its per-file destination conflict probe on a
    /// `Created` answer, so a backend that claims to have created a directory it
    /// merely found would turn "would have prompted" into "overwrote". When in
    /// doubt, answer `AlreadyExisted`.
    ///
    /// **A shared conformance assertion enforces the dangerous direction**: every
    /// backend runs
    /// `conformance::assert_create_directory_all_reports_an_existing_dir_honestly`
    /// (test builds only). Answering `AlreadyExisted` for a leaf it did create is
    /// merely slower, so that direction stays unpinned.
    fn create_directory_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<DirectoryCreation, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            // Collect the missing ancestors, walking leaf→root until we reach
            // one that already exists (or run out). A component with no file
            // name (the volume root `/`, an empty path, or `.`) has nothing to
            // create above it — the root always exists — so it stops the walk.
            // ❗ "Exists" has to mean "leads to a directory": a bare `exists()`
            // stops at a FILE just as happily, and the walk then answers
            // `AlreadyExisted` for it or fails one level below it.
            let mut missing: Vec<PathBuf> = Vec::new();
            for ancestor in path.ancestors() {
                if ancestor.file_name().is_none() {
                    break;
                }
                match mkdir_all::volume_path_leads_to(self, ancestor).await {
                    LeadsTo::Directory => break,
                    LeadsTo::NotADirectory => return Err(VolumeError::NotADirectory(ancestor.display().to_string())),
                    LeadsTo::Nothing => missing.push(ancestor.to_path_buf()),
                }
            }

            // `ancestors()` yields leaf→root, so `missing[0]` is the leaf and
            // creating shallowest-first means the leaf goes last: a child can't
            // be created before its parent.
            let mut leaf = DirectoryCreation::AlreadyExisted;
            for (index, dir) in missing.iter().enumerate().rev() {
                match self.create_directory(dir).await {
                    Ok(()) => {
                        if index == 0 {
                            leaf = DirectoryCreation::Created;
                        }
                    }
                    // A concurrent op created it between our probe and this call.
                    // Treat as success to keep the create idempotent — but NOT as
                    // ours: somebody else's directory may already have something
                    // in it. And only a DIRECTORY is success: what won the race
                    // can be a file.
                    Err(VolumeError::AlreadyExists(_)) => {
                        if mkdir_all::volume_path_leads_to(self, dir).await == LeadsTo::NotADirectory {
                            return Err(VolumeError::NotADirectory(dir.display().to_string()));
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
            Ok(leaf)
        })
    }

    /// Puts `carried` and `modified` on the file or folder at `path`, which has
    /// already landed under its final name, at the finest precision the
    /// backend keeps (whole seconds on most).
    ///
    /// Two callers, one body: the cross-volume copy calls it on every FOLDER it
    /// created once that folder's contents have landed (writing a child bumps
    /// its folder's date), and on a FILE right after its landing rename for the
    /// part of its metadata that would have refused that rename
    /// ([`CarriedMetadata::after_landing`]: Locked, with the rest of its flags).
    /// Everything else a file
    /// carries travels on its write (`write_from_stream` asks
    /// [`VolumeReadStream::metadata`] and reads
    /// [`VolumeReadStream::modified_at`]). A routed mode is folded
    /// ([`crate::metadata::routed_mode`]), never put on as is.
    ///
    /// Total, like the host helper it usually wraps: every class reports
    /// `Kept`, `Refused`, `NotApplicable`, or `Absent`, and nothing here fails
    /// a copy. The contract:
    /// `crates/cmdr-operations/src/transfer/volume/DETAILS.md`
    /// § "Copies keep the source's metadata".
    ///
    /// **Shared conformance assertions enforce it**: every backend that
    /// implements it runs `conformance::assert_apply_metadata_dates_a_folder`
    /// and `conformance::assert_apply_metadata_keeps` (test builds only).
    ///
    /// Default: `NotApplicable` for every class handed in
    /// ([`MetadataReport::none_applicable`]), for a store with no folder date
    /// to set (S3's prefixes, an MTP device, WebDAV) or no way to set one
    /// (ADB's sync protocol), and for every read-only backend.
    fn apply_metadata<'a>(
        &'a self,
        path: &'a Path,
        carried: &'a CarriedMetadata,
        modified: Option<std::time::SystemTime>,
    ) -> Pin<Box<dyn Future<Output = MetadataReport> + Send + 'a>> {
        let _ = path;
        Box::pin(async move { MetadataReport::none_applicable(carried, modified) })
    }

    /// What the FOLDER at `path` carries beyond its listing entry, restricted
    /// to `wanted`: the one read a copy spends on a source folder, since a
    /// folder has no stream to ask ([`VolumeReadStream::metadata`] is a file's).
    /// The copy calls it once per folder it created, when it stamps that
    /// folder's metadata, and merges the answer with the listing's dates.
    ///
    /// Default: empty, with no I/O, which is every backend whose listing entry
    /// already says all it knows. The archive answers from its index: a
    /// Finder-made zip's `__MACOSX` sidecar or a tar's pax records.
    fn folder_metadata<'a>(
        &'a self,
        path: &'a Path,
        wanted: MetadataClasses,
    ) -> Pin<Box<dyn Future<Output = CarriedMetadata> + Send + 'a>> {
        let _ = (path, wanted);
        Box::pin(async { CarriedMetadata::default() })
    }

    /// Clears what makes the item at `path` refuse its own removal, a removal
    /// that was just refused: what a copy put on it from its source (Locked, a
    /// `deny delete` ACL, SMB's Read-only). Never follows a symlink.
    ///
    /// ❗ Only a reversal calls it, on an item it already verified as the
    /// operation's own (invariant 9 in the app's `transfer/volume/DETAILS.md` §
    /// "Copies keep the source's metadata").
    /// The user's own delete never does: unlocking is not a delete's business.
    /// [`Unblock::Cleared`] says to try the removal again; what it hands back is
    /// what a landing would put back, and a reversal drops it.
    ///
    /// Default [`Unblock::NothingToClear`], with no I/O: a backend whose items
    /// can't wear a blocker.
    fn clear_blockers<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Unblock> + Send + 'a>> {
        let _ = path;
        Box::pin(async { Unblock::NothingToClear })
    }

    /// Deletes a single file or **empty** directory.
    ///
    /// **Strict contract: must NOT recurse.** If `path` is a non-empty directory,
    /// the implementation must return an error (typically `VolumeError::IoError`
    /// with errno `ENOTEMPTY` or equivalent), not silently delete the contents.
    /// The conflict resolver and several callers rely on this: `apply_volume_conflict_resolution`
    /// uses `is_directory` + skip-delete to enforce "Overwrite means merge for dirs"
    /// architecturally, but other call sites (rollback, partial-file cleanup) assume
    /// they only ever delete one node at a time and would over-delete if this contract
    /// loosened. The same-volume move's source cleanup goes further and treats the
    /// refusal AS the guarantee: a level still holding a child the user chose to
    /// Skip survives only because its delete fails.
    ///
    /// **A shared conformance assertion enforces this**, not the wording above:
    /// every backend's suite runs
    /// `conformance::assert_delete_leaves_a_non_empty_dir_intact` (test builds
    /// only). A backend that recurses fails it.
    ///
    /// For recursive deletes, callers should walk the tree themselves and call
    /// `delete` per leaf. See `remove_tree` in
    /// `crates/cmdr-operations/src/transfer/volume/cleanup.rs`.
    ///
    /// Default: `NotSupported`.
    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let _ = path;
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Cancel-aware version of [`delete`](Self::delete).
    ///
    /// MTP overrides this to thread the token through to mtp-rs's
    /// `delete_with_cancel`, which bails before issuing the `DeleteObject` PTP
    /// request once the token is cancelled, and through the per-handle
    /// `GetObjectInfo` roundtrips of the directory listing it takes on the way.
    ///
    /// Default impl delegates to `delete`, dropping the token.
    fn delete_with_cancel<'a>(
        &'a self,
        path: &'a Path,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let _ = cancel;
        self.delete(path)
    }

    /// Deletes the **empty directory** at `path`, for a caller that knows it
    /// walked a directory there (a delete walk's folders, a move's source
    /// sweep, a tree removal). Same no-recursion contract as
    /// [`delete`](Self::delete).
    ///
    /// ❗ **Where one name can hold a file AND a folder, this never reaches the
    /// file.** On an object store `notes` can sit beside `notes/…`, and a
    /// folder with no marker is gone the moment its last child is: a plain
    /// `delete` of its path then finds only the object and deletes that, a
    /// file nobody selected. Such a backend overrides this to act on the
    /// folder alone (S3, `crates/cmdr-s3/DETAILS.md` § "Folders, delete, and
    /// rename"); a folder already gone answers `NotFound`.
    ///
    /// Default: [`delete_with_cancel`](Self::delete_with_cancel), right for
    /// every backend where a name holds one thing.
    fn delete_directory<'a>(
        &'a self,
        path: &'a Path,
        cancel: Option<&'a CancellationToken>,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.delete_with_cancel(path, cancel)
    }

    /// Renames/moves a file or directory within this volume.
    ///
    /// Both source and destination paths are relative to the volume root.
    /// When `force` is false, returns `AlreadyExists` if the destination exists.
    /// When `force` is true, proceeds even if the destination exists (POSIX rename
    /// silently overwrites).
    ///
    /// **`force == true` still never takes a folder that holds something**: it
    /// refuses (typically `ENOTEMPTY`), as the local kernel does. The user
    /// confirmed replacing one entry, never a tree, and the verbs a backend
    /// reaches for don't refuse by themselves: WebDAV's `MOVE` with
    /// `Overwrite: T` deletes the whole destination tree, and `mv -f` moves the
    /// source INTO the folder. Enforced by
    /// `conformance::assert_forced_rename_keeps_a_non_empty_dir`.
    ///
    /// **`force == false` is a strict contract**, and it's the only thing standing
    /// between a move and the file it would replace: every caller that hasn't yet
    /// asked the user passes `false` and reads the refusal as "stop, there's
    /// something here". A backend that overwrote anyway turns each of those
    /// prompts into a destroyed file with no error to notice.
    ///
    /// **A shared conformance assertion enforces this**: every backend that
    /// implements `rename` runs
    /// `conformance::assert_rename_refuses_an_existing_destination` (test builds
    /// only).
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        let _ = (from, to, force);
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Whether renaming the entry at `path` is one cheap server-side call here,
    /// or a copy of its bytes plus a delete ([`RenameWork`], `server_side.rs`).
    ///
    /// ❗ Every caller of [`rename`](Self::rename) asks this first, and sends a
    /// [`RenameWork::CopyThenDelete`] entry through the transfer engine as a
    /// same-volume move (progress, pause, cancel, journaling) instead of
    /// calling `rename`. ❌ Never inferred from a backend kind: the answer is
    /// per entry (S3 renames a small file in one call, a folder never).
    ///
    /// Default `OneCall`, with no I/O.
    ///
    /// A volume that can answer `CopyThenDelete` says so up front with
    /// [`renames_can_copy`](Self::renames_can_copy).
    fn rename_work<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<RenameWork, VolumeError>> + Send + 'a>> {
        let _ = path;
        Box::pin(async { Ok(RenameWork::OneCall) })
    }

    /// Whether [`rename_work`](Self::rename_work) can answer `CopyThenDelete`
    /// for some entry here, asked with no I/O: a move within this volume may be
    /// a server-side copy per object. Published as
    /// `VolumeCapabilities::renames_can_copy`, so the Move dialog scans such a
    /// move instead of skipping the scan as it does for a one-call rename.
    ///
    /// Default `false`, matching `rename_work`'s `OneCall`. S3 answers `true`.
    fn renames_can_copy(&self) -> bool {
        false
    }

    /// Counts the files under `path` (a file counts as one), stopping once
    /// more than `cap` are found ([`SubtreeTally::complete`] is then `false`).
    /// What a rename that copies would move, for the rename editor to decide
    /// whether to confirm first.
    ///
    /// Default: one listing per folder ([`server_side::tally_by_listing`]). An
    /// object store overrides it with a recursive listing, a thousand keys per
    /// request whatever the nesting.
    fn tally_subtree<'a>(
        &'a self,
        path: &'a Path,
        cap: u64,
    ) -> Pin<Box<dyn Future<Output = Result<SubtreeTally, VolumeError>> + Send + 'a>> {
        Box::pin(server_side::tally_by_listing(self, path, cap))
    }

    /// Deletes several FILES, answering one result per path, in order.
    ///
    /// The batch form of [`delete`](Self::delete), for a move's source sweep:
    /// an object store deletes a thousand keys per request (`DeleteObjects`).
    /// ❗ Files only, each one a path the caller just listed as a file: a
    /// backend may delete by key without the folder check `delete` makes. A
    /// path already gone answers `Ok`.
    ///
    /// Default: [`delete`](Self::delete) per path.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn delete_files<'a>(
        &'a self,
        paths: &'a [PathBuf],
    ) -> Pin<Box<dyn Future<Output = Vec<Result<(), VolumeError>>> + Send + 'a>> {
        Box::pin(async move {
            let mut results = Vec::with_capacity(paths.len());
            for path in paths {
                results.push(match self.delete(path).await {
                    Err(VolumeError::NotFound(_)) => Ok(()),
                    other => other,
                });
            }
            results
        })
    }

    /// How many files one [`delete_files`](Self::delete_files) call removes in
    /// a single request, for a backend that batches on the wire; `None` (the
    /// default) when it deletes one file at a time.
    ///
    /// A delete walker hands such a backend its files in chunks of this size,
    /// pausing and checking Cancel between chunks, instead of one
    /// [`delete`](Self::delete) per file: on an object store that's a capped
    /// listing, a stat, and a delete per object, three thousand requests for a
    /// folder of a thousand files where two batches do.
    fn delete_batch_size(&self) -> Option<std::num::NonZeroUsize> {
        None
    }

    // ========================================
    // Mutation notification
    // ========================================

    /// Called after a successful mutation (create, delete, rename, or a
    /// `write_from_stream` that landed) so a pane showing `parent_path` updates
    /// without waiting for a watcher event.
    ///
    /// **Default is a no-op**, because what "update the listing" means belongs to
    /// the host, not to this crate. Every mutable backend overrides it:
    /// `LocalPosixVolume` stats through `std::fs` and patches the app's listing
    /// cache (`cmdr_local::mutation`), while `SmbVolume` and `MtpVolume`
    /// build the entry from their own protocol's `get_metadata`. Their watchers
    /// are lossy under load, so this patch is what keeps a destination pane
    /// honest after a bulk copy. A read-only or test backend leaves the default.
    ///
    /// Fire-and-forget: no error propagation, because a failed cache patch must
    /// never fail the mutation that already succeeded.
    fn notify_mutation<'a>(
        &'a self,
        volume_id: &'a str,
        parent_path: &'a Path,
        mutation: MutationEvent,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let _ = (volume_id, parent_path, mutation);
        Box::pin(async {})
    }

    // ========================================
    // Lifecycle: Optional, default no-op
    // ========================================

    /// How live this volume's SESSION is, or `None` when it has no session to
    /// speak of (a local disk, an archive, the git portal).
    ///
    /// Every connecting backend answers: SMB, SFTP, WebDAV, and ADB. The frontend
    /// draws the switcher dot from it and the reconnect manager decides whether to
    /// run a backoff cycle.
    ///
    /// ❗ **`is_some()` is not an "is this SMB" test.** Four backends answer, so a
    /// caller that means "SMB" has to ask [`backend_kind`](Self::backend_kind)
    /// instead; the alternative is the SMB indexer receiving an `sftp://` root.
    fn connection_state(&self) -> Option<ConnectionState> {
        None
    }

    /// Which session this volume is on: a counter its backend bumps each time
    /// a reconnect SUCCESSFULLY installs a new session in place, and never
    /// otherwise (❌ not on a reconnect that found the session already live, ❌
    /// not on a state flip). `None` (the default) when the backend keeps no such
    /// counter.
    ///
    /// Read beside [`connection_state`](Self::connection_state): a volume that
    /// reads live on a NEWER generation than the one a failing call started on
    /// was reconnected while that call ran, so the call's transport error was the
    /// old session's and the work is worth running again
    /// (`crates/cmdr-operations/src/transfer/volume_outage.rs`). A `None` keeps
    /// such an error what it was.
    fn connection_generation(&self) -> Option<u64> {
        None
    }

    /// Which backend serves this volume.
    ///
    /// For the few app decisions that are genuinely about the transport (which
    /// indexer transport may walk it, and whether any may at all; whether the file
    /// viewer treats its paths as local; whether an SMB upgrade has anything to do).
    ///
    /// The default is [`BackendKind::Local`], so a test double compiles without
    /// naming one and an unclassified backend gets no remote treatment.
    /// ❌ The kind itself is never published to the frontend: `volume-capabilities.ts`
    /// classifies a pane off `fsType` and category, and an un-upgraded SMB share
    /// served by `LocalPosixVolume` would answer `Local` here. A capability derived
    /// from it (`can_be_indexed`) is published, because that's a question about the
    /// backend.
    fn backend_kind(&self) -> BackendKind {
        BackendKind::Local
    }

    /// What a "Sign in" affordance on this volume may ask for, **right now**.
    ///
    /// Asked when the affordance renders, which is after this volume reported
    /// [`VolumeConnection::NeedsCredentials`](host::events::VolumeConnection::NeedsCredentials).
    /// ❗ An implementation whose credential is decided per dial has to answer
    /// from its live state, ❌ never from what the first connection used: that
    /// value is a prediction, and by the time anything reads it the session it
    /// described is gone.
    ///
    /// The default is [`SignInShape::Password`], which is what every backend
    /// that asks for one uses, and it is the safe way to be wrong: a needless
    /// password box is recoverable, while a wrong [`SignInShape::Nothing`]
    /// leaves a volume the user can't sign in to at all. Override it where a
    /// backend has rungs a password can't mend (`SftpVolume`).
    fn sign_in_prompt(&self) -> SignInShape {
        SignInShape::Password
    }

    /// This volume's [`Retirement`] flag, when it keeps one.
    ///
    /// The registry retires a volume as it leaves the registry, and this is how
    /// it reaches the flag: it holds a `dyn Volume` and knows nothing about any
    /// backend's concrete type. A backend reads the same flag back through a
    /// [`SelfHandle`], which is how its watcher and its reconnect loop learn they
    /// have nothing left to act on.
    ///
    /// **Default is `None`**, and that's the honest answer for a backend whose
    /// work never outlives one operation: a local filesystem, an in-archive
    /// listing, a test double. Override it the moment you spawn something that
    /// keeps running between calls, or that thing keeps running after the app has
    /// forgotten your volume. `volume/host/DETAILS.md` § "Writing a new backend".
    fn retirement(&self) -> Option<&Retirement> {
        None
    }

    /// Called when the volume is about to be unmounted/unregistered.
    ///
    /// Implementations can use this to clean up resources (disconnect network
    /// sessions, cancel background tasks, etc.). Default is a no-op.
    fn on_unmount(&self) {}

    /// Called when a NEWER instance is taking this volume's id in the registry
    /// while this one may still be serving in-flight work.
    ///
    /// This is NOT an unmount: the device is still there, and every caller that
    /// already holds an `Arc` to this instance (a running transfer, an open
    /// viewer stream, an in-flight listing, the indexer) must keep working on
    /// it. So an implementation may only retire the parts that belong to the
    /// *id* (background tasks, outward-facing events) and must leave the parts
    /// that belong to its *holders* (sessions, handles) alone. Those are
    /// released when the last `Arc` drops.
    ///
    /// The default delegates to [`Volume::on_unmount`], which is the safe
    /// choice for a backend with no live session to protect. Override it when
    /// tearing the resources down mid-flight would break a holder.
    fn on_superseded(&self) {
        self.on_unmount();
    }

    /// Tries to rebuild this volume's underlying session in place after a
    /// transient connection loss. Idempotent and expected to be single-flight.
    ///
    /// Default returns `Err(NotSupported)`. Every backend that answers
    /// [`connection_state`](Self::connection_state) overrides it (SMB, SFTP,
    /// WebDAV, S3, and ADB). Three callers dial it: the frontend reconnect
    /// manager (each backoff tick, and "Retry now"), a backend's own background
    /// loop, and a transfer waiting for its volume to come back
    /// (`crates/cmdr-operations/src/transfer/volume_outage.rs`), which is why it must stay
    /// ONE attempt: the caller owns the patience.
    ///
    /// ❗ An auth refusal comes back as `PermissionDenied` (or `NeedsPassword`)
    /// and stops every caller's retrying: dialing again on stale credentials is
    /// how an account gets locked.
    fn attempt_reconnect<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Reconnect using freshly-entered credentials, replacing whatever was cached.
    ///
    /// Invoked by the "Sign in" affordance the frontend shows when an in-place reconnect
    /// gave up on an auth failure (a password changed on the server). The implementation
    /// runs the normal reconnect with them and, ❗ only once that connected, files them the way
    /// `sign_in.remember` asks (`secret_store::keep_signed_in_secret`, #530), so the next
    /// reconnect is silent. Answers `SignedIn`, which says when a checked Remember box was refused
    /// by the store, so the sheet can say so while the connection stays up. Default
    /// `Err(NotSupported)`; SMB, SFTP, WebDAV, and S3 override it.
    fn reconnect_with_credentials<'a>(
        &'a self,
        _sign_in: secret_store::SignIn,
    ) -> Pin<Box<dyn Future<Output = Result<secret_store::SignedIn, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Files the credentials of a sign-in that ALREADY connected this volume, by
    /// the same rule [`reconnect_with_credentials`](Self::reconnect_with_credentials)
    /// files them after its dial, ❌ without dialing.
    ///
    /// The sheet's "Try again" after `remember_refused` (#603): the round proved
    /// the secret, and a second sign-in would find the session live, dial
    /// nothing, and so save nothing. ❗ Only for the secret that round connected
    /// with: the caller keeps that promise. Refuses what the sign-in refuses
    /// (another account, a rung no secret mends). Default `Err(NotSupported)`;
    /// SMB, SFTP, WebDAV, and S3 override it.
    fn remember_sign_in<'a>(
        &'a self,
        _sign_in: secret_store::SignIn,
    ) -> Pin<Box<dyn Future<Output = Result<secret_store::SignedIn, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    // ========================================
    // Watching: Optional, default no-op
    // ========================================

    /// Whether a `notify`-based OS watch can be established on this volume's
    /// paths, which is the one thing this gates: the listing engine's watch
    /// observer arms a watch only when it's true.
    ///
    /// ❌ Says NOTHING about what such a watch would see. A backend whose paths
    /// don't exist on the local filesystem (MTP), or that has no watcher yet
    /// (SMB), answers `false`; a backend answering `true` still has to declare
    /// its coverage in [`listing_watch_coverage`](Self::listing_watch_coverage).
    fn can_watch_listings(&self) -> bool {
        false
    }

    /// Whether this volume's paths can be accessed via `std::fs` operations
    /// (stat, read_dir, metadata, etc.). True for local filesystems and
    /// OS-mounted network shares. False for protocol-only volumes like MTP.
    fn supports_local_fs_access(&self) -> bool {
        true
    }

    /// Whether OTHER apps can reach this volume's paths as real filesystem
    /// paths — that is, whether a `file://` URL built from a path this volume
    /// hands out is one Finder, a browser, or a mail client can open.
    ///
    /// A different question from
    /// [`supports_local_fs_access`](Self::supports_local_fs_access), which asks
    /// whether CMDR should reach a path through `std::fs`. A direct-SMB volume
    /// answers `false` there (its own I/O goes over smb2) while answering `true`
    /// here, because the share stays OS-mounted alongside the smb2 session and
    /// its paths are ordinary `/Volumes/…` paths. Conflating the two is what
    /// made a drag out of an SMB pane offer file promises only, which every
    /// drop target except Finder rejects.
    ///
    /// Consumed by the macOS drag-out path (`commands::file_system::drag`) and
    /// Quick Look to pick what they can offer. Default: whatever
    /// `supports_local_fs_access` says, which is right for every backend where the
    /// two questions coincide.
    ///
    /// An overriding backend owes an answer that tracks the MOUNT, not just the
    /// backend kind: a direct SMB volume answers `true` while its share is mounted
    /// and `false` once [`note_root_mount_gone`](Self::note_root_mount_gone) says
    /// otherwise, because a `file://` URL under a mount that's gone opens nowhere.
    fn paths_are_os_visible(&self) -> bool {
        self.supports_local_fs_access()
    }

    /// What a live watch on the listing at `path` observes right now. Used by
    /// `cmdr_listing::Listings::authoritative_listing` to decide
    /// whether a cached listing can replace a real read in write-op pre-flight,
    /// which only [`WatchCoverage::EveryWriter`] allows.
    ///
    /// Two facts fold into the answer, and a backend owes both: whether a watch
    /// is live for this path at all, and what a live one can see. Answer
    /// [`None`](WatchCoverage::None) whenever no watch is attached, including
    /// during the gap between a listing being populated and its watcher being
    /// registered.
    ///
    /// `EveryWriter` is a claim about WHICH WRITERS reach us, not about latency:
    /// every backend has a debounce or settling window between a real change and
    /// the cache reflecting it. See the freshness contract on
    /// `Listings::authoritative_listing` for the per-backend windows callers must
    /// tolerate.
    ///
    /// Default `None`: a new backend claims coverage explicitly or gets none.
    fn listing_watch_coverage(&self, _path: &Path) -> WatchCoverage {
        WatchCoverage::None
    }

    /// The one spelling a cached pane listing of `path` is keyed under.
    ///
    /// The app's listing cache stores every listing and answers every lookup (a
    /// backend's change report, the fresh-listing oracle) through this, so two
    /// spellings of one directory reach the same pane. Override it when this
    /// backend's paths arrive spelled more than one way: an MTP storage reports
    /// changes at its `mtp://` URL while its rows carry the inner `/DCIM`.
    ///
    /// Default: `path` verbatim, for a backend with one spelling. Must be pure and
    /// cheap: it runs on every listing lookup.
    fn listing_path(&self, path: &Path) -> PathBuf {
        path.to_path_buf()
    }

    // ========================================
    // Copy/Export: Optional, default no-op
    // ========================================

    /// Returns whether this volume can stream its bytes via `open_read_stream`
    /// (that is, it can act as a source in a cross-volume copy). Gates the copy
    /// dialog's "copy from this volume" UI.
    fn supports_export(&self) -> bool {
        false
    }

    /// Whether [`share_link`](Self::share_link) can mint a link to a file here:
    /// a URL anyone can open to download it, for a while. Gates "Copy share
    /// link" in the context menu and the command palette.
    ///
    /// Default `false`, matching `share_link`'s `NotSupported`. S3 answers `true`
    /// (a presigned GET, computed offline).
    fn supports_share_links(&self) -> bool {
        false
    }

    /// A link to the FILE at `path` that anyone can open to download it, valid
    /// for `expires_in`. ❗ The link is a credential: [`ShareLink`] keeps it out of
    /// `Debug`, and ❌ nothing may log what it holds.
    ///
    /// A folder answers [`VolumeError::IsADirectory`]: a link names one object.
    /// Default `NotSupported`; a backend that overrides it answers
    /// [`supports_share_links`](Self::supports_share_links) `true`.
    fn share_link<'a>(
        &'a self,
        path: &'a Path,
        expires_in: ShareLinkExpiry,
    ) -> Pin<Box<dyn Future<Output = Result<ShareLink, VolumeError>> + Send + 'a>> {
        let _ = (path, expires_in);
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Which metadata classes this backend keeps on what a copy writes, and
    /// which its read streams report: [`MetadataCaps`](crate::metadata::MetadataCaps).
    ///
    /// A claim about the BACKEND, answered without touching a path. The copy
    /// engine reads it to know which classes a copy is about to leave behind
    /// (it asks the source only about those, and only when there are any), and
    /// the archive compress planner reads `reports` to know whether a listing's
    /// [`FileEntry::permissions`](crate::entry::FileEntry::permissions) is a real
    /// mode somebody recorded rather than the `0` of a backend with no
    /// permission concept.
    ///
    /// Default [`MetadataCaps::NONE`](crate::metadata::MetadataCaps::NONE): a
    /// backend that keeps or reports a class says so. ❗ Claiming a class it
    /// doesn't honor fails `conformance::metadata`; reporting a mode without real
    /// bits is worse than reporting nothing, since a non-zero mode is a fact
    /// somebody puts on the user's file. Where each backend stands:
    /// `crates/cmdr-operations/src/transfer/volume/DETAILS.md`
    /// § "Copies keep the source's metadata".
    fn metadata_caps(&self) -> crate::metadata::MetadataCaps {
        crate::metadata::MetadataCaps::NONE
    }

    /// Whether this volume accepts mutations at all: creating files and
    /// directories, renaming, and deleting.
    ///
    /// A claim about the BACKEND, not about one path or one mount. A read-only
    /// mount of a writable backend still answers `true`; that mount's own
    /// read-only flag is separate and layers on top (it reaches the frontend as
    /// `mountIsReadOnly` on the location).
    ///
    /// The predicate keeps the bare name and the published field spells its
    /// subject out ([`VolumeCapabilities::backend_can_write`]): inside a
    /// `Volume` impl the subject can only be the backend, while the published
    /// struct sits next to the location's mount flag, where it can't.
    ///
    /// Default `false`, matching the `NotSupported` default of every mutation
    /// method above: a backend that implements them opts in, and one that
    /// doesn't can't accidentally advertise writes it will refuse.
    ///
    /// **A shared conformance assertion enforces this**, in whichever direction
    /// the answer claims: every backend's suite runs
    /// `conformance::assert_writability_matches_the_mutations_offered` (test
    /// builds only), so an out-of-date `true` fails rather than reaching the UI
    /// as an enabled button that can't work.
    fn is_writable(&self) -> bool {
        false
    }

    /// The mount this volume sits on, for a backend reached through `std::fs`;
    /// `None` for one that isn't (a session, a device, a view inside a drive).
    /// Folded into `can_be_indexed` below: a local volume on an NFS or FUSE mount
    /// has no index transport, though its backend in general does.
    fn mount_class(&self) -> Option<MountClass> {
        None
    }

    /// This volume's capability surface as DATA, for consumers outside the
    /// backend (it travels over IPC to the frontend).
    ///
    /// ❌ Never override this, and ❌ never compute an answer inside it. It's a
    /// pure fold of the predicates above, and that's the whole point: a
    /// capability has one answer, so growing the surface means adding a
    /// predicate and folding it here. An override would reintroduce exactly the
    /// second source of truth this retired. See
    /// `crates/cmdr-fs/src/volume/capabilities.rs` for what belongs in the
    /// published struct and what stays a backend-side predicate.
    fn capabilities(&self) -> VolumeCapabilities {
        VolumeCapabilities {
            backend_can_write: self.is_writable(),
            can_export: self.supports_export(),
            can_share_links: self.supports_share_links(),
            can_be_indexed: self.backend_kind().can_be_indexed()
                && self.mount_class().is_none_or(MountClass::can_be_indexed),
            renames_can_copy: self.renames_can_copy(),
            has_os_mount_fallback: self.backend_kind().has_os_mount_fallback(),
        }
    }

    /// How many streaming copy operations can be driven concurrently on this
    /// volume.
    ///
    /// Volumes serialized by a single underlying transport (MTP over USB,
    /// single SMB session without pipelining) return `1`; volumes that support
    /// parallel I/O (local disk, SMB with Phase 3 concurrent `execute`, S3)
    /// return higher. The copy engine takes `min(src, dst, 32)` to decide how
    /// many `FuturesUnordered` tasks to keep in flight. Default `1` preserves
    /// current sequential behavior for any new backend that doesn't override.
    fn max_concurrent_ops(&self) -> usize {
        1
    }

    /// Whether ONE operation on this volume is a local syscall — microseconds,
    /// no transport round trip — rather than a request over a network or a bus.
    ///
    /// This is a claim about COST, which makes it a different question from
    /// [`supports_local_fs_access`](Self::supports_local_fs_access): an
    /// OS-mounted SMB share answers `true` there (its paths do go through
    /// `std::fs`) and would answer `false` here.
    ///
    /// The transfer driver reads it to size its concurrency window
    /// (`crates/cmdr-operations/src/transfer/volume/copy.rs::transfer_concurrency`). A
    /// local volume's [`max_concurrent_ops`](Self::max_concurrent_ops) is a
    /// CPU-core heuristic that has nothing to say about how many requests a
    /// network peer should carry, so it isn't allowed to bound one; a remote
    /// volume's cap is a real transport limit and always is.
    ///
    /// Default `false`, which is the conservative answer in both directions: an
    /// undeclared backend keeps bounding its peer, and keeps every per-file
    /// destination probe the driver would otherwise run.
    fn operations_are_local(&self) -> bool {
        false
    }

    /// Whether extracting from this volume is inherently SEQUENTIAL: the stream
    /// (or a solid block) must be decoded front-to-back, so there's no cheap
    /// random access to an arbitrary entry. `true` for a compressed tar
    /// (`.tar.gz`/`.xz`/`.bz2`/`.zst`) and 7z; `false` for a plain `.tar`, a zip,
    /// and every real filesystem.
    ///
    /// The access-class declaration for the copy planner's one-pass strategy: a
    /// per-entry random read of a sequential archive re-decodes the prefix in
    /// front of each entry, so extracting a whole subtree entry-by-entry is
    /// O(n²). A planner that sees `true` should extract the subtree in ONE
    /// sequential pass. Default `false` keeps the existing per-entry behavior for
    /// random-access backends.
    fn extraction_is_sequential(&self, _path: &Path) -> bool {
        false
    }

    /// Opens a ONE-PASS extractor over the subtree at `path`, for a SEQUENTIAL
    /// source (see [`extraction_is_sequential`](Self::extraction_is_sequential)).
    /// Decoding the stream once, it yields the subtree's files in archive order so
    /// the copy engine can materialize them without the O(n²) per-entry prefix
    /// re-decode.
    ///
    /// Default `NotSupported`: only a sequential backend (the archive volume)
    /// implements it, and the copy planner calls it only after
    /// `extraction_is_sequential` returns `true`.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn open_sequential_extract<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn SequentialExtract>, VolumeError>> + Send + 'a>> {
        let _ = path;
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Scans a path recursively to get statistics for a copy operation.
    /// Returns file count, directory count, and total bytes.
    ///
    /// ⚠️ **No stop and no progress**: this method takes neither, so a walk behind
    /// it runs to completion whatever the user presses. That is fine for its
    /// callers — a single cheap subtree, a test, a per-path step of the default
    /// batch below — and it is why the batch method is the one every transfer
    /// goes through. ❌ Don't reach for this from an operation's pre-flight.
    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        let _ = path;
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Scans multiple paths to get aggregate + per-path copy statistics, with
    /// nothing to report to and nobody to answer to.
    ///
    /// Convenience over [`scan_for_copy_batch_with_boundary`](Self::scan_for_copy_batch_with_boundary)
    /// for a caller with no dialog and no Cancel button (a test, an internal
    /// probe). ❌ Never override this one; override the boundary-taking method,
    /// which is where every backend's real batch strategy lives.
    fn scan_for_copy_batch<'a>(
        &'a self,
        paths: &'a [PathBuf],
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let boundary = ScanBoundary::silent();
            self.scan_for_copy_batch_with_boundary(paths, &boundary).await
        })
    }

    /// The batch copy scan every transfer runs: aggregate + per-path statistics,
    /// reporting its progress and honoring the user's Cancel and Pause through
    /// one [`ScanBoundary`].
    ///
    /// The returned `BatchScanResult` carries both the rolled-up `aggregate`
    /// (what the scan-preview / pre-flight checks want) and a `per_path` vec
    /// (what the copy engine uses to seed its per-source hints, so it doesn't
    /// have to re-probe each source's type and size with a separate stat).
    ///
    /// **What an implementation owes the boundary.** Call `boundary.file(path, size)`
    /// for every leaf and `boundary.dir(path)` before every directory listing, and
    /// propagate what they return with `?`. That single habit buys four things
    /// at once: a transfer's live tree gets every entry (paths in the CALLER's
    /// form, `ScanRecorder`), the dialog's counters climb instead of showing a frozen
    /// `0 files` for a slow enumeration (the MTP listing of `/DCIM/Camera` with
    /// 1k+ entries is ~17 s of USB round trips), the scan watchdog can tell a
    /// slow share from a dead one, and Cancel and Pause reach a walk that would
    /// otherwise run for minutes with no way to stop it.
    ///
    /// ❗ **Ask BEFORE the round trip, not after.** A boundary on the far side of
    /// a listing is a boundary the user waits out; over a sleeping NAS that is
    /// seconds per directory.
    ///
    /// ❗ **A stopped scan returns [`VolumeError::Cancelled`], ❌ never a partial
    /// result.** Callers read the totals as the size of the transfer they are
    /// about to run, so a truncated total that looks successful is a progress bar
    /// that finishes at 30% and a space check that passes when it shouldn't.
    /// `ScanBoundary::file` / `dir` already hand back exactly that error.
    ///
    /// The default iterates over `scan_for_copy` per path, asking the boundary
    /// between paths. That is honest for a backend whose per-path scan is cheap
    /// and bounded (in-memory, an archive's central directory) and ❌ not enough
    /// for one that walks a real tree per path: a backend with expensive per-path
    /// I/O overrides this to batch (SMB pipelines its stats over one session; MTP
    /// groups paths by parent and lists each parent once) and gets a boundary
    /// inside its own recursion at the same time.
    fn scan_for_copy_batch_with_boundary<'a>(
        &'a self,
        paths: &'a [PathBuf],
        boundary: &'a ScanBoundary<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let mut aggregate = CopyScanResult {
                file_count: 0,
                dir_count: 0,
                total_bytes: 0,
                dedup_bytes: 0,
                // Aggregate over multiple paths: meaningless for a batch.
                // Callers that need per-path type should read `per_path`.
                top_level_is_directory: false,
                top_level_modified_at: None,
                top_level_created_at: None,
            };
            let mut per_path = Vec::with_capacity(paths.len());
            for path in paths {
                boundary.check().await?;
                let scan = self.scan_for_copy(path).await?;
                aggregate.file_count += scan.file_count;
                aggregate.dir_count += scan.dir_count;
                aggregate.total_bytes += scan.total_bytes;
                aggregate.dedup_bytes += scan.dedup_bytes;
                boundary.subtree(path, &scan).await?;
                per_path.push((path.clone(), scan));
            }
            Ok(BatchScanResult {
                aggregate,
                per_path,
                files: None,
            })
        })
    }

    /// Checks destination for conflicts with source items.
    /// Returns list of files that already exist at destination.
    ///
    /// ❗ **A destination that isn't there yet holds nothing, so the answer is an
    /// empty list, never [`VolumeError::NotFound`].** Pasting into a folder the
    /// transfer is about to create is an ordinary thing to do, and this
    /// pre-flight runs before anything is created. The caller propagates what
    /// comes back, so reporting the missing directory doesn't produce an odd
    /// conflict list; it refuses a legitimate copy outright, over a destination
    /// that is empty in the only sense that matters.
    ///
    /// [`scan_walk::scan_conflicts`]
    /// keeps that for a backend with nothing special to say about the listing; a
    /// backend that lists its own way owes it by hand. Pinned by
    /// [`conformance::assert_conflict_scan_reads_a_missing_destination_as_empty`].
    fn scan_for_conflicts<'a>(
        &'a self,
        source_items: &'a [SourceItemInfo],
        dest_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        let _ = (source_items, dest_path);
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Gets space information for this volume.
    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Space for a readout that polls it every few seconds (the space poller).
    /// Defaults to [`get_space_info`](Self::get_space_info).
    ///
    /// A backend whose exact figure is expensive may answer from a recent exact
    /// reading plus what a cheap one says moved since: on macOS the local backend
    /// pays `statfs` here, where `get_space_info` asks CacheDelete for purgeable
    /// space at ~6.5 ms of CPU. Anything that acts on the figure once (the transfer
    /// pre-flight) keeps calling `get_space_info` / `get_space_info_at`.
    fn get_live_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        self.get_space_info()
    }

    /// Space for the filesystem that holds `path`. Defaults to
    /// [`get_space_info`](Self::get_space_info), the volume's one figure.
    ///
    /// A volume spanning several filesystems answers per path: a phone over ADB
    /// mounts a read-only system image at `/` (no room ever) beside its shared
    /// storage and any SD card. The transfer pre-flight asks this about the
    /// destination folder, so a copy is judged against the filesystem it lands
    /// on. The pane's indicator and the space poller keep the volume's figure.
    fn get_space_info_at<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        let _ = path;
        self.get_space_info()
    }

    /// Whether a write into `path` would be taken right now, and if not, why, as
    /// far as this backend can tell WITHOUT writing anything.
    ///
    /// `path` is a destination folder that may not exist yet (a copy creates it),
    /// so a backend answers for the nearest existing folder at or above it.
    /// Default [`WriteAccess::Unknown`]: a backend with no way to ask says so, and
    /// ❌ never answers a guess. The transfer pre-flight asks this BEFORE it
    /// measures space, so a folder nothing can be written to never reads as a full
    /// one.
    fn write_access_at<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = WriteAccess> + Send + 'a>> {
        let _ = path;
        Box::pin(async { WriteAccess::Unknown })
    }

    /// Recommended poll interval for live disk-space monitoring.
    ///
    /// Local volumes use a short interval (2 s) because their live reading
    /// ([`get_live_space_info`](Self::get_live_space_info)) is mostly a
    /// microsecond `statfs`. Network and MTP volumes use a longer interval (5 s)
    /// to avoid unnecessary traffic. Returns `None` if space polling is not
    /// meaningful for this volume type (for example, in-memory test volumes).
    fn space_poll_interval(&self) -> Option<std::time::Duration> {
        Some(std::time::Duration::from_secs(2))
    }

    // ========================================
    // Capability hints for copy optimization
    // ========================================

    /// Returns the local filesystem path if this volume is backed by one.
    /// Used to optimize local-to-local copies using native OS APIs (such as copyfile on macOS).
    /// Returns None for non-local volumes (MTP, S3, FTP, etc.).
    fn local_path(&self) -> Option<PathBuf> {
        None
    }

    /// The one local FILE this volume presents as a folder: an archive whose
    /// `.zip` (or tar, 7z) sits on a local filesystem. `None` for every other
    /// backend, and for an archive on a remote parent (direct SMB, MTP) or inside
    /// another archive, where no `std::fs` call reaches it.
    ///
    /// The copy engine reads that file's own `com.apple.quarantine` here, once per
    /// extract, to hand it to every item the extract creates (Gatekeeper's rule).
    /// It's a question about the volume's backing, so the engine asks it without
    /// knowing which backend it holds, and the backend never reads its own
    /// parent's metadata.
    fn local_backing_file(&self) -> Option<&Path> {
        None
    }

    /// Returns true if this volume supports streaming read/write operations.
    fn supports_streaming(&self) -> bool {
        false
    }

    /// Whether this backend can consume a stream whose final length is unknown.
    ///
    /// This is separate from [`supports_streaming`](Self::supports_streaming): a
    /// protocol may stream ordinary files while still requiring an exact length
    /// before it opens a destination. The default is conservative.
    fn supports_unknown_length_writes(&self) -> bool {
        false
    }

    /// Whether `rename(from, to, force = true)` replaces an existing file at
    /// `to` atomically, without first removing or moving it in a separate step.
    ///
    /// Callers publishing validated generated output use this to choose between
    /// one atomic replacement and a tracked aside/restore sequence. The default
    /// is conservative because many protocols implement force by deleting first.
    fn supports_atomic_replace_rename(&self) -> bool {
        false
    }

    /// Whether `create_directory` reliably returns `VolumeError::AlreadyExists`
    /// when a directory of the same name already exists at the path.
    ///
    /// The scan-as-you-merge folder-merge walker
    /// (`crates/cmdr-operations/src/transfer/volume/strategy.rs`) uses the `AlreadyExists`
    /// result as the signal that a destination level PRE-EXISTED and must be
    /// merged into (list it once, resolve clashing children) rather than created
    /// fresh. Default `true` covers LocalPosix (`std::fs::create_dir` →
    /// `ErrorKind::AlreadyExists`), SMB (smb2 typed STATUS_OBJECT_NAME_COLLISION),
    /// and InMemory. `MtpVolume` overrides to `false`: the MTP protocol allows
    /// same-name sibling objects and `create_folder` silently makes a duplicate
    /// `photos` instead of erroring, so the walker must pre-check existence on
    /// MTP before creating — a blindly-created duplicate would make the merge
    /// target the wrong directory.
    fn create_directory_errors_on_existing_dir(&self) -> bool {
        true
    }

    /// Whether a NEW name Cmdr creates here goes out composed (NFC), whatever
    /// spelling the name arrived in. Read through
    /// [`spell_new_name`](Self::spell_new_name), ❌ never directly.
    ///
    /// For a backend that stores a name's bytes as sent while the clients sharing
    /// it expect one spelling. SMB, SFTP, and WebDAV answer `true`: macOS hands
    /// out many local names decomposed, and a decomposed name on a share is one
    /// Finder (over the kernel mount, which composes on lookup), Windows, and
    /// Linux clients list and then can't open, while on an SFTP or WebDAV server
    /// it's one that web servers and scripts matching bytes don't find. Default
    /// `false`: the name goes out as given.
    ///
    /// ❗ A new name only: a copy or move to a free name, a new folder or file, a
    /// rename's target, a ` (1)` rename pick. An operation that addresses an
    /// EXISTING entry (an overwrite, a merge into a folder that's there, a move
    /// that keeps an entry's name) uses that entry's stored bytes, or it plants a
    /// look-alike beside it.
    fn composes_new_names(&self) -> bool {
        false
    }

    /// Whether this volume's own lookups find a name whatever Unicode form it's
    /// asked in, so one folder can never hold two spellings of it.
    ///
    /// A write guard reads it to skip the listing it would otherwise take to find
    /// a look-alike after a lookup missed (`crates/cmdr-operations/src/look_alike.rs`): here
    /// the miss already means there's none. `LocalPosixVolume` answers `true` on
    /// macOS (APFS and HFS+ match names normalization-insensitively). Default
    /// `false`, the byte-exact answer (SMB, SFTP, MTP, ADB, an in-memory store),
    /// which costs a listing only for a non-ASCII name nothing matched.
    fn matches_names_in_any_unicode_form(&self) -> bool {
        false
    }

    /// `name` as this volume wants a NEW entry spelled: composed where
    /// [`composes_new_names`](Self::composes_new_names) asks for it, else as given.
    /// ❌ Don't override; answer `composes_new_names` instead.
    fn spell_new_name<'n>(&self, name: &'n str) -> std::borrow::Cow<'n, str> {
        if self.composes_new_names() {
            crate::name_fold::composed(name)
        } else {
            std::borrow::Cow::Borrowed(name)
        }
    }

    /// Opens a streaming reader for the given path.
    ///
    /// Returns a VolumeReadStream that yields chunks of data.
    /// The stream must be fully consumed or dropped before other operations.
    ///
    /// # Streaming requirement
    ///
    /// **Must stream.** Don't read the whole file into a `Vec<u8>` inside
    /// this method and hand chunks of it back. That's just pre-buffering
    /// with extra steps. A user streaming an 8 GB file would allocate 8 GB
    /// of RAM before the consumer sees a single byte. Drive the backend on
    /// demand from `next_chunk` (smb2: an `smb2::FileDownload`; MTP: bounded
    /// `GetPartialObject64` windows). If the backend gives you a borrowed
    /// handle, use a bounded producer/consumer channel (see `SmbReadStream`
    /// for the pattern).
    ///
    /// Peak memory per transfer should be bounded by a small chunk buffer
    /// (~1 MiB) regardless of file size.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let _ = path;
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Opens a streaming reader with an optional size hint from the caller.
    ///
    /// Network-backed volumes can use the hint to pick a faster compound
    /// request path for small files (e.g., SMB's CREATE+READ+CLOSE compound)
    /// instead of the 3-RTT streaming open. Backends that can't use the hint
    /// fall through to `open_read_stream`.
    ///
    /// The hint is best-effort. Callers pass `None` when they don't know
    /// the size ahead of time, and the backend must work correctly either
    /// way.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn open_read_stream_with_hint<'a>(
        &'a self,
        path: &'a Path,
        size_hint: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let _ = size_hint;
        self.open_read_stream(path)
    }

    /// Opens a read stream for BACKGROUND bulk work (media enrichment prefetch),
    /// bracketed by [`begin_scan_session`](Self::begin_scan_session) /
    /// [`end_scan_session`](Self::end_scan_session).
    ///
    /// A backend may route it over scan-scoped resources so concurrent background
    /// reads don't serialize on — or compete with — the session the pane browses
    /// through: `SmbVolume` serves small hinted files from its scan-connection
    /// pool via the 1-RTT compound read (see `crates/cmdr-smb/src/volume/scan_pool.rs`). Semantically
    /// identical to [`open_read_stream_with_hint`](Self::open_read_stream_with_hint),
    /// which is also the default.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn open_read_stream_for_scan<'a>(
        &'a self,
        path: &'a Path,
        size_hint: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.open_read_stream_with_hint(path, size_hint)
    }

    /// Opens a streaming reader that starts at a byte offset (resumable read).
    ///
    /// Streams `[offset, size)` of `path`. `offset == 0` is equivalent to
    /// [`open_read_stream`](Self::open_read_stream) (the whole file). The
    /// returned stream's `total_size()` reports the FULL file size (not the
    /// remaining tail), so a resumed transfer's progress stays anchored to the
    /// whole file; `bytes_read()` counts only this segment.
    ///
    /// A resumable-read primitive. The copy path no longer reopens at an offset
    /// (pause and foreground yield park in place between bounded windows in the
    /// transfer wrapper `CheckpointStream`, so nothing calls this with a non-zero
    /// offset today), but MTP keeps it correct: a non-zero `offset` streams
    /// exactly `[offset, size)` with no gap or overlap.
    ///
    /// Default is `NotSupported`; only MTP implements it. `MtpVolume`'s
    /// `open_read_stream` routes through it with `offset == 0`.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn open_read_stream_at_offset<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        let _ = (path, offset);
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Reads a bounded byte range `[offset, offset + len)` of `path`, returning
    /// the bytes actually read.
    ///
    /// The positioned, `pread`-shaped primitive that backs **remote-archive
    /// browsing**: the archive byte source calls this to feed `rc-zip`'s sans-IO
    /// reader a few ranges (the tail for the central directory, then each entry's
    /// compressed span) without downloading the whole `.zip`. Unlike
    /// [`open_read_stream_at_offset`](Self::open_read_stream_at_offset) — a
    /// stream-to-EOF — this returns exactly the requested window.
    ///
    /// Returns fewer than `len` bytes ONLY at end of file (a read wholly past the
    /// end yields an empty `Vec`); the backend fills the window from as few
    /// backend round-trips as it can, so a caller never has to loop for a network
    /// short read. `len` is caller-bounded (the archive source uses ≤ a tail-sized
    /// window), so buffering the range is safe — this is NOT a whole-file read.
    ///
    /// Default is `NotSupported`. Implemented by the backends that can back a
    /// remote archive: `LocalPosixVolume` (`pread`), `SmbVolume` (a positioned
    /// SMB READ), and `MtpVolume` (a `GetPartialObject64` window). A backend that
    /// can't do positioned reads leaves the default, and the archive layer treats
    /// its archives as unreadable rather than misbehaving.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn read_range<'a>(
        &'a self,
        path: &'a Path,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + 'a>> {
        let _ = (path, offset, len);
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Opens `path` once for many positioned reads ([`VolumeRandomReader`]).
    ///
    /// [`read_range`](Self::read_range)'s sibling for a caller that reads one file
    /// over and over: the file viewer holds one for a whole session over a file
    /// whose bytes it would otherwise pull through an OS mount or a temp copy.
    ///
    /// Default is `NotSupported`, and the caller falls back to what it did before.
    /// Implemented by `SmbVolume` (one smb2 `FileReader`). A local volume leaves
    /// the default: its paths open with `std::fs`, which is already a held handle.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn open_random_reader<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeRandomReader>, VolumeError>> + Send + 'a>> {
        let _ = path;
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Whether pausing a streaming read from this volume needs to release a
    /// scarce backend resource (rather than park the open stream in place).
    ///
    /// `false` for every backend now — including MTP, whose reads are bounded
    /// windows that hold the one-per-device PTP session only DURING a window, not
    /// between them, so a pause has nothing to release and just stops starting the
    /// next window. The predicate is kept as the trait extension point for a
    /// hypothetical future backend whose stream genuinely pins a resource across
    /// the whole read; the copy wrapper no longer reads it. `MtpVolume` restates
    /// the same `false` explicitly, because "the PTP session is held only DURING
    /// a window" is the non-obvious half of that answer and belongs next to the
    /// windowing code.
    fn pause_releases_read_stream(&self) -> bool {
        false
    }

    /// Whether a running transfer reading from this volume should AUTO-YIELD to
    /// foreground device work mid-copy (don't start the next read window while
    /// foreground work is pending; resume from the current offset once it drains),
    /// without the user pausing.
    ///
    /// `true` only for MTP. Its reads are bounded windows, so a foreground
    /// listing/nav already slips in between windows; this opt-in additionally
    /// keeps the copy from immediately re-grabbing the device lock and starving
    /// foreground — the copy's per-window checkpoint behaves like the index scan's
    /// `background_yield_point`, parking until foreground drains. No session is
    /// released; "yield" means "don't start the next window."
    ///
    /// `false` (default): the auto-yield arm in `CheckpointStream` is a no-op, so
    /// local FS, SMB, and in-memory transfers behave exactly as before.
    fn supports_foreground_yield(&self) -> bool {
        false
    }

    /// Whether a running transfer WRITING to this volume should stand aside for
    /// foreground work on it, in SHORT, HARD-CAPPED slices, between write chunks.
    ///
    /// Separate from [`supports_foreground_yield`](Self::supports_foreground_yield)
    /// (the SOURCE/read opt-in) ON PURPOSE, and must NOT be collapsed into it. A
    /// write holds an OPEN handle across the pause, so the destination yield MUST
    /// be bounded (a hard cap in `CheckpointStream`), never the unbounded park the
    /// read side uses. Only SMB opts in: its writes are discrete SMB2 WRITE chunks
    /// with NO oplock or lease requested (`crates/cmdr-smb/src/volume/write_stream.rs`), so a brief, capped
    /// park between chunks is safe. ❌ MTP must NEVER opt in: an MTP upload streams
    /// inside ONE `SendObject` PTP transaction, so pausing mid-write would PIN the
    /// device session, the opposite of the read side.
    ///
    /// When `true`, the destination auto-yield arm probes
    /// [`foreground_pending`](Self::foreground_pending) on THIS (destination)
    /// volume, so a backend opting in must also implement that probe.
    ///
    /// `false` (default): the destination auto-yield arm is a no-op.
    fn supports_foreground_yield_as_destination(&self) -> bool {
        false
    }

    /// Has this volume's connection been PROVEN dead, as opposed to merely slow
    /// to answer?
    ///
    /// This is one of the two gates on the transfer watchdog's aggressive action:
    /// ending the wait on a task that has stopped moving (`transfer_probe.rs`).
    /// ❌ Elapsed silence is NOT an answer to this question and must never be
    /// dressed up as one: a large write to a loaded spinning-disk NAS is
    /// legitimately slow, and killing it trades a rare wedge for frequent
    /// spurious failures, which is the worse bargain.
    ///
    /// **A `Dead` answer is evidence, NOT a licence to act.** Measured against a
    /// QNAP TS-464 (2026-08-02, smb2's live-hardware suite): under heavy write
    /// load an ECHO keepalive reported `2 answered, 1 unanswered` — a false
    /// `Dead` — while five consecutive idle runs reported `0 unanswered`. The
    /// verdict is least trustworthy exactly when a transfer is running, so the
    /// caller ANDs it with its own stillness window. ❌ Don't add a caller that
    /// acts on this answer alone.
    ///
    /// **`SmbVolume` answers from `smb2`'s own pollable reading**
    /// (`Connection::liveness`, 0.23.0+): `Dead` only when the keepalive is armed,
    /// a request is outstanding, and the server has put nothing at all on the wire
    /// past the liveness window, ECHO replies and the bytes of a slow response
    /// included; `Alive` when a byte landed inside that window; `None` otherwise.
    /// The mapping and why each arm is what it is:
    /// `crates/cmdr-smb/src/volume/liveness.rs`. Every other backend answers `None`, which is the honest answer for a
    /// client with no keepalive. ❌ Don't answer `Dead` from a missed probe, a
    /// slow response, or elapsed silence: only a reading that already tells slow
    /// from dead qualifies. Full reasoning:
    /// `crates/cmdr-operations/src/transfer/DETAILS.md` § "The watchdog ACTS".
    fn connection_liveness(&self) -> Option<ConnectionLiveness> {
        None
    }

    /// Payload bytes this volume's connection has received so far, counted as
    /// they land, the part of a response still arriving included. `None` for a
    /// backend with no connection to count, which is every backend but SMB.
    ///
    /// A RATE source, ❌ never progress: bytes inside a response that hasn't
    /// completed are unverified (a signature or AEAD tag covers the whole
    /// message), so crediting them to a byte bar would mean rolling it back when
    /// a frame fails. It's also CONNECTION-wide: every request on the connection
    /// shares one inbound stream, so with a listing and a copy in flight the
    /// count is theirs together.
    ///
    /// Only ever compared against an earlier reading of the same volume, and it
    /// can drop: a rebuilt session starts counting from zero. The transfer
    /// watchdog reads a drop as "no rate this tick"
    /// (`crates/cmdr-operations/src/transfer/transfer_probe.rs`).
    fn connection_bytes_received(&self) -> Option<u64> {
        None
    }

    /// Whether a foreground op is currently pending on this volume's device.
    ///
    /// Polled once per chunk by `CheckpointStream` (cheap — an atomic load behind
    /// the device's priority gate). `MtpVolume` delegates to the connection
    /// manager's per-device gate; every other backend uses the default `false`,
    /// so they never trigger an auto-yield. See [`supports_foreground_yield`](Self::supports_foreground_yield).
    fn foreground_pending<'a>(&'a self) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { false })
    }

    /// Park until this volume's device is clear of foreground work.
    ///
    /// Called by `CheckpointStream`'s auto-yield arm to hold off the next read
    /// window: it waits here so the foreground listing/nav owns the device, then
    /// the checkpoint lets the next window proceed from the current offset.
    /// `MtpVolume` delegates to the per-device gate's `background_yield_point`
    /// (returns the instant the last foreground guard drops); every other backend
    /// uses the default no-op. See [`supports_foreground_yield`](Self::supports_foreground_yield).
    fn wait_until_foreground_idle<'a>(&'a self) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }

    /// Whether a write of exactly `size` bytes to this volume lands in ONE
    /// all-or-nothing shot: either every byte arrives at the destination path or
    /// nothing does, with no window in which the path holds a byte-incomplete
    /// file — not even if the process is killed mid-transfer.
    ///
    /// The transfer layer stages every write on a `.cmdr-tmp-*` sibling exactly
    /// to keep a half-written file from wearing the user's real filename. A
    /// single-shot write can't produce one, so it skips the staging and the
    /// rename round trip that lands it.
    ///
    /// ❌ Answer `true` ONLY for writes this backend performs as one indivisible
    /// operation, and answer with the SAME condition
    /// [`write_from_stream`](Self::write_from_stream) branches on. Size is the
    /// shape the guarantee happens to take (an SMB compound frame carries at most
    /// `max_write_size` bytes), never the reason for it: a "small files are fine"
    /// answer silently brings back truncated files at real names.
    ///
    /// ❌ A `true` answer promises a SECOND property: no write handle is open on
    /// the far side while the source stream drains, so buffer the bytes first and
    /// commit in the one operation. Opening a handle up front and streaming chunks
    /// into it still lands all-or-nothing, and breaks this. The transfer layer's
    /// foreground yield leans on it to let a small upload stand aside mid-drain;
    /// `crates/cmdr-operations/src/transfer/volume/DETAILS.md` § "The single-shot exemption".
    ///
    /// ❌ And a THIRD: a single-shot write has no staged landing to refuse a name
    /// someone took mid-upload, so its [`WriteMode::CreateNew`] refusal must be
    /// atomic (SMB's `FileCreate`), never a check before the write.
    ///
    /// `false` (the default): every write to this volume stages.
    fn write_is_single_shot<'a>(&'a self, length: StreamLength) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        let _ = length;
        Box::pin(async { false })
    }

    /// Whether EVERY [`write_from_stream`](Self::write_from_stream) here is
    /// published whole by the protocol itself: the name shows nothing new until
    /// the write completes, a write that never completes leaves nothing at the
    /// name (not after a crash, a dropped connection, or a cancel), and a
    /// replacing write keeps the old content readable until the new is complete.
    /// An object store answers `true` (S3's PUT and multipart completion).
    ///
    /// The transfer layer then writes straight to the final name, a fresh file
    /// and a file→file Overwrite alike: staging there would buy nothing the
    /// protocol doesn't already give, and its landing rename can cost a full
    /// server-side copy plus a delete (`crates/cmdr-operations/src/transfer/volume/DETAILS.md`
    /// § "Whole-publish destinations").
    ///
    /// It promises less than [`write_is_single_shot`](Self::write_is_single_shot),
    /// and that's why it's a separate answer: a request stays open on the server
    /// while the source drains, and the [`WriteMode::CreateNew`] refusal may be
    /// a check just before the write where the server has no atomic primitive.
    /// So the destination-side foreground yield and the stall watchdog keep
    /// reading `write_is_single_shot` alone.
    ///
    /// ❌ Answer `true` only when it holds for every size and both modes, by
    /// protocol, ❌ never by a backend's own cleanup: a cleanup doesn't run on a
    /// force-quit. Default `false`.
    fn publishes_writes_whole(&self) -> bool {
        false
    }

    /// Writes data from a stream to the given path.
    ///
    /// `on_progress` receives structured byte progress after each chunk is
    /// written. Return `ControlFlow::Break(())` to cancel the transfer.
    ///
    /// # Arguments
    /// * `dest` - Destination path
    /// * `mode` - What to do with a file already at `dest` ([`WriteMode`])
    /// * `length` - Exact total size, or unknown for generated output
    /// * `stream` - Source data stream
    /// * `on_progress` - Progress callback; return `ControlFlow::Break(())` to cancel
    ///
    /// # `WriteMode::CreateNew` must not clobber
    ///
    /// Under [`WriteMode::CreateNew`] a file at `dest` is refused with
    /// `VolumeError::AlreadyExists` and left byte for byte, however late it
    /// arrived: the caller found the name free, and a file there now is another
    /// writer's. This matters most for a [`write_is_single_shot`](Self::write_is_single_shot)
    /// write, which goes straight to the final name with no staged landing to
    /// refuse for it. Enforced by `conformance::assert_write_from_stream_create_new_refuses_to_clobber`.
    ///
    /// A refusal must also leave the backend's own cleanup alone: nothing at
    /// `dest` is ours, so a "delete the partial" arm must not fire on it.
    ///
    /// # Streaming requirement
    ///
    /// **Must stream.** Don't drain `stream` into a `Vec<u8>` before writing
    /// to the backend. A user copying an 8 GB file through this path would
    /// allocate 8 GB of RAM. Pull each chunk from `stream.next_chunk().await`
    /// and push it straight into the backend's streaming writer (smb2:
    /// `FileWriter`, mtp-rs: `upload_stream`) in the same loop. Holding the
    /// backend's session mutex across the source `next_chunk` awaits is
    /// fine. Different volumes use different mutexes, so there's no
    /// deadlock risk.
    ///
    /// Peak memory per transfer should be bounded by a small chunk buffer
    /// (~1 MiB) regardless of file size.
    ///
    /// # Returns
    ///
    /// The bytes written and what of the source's metadata the destination
    /// refused ([`StreamWritten`]): build it with
    /// [`StreamWritten::with_report`] from the apply's report, or
    /// [`StreamWritten::nothing_refused`] when nothing applied here can be
    /// refused alone. The copy's loss note counts the refusals.
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        mode: WriteMode,
        length: StreamLength,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<StreamWritten, VolumeError>> + Send + 'a>> {
        let _ = (dest, mode, length, stream, on_progress);
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Copies one FILE from `from` to `to` inside THIS volume, without the bytes
    /// travelling through Cmdr.
    ///
    /// A pure optimization over `open_read_stream` + `write_from_stream`, for the
    /// backends whose server can copy for itself (SFTP's
    /// `copy-data@openssh.com`). Duplicating a large file inside one server
    /// otherwise sends it down the link and straight back up.
    ///
    /// Contract, all four parts load-bearing:
    ///
    /// - **Files only.** A directory source is the caller's to walk; this answers
    ///   for one file at a time so the walk keeps its conflict handling.
    /// - **`to` is created, or TRUNCATED if it exists**, exactly like
    ///   `write_from_stream`'s destination, so the caller's staging and
    ///   conflict-resolution temps work unchanged.
    /// - ❗ **Never single-shot.** The destination genuinely holds a
    ///   byte-incomplete file while this runs, so the caller must stage it the way
    ///   it stages a streamed write. ❌ A backend may not answer here in a way
    ///   that contradicts [`write_is_single_shot`](Self::write_is_single_shot).
    /// - **Cancellation arrives as `ControlFlow::Break` from `on_progress`**, and
    ///   the implementation removes its partial before returning
    ///   [`VolumeError::Cancelled`].
    ///
    /// Returns bytes copied. Default `NotSupported`, and ❗ a caller MUST treat
    /// that as "do it the ordinary way" rather than as a failure: every backend
    /// answers it today, and a server that simply lacks the extension answers it
    /// at runtime.
    ///
    /// ❗ A backend that implements this also answers
    /// [`supports_copy_within`](Self::supports_copy_within), or the engine never
    /// asks it.
    fn copy_within<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        let _ = (from, to, on_progress);
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Copies one FILE from `from` on `source` to `to` here, without the bytes
    /// travelling through Cmdr: the transfer engine's one server-side-copy
    /// entry point, asked before every streamed file.
    ///
    /// `source` may be THIS volume, or another view of the same storage the
    /// backend can copy across (two S3 places of one account). ❗ The backend
    /// decides that from `source`'s concrete type and identity, ❌ never from a
    /// path: a copy on a server the source path doesn't belong to copies the
    /// wrong file, silently.
    ///
    /// `to` is written the way [`write_from_stream`](Self::write_from_stream)
    /// writes it under `mode`; a whole-publishing backend
    /// ([`publishes_writes_whole`](Self::publishes_writes_whole)) publishes the
    /// copy whole too, so the caller writes its final name. Cancellation comes
    /// through `progress`, and the backend removes its partial before
    /// answering [`VolumeError::Cancelled`].
    ///
    /// `NotSupported` means "stream it instead", never a failure. Default:
    /// [`copy_within`](Self::copy_within) when
    /// [`may_copy_on_server_from`](Self::may_copy_on_server_from) holds and
    /// `mode` is `CreateOrReplace` (its contract truncates), else
    /// `NotSupported`.
    fn copy_on_server<'a>(
        &'a self,
        source: &'a dyn Volume,
        from: &'a Path,
        to: &'a Path,
        mode: WriteMode,
        progress: &'a dyn ServerCopyProgress,
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if mode.refuses_occupied() || !self.may_copy_on_server_from(source) {
                return Err(VolumeError::NotSupported);
            }
            self.copy_within(from, to, &|done, total| progress.advanced(done, total))
                .await
        })
    }

    /// Whether [`copy_within`](Self::copy_within) is implemented here at all.
    /// Default `false`.
    ///
    /// A claim about the BACKEND: a server without the extension still answers
    /// `true` and refuses at runtime, which costs one wasted question per file.
    fn supports_copy_within(&self) -> bool {
        false
    }

    /// Whether [`copy_on_server`](Self::copy_on_server) from `source` CAN
    /// succeed, asked before the engine stages anything for it. `false` means it
    /// would certainly answer `NotSupported`; `true` means "ask", and it may
    /// still say no (a server lacking the extension, a provider that copies
    /// within one bucket only).
    ///
    /// ❗ No request and no I/O: it runs on an async worker, on the per-file hot
    /// path of every cross-volume copy. And ❌ a false `false` silently loses the fast path,
    /// so a backend that overrides `copy_on_server` overrides this too, from the
    /// same concrete-type-and-identity test, ❌ never a path.
    ///
    /// Default: `source` is this very volume instance and it
    /// [`supports_copy_within`](Self::supports_copy_within).
    fn may_copy_on_server_from(&self, source: &dyn Volume) -> bool {
        self.supports_copy_within() && std::ptr::addr_eq(self as *const Self, source as *const dyn Volume)
    }
}

/// Anchors a caller-supplied path at `root`, giving the absolute, root-anchored
/// form every backend accepts.
///
/// Cmdr's UI speaks two path dialects: a pane sends the absolute path it
/// displays (`/Volumes/naspi/photos`), while the transfer dialog's destination
/// box is volume-relative (`/photos`, because the volume is a separate
/// dropdown). Both are legitimate, and the difference is invisible to the
/// receiving backend: a leading `/` says nothing about which dialect this is.
///
/// So the CALLER anchors, once, at the point where it still knows which volume
/// the path belongs to, and the backends stay strict about what they accept.
/// `SmbVolume::to_smb_path` is the one that made the difference load-bearing: it
/// answers `NotFound` for an absolute path outside its mount rather than
/// guessing (a guess used to address a real file at the wrong place), so an
/// unanchored `/photos` failed a move into an SMB subfolder before any I/O.
///
/// The rules, in order:
///
/// - Every spelling of "the volume root" (empty, `.`, `/`) is `root` itself.
/// - A path already under `root` is returned untouched, matched by whole
///   COMPONENTS so the sibling mount `/Volumes/naspi-1` can't pass as being
///   under `/Volumes/naspi`.
/// - Anything else is volume-relative: its leading `/` (if any) goes, and the
///   rest hangs off `root`.
///
/// Idempotent by construction, which is what lets a call site anchor without
/// first asking which dialect it holds. A scheme-shaped root
/// (`mtp://device/storage`, which `Path::is_absolute` calls relative) anchors
/// the same way, because the rules never ask that question.
pub fn root_anchored(root: &Path, path: &Path) -> PathBuf {
    if path.as_os_str().is_empty() || path == Path::new(".") || path == Path::new("/") {
        return root.to_path_buf();
    }
    if path.starts_with(root) {
        return path.to_path_buf();
    }
    root.join(path.strip_prefix("/").unwrap_or(path))
}

// Shared data types (`VolumeError`, `SpaceInfo`, `CopyScanResult`, `MutationEvent`,
// …) live in `types`; the volume ID funnel (`local_volume_id`, `smb_volume_id`,
// …) lives in `ids`. Both are re-exported below so callers import
// `volume::VolumeError`, `volume::smb_volume_id`, etc.
pub mod app_paths;
mod capabilities;
mod channel_stream;
mod child_name;
mod connection;
mod connection_cell;
mod connection_reporter;
mod entry_kind;
mod error;
pub mod http_session;
mod ids;
mod in_memory;
pub mod listing_progress;
pub mod liveness;
pub mod mkdir_all;
pub mod mtp_ids;
pub mod patching;
mod read_stream;
pub mod remote_paths;
mod retirement;
mod root_echo;
mod routed;
mod scan_boundary;
mod scan_recorder;
pub mod scan_stop;
pub mod scan_walk;
pub mod secret_store;
pub mod server_side;
pub mod share_link;
pub mod tls;
mod transport_fault;
mod types;
mod usb_speed;

// Docs live in the file's own `//!` header. ❌ Never add an outer `///` here on
// top of it: rustdoc resolves the concatenated fragments in THIS module's scope,
// so the file's own inner-doc links to its items stop resolving.
pub mod canonical_root;
pub mod git_error;
pub mod published_locations;
pub mod smb_mount_source;

/// Typed, word-free classification of why a volume operation failed.
pub mod friendly_error;

/// The safety promises every `Volume` implementation is asserted against, so a
/// backend can't quietly opt out of one. `any(test, feature = "testing")`, not
/// `cfg(test)`: the backends that need it most live in other crates.
#[cfg(any(test, feature = "testing"))]
pub mod conformance;

// Everything a backend needs from the application around it, as named seams.
pub mod host;

pub use capabilities::VolumeCapabilities;
pub use channel_stream::ChannelReadStream;
pub use child_name::{ChildName, NotAChildName};
pub use connection::{BackendKind, ConnectionState, DeviceReadiness, DeviceUnavailableReason, MountClass, SignInShape};
pub use connection_cell::{CellState, ConnectionCell};
pub use connection_reporter::{ConnectionReporter, ReportedState};
pub use entry_kind::EntryKind;
pub use error::{ErrnoField, VolumeError};
pub use ids::*;
pub use server_side::{RenameWork, ServerCopyProgress, SubtreeTally};
pub use share_link::{ShareLink, ShareLinkExpiry};
// The app-path schemes live beside the translation they feed; re-exported here
// so callers keep `volume::sftp_app_root` and friends.
pub use in_memory::InMemoryVolume;
pub use mkdir_all::{LeadsTo, MadeDirectories, MakesDirectories};
pub use patching::{PatchSource, patch_created, patch_deleted, patch_mutation, patch_renamed};
pub use read_stream::{SequentialExtract, VolumeRandomReader, VolumeReadStream};
pub use remote_paths::{
    ServerPath, adb_app_root, adb_serial_of_path, s3_app_root, server_of_path, sftp_app_root, webdav_app_root,
};
pub use retirement::{Retirement, Retires, SelfHandle};
pub use root_echo::{RootEcho, root_echo};
pub use routed::{ResolvedVolume, RoutedKind};
pub use scan_boundary::{ScanBoundary, stopped as scan_stopped};
pub use scan_recorder::ScanRecorder;
pub use scan_stop::{ScanStop, ScanStopSignal};
pub use scan_walk::{ScanSource, Walking, conflicts_against, fold_batch, scan_conflicts, scan_one, scan_trees};
pub use transport_fault::{TransportFault, is_transport_errno};
pub use types::*;
pub use usb_speed::UsbSpeed;

#[cfg(test)]
mod capabilities_test;
#[cfg(test)]
mod connection_cell_test;
#[cfg(test)]
mod connection_reporter_test;
#[cfg(test)]
mod entry_kind_test;
#[cfg(test)]
mod in_memory_scan_test;
#[cfg(test)]
mod in_memory_stream_test;
#[cfg(test)]
mod in_memory_test;
#[cfg(test)]
mod listing_progress_test;
#[cfg(test)]
mod retirement_test;
#[cfg(test)]
mod root_anchored_path_test;
#[cfg(test)]
mod root_echo_test;
#[cfg(test)]
mod transport_fault_test;
