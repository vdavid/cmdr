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
`move_object`, `upload_from_stream`, and `resolve_handle_to_path` (the visible-pane live update) each take
a guard. Nested guards (e.g. recursive `delete`, or `upload_from_stream` → `refresh_dir_handle` → `list_directory`) just
stack the count — harmless, they keep the scan yielded for the whole op. ❌ A READ (download / drag-out) takes NO guard:
it's a *background* gate user that yields TO foreground, so raising `foreground_pending` would make a copy yield to
itself forever (livelock). See "Bounded-window reads" below.

**Bounded-window reads (download + drag-out).** A read is NOT one held-open `GetObject` for the whole file. It's a
sequence of bounded `GetPartialObject64` transactions (window = `MTP_READ_WINDOW` = 8 MiB; the
throughput-vs-yield-latency knob). `open_read_session` resolves the handle and builds an mtp-rs `WindowedDownload`
(`storage.download_windowed_from_offset`, which reads `total_size` via one `get_object_info`) ONCE under the device lock,
returning an `MtpReadSession` the caller caches; each `read_next_window` then takes the per-device lock for just one
window (~80 ms on a Pixel) and releases it. **Between windows nothing is in flight and the single PTP session is free**,
so a foreground listing/nav slips in at window granularity — the whole "navigate the phone during a copy" property, with
no abort/drain (the old held-open `GetObject` pinned mtp-rs's `operation_lock` for the entire file, starving everything
until a ~35 s CLASS_CANCEL drain). `MtpVolume`'s read stream (copy + drag-out) is the one consumer of this pair.

**mtp-rs owns the window bookkeeping; Cmdr owns the LOCK.** The offset tracking, clamp-to-remaining, EOF (`None`),
advance-by-bytes-actually-returned (a short read mid-file is legal), and the 0-byte-before-EOF stall (surfaced as an
error, not loop continuation) all live in mtp-rs's `WindowedDownload::next_window` — Cmdr no longer hand-rolls any of it.
But `next_window` reaches the PTP session DIRECTLY (it holds its own `Arc<MtpDeviceInner>`) and does NOT take Cmdr's
per-device lock. The foreground-priority scheduler relies on every device op taking that lock for its turn, so
❌ `read_next_window` MUST call `next_window` under `acquire_device_lock` (it does). Calling `next_window` without the
lock would let a concurrent listing and the window read drive the same USB session → desync, and break the scheduler
serialization. **Drop-safety:** a window-read future dropped mid-flight (task abort, disconnect) does NOT desync the
session — mtp-rs's `TransactionScope` flags the pipe and the next op drains it under the operation lock (one ~300 ms
self-heal). That's what makes the buffered-window model safe to abort at any point.

**Ranged reads take the DIRECT path, not a session.** A bounded read (`MtpVolume::read_range`, driving archive
browsing and extraction) wants bytes at an offset, not a session, so it goes through `read_range_direct`: one
`GetPartialObject64` under the device lock, and nothing else. Routing it through `open_read_session` instead would add
two USB round trips per call whose products the caller discards — `GetStorageInfo` (`device.storage()`) and
`GetObjectInfo` (`download_windowed` reads `total_size`) — and rc-zip's `EntryFsm` issues one read per 256 KiB, so that
overhead scaled with every extracted byte.

**What the change is worth: ~2.3-2.7 ms of protocol overhead per bounded read, i.e. the two round trips.** That's the
durable number. ❌ Don't quote a percentage as a property of the change: the useful read itself swings hard with device
state (thermal, recent heavy I/O, where the file sits in flash), so the same saving is anywhere from ~20% to ~45%.
Measured on a Pixel 9 Pro XL, mtp-rs 0.28.0, 256 KiB reads, 30 iterations, medians, warmup discarded, disjoint walking
offsets, two sessions on 2026-07-20/21:

- Session 1 (cool device): waste 2.27 ms, bare read 6.06 ms, old path 11.18 ms.
- Session 2 (warm device): waste 2.70 ms, bare read 11.02 ms, old path 13.63 ms.

The waste is stable across both; the bare read nearly doubled. In session 2 `old path - bare read` (2.62 ms) reconciles
with the measured waste (2.70 ms) almost exactly, which is the check that the model is right. Cmdr's own `read_range`
path, measured through `backends/mtp_read_bench.rs` in session 1, went from ~7.6-10.0 ms to ~5.7-6.2 ms per 256 KiB.

