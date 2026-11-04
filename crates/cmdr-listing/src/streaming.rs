//! Streaming directory listing: types, state, and async implementation.
//!
//! Provides non-blocking directory reading with progress events and cancellation.
//! The read runs on the host's runtime and reports through the `ListingEvents`
//! seam (`host/`); the host turns those facts into what the pane sees.

use std::path::Path;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::log_private::LogPrivate;

use crate::Inner;
use crate::cached_listing::CachedListing;
use crate::foreign_path::Listed;
use crate::host::{ForegroundHold, ForegroundLeases, ListingEnd, ListingEvents, OpenedListing, Unregistered};
use crate::listing_id::ListingId;
use crate::pipeline::ReadBy;
use crate::sorting::{DirectorySortMode, SortColumn, SortOrder};
use crate::stall::{ListingRead, ReadOutcome, StallWatch, read_until_answered};
use cmdr_fs::benchmark;
use cmdr_fs::volume::{ListingProgress, VolumeError};

// ============================================================================
// Types and state
// ============================================================================

/// State for an in-progress streaming listing
#[cfg_attr(
    not(any(test, feature = "testing")),
    allow(
        unreachable_pub,
        reason = "public for the `testing` fixtures, which hand it to a host's tests"
    )
)]
pub struct StreamingListingState {
    /// The listing's stop signal. Read at the sync cancellation points (before
    /// read, after read, at cache insert), awaited by the `select!` that races
    /// the read, AND handed to the backend, so a listing made of many round
    /// trips (MTP) stops on the wire and not just in the loop above it. One
    /// token rather than a flag plus a `Notify`: they could never disagree.
    pub cancel: CancellationToken,
    /// When the read counts as stalled, and the per-volume bound on hung reads.
    pub(crate) stall: StallWatch,
}

impl StreamingListingState {
    pub(crate) fn with_stall_watch(stall: StallWatch) -> Self {
        Self {
            cancel: CancellationToken::new(),
            stall,
        }
    }
}

/// One streaming read, from its first statement to its last: the foreground
/// hold, and the cancel token and stall watch the read answers to.
///
/// The hold is a private field, so it lives exactly as long as the read: the
/// read's body takes the session by value and reaches its cancel token and
/// stall watch only through it, so there's no hold to bind to `_` and drop
/// early. Released by drop on every exit (error, cancel, panic, or the task
/// being dropped at shutdown), so no path can leak it and pin a share busy.
#[must_use = "a session that isn't kept releases its foreground hold at once"]
pub(crate) struct ReadSession {
    /// The user is waiting on this volume until the read ends (`host/leases.rs`).
    _hold: ForegroundHold,
    /// The read's cancel token and stall watch, shared with the cancel map.
    state: Arc<StreamingListingState>,
}

impl ReadSession {
    /// Takes `volume_id`'s foreground hold for a read answering to `state`.
    fn begin(leases: &dyn ForegroundLeases, volume_id: &str, state: &Arc<StreamingListingState>) -> Self {
        Self {
            _hold: leases.hold(volume_id),
            state: Arc::clone(state),
        }
    }

    /// The read's stop signal (see [`StreamingListingState::cancel`]).
    fn cancel(&self) -> &CancellationToken {
        &self.state.cancel
    }

    /// When the read counts as stalled.
    fn stall(&self) -> &StallWatch {
        &self.state.stall
    }
}

// ============================================================================
// Streaming implementation
// ============================================================================

/// What a pane asked to list ([`Listings::start`](super::Listings::start)).
#[derive(Debug, Clone)]
pub struct ListingRequest {
    /// The listing's id, minted by the pane per navigation.
    pub id: ListingId,
    /// The volume to read through.
    pub volume_id: String,
    /// The directory, as the pane spelled it.
    pub path: std::path::PathBuf,
    /// Whether the pane shows hidden files.
    pub include_hidden: bool,
    /// The pane's sort column.
    pub sort_by: SortColumn,
    /// The pane's sort direction.
    pub sort_order: SortOrder,
    /// Where folders sort.
    pub dir_sort_mode: DirectorySortMode,
}

