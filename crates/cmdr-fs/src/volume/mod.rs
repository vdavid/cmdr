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
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Default volume ID for the root filesystem.
pub const DEFAULT_VOLUME_ID: &str = "root";

/// A stream of bytes read from a volume.
///
/// This is an async interface for reading file data in chunks. Used for
/// streaming transfers between volumes. `next_chunk` is async (returns a
/// pinned boxed future) so that network-backed volumes (MTP, SMB) can
/// yield to the runtime instead of blocking. `total_size` and `bytes_read`
/// stay sync because they return cached values.
pub trait VolumeReadStream: Send {
    /// Returns the next chunk of data, or None if complete.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>>;

    /// Total size of the file in bytes.
    fn total_size(&self) -> u64;

    /// Bytes read so far (for progress tracking).
    fn bytes_read(&self) -> u64;

    /// Promptly release any scarce backend resource this stream holds across
    /// chunks, before the stream is dropped. After this call the stream is spent;
    /// `next_chunk` must not be called again on it.
    ///
    /// Default is a no-op, and that's what every current backend uses: reads that
    /// could otherwise pin a scarce resource (MTP's one-per-device PTP session)
    /// are bounded windows that hold nothing between chunks, so the copy wrapper
    /// (`CheckpointStream`) parks in place rather than releasing anything. This
    /// stays a trait hook for a hypothetical future backend whose stream genuinely
    /// holds a resource across chunks; nothing in the copy path calls it today.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn cancel_and_release(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }
}

