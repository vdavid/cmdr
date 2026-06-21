# MTP device scheduler: foreground-priority access for indexing

## Problem (diagnosed)

A background MTP index scan livelocks the phone. While Cmdr indexes a storage, user-initiated foreground actions (folder
navigation, copy, delete) and live updates to the current folder stall for tens of seconds. Two distinct policy bugs in
Cmdr (not in `mtp-rs`, which is already correct) cause it:

1. **Whole-folder device hold.** The scan walks one directory at a time over `Volume::list_directory`
   (`indexing/volume_scanner.rs`), which on MTP reaches `connection_manager().list_directory_with_cancel`. That call
   acquires the single per-device lock (`Arc<tokio::sync::Mutex<MtpDevice>>`) and holds it across the ENTIRE directory
   enumeration: one `GetObjectHandles` plus one `GetObjectInfo` per child. A 9,000-file folder = ~9,000 serial USB round
   trips under one continuous lock hold (~30 s). Any foreground op that needs the device waits the whole time, and the
   30 s op timeout then fires: real logs show `list_objects timed out after 30s` interleaved with
   `resolve_object_for_index: timed out waiting for device lock`.

2. **Gate-too-late on the live watch→index path.** A PTP change event runs `feed_index_added_or_changed`
   (`mtp/connection/event_loop.rs`), which spawns a task that calls `resolve_object_for_index` — a device round trip
   (handle→path walk + `GetObjectInfo`) — and ONLY THEN calls `apply_mtp_added_or_changed` (`indexing/mtp_watch.rs`),
   where the "are we scanning? → buffer" check lives. So during a scan, every change event in a non-visible folder still
   hits the contended device before the buffer gate can spare it. The removal path already buffers the raw handle
   without a device hit; adds/changes don't.

### What is and isn't ours

- **`mtp-rs` needs no change.** PTP is protocol-serial (one bulk transaction at a time); `mtp-rs` already enforces this
  with an internal per-transaction `operation_lock`, and the interrupt/event endpoint is independent of it. That gives
  per-transaction granularity, which is all a priority policy needs.
- **The device-access POLICY belongs to Cmdr** (the consumer). This plan replaces Cmdr's coarse "hold the device for a
  whole folder" with a priority-aware gate around per-unit access.

## Design

A per-device **priority gate** arbitrates device access in two classes:

- **Foreground**: user-initiated nav / copy / delete / rename / move, plus enriching or live-resolving the
  CURRENT/visible folder.
- **Background**: the index scan, and resolving changes in non-visible folders.

Contract: when any foreground op is pending, the background scan must not start a new unit until foreground drains.
Foreground always gets the device after at most one in-flight background transaction; the scan always makes progress
when no foreground work pends.

### The priority primitive: `DevicePriorityGate`

One gate per connected device, owned by `MtpConnectionManager` alongside the device lock
(`mtp/connection/scheduler.rs`). It holds:

- `foreground_pending: AtomicUsize` — count of foreground ops currently entered (waiting for, or holding, the device).
- `notify: tokio::sync::Notify` — wakes a yielding background unit when foreground drains to zero.

Operations:

- **`foreground_guard()`** — RAII: increments `foreground_pending` on entry, decrements on drop and calls
  `notify_waiters()` when it reaches zero. Every foreground device op takes one of these BEFORE acquiring the device
  lock. (Acquiring the OS device lock is unchanged; the guard only makes the op COUNTED so the scan yields to it.)
- **`background_yield_point().await`** — the scan calls this between units. It loops:
  `while foreground_pending > 0 { notify.notified().await }`. This is the auto-pause: a foreground op raised the count,
  so the scan parks here until the last foreground op drops its guard and notifies. No explicit pause UI.

The gate is a pure-logic primitive (no I/O, no device handle), so its ordering is unit-testable with synthetic counters
and notifies.

### Per-unit scan access (never hold the device across a folder)

The scan gets a dedicated MTP listing path, `list_directory_for_scan`, that splits one folder into bounded units, each a
separate lock acquisition, with a yield point between them:

- **Unit 0 — handles.** Yield point → acquire device lock → `list_objects_stream_with_cancel` (one `GetObjectHandles`) →
  release. This builds an `ObjectListing` that owns its own `Arc<MtpDeviceInner>` (independent of Cmdr's lock), so it
  survives across lock release/re-acquire.
- **Units 1..n — metadata batches.** Repeat: yield point → acquire device lock → call `listing.next()` up to
  `SCAN_METADATA_BATCH` times (each one `GetObjectInfo`) → release. Between batches the lock is free, so a foreground op
  takes it immediately.

Worst-case foreground wait is one in-flight unit = one `GetObjectInfo` batch. `SCAN_METADATA_BATCH` is sized so that
batch stays at or under ~1 s on a slow USB 2.0 link. We pick **32** (a `GetObjectInfo` is typically single-digit to low
tens of ms; 32 keeps the batch well under a second while keeping lock-acquire overhead negligible against the round
trips). The constant is documented and easy to retune.

Why not push this into `mtp-rs`? Because the unit boundary is where Cmdr's PRIORITY decision happens (yield to
foreground), and priority is Cmdr policy. `mtp-rs` already gives the per-transaction granularity that makes the split
possible; Cmdr decides when to yield.