impl Inner {
    /// Registers the listing's cancel token, then spawns its read onto the host's
    /// runtime, reporting to the host's events. The token is registered BEFORE
    /// this returns, so a cancel that follows at once still finds the read.
    pub(crate) fn start_streaming_listing(self: &Arc<Self>, request: ListingRequest) {
        // Create streaming state with cancellation flag
        let state = Arc::new(StreamingListingState::with_stall_watch(self.stall_watch()));

        // Store state for cancellation
        self.streaming
            .write_ignore_poison()
            .insert(request.id.as_str().to_string(), Arc::clone(&state));

        // A strong handle: the read ends.
        let engine = Arc::clone(self);
        self.runtime.spawn(async move {
            let result = engine.read_directory_with_progress(&state, &request).await;

            // Clean up streaming state
            engine.streaming.write_ignore_poison().remove(request.id.as_str());

            report_failure(
                engine.events.as_ref(),
                request.id.as_str(),
                &request.volume_id,
                &request.path,
                result,
            );
        });
    }
}

/// Reports a read that `read_directory_with_progress` returned as an error:
/// the one `finished` it doesn't report itself, since the read's own task removes
/// its cancel token first. An `Ok` was already reported, as complete or cancelled.
pub(crate) fn report_failure(
    events: &dyn ListingEvents,
    listing_id: &str,
    volume_id: &str,
    path: &Path,
    result: Result<(), VolumeError>,
) {
    if let Err(error) = result {
        events.finished(
            &ListingId::new(listing_id),
            ListingEnd::Failed {
                volume_id: volume_id.to_string(),
                path: path.to_path_buf(),
                error,
            },
        );
    }
}

impl Inner {
    /// Reads a directory with progress reporting, answering to `state`'s cancel
    /// token and stall watch.
    ///
    /// Async implementation that spawns the Volume I/O in a background task
    /// and uses `tokio::select!` with a cancellation polling loop for responsive ESC handling.
    /// Uses the Volume abstraction to support both local filesystem and MTP devices.
    pub(crate) async fn read_directory_with_progress(
        self: &Arc<Self>,
        state: &Arc<StreamingListingState>,
        request: &ListingRequest,
    ) -> Result<(), VolumeError> {
        // Hold the volume busy for the REAL duration of this listing. The command
        // entry point's timestamp seeds the debounce but decays in half a second,
        // while an SMB listing of a big folder can take ten; without the lease a
        // background upload or index scan resumes and competes for the rest of the
        // user's wait. Cancel releases it deliberately: the pane has moved on, so the
        // user is no longer waiting on this listing even while the backend unwinds
        // behind it. The host owns the signal (the leases seam).
        let session = ReadSession::begin(self.leases.as_ref(), &request.volume_id, state);
        self.read_in_session(session, request).await
    }