/// A ONE-PASS extractor over a subtree of a SEQUENTIAL source (a compressed tar
/// or solid 7z), where a per-entry random read re-decodes the prefix and makes a
/// subtree extract O(n²). Decoding the stream a single time, it yields each file
/// in ARCHIVE order: [`next_file`](Self::next_file) advances to the next member,
/// then [`current_stream`](Self::current_stream) hands its bytes to the
/// destination's `write_from_stream`.
///
/// The copy engine drives it after creating the destination directory structure
/// from the tree (cheap, no decode), so this never yields directories. Dropping
/// the extractor stops the underlying decoder (drop-based cancellation).
pub trait SequentialExtract: Send {
    /// Advances to the next file member (draining any unread bytes of the current
    /// one), or `Ok(None)` at the end of the subtree.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future by design"
    )]
    fn next_file(&mut self) -> Pin<Box<dyn Future<Output = Result<Option<ExtractedFile>, VolumeError>> + Send + '_>>;

    /// An owned read stream over the CURRENT member's decoded bytes, to hand to
    /// the destination's `write_from_stream`. Valid until the next
    /// [`next_file`](Self::next_file); call exactly once per member.
    fn current_stream(&self) -> Box<dyn VolumeReadStream>;
}

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
    /// USB or network roundtrips inside one listing (currently MTP — a 950-entry
    /// folder is 950 `GetObjectInfo` calls). Once the token is cancelled,
    /// the backend bails between roundtrips with `VolumeError::Cancelled`
    /// instead of running to completion.
    ///
    /// Local and in-memory backends ignore the token (their listings are
    /// effectively atomic from the caller's perspective). SMB ignores it
    /// today — adding SMB cancel propagation is a follow-up.
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
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>>;

    /// Checks if a path is a directory.
    /// Returns Ok(true) if directory, Ok(false) if file, Err if path doesn't exist.
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>>;

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
    /// that already `exists()` (or at the volume root), then creates the missing
    /// ones shallowest-first via `create_directory`. Probing existence per
    /// ancestor before creating means it never calls `create_directory` on a dir
    /// that's already there, so backends whose `create_directory` can't signal a
    /// collision (`MtpVolume`, `create_directory_errors_on_existing_dir() ==
    /// false`) never make a duplicate sibling. An `AlreadyExists` from
    /// `create_directory` (a concurrent op won a race) is also treated as
    /// success. These are network/IPC round-trips, so the leaf-first walk keeps
    /// them minimal: when the parent already exists (the common "new folder name
    /// under an existing dir" case), it's one `exists()` plus one
    /// `create_directory`.
    ///
    /// Backends override only if they have a cheaper native recursive mkdir;
    /// SMB and MTP don't, so the per-component loop is the right shape there.
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
            let mut missing: Vec<PathBuf> = Vec::new();
            for ancestor in path.ancestors() {
                if ancestor.file_name().is_none() {
                    break;
                }
                if self.exists(ancestor).await {
                    break;
                }
                missing.push(ancestor.to_path_buf());
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
                    // A concurrent op created it between our `exists()` check and
                    // this call. Treat as success to keep the create idempotent —
                    // but NOT as ours: somebody else's directory may already have
                    // something in it.
                    Err(VolumeError::AlreadyExists(_)) => {}
                    Err(e) => return Err(e),
                }
            }
            Ok(leaf)
        })
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
    /// `apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume/cleanup.rs`.
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

    /// Renames/moves a file or directory within this volume.
    ///
    /// Both source and destination paths are relative to the volume root.
    /// When `force` is false, returns `AlreadyExists` if the destination exists.
    /// When `force` is true, proceeds even if the destination exists (POSIX rename
    /// silently overwrites).
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
    /// cache (`file_system::listing::mutation`), while `SmbVolume` and `MtpVolume`
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

    /// Returns the SMB connection state if this is an SMB volume.
    ///
    /// Only `SmbVolume` returns `Some`. Used by the frontend to show a connection
    /// quality indicator (green = direct smb2, yellow = OS mount fallback).
    fn smb_connection_state(&self) -> Option<SmbConnectionState> {
        None
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
    /// Default returns `Err(NotSupported)`. Only `SmbVolume` overrides today;
    /// it's invoked by the FE reconnect manager on each backoff tick and on the
    /// "Retry now" button. Future network/cloud volumes should override this
    /// when they have a story for in-place reconnect.
    fn attempt_reconnect<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Reconnect using freshly-entered credentials, replacing whatever was cached.
    ///
    /// Invoked by the "Sign in" affordance the frontend shows when an in-place reconnect
    /// gave up on an auth failure (a password changed on the server). The implementation
    /// persists the new credentials so the next reconnect is silent, then runs the normal
    /// reconnect. Default `Err(NotSupported)`; only `SmbVolume` overrides today.
    fn reconnect_with_credentials<'a>(
        &'a self,
        _username: String,
        _password: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    // ========================================
    // Watching: Optional, default no-op
    // ========================================

    /// Whether a `notify`-based OS watch can be established on this volume's
    /// paths, which is the one thing this gates: `listing::operations` and
    /// `listing::streaming` call `start_watching` only when it's true.
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
    /// `file_system::listing::caching::try_get_authoritative_listing` to decide
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
    /// `try_get_authoritative_listing` for the per-backend windows callers must
    /// tolerate.
    ///
    /// Default `None`: a new backend claims coverage explicitly or gets none.
    fn listing_watch_coverage(&self, _path: &Path) -> WatchCoverage {
        WatchCoverage::None
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
    /// (`write_operations/transfer/volume/copy.rs::transfer_concurrency`). A
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
    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        let _ = path;
        Box::pin(async { Err(VolumeError::NotSupported) })
    }

    /// Scans multiple paths to get aggregate + per-path copy statistics.
    ///
    /// The default iterates over `scan_for_copy` per path, which is correct for
    /// volumes where per-path I/O is cheap (local FS, in-memory). Volume types
    /// with expensive per-path I/O (MTP, SMB, FTP, S3) should override this to
    /// batch, typically by pipelining per-path stats over a shared session
    /// (SMB) or grouping paths by parent directory and listing each parent
    /// once (MTP).
    ///
    /// The returned `BatchScanResult` carries both the rolled-up `aggregate`
    /// (what the scan-preview / pre-flight checks want) and a `per_path` vec
    /// (what the copy engine uses to seed its per-source hints, so it doesn't
    /// have to re-probe each source's type and size with a separate stat).
    fn scan_for_copy_batch<'a>(
        &'a self,
        paths: &'a [PathBuf],
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        self.scan_for_copy_batch_with_progress(paths, None)
    }

    /// Same as `scan_for_copy_batch`, but emits running progress as the scan
    /// walks. `on_progress(files_found)` is called repeatedly as entries are
    /// discovered, letting the scan-preview dialog show a climbing count
    /// instead of a frozen "0 files" spinner during a slow enumeration (the
    /// MTP listing of /DCIM/Camera with 1k+ entries takes ~17 s of USB
    /// round-trips, and there's nothing for the user to look at during it).
    ///
    /// The default implementation ignores `on_progress` and delegates to the
    /// existing `scan_for_copy_batch`. Volumes with expensive per-path I/O
    /// (currently MTP) override this to thread the callback through to their
    /// underlying streaming listing primitive (`list_directory_with_progress`).
    ///
    /// The callback receives a `ListingProgress` carrying running files / dirs
    /// / bytes. Backends accumulate from the entries they've enumerated and
    /// report the cumulative totals for the current scan call. The FE renders
    /// all three counters climbing live during the scan dialog.
    #[allow(unused_variables, reason = "Default impl intentionally ignores `on_progress`")]
    fn scan_for_copy_batch_with_progress<'a>(
        &'a self,
        paths: &'a [PathBuf],
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
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
            };
            let mut per_path = Vec::with_capacity(paths.len());
            for path in paths {
                let scan = self.scan_for_copy(path).await?;
                aggregate.file_count += scan.file_count;
                aggregate.dir_count += scan.dir_count;
                aggregate.total_bytes += scan.total_bytes;
                aggregate.dedup_bytes += scan.dedup_bytes;
                per_path.push((path.clone(), scan));
                if let Some(cb) = on_progress {
                    cb(ListingProgress {
                        files: aggregate.file_count,
                        dirs: aggregate.dir_count,
                        bytes: aggregate.total_bytes,
                    });
                }
            }
            Ok(BatchScanResult { aggregate, per_path })
        })
    }

    /// Checks destination for conflicts with source items.
    /// Returns list of files that already exist at destination.
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

    /// Recommended poll interval for live disk-space monitoring.
    ///
    /// Local volumes use a short interval (2 s) because `statvfs`/NSURL is
    /// microsecond-cheap. Network and MTP volumes use a longer interval (5 s)
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

    /// Returns true if this volume supports streaming read/write operations.
    fn supports_streaming(&self) -> bool {
        false
    }

    /// Whether `create_directory` reliably returns `VolumeError::AlreadyExists`
    /// when a directory of the same name already exists at the path.
    ///
    /// The scan-as-you-merge folder-merge walker
    /// (`write_operations/transfer/volume/strategy.rs`) uses the `AlreadyExists`
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
    /// with NO oplock or lease requested (`crates/cmdr-smb/src/volume/streams.rs`), so a brief, capped
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
    /// **Every backend answers `None` today, and that is the honest answer** —
    /// including `SmbVolume` on `smb2` 0.16.0, which HAS the keepalive. The
    /// keepalive deliberately declares no deaths (a busy NAS drops probes, so
    /// `keepalive_failures` counts non-events), and the crate's one sound verdict,
    /// `Error::ServerUnresponsive`, is handed to a caller and tears the connection
    /// down — so by the time it is observable every waiter has already been
    /// failed, which the transfer's per-file retry handles without this. ❌ Don't
    /// answer `Dead` from a missed probe, a slow response, or elapsed silence.
    ///
    /// **To turn the watchdog's teeth on**, `smb2` has to expose that verdict as
    /// pollable state — "the keepalive is armed AND the wire has been silent past
    /// the liveness window with a request outstanding" — readable BEFORE a request
    /// burns its deadline and without the connection being torn down. Then
    /// override this on `SmbVolume` alone. Nothing else moves; the mechanism, its
    /// stillness window, and its tests are already in place and gated only on this
    /// answer. Full reasoning:
    /// `write_operations/transfer/DETAILS.md` § "The watchdog ACTS".
    fn connection_liveness(&self) -> Option<ConnectionLiveness> {
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
    /// `false` (the default): every write to this volume stages.
    fn write_is_single_shot<'a>(&'a self, size: u64) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        let _ = size;
        Box::pin(async { false })
    }

    /// Writes data from a stream to the given path.
    ///
    /// `on_progress(bytes_written, total_size)` is called after each chunk is
    /// written. Return `ControlFlow::Break(())` to cancel the transfer.
    ///
    /// # Arguments
    /// * `dest` - Destination path (file will be created/overwritten)
    /// * `size` - Total size in bytes (required for protocols like MTP)
    /// * `stream` - Source data stream
    /// * `on_progress` - Progress callback; return `ControlFlow::Break(())` to cancel
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
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        let _ = (dest, size, stream, on_progress);
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
    fn copy_within<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        let _ = (from, to, on_progress);
        Box::pin(async { Err(VolumeError::NotSupported) })
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
mod capabilities;
mod channel_stream;
mod ids;
mod in_memory;
pub mod mtp_ids;
mod retirement;
mod scan_ticker;
mod types;

// Docs live in the file's own `//!` header. ❌ Never add an outer `///` here on
// top of it: rustdoc resolves the concatenated fragments in THIS module's scope,
// so the file's own inner-doc links to its items stop resolving.
pub mod canonical_root;

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
pub use ids::*;
pub use in_memory::InMemoryVolume;
pub use retirement::{Retirement, Retires, SelfHandle};
pub use scan_ticker::ScanTicker;
pub use types::*;

#[cfg(test)]
mod capabilities_test;
#[cfg(test)]
mod in_memory_scan_test;
#[cfg(test)]
mod in_memory_stream_test;
#[cfg(test)]
mod in_memory_test;
#[cfg(test)]
mod retirement_test;
#[cfg(test)]
mod root_anchored_path_test;