The foreground `list_directory*` paths are unchanged except that they now take a `foreground_guard()`. They still fetch
the whole folder in one lock hold — that's correct for foreground (the user wants the listing now, and it's not the
thing being starved).

### Gate-before-resolve fix (live watch→index)

Move the scanning check to BEFORE the device round trip. `feed_index_added_or_changed` (event loop) first asks, per
indexed storage, whether that volume is currently scanning AND the change is for a non-visible folder:

- **Scanning + non-visible folder**: buffer the RAW HANDLE (no device hit) via a new `apply_mtp_added_or_changed_handle`
  that records `BufferedChange::UpsertHandle(handle)` and returns without resolving. The post-scan replay resolves the
  handle then (the device is no longer contended), mirroring how removals already buffer the raw handle.
- **Current/visible folder OR not scanning**: resolve at FOREGROUND priority (the resolve takes a `foreground_guard()`),
  so live updates to the open pane land in ~1–2 s even while the scan runs.

"Visible folder" is decided by the same open-listing oracle the targeted-refresh path already uses
(`get_listings_by_volume_prefix`): if the device has an open listing whose storage matches, the change is treated as
foreground-relevant and resolved live; otherwise it's background and buffered during a scan.

Because buffered adds/changes now carry an unresolved handle, the replay path resolves each handle once the scan ends
(device idle), then applies the upsert. Overflow and discard semantics are unchanged.

## Deadlock-freedom and progress argument

- **No lock-ordering cycle.** The gate's `foreground_pending`/`notify` are touched without holding the device lock; the
  device lock is the only OS lock and is always released at a unit boundary. A foreground op takes its gate guard, then
  the device lock, then releases both — never the reverse, never both held while awaiting the gate.
- **Foreground never blocks on background.** A foreground op never waits on `background_yield_point`; it only raises the
  counter and contends for the device lock, which the scan holds for at most one batch (≤ ~1 s).
- **Background always progresses when idle.** `background_yield_point` returns immediately when
  `foreground_pending == 0`. The decrement-and-notify on the last foreground guard drop guarantees a parked scan is
  woken. A foreground op that starts and ends between the scan's counter-read and its `notified().await` is handled by
  `Notify`'s stored-permit semantics (a `notify_waiters` while no one waits is fine; the next loop iteration re-reads
  the counter, sees zero, and proceeds), so there's no lost-wakeup stall. We re-check the counter after every wake (the
  `while` loop), so a spurious or stale wake just re-parks.
- **No priority inversion.** The scan yields the device at every unit boundary; foreground gets it after the current
  in-flight transaction completes (mtp-rs's `operation_lock` guarantees that transaction is atomic and bounded).
- **Cancellation preserved.** The scan's `cancelled: Arc<AtomicBool>` is checked at every unit boundary (already checked
  per directory; now also between metadata batches), and the same flag is threaded into `mtp-rs` as a `CancelToken` so
  an in-flight `listing.next()` bails within one round trip. Heal-to-rescan, the freshness model, and the
  buffer/replay/overflow flow are untouched.

## What stays where

- **`mtp-rs`**: unchanged. Owns per-transaction serialization (`operation_lock`) and the independent event endpoint.
- **Cmdr `mtp/connection/scheduler.rs`** (new): the `DevicePriorityGate` primitive + per-device map.
- **Cmdr `mtp/connection/directory_ops.rs`**: add `list_directory_for_scan` (per-unit, yielding); existing foreground
  list paths take a foreground guard.
- **Cmdr `mtp/connection/event_loop.rs` + `indexing/mtp_watch.rs`**: the gate-before-resolve reordering and the
  raw-handle buffering for adds/changes.
- **Cmdr `file_system/volume/backends/mtp.rs`**: route the SCAN listing (no progress callback, cancel present, called
  from `volume_scanner`) to `list_directory_for_scan`; keep foreground listings on the existing path.

## Testing

Pure/unit-testable (no hardware):

- **Gate ordering**: a foreground guard raises the count; `background_yield_point` parks; dropping the guard wakes it;
  with zero pending it returns immediately. (`scheduler.rs` tests.)
- **Scan-yields-while-foreground-pending decision**: the pure predicate the scan uses between units.
- **Buffer-before-resolve**: a scanning + non-visible change buffers a raw handle and does NOT resolve; a visible or
  non-scanning change resolves; the replay resolves buffered handles. (`mtp_watch.rs` tests, extending the existing
  buffer tests.)

Needs real-phone QA (full device interaction):

- During a scan, nav / copy / delete start within ~1 s; a file added on the phone in the open folder appears within ~1–2
  s; the scan still completes (slower while the user is active, resuming when idle).