    /// The read itself. It owns `session`, so the foreground hold lives exactly
    /// as long as this body, and the cancel token is reachable only through it.
    async fn read_in_session(
        self: &Arc<Self>,
        session: ReadSession,
        request: &ListingRequest,
    ) -> Result<(), VolumeError> {
        let ListingRequest {
            id: listing_id,
            volume_id,
            path,
            include_hidden,
            sort_by,
            sort_order,
            dir_sort_mode,
        } = request;
        let (listing_id, volume_id, path) = (listing_id.as_str(), volume_id.as_str(), path.as_path());
        let (include_hidden, sort_by, sort_order, dir_sort_mode) =
            (*include_hidden, *sort_by, *sort_order, *dir_sort_mode);

        benchmark::log_event("read_directory_with_progress START");
        log::debug!(
            "read_directory_with_progress: listing_id={}, volume_id={}, path={}",
            listing_id,
            volume_id,
            LogPrivate::dir_path(path)
        );

        let events = &self.events;
        let id = ListingId::new(listing_id);
        let cancelled = || ListingEnd::Cancelled {
            volume_id: volume_id.to_string(),
            path: path.to_path_buf(),
        };

        // Emit opening event - this is the slow part for network folders
        // (SMB connection establishment, directory handle creation, MTP queries)
        events.opening(&id);

        // Check cancellation before starting
        if session.cancel().is_cancelled() {
            benchmark::log_event("read_directory_with_progress CANCELLED (before read)");
            events.finished(&id, cancelled());
            return Ok(());
        }

        // Resolve the volume, routing a `.zip`-crossing path to its read-only
        // `ArchiveVolume`. The cache still keys on the FE-provided `volume_id` (the
        // parent drive), so the downstream re-read sites re-resolve the same archive
        // from `(volume_id, path)` — eviction-safe (see `refresh.rs`).
        let resolved = self.volumes.resolve(volume_id, path).await;
        let is_routed = resolved.is_routed();
        let volume = match resolved.volume {
            Some(volume) => volume,
            // Nothing serves this ID. Before calling it gone, ask the host once
            // whether the mount under `path` IS this volume and nobody had
            // registered it, and if not, why it's missing: a listed phone or saved
            // server nobody connected is `NotConnected`, any other id `NotFound`,
            // as an unmount race is.
            None => match self.volumes.unregistered(volume_id, path).await {
                Unregistered::Adopted(adopted) => adopted,
                Unregistered::NotConnected => return Err(VolumeError::NotConnected(path.display().to_string())),
                Unregistered::Gone => return Err(VolumeError::NotFound(path.display().to_string())),
            },
        };
        // Read directory entries via Volume abstraction. Each read runs in its own
        // tokio task, raced against the cancel, so the pane stays responsive even
        // when filesystem I/O blocks (slow or stuck network drives). A read that goes
        // quiet reports the listing stalled and keeps it alive until the volume
        // answers again (`stall.rs`).
        let total_start = crate::rt::Instant::now();
        let read_start = crate::rt::Instant::now();
        let on_stalled = |stalled_on| events.stalled(&id, stalled_on);
        let on_progress: Arc<dyn Fn(ListingProgress) + Send + Sync> = {
            let events = Arc::clone(events);
            let id = id.clone();
            Arc::new(move |progress| events.progress(&id, progress))
        };
        let outcome = read_until_answered(
            ListingRead {
                runtime: &self.runtime,
                mounts: &self.mounts,
                on_stalled: &on_stalled,
                on_progress,
                listing_id,
                volume_id,
                volume: &volume,
                path,
                cancel: session.cancel(),
            },
            session.stall(),
        )
        .await;
        let listed = match outcome {
            ReadOutcome::Listed(listed) => listed,
            ReadOutcome::Refused(error) => return Err(error),
            ReadOutcome::Cancelled => {
                benchmark::log_event("read_directory_with_progress CANCELLED (during read_dir)");
                // The reads were detached, never aborted: an aborted future on MTP
                // abandons an in-flight PTP transaction and wedges the phone
                // (`mtp/connection/CLAUDE.md`). The backend sees the token and unwinds
                // at its own safe boundary while the user gets a prompt cancel.
                events.finished(&id, cancelled());
                return Ok(());
            }
        };
        // From here on the listing is the directory as its volume spells it: the cache
        // key, the watch, the overlays, and every log line use the stored spelling.
        let stored_path = listed.stored_spelling_of(path);
        let asked_path = path;
        let Listed {
            path: listed_path,
            entries,
        } = listed;
        let path = listed_path.as_path();
        let read_dir_time = read_start.elapsed();
        benchmark::log_event_value("read_dir COMPLETE, entries", entries.len());

        // Emit read-complete event (before sorting/caching) so UI can show "All N files loaded"
        events.read_complete(&id, entries.len());

        // Check cancellation one more time before finalizing
        if session.cancel().is_cancelled() {
            benchmark::log_event("read_directory_with_progress CANCELLED (after read)");
            events.finished(&id, cancelled());
            return Ok(());
        }

        // Enrich directory entries with index data (recursive_size etc.) before sorting,
        // so that sort-by-size works correctly for directories, and fold in the rows a
        // PANE sees that the volume doesn't hold (`pipeline.rs`). A routed volume has
        // no drive index (an archive's inner paths and a git snapshot's paths aren't
        // real FS paths), so enrich and verify are skipped.
        let enrich_start = crate::rt::Instant::now();
        let read_by = ReadBy::Volume {
            volume: &volume,
            routed: is_routed,
        };
        let entries = self.prepare(read_by, volume_id, path, entries).await;
        if !is_routed {
            self.index.verify_directory(volume_id, path);
        }
        let enrich_ms = enrich_start.elapsed().as_millis();

        // Build the listing (it asks the volume seam for its spelling, so before
        // the cache lock), then sort it: the listing resolves the requested column
        // against the dates its volume records, so a date this volume doesn't
        // record sorts by Modified.
        let entries_count_for_log = entries.len();
        let mut listing = CachedListing::new(
            self,
            volume_id.to_string(),
            path.to_path_buf(),
            entries,
            include_hidden,
            sort_by,
            sort_order,
            dir_sort_mode,
        );
        benchmark::log_event("sort START");
        let sort_start = crate::rt::Instant::now();
        listing.sort_rows(&self.collator());
        let sort_ms = sort_start.elapsed().as_millis();
        benchmark::log_event("sort END");
        let effective_sort_by = listing.effective().column();

        // Cache the completed listing, with atomic cancellation check.
        // We check cancellation WHILE holding the cache lock to avoid a race condition:
        // without this, a cancel arriving between a check and insert would leave a stale
        // entry (listDirectoryEnd would try to remove before the entry exists, then this
        // insert would add it permanently).
        let cache_write_start = crate::rt::Instant::now();
        let cached_path = listing.path.as_path().to_path_buf();
        // The row count comes off the listing's own map, so the number the frontend
        // sizes its scroller with is produced by the same filter every later fetch
        // goes through — including the scratch-file half a plain dotfile count misses.
        let total_count = listing.rows(include_hidden).len();
        {
            let mut cache = self.cache.write_ignore_poison();
            // Check cancellation while holding the lock - makes check+insert atomic
            if session.cancel().is_cancelled() {
                // Out of the lock before reporting: no seam runs under it.
                drop(cache);
                benchmark::log_event("read_directory_with_progress CANCELLED (at cache insert)");
                events.finished(&id, cancelled());
                return Ok(());
            }

            cache.insert(listing_id.to_string(), listing);
        }
        let cache_write_ms = cache_write_start.elapsed().as_millis();
        if !is_routed {
            self.respell_if_read_by_a_replaced_backend(volume_id, listing_id, path, &volume);
        }

        // Whatever a subsystem keeps alive while a pane shows this directory: in
        // Cmdr the change watch first, then the git portal's per-repo watcher.
        // `host/observers.rs`.
        let watcher_start_t = crate::rt::Instant::now();
        self.listing_opened(&OpenedListing {
            id: &id,
            volume_id,
            volume: volume.as_ref(),
            path,
            stored_path: &cached_path,
            engine: self,
        });
        let watcher_start_ms = watcher_start_t.elapsed().as_millis();

        // Volume root for the event (the FE uses it to decide "at volume root"). For
        // an archive this is the `.zip` path, so the FE breadcrumb renders
        // `…/foo.zip/inner` and treats the archive root as the volume root.
        let volume_root = volume.root().to_path_buf();
        // An empty listing AT the volume root is what macOS TCC makes of a hidden
        // volume (iCloud Drive without Full Disk Access); the host decides whether
        // that reads as a hint instead of a blank pane.
        let empty_at_volume_root = total_count == 0 && volume_root == path;

        // Emit completion event
        let emit_t = crate::rt::Instant::now();
        events.finished(
            &id,
            ListingEnd::Complete {
                volume_id: volume_id.to_string(),
                path: asked_path.to_path_buf(),
                total_count,
                volume_root,
                stored_path,
                empty_at_volume_root,
                effective_sort_by,
            },
        );
        let to_complete_emit_ms = emit_t.elapsed().as_millis();
        let total_ms = total_start.elapsed().as_millis();

        // Consolidated log for the listing pipeline: INFO, or WARN when the read was
        // slow (`listing_done_level`). Grepable single-line structured record. See
        // stall_probe::listing target.
        log::log!(
            target: "stall_probe::listing",
            listing_done_level(read_dir_time),
            "listing_done listing_id={} path={} entries={} read_dir_ms={} sort_ms={} cache_write_ms={} enrich_ms={} watcher_start_ms={} to_complete_emit_ms={} total_ms={}",
            listing_id,
            LogPrivate::dir_path(path),
            entries_count_for_log,
            read_dir_time.as_millis(),
            sort_ms,
            cache_write_ms,
            enrich_ms,
            watcher_start_ms,
            to_complete_emit_ms,
            total_ms,
        );

        benchmark::log_event_value(
            "read_directory_with_progress COMPLETE, read_dir_time_ms",
            read_dir_time.as_millis(),
        );
        Ok(())
    }
}

/// The level `listing_done` logs at, from how long the read itself took.
///
/// `warn` once the volume held the read for a second: that's a busy NAS (a QNAP
/// under load holds single requests for 0.3–6 s) or a slow disk, and a reader
/// scanning a bundle for why a folder took long to open should find it without
/// grepping every `read_dir_ms`. `entries=` on the same line tells a huge folder
/// from a held request. The line and its fields stay the same either way.
pub(crate) fn listing_done_level(read_dir_time: std::time::Duration) -> log::Level {
    if read_dir_time >= std::time::Duration::from_secs(1) {
        log::Level::Warn
    } else {
        log::Level::Info
    }
}

impl Inner {
    /// Cancels an in-progress streaming listing.
    ///
    /// Cancels its token, which both the background task's checks and the `select!`
    /// racing the read observe.
    pub(crate) fn cancel_listing(&self, listing_id: &str) {
        if let Some(state) = self.streaming.read_ignore_poison().get(listing_id) {
            state.cancel.cancel();
            benchmark::log_event_value("cancel_listing", listing_id);
        }
    }
}
