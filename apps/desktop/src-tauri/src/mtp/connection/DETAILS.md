# MTP connection details

Depth and rationale. `CLAUDE.md` holds the must-knows; the depth lives here.

## Conventions

- **Event debounce**: `EventDebouncer` collapses MTP event bursts to one frontend emit per 500 ms; cleared on disconnect.
- **Async recursion** (`bulk_ops.rs`, recursive `delete()`): `Box::pin(async move { ... })` breaks the infinite future.
- **Event-loop shutdown**: biased `tokio::select!` so the shutdown signal always wins over the event poll.

## Foreground-priority device scheduler

**Problem.** A background MTP index scan livelocked the phone: while Cmdr indexed a storage, folder navigation, copy,
delete, and live updates to the current folder stalled for tens of seconds. Two Cmdr POLICY bugs (not `mtp-rs`, which is
already protocol-serial-correct via its per-transaction `operation_lock`):

1. The scan held Cmdr's single per-device lock across an ENTIRE directory enumeration — one `GetObjectHandles` plus one
   `GetObjectInfo` per child, so a 9,000-file folder pinned the device for ~30 s and any foreground op timed out
   (`list_objects timed out after 30s`).
2. The live watch→index feed resolved the change handle (a device round trip) BEFORE checking "are we scanning?", so
   every change event hit the contended device during a scan (`resolve_object_for_index: timed out waiting for device
   lock`). This was "gate-too-late".

**The primitive (`scheduler.rs`).** Per connected device, `DevicePriorityGate` holds `foreground_pending: AtomicUsize`
plus a `tokio::sync::Notify`. It owns no device handle and does no I/O, so its ordering is unit-tested with synthetic
counters (`scheduler.rs` tests).

- `foreground_guard()` (RAII): increments the counter on entry, decrements on drop, and `notify_one`s when it hits zero.
  Every foreground device op takes one for its whole lifetime (`MtpConnectionManager::foreground_guard(device_id)`).
- `background_yield_point()`: `while foreground_pending > 0 { drained.notified().await }`. The scan calls it between
  units. Returns immediately at zero pending, so an idle scan never stalls.

We use `notify_one`, NOT `notify_waiters`: `notify_one` STORES a permit when no waiter is parked, so a foreground drain
that races the yield point's check-then-await is not lost (the stored permit makes the `.await` return, and the `while`
re-reads the counter). A leftover permit at most causes one extra wake on a later yield, which re-checks and re-parks —
never a wrong "proceed", since the loop gates on the counter, not the wake. `notify_waiters` keeps no permit and would
deadlock on that race.

**Per-unit scan listing (`list_directory_for_scan`).** Splits one folder into bounded UNITS, each a separate device-lock
acquisition with a yield point between:

- Unit 0: yield → lock → `list_objects_stream_with_cancel` (one `GetObjectHandles`) → release. The returned
  `ObjectListing` owns its own `Arc<MtpDeviceInner>` (independent of Cmdr's lock), so it survives lock release/re-acquire.
- Units 1..n: yield → lock → up to `SCAN_METADATA_BATCH` (32) `listing.next()` calls (each one `GetObjectInfo`) →
  release.

**Batch sizing.** Worst-case foreground wait is one in-flight unit = one metadata batch. A `GetObjectInfo` is
single-digit-to-low-tens of ms over USB, so 32 keeps a unit well under ~1 s while keeping lock-acquire overhead
negligible against the round trips. Retune the constant in `directory_ops.rs` if the latency target changes.

**Which ops are foreground.** `list_directory*` (pane nav), `delete_object*`, `create_folder`, `rename_object`,
`move_object`, `upload_file`, `upload_from_stream`, `download_file_with_progress`, `open_download_stream_at_offset` (setup), and
`resolve_handle_to_path` (the visible-pane live update) each take a guard. Nested guards (e.g. recursive `delete`, or
`upload_from_stream` → `refresh_dir_handle` → `list_directory`) just stack the count — harmless, they keep the scan
yielded for the whole op. Streaming-download per-chunk reads go through mtp-rs's own `Arc`/operation lock, not Cmdr's
device lock, so they interleave with scan units at mtp-rs's per-transaction granularity even though the guard only
covers the stream SETUP — no 30 s starvation either way.

**Gate-before-resolve (`event_loop.rs` + `indexing/mtp_watch.rs`).** `feed_index_added_or_changed` now asks
`indexing::buffer_mtp_handle_if_scanning(volume_id, storage_id, handle)` FIRST, per indexed storage, with NO device
touch. If that volume is scanning it buffers the RAW handle (`BufferedChange::UpsertHandle`) and the caller skips the
resolve; only a non-scanning storage resolves live. Removals already buffered the bare handle; now adds/changes do too.
`replay_buffered_mtp_changes` applies the sync changes immediately, then spawns one task to resolve the buffered raw
handles (post-scan, device idle) and upsert them — a failed resolve is dropped (the scan already captured the object;
any later change re-fires). The buffered-handle storage can be the wrong one (we don't know which storage owns a handle
without resolving), but the wrong storage's replay resolve fails cleanly, matching the existing per-storage skip.

**Deadlock-freedom and progress.** The gate state is touched without holding the device lock, and the device lock is the
only OS lock and always released at a unit boundary — no lock-ordering cycle. Foreground never waits on
`background_yield_point` (it only raises the counter and contends for the device lock, which the scan holds for ≤ one
batch). Background always progresses when idle (the yield returns at zero pending) and a parked scan is always woken (the
last guard drop decrements to zero and `notify_one`s). No priority inversion: the scan yields at every unit boundary and
foreground gets the device after the current in-flight transaction (mtp-rs guarantees it's atomic and bounded). The
scan's `cancelled` flag is checked at every unit boundary AND threaded into `mtp-rs` per `GetObjectInfo`, so a cancel
stops within one round trip; heal-to-rescan, freshness, and buffer/replay/overflow are untouched.

**Plan.** `docs/specs/mtp-device-scheduler-plan.md`.

## Upload partial cleanup (two-phase PTP uploads)

PTP uploads are two-phase: `SendObjectInfo` creates the object on the device, then `SendObject` streams the bytes. If the
data phase fails (a genuine error or a user cancel), mtp-rs's `Storage::upload` returns `Err(mtp_rs::UploadError)` whose
`partial: Some(handle)` carries the created-but-incomplete object. The library deliberately does not auto-delete it; the
caller owns the cleanup-or-resume decision.

Per cmdr's no-corrupt-artifact policy (AGENTS.md principle #4), both upload call sites in `file_ops.rs` (`upload_file`,
`upload_from_stream`) best-effort delete the partial via `storage.delete(handle)` before returning, then surface the
mapped `upload_err.source` error. This holds for cancel too: on cancel, `source` is `Error::Cancelled` and `partial` is
`Some`, so a cancelled upload also deletes the half-file (the user cancelled, don't leave it on their phone), and
`map_mtp_error(Error::Cancelled)` still yields `MtpConnectionError::Cancelled`, preserving cancel classification for the
write-op layer.

The delete needs a live device/session. If the device just disconnected, the delete fails: we log under
`target: "mtp_upload"` and move on (the partial lingers, recognizable; nothing we can do with a dead device). A failed
cleanup never masks the original upload error. Pinned by `upload_failure_deletes_partial_object_on_device` and
`upload_cancel_deletes_partial_and_surfaces_cancelled` (virtual-mtp tests in `volume/backends/mtp.rs`).

## Stale parent handle on upload (self-heal + one-shot retry)

`resolve_path_to_handle` is cache-only: the parent-folder handle an upload uses comes from whenever the user last listed
that folder. Android routes MTP through MediaProvider, whose object handles are NOT stable across a media rescan, so a
handle can go stale between the listing and a later upload into the folder. The device then rejects `SendObjectInfo`
(phase 1, before any source byte is read) with `InvalidParentObject` (or `InvalidObjectHandle`). Field report: a 307 MB
upload into a Pixel's `/Documents` failed this way, surfaced to the user as a "Path not found" on the intact *source*
file (`map_volume_error` funneled `VolumeError::NotFound` into `SourceNotFound`), with no log and no retry.

The recovery, split across two layers because the data stream is single-use:

- **Connection layer (`upload_from_stream`)**: `is_stale_handle_rejection(&upload_err.source)` classifies the rejection.
  Then — crucially — it DROPS the device lock before healing: `refresh_dir_handle` re-acquires the lock through
  `list_directory`, and the per-device lock is a non-reentrant `tokio::sync::Mutex`, so refreshing while still holding it
  deadlocks. `refresh_dir_handle` re-lists the folder's ancestors root-first (invalidating each listing cache first so
  the 5 s TTL can't serve a stale listing); listing `parent(dir)` repopulates the fresh handle for `dir`. Root is a
  constant, so a top-level folder like `/Documents` heals with a single re-list of `/`. The method then returns
  `MtpConnectionError::StaleParentHandle { dest_folder }` (→ `VolumeError::StaleDestinationHandle`). It does NOT retry
  the upload itself — the `data_stream` was moved into `Storage::upload` and consumed.
- **Transfer engine (`write_operations/transfer/volume_strategy.rs::stream_pipe_file`)**: owns the retry because it can
  re-open the source. On `VolumeError::StaleDestinationHandle` it re-opens the source stream and re-runs
  `write_from_stream` once (`retried` budget of 1). Safe to restart the whole file: the rejection lands before any
  source byte is read or destination byte written, so no progress double-counts and no partial lingers (on
  `InvalidParentObject` mtp-rs creates nothing, so `UploadError.partial` is `None`).

If the retry also fails, `map_volume_error` maps `StaleDestinationHandle` → `WriteOperationError::WriteError { path:
dest_folder }` ("Couldn't write to the destination…"), a destination-correct message — never `SourceNotFound`. All
upload failures now also `log::warn!` under `target: "mtp_upload"`, so a bare protocol rejection leaves a breadcrumb.
Pinned by `upload_into_stale_parent_handle_heals_and_retry_succeeds` (connection layer) and
`stream_pipe_file_retries_once_on_stale_destination_handle` (engine).

## Pathful change events: handle → path resolution (`handle_resolver.rs`)

PTP change events carry only an opaque `ObjectHandle` (a `u32`), never a path: every event on the interrupt endpoint is
`code + 3×u32`, a property of the wire format, not a Cmdr or mtp-rs gap. `event_loop.rs` turns `ObjectAdded` /
`ObjectInfoChanged` into a **targeted** refresh of just the affected directory, instead of the old blanket re-list of
every open pane on the device. `ObjectRemoved` stays blanket (see below).

### The resolver

`MtpConnectionManager::resolve_handle_to_path(device_id, storage_id, handle) → Result<PathBuf, MtpConnectionError>`
returns the object's full virtual path (`/DCIM/Camera/IMG_0001.jpg`). It asks the device for the object's `ObjectInfo`
(`{ parent, filename }`) and walks the `parent` chain up to the storage root, prepending each filename. Root-level
objects resolve to `/<name>`; the root handle itself resolves to `/`.

It's split in two on purpose:

- **Phase 1 — `prefetch_handle_chain` (async, the only USB-touching half).** Follows parents hop-by-hop via
  `GetObjectInfo`, memoizing `(parent, filename)`, stopping at a cached ancestor, a root sentinel, or `MAX_WALK_DEPTH`.
  The device/storage open lazily on the first miss, so a fully-cached resolve issues **zero** USB calls. Each round trip
  is `MTP_TIMEOUT_SECS`-bounded; the device lock is held across the (few, shallow) hops so an event burst can't
  interleave them and thrash the session.
- **Phase 2 — `walk_handle_to_path` (pure).** Assembles the path from the phase-1 memo plus the reverse cache. It owns
  the canonical stop/assembly logic (short-circuit, root sentinels, depth cap), so phase 1 only over-approximates "what
  might be needed" and never computes the path. Being pure, it unit-tests against an in-memory handle graph with no
  device (cached-ancestor short-circuit, full walk to root, root object, invalid handle, cyclic chain).

**Root sentinels:** the walk stops at both `ObjectHandle::ROOT` (`0`, the spec value) and `ObjectHandle::ALL`
(`0xFFFFFFFF`) — the Android root quirk lets a root child report either as its parent (mirrors mtp-rs's `AndroidRoot`
filter). Treating only `ROOT` as root would fail the walk for those devices.

**Cycle guard:** `MAX_WALK_DEPTH` (256) bounds a self-referential/cyclic parent chain from malformed firmware. Real MTP
trees are a handful of levels deep; the cap exists only so a bad device can't wedge the walk — it fails cleanly and the
caller falls back.

### Reverse cache (`PathHandleCache::handle_to_path`)

**Decision:** `PathHandleCache` keeps both `path → handle` (forward, for browsing's `resolve_path_to_handle`) and
`handle → path` (reverse, for the resolver). **Why:** the resolver's walk would otherwise hit USB for every ancestor up
to root on every event; the reverse map lets it stop the instant it reaches an ancestor the user has already browsed, so
resolving a newly added file under an open folder is usually one `GetObjectInfo` (the file itself). The two maps are
populated together at the same sites (`finalize_listing`, fed by both `convert_object_infos` and the streaming path) via
`PathHandleCache::insert`, which writes both directions — **never** insert into `path_to_handle` directly, or the maps
drift. The resolver also caches the resolved leaf `(handle → path)` so a follow-up event under the same folder
short-circuits on it.

### Targeted refresh and the blanket fallback (`event_loop.rs`)

`emit_change_for_handle` resolves the handle, derives the **affected directory** (the object's parent — the folder whose
listing shows it), and re-reads only the listing(s) showing that exact directory on that storage, through the same
debouncer and diff coalescer as the blanket path. PTP handles are device-wide but storages are separate namespaces, so
it attempts resolution against each storage that has an open listing and targets the first whose resolved parent matches
an open listing.

**Blanket fallback — never lose an update.** On any resolution failure (handle invalid, parent uncached and the walk
fails, timeout) or when no open listing shows the affected dir on any storage, it falls back to
`emit_directory_changed` (re-read + diff every open listing on the device). This keeps the live pane correct even when
the resolver can't help — the cost is precision, not correctness.

**`ObjectRemoved` is always blanket** for the live pane: the object is already gone, so `GetObjectInfo(handle)` fails and
the resolver can't recover a path. The index path resolves removals via the handle stored per index entry instead
(`inode` column; see below).

`listing_inner_mtp_path` reduces a listing's stored path (`mtp://<device>/<storage>/<inner…>` or a `/`-rooted inner
path) to the leading-`/` inner form the resolver produces, so the affected-dir match compares apples to apples in both
representations. Pinned by the `event_loop` tests.

### Feeding the per-volume index (the second consumer)

Each handle event also feeds the persisted index, so dir sizes stay correct while the device is Fresh even with no pane
open — alongside the live-pane refresh above, not instead of it. `feed_index_added_or_changed` runs as a spawned task
(it does USB I/O): for each indexed storage on the device (`indexing::registered_mtp_volume_ids_for_device`), it calls
`resolve_object_for_index` (the handle→path walk plus one `GetObjectInfo` for size / is-dir / modified) and forwards an
`indexing::MtpUpsert` carrying the handle; the first storage where the handle resolves wins (the object lives in exactly
one). `feed_index_removed` is synchronous (DB + writer enqueue only, no USB): it forwards the bare handle to each indexed
storage, and the one that indexed the object resolves it by the STORED handle. The translation, ordering, and
buffer-during-scan logic live in `indexing/mtp_watch.rs` (see `indexing/DETAILS.md` § "MTP indexing"); the event loop
only resolves + forwards. The handle is stored in the index `inode` column at scan time too (`directory_ops.rs`).