An earlier reading suggested `next_window` was ~2.45 ms slower than a bare `read_range` for the same bytes. It did NOT
reproduce (session 2 had it faster), so it was noise: don't build anything on it.

The storage handle comes from a per-`(device, storage)` cache on `DeviceEntry` (`storage_cache`), so the
`GetStorageInfo` is paid once per device, not once per read. That cache is safe because an `mtp_rs::Storage` is
`{ Arc<dyn MtpBackend>, id, info }` and `Storage::read_range` goes straight to the backend: a stale entry can only serve
a stale `info()` snapshot (free space, capacity), never stale bytes, and the backend `Arc` is the same one the
`MtpDevice` holds, so it stays valid for the entry's whole life. It's invalidated on `StorageInfoChanged` and
`StoreRemoved` (both via `invalidate_storage_cache`), and a disconnect needs no invalidation at all — the whole
`DeviceEntry` leaves the registry. ❌ Don't point the COPY path at `read_range_direct`: there the single `GetObjectInfo`
amortizes over hundreds of windows, and `total_size` is what anchors progress, ETA, and the foreground-yield checkpoint.

**Two background consumers, not one.** The scan is no longer the only yielding background user of the gate. A RUNNING MTP transfer is the second: its between-window checkpoint polls `MtpConnectionManager::foreground_pending(device_id)` and, when foreground pends, simply does NOT start the next window — it awaits `background_yield_point(device_id)`, then resumes reading from the current offset (it stays `Running`, not Paused). Because the read is already bounded windows (see "Bounded-window reads" above), the session is free between windows, so this yield is cheap and needs no release/reopen — there is no in-flight transaction to abort/drain (`cancel_and_release` is a no-op, never called by the copy path). This is the "navigate the phone DURING a transfer" feature; the gate sees a transfer exactly as it sees the scan — a background user that consults `foreground_pending` / `background_yield_point` between work units. The manager exposes both `foreground_pending(device_id) -> bool` (the gate's `foreground_pending()`, `false` if absent) and `background_yield_point(device_id)` for this. Lane budget 1 on the MTP device means the only foreground contender is a listing/nav/metadata op, never a second transfer, so there's no transfer-vs-transfer or transfer-vs-scan priority inversion — both yield to the same signal. Mechanics + the debounce/min-progress-floor tuning live in `write_operations/transfer/DETAILS.md` § "Foreground auto-yield".

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

**Design history** is in git (former `docs/specs/mtp-device-scheduler-plan.md`).

## Upload partial cleanup (two-phase PTP uploads)

PTP uploads are two-phase: `SendObjectInfo` creates the object on the device, then `SendObject` streams the bytes. If the
data phase fails (a genuine error or a user cancel), mtp-rs's `Storage::upload` returns `Err(mtp_rs::UploadError)` whose
`partial: Some(handle)` carries the created-but-incomplete object. The library deliberately does not auto-delete it; the
caller owns the cleanup-or-resume decision.

Per cmdr's no-corrupt-artifact policy (AGENTS.md principle #4), `upload_from_stream` in `file_ops.rs` best-effort
deletes the partial via `storage.delete(handle)` before returning, then surfaces the mapped `upload_err.source` error.
This holds for cancel too: on cancel, `source` is `Error::Cancelled` and `partial` is
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

## Session reset is not a disconnect (`MtpConnectionError::SessionReset`)

mtp-rs's `Error::DeviceReset` means IT reset the device in software to recover from a wedged transfer cancel: the PTP
session is gone, the device is still plugged in and reopenable with no replug. `map_mtp_error` gives it its own typed
variant and its own `warn!` line, so a reset is diagnosable in a log instead of hiding in the generic `Other` bucket,
and ❌ it never maps to `Disconnected` / `VolumeError::DeviceDisconnected` — that would drop a live device out of the
sidebar and flip its index Stale for nothing. mtp-rs agrees: `Error::is_disconnected()` is deliberately false for it.

**How Cmdr reaches it** (the 0.24.0 changelog's "cancel-only" framing understates this): not via `FileDownload::cancel`
(Cmdr never calls `download()`), but via `PtpSession::recover_if_needed()`. Every data-phase op arms a
`TransactionScope`; if that future is dropped before it disarms, the NEXT op drains the pipe with `cancel_transfer` and
propagates the drain's outcome verbatim. Cmdr no longer creates that state deliberately (see § "No dropping timeouts"
below); the drain is the seatbelt for what's left: a genuine device disconnect mid-transfer.

### The recovery (`session_reset.rs`)

**Hardware evidence** (verified on Galaxy S23 Ultra SM-S918B, macOS/nusb, Cmdr's exact call shape, 2026-07-20). Dropping
a windowed `GetPartialObject64` future mid-flight (8 MiB window, dropped after 25 ms) armed mtp-rs's
`TransactionScope`. The NEXT op ran `recover_if_needed`'s drain and returned `Err(DeviceReset)`; `is_disconnected()`
was **false**; a follow-up listing then returned `Timeout` — the session was dead and stayed dead until a physical
replug. Recovery IS possible in software, but only with SPACED retries. What worked, in order: fresh open → `Timeout`;
transport reset → "didn't answer yet"; fresh open → `SessionAlreadyOpen`; fresh open → SUCCESS. **Hammering re-wedges
it into a hard `Timeout`** (mtp-rs's own notes say so, and it reproduced).

So `handle_device_session_reset` (the sibling of `handle_device_disconnected`, triggered from `map_mtp_error`'s
`DeviceReset` arm — the one choke point every device op funnels through, deduplicated per device by `RECOVERING`):

1. **Drop the `DeviceEntry`** and stop the event loop. The path, listing, and storage caches live ON the entry, so
   dropping it clears them — required, not hygiene: handles don't survive the reset, and a stale reverse
   `PathHandleCache` entry resolves a NEW object to a dead path (devices reuse handles).
2. **Flip every indexed storage Stale** (`indexing::on_mtp_watch_continuity_lost`). Same call the disconnect path
   makes, for the same reason: events fired while the session was dead are lost, and the handles the scan stored in
   `inode` may no longer identify the same objects, so an `ObjectRemoved` could resolve to the wrong row. The device
   being present doesn't buy freshness back — the model's rule is "Stale ⇒ Fresh only via rescan". The cost is real
   (a 2-second blip costs a phone rescan), and it's the right side to err on.
3. **Keep the volume registered and emit NO `MtpDeviceDisconnected`.** The device is still attached, so it stays in the
   sidebar while the reopen runs. ❌ Conflating this with a disconnect throws away a live device.
4. **Reopen with idle-spaced backoff**: 1.5 s quiet pause, then ×1.5 per attempt capped at 15 s, 10 attempts (~100 s).
   ❌ Don't collapse it to a single retry — on hardware attempts 1 and 2 failed (`Timeout`, then `SessionAlreadyOpen`)
   and the third succeeded, so "try once and give up" declares a device dead that is two seconds from working. ❌ Don't
   tighten the spacing either; that's what re-wedges the device. Between attempts it re-checks
   `watcher::is_mtp_enabled()` so a recovery in flight can't resurrect a device the user just switched MTP off for.
   A `DeviceNotFound` (unplugged mid-recovery) means this IS a disconnect now: it runs the real teardown and stops.
   Exhausting the budget does the same.

mtp-rs self-heals `SessionAlreadyOpen` inside `PtpSession::open` (it closes and reopens), so step 4 doesn't
special-case it.

### No transport reset in recovery

**❌ Never add a USB transport reset (mtp-rs `reset_by_serial` / `reset_by_location` / `reset_first`) between steps 1
and 4.** It reads like the obvious missing piece — the Still Image Class `DEVICE_RESET` control request is exactly the
"unwedge the pipe" primitive, and it's the step the Galaxy repro sequence above contains. It isn't. Enforced by
`pnpm check mtp-no-transport-reset`, which has no opt-out directive on purpose.

**On Android the reset is a kill switch, not a recovery step** (verified on a Pixel 9 Pro XL running the reset against
a HEALTHY device, `adb logcat`, 2026-07-21). Android's `MtpServer` answers it by tearing down and never re-arming:

```
MtpServer: got response 0x201E in MTP_OPERATION_OPEN_SESSION
MtpServer: request read returned -1, errno: 125        <- ECANCELED
d.process.media: Mtp got error event at 0 and 1 total: Broken pipe
MtpServer: request read returned -1, errno: 32         <- EPIPE
libpixelusb-UsbDataSessionMonitor: Update device state udc: configured
```

`MtpServer` then logs nothing further: no restart, no re-arm of its FunctionFS endpoints, while the USB device
controller still reports `configured`. That's the failure users see as "the phone is listed but nothing works": it
keeps enumerating and keeps showing up in a device list while answering no PTP at all, until a physical replug. So the
reset takes a working phone and costs the user a replug.

**And recovery doesn't need it.** A dropped-future wedge on a Pixel self-heals on a fresh open (observed: two hung
in-process attempts, then normal answers once the process exited), which is precisely what steps 1 and 4 already do.
Inserting a reset converts a self-healing situation into a replug.

**The Galaxy evidence doesn't outweigh that**, even though the reset appeared to help there: the control was never run,
so spaced reopens alone may well have carried it. Ambiguous evidence on one device loses to a proven kill on another.

**Coverage** (`session_reset.rs` § `device_tests`). `mtp_rs::force_operation_wedge(serial)` (mtp-rs ≥ 0.29.0, feature
`virtual-device`) arms a one-shot so the next PTP operation returns `DeviceReset`, which is what makes
`a_wedged_listing_recovers_without_dropping_the_device` a real end-to-end run through Cmdr's own path: an ordinary
`list_directory` trips it, and the test asserts the whole chain (mapped to `SessionReset`, recovery ran, the volume
stayed in the sidebar, the disconnect teardown never ran, the caches came back empty, the device serves listings
again). ❌ Don't reach for the older `force_cancel_wedge`: it arms on `cancel_transfer`, which Cmdr never calls. What
the virtual device CANNOT model is the aftermath: a real session stays dead until a spaced-retry reopen, while the
virtual one is healthy on the very next call, so the test proves the plumbing, not the timing.

**The failing operation is not retried, and reports a RECOVERABLE reason.** `MtpConnectionError::SessionReset` maps to
`VolumeError::DeviceSessionReset` → the `DeviceReconnecting` listing reason (`Transient`, retry hint) and
`WriteOperationError::ConnectionInterrupted` on the write path. ❌ Never `io_serious` (a dead end that tells the user
their data is broken) and ❌ never `DeviceDisconnected` (tells them to re-plug a phone that never left).

**No auto-resume of an in-flight transfer.** `MtpReadStream`'s offset is byte-exact and `open_read_session` already
takes one, so resuming the READ side is trivial; the destination side isn't. `stream_pipe_file` writes through the
safe-overwrite temp+rename path, and resuming would mean keeping a partial temp file alive across the reopen and
re-entering the write mid-stream — real surface in the engine's partial-cleanup and progress accounting, for a case
that costs the user one click today. The retryable classification is what makes that click cheap. Revisit if field
reports show resets hitting multi-GB copies often.

**Testing without hardware.** Alongside the end-to-end wedge above, `session_reset.rs`'s tests also drive the state
machine directly: the backoff schedule is a pure function with its own tests, and the virtual-device tests call
`tear_down_reset_session` / `reopen_after_session_reset` and assert the entry is dropped, the path cache cleared, the
volume still registered, the device reopened, and `handle_device_disconnected` never reached (via a test-only call
counter in `directory_ops.rs` — nothing else observable distinguishes the two paths in a unit test).

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
  is bounded by mtp-rs's own per-transfer USB timeout; the device lock is held across the (few, shallow) hops so an event burst can't
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

### Keeping both directions in lockstep

Every write to `PathHandleCache` goes through `insert` or `remove_path`, which touch both maps. ❌ Never poke
`path_to_handle` (or `handle_to_path`) directly — the desync is invisible at the time and expensive later:

- **A missing reverse entry costs round trips.** `resolve_handle_to_path` falls back to a USB parent-chain walk when the
  handle isn't cached, so a one-sided write doesn't fail, it just makes every subsequent PTP event for objects Cmdr
  itself created re-walk to the storage root.
- **A stale reverse entry returns the WRONG path.** `walk_handle_to_path` consults the reverse cache FIRST, so a handle
  left pointing at a removed or renamed path resolves to it. MTP devices reuse object handles, so the next object to
  inherit that handle lands a targeted refresh on the wrong directory and an index upsert at the wrong path.

`remove_path` deletes the reverse entry only when it still points at the path being removed. A rename is
`remove_path(old)` then `insert(new, handle)`; without that guard, a later cleanup of a stale forward entry would erase
the handle's current, correct reverse mapping.

Pinned by `path_cache_sync_test.rs` (create folder, rename, move, upload, delete), which asserts the reverse map through
a test-only accessor rather than through `resolve_handle_to_path` — the USB fallback would otherwise make a desynced
cache look healthy.

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

## No dropping timeouts

**The rule:** nothing in this module wraps an mtp-rs call in `tokio::time::timeout`, and nothing aborts a task holding
one. `pnpm check mtp-dropping-timeout` enforces it across `src/mtp/`.

**Why.** A PTP transaction is command → data → response over one bulk pipe. Dropping the future mid-data-phase leaves
the device expecting bytes nobody will send, or holding bytes nobody will read; the next transaction desyncs. mtp-rs's
`TransactionScope` + `recover_if_needed` drain is a MITIGATION, and Cmdr's `SessionReset` path is a second one. Neither
is a guarantee: on some devices the software recovery doesn't get them back, so the user replugs the phone. A
wall-clock timeout that drops therefore CONVERTS A SLOW PHONE INTO A BROKEN ONE.

**What bounds an op instead.** mtp-rs's transport applies `USB_TRANSFER_TIMEOUT_SECS` (30 s) to every bulk transfer and,
on expiry, returns `PtpError::Timeout` while LEAVING the transfer pending on the endpoint — a clean failure that
abandons nothing and that a retry can pick up. An outer `tokio::time::timeout` with the same budget is strictly worse:
its clock starts EARLIER (it covers mtp-rs's operation-lock wait and recovery drain too), so it always fires first and
can only ever preempt the clean failure with a wedge. That argument holds for every single-transaction call.

**Multi-round-trip ops are unbounded in total, and that's correct.** A 2,000-entry `/DCIM/Camera` listing is 2,001
transactions; a 30 s (or 120 s) wall-clock cap on it is wrong by construction, and crossing it is exactly the wedge
users hit. Each round trip stays bounded, the listing reports honest progress as entries arrive, and a `CancelToken`
gives a prompt out. The background scan additionally checks the token at every unit boundary.

**Cancellation is cooperative everywhere.** `CancelToken` (`list_objects_with_cancel`, `list_objects_stream_with_cancel`,
`delete_with_cancel`) is checked BETWEEN per-handle round trips, so a cancel lands within one round trip's latency —
prompt from the user's point of view, and never mid-transaction. Windowed reads and `read_range_direct` are single
bounded transactions, so a copy cancel simply stops issuing windows.

**Detach, don't abort, when a caller must answer NOW.** Some callers genuinely have a deadline (an IPC reply, the index
walk giving up on a directory). They run the work in its own task and race the deadline against that task's JOIN
HANDLE. Dropping a `JoinHandle` DETACHES the task, it does not cancel it: the caller answers on time, the transaction
finishes safely behind it. `commands::util::timeout_detached` is the shared helper; `indexing::volume_scanner`'s
`list_one_directory` and the streaming listing's cancel arm use the same shape.

**Where the drops used to be** (all fixed, listed so nobody reintroduces one): every `tokio::time::timeout` in
`file_ops.rs` / `directory_ops.rs` / `mutation_ops.rs` / `handle_resolver.rs`, the 3 s connect-time
`probe_write_capability`, the 300 s upload cap, `listing_task.abort()` in `file_system/listing/streaming.rs`, the 120 s
`LIST_TIMEOUT` in `indexing/volume_scanner.rs`, and the 2 s / 5 s / 30 s IPC caps in `commands/rename.rs` and
`commands/file_system/volume_copy.rs`.

**The two deliberate exceptions**, both annotated `// allowed-dropping-timeout:`: the device-lock wait
(`acquire_device_lock` — a `tokio::Mutex`, nothing on the wire) and the event loop's 5 s `next_event()` poll (the
INTERRUPT endpoint, not the bulk pipe; mtp-rs leaves that transfer pending on drop and picks it up next poll).
