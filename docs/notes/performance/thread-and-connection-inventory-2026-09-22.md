# Worker threads and SQLite connections: what is per-volume, what is bounded, and the one real duplication (2026-09-22)

**What this settles:** whether Cmdr leaks worker threads and SQLite connections across volume unmount/remount cycles. It
does not. Every named thread type has an owner and a retirement path, and the connection count is bounded twice over.
The duplication that looks like a leak is real but is a different bug: **one SMB share reached at three different
addresses becomes three volumes**, each with its own writer threads, watcher, database, and scan.

It also retires the "132 open SQLite connections × 16 MB page cache" line that `idle-cpu-attribution-2026-08-03.md` and
`idle-memory-profile-2026-07-28.md` left open. Page memory stopped tracking connection count when the shared slab
landed; the measurement below confirms it held in production.

## The measurement

Prod Cmdr, PID **69181**, `/Applications/Cmdr.app`, launched 2026-09-21 20:22:07, sampled 2026-09-22 21:16–21:22 at **24
h 54 m** uptime, macOS 27.0.0 (Darwin). Read-only throughout: two `sample` runs ~3 minutes apart, `ps -M`, `lsof`,
`footprint -s`, and `~/Library/Logs/com.veszelovszki.cmdr/cmdr.log` plus its three rotations.

⚠️ The effort that produced this app's two earlier profiles was misled four times by ranking work off a single `sample`
window (`idle-cpu-attribution-2026-08-03.md`). Counting threads is less exposed to that than attributing CPU, but the
counts below are still **two windows plus a 25-hour history read out of the log**, never one window. The history is what
carries the argument: the sync-status pool's thread NAMES are an incrementing counter, so `cmdr-sync-status-0…4` records
how many were ever spawned in 25 hours, which no instantaneous sample can say.

⚠️ **PID 37718 in the request does not exist and never appears in this log.** The process that matches the described
25-hour run is 69181. The x2 writer counts in that earlier sample are explained below, and they came from a run with
three volumes indexed, not from duplicated workers on one volume.

### Thread inventory, both windows identical

56 threads total (`ps -M`), 119 app SQLite connections, 331 file descriptors.

- `tokio-rt-worker` × 30
- `cmdr-sync-status-0` … `-4` × 5
- `notify-rs debouncer loop` × 2 + `notify-rs fsevents loop` × 2 (2 pairs)
- `index-writer` × 1, `importance-writer` × 1, `operation-log-writer` × 1
- `media-vision`, `nusb-events`, `mDNS_daemon`, `mdns-event-loop`, `agent-wake-loop`, `futures-timer`,
  `importance-mditem-sample` × 1 each
- the AppKit/WebKit set (`com.apple.main-thread`, `NSEventThread`, `CVDisplayLink`, `WebCore: Scrolling`,
  `JavaScriptCore libpas scavenger`, `com.apple.IPC.ReceiveQueue`, `Log work queue`) × 1 each

Exactly **one** volume is indexed in this run (`root`), and there is exactly one `index-writer` and one
`importance-writer`. That is the per-volume contract holding.

## Verdict per thread type

### `index-writer` — legitimate, one per indexed volume

Spawned in `IndexWriter::spawn_for`, `crates/cmdr-index/src/indexing/writer/mod.rs:608-628`, taking a `volume_id` and
its own write connection.

Retired by `IndexWriter::shutdown` (`writer/mod.rs:797-803`): sends `WriteMessage::Shutdown` and **joins** the handle,
warning if the thread panicked. Its caller is `IndexManager::shutdown` (`indexing/lifecycle/manager.rs:725-762`), which
runs a four-step ordered teardown — cancel the scan, stop the watcher, drain the live-event task with a 5 s timeout,
then shut the writer down — so `last_event_id` is durable before the thread goes.

That is reached from `lifecycle/state/teardown.rs:293` (`finish_stopping`) and `:558`, from `state/supervisor.rs:128` on
a fatal storage failure, and from `state/startup.rs:455,460` when a start has to unwind.

### `importance-writer` — legitimate, one per volume with importance scoring

Spawned in `ImportanceWriter::spawn`, `crates/cmdr-index/src/importance/writer.rs:136-152`. Retired by `shutdown` at
`importance/writer.rs:259-263`, which sends `WriteMessage::Shutdown` and joins. Idempotent by construction (the handle
is an `Option` inside a `Mutex`, taken on first call).

### `cmdr-sync-status-N` — legitimate, bounded at 12 by construction, NOT per-volume

A long-lived pool of 8 MB-stack OS threads for synchronous File Provider XPC calls, sized at
`apps/desktop/src-tauri/src/file_system/sync_status/mod.rs:91-96`: `target_workers: 4`, `max_workers: 12`,
`wedged_after: 30s`. Grown lazily in `Pool::submit` → `needs_worker` (`file_system/sync_status/pool.rs:76-90` and the
`needs_worker` body), which refuses to grow past `max_workers` and refuses to grow while any worker is idle.

**Workers never exit** — `pool.rs:59-60` says so in the `State::busy_since` doc, and that is deliberate: an XPC call
into a wedged provider never returns, so a thread can be lost for the process's lifetime. A fixed pool would die
permanently the first time a provider hung; an unbounded pool is the leak being prevented. So the design accepts a
**bounded** leak: at most 12 threads ever, whatever happens (`pool.rs:10-18`).

Five threads after 25 hours is that mechanism working, and the log names the moment:

```
2026-09-22T16:55:20.565+02:00 WARN sync_status  1 of 4 pool threads are still inside a File Provider call that never answered
```

Four target workers, one wedged, one replacement spawned as `cmdr-sync-status-4`. **One wedged thread in 25 hours, 7 of
the 12-thread budget still unspent.** ⚠️ The wedged thread is gone for the life of the process and its 8 MB stack with
it; that is the accepted cost, not a defect.

### `notify-rs debouncer loop` / `fsevents loop` pairs — legitimate, scales with open UI, NOT per-volume

Each `new_debouncer` costs one pair. Five sites create them, all on macOS:

- `apps/desktop/src-tauri/src/file_system/watcher.rs:130` — one per open **listing**, keyed by `listing_id` in
  `WATCHER_MANAGER.watches`, removed and dropped by `stop_watching` (`watcher.rs:265-273`).
- `apps/desktop/src-tauri/src/file_viewer/watcher.rs:197` — the open file viewer.
- `apps/desktop/src-tauri/src/downloads/watcher.rs:313` — the downloads folder.
- `crates/cmdr-archive/src/watch/mod.rs:92` — one per open archive.
- `crates/cmdr-git/src/watcher.rs:92` — one per repository, **refcounted** in `GitWatcherRegistry`, torn down with the
  last subscriber (`cmdr-git/src/watcher.rs:261-274, 348`).

**Gotcha, and it is the one that makes this look like a per-volume leak:** on macOS the index's own volume watcher is
**not** a notify watcher. `DriveWatcher` (macOS) runs the vendored `fsevent-stream` and stops via `handler.abort()` +
`forward_task.abort()` (`crates/cmdr-index/src/indexing/watch/watcher.rs:213-229`, `Drop` at `:258-265`). Only the Linux
`DriveWatcher` uses `notify::RecommendedWatcher` (`watcher.rs:331`). So on macOS a notify pair NEVER corresponds to an
indexed volume, and counting pairs against volume count will mislead every time.

Five pairs in the earlier sample = five concurrently open listings, viewers, archives, and repos. Two pairs now. This
tracks what is on screen.

### `tokio-rt-worker` × 30 — legitimate, but the ceiling is tokio's default rather than a chosen one

`tokio-rt-worker` is tokio's own default thread name (tokio 1.52.0, `runtime/builder.rs:303`), applied to worker threads
and blocking-pool threads alike. The index's fallback runtime is built at
`crates/cmdr-index/src/indexing/host/runtime.rs:70-73` with `Builder::new_multi_thread().enable_all().build()`.

**Nothing in the repo sets `max_blocking_threads` or `worker_threads`.** The blocking ceiling is therefore tokio's
default of **512**, with idle blocking threads retired after tokio's default 10 s keep-alive. 30 live threads is
cores-worth of workers plus whatever blocking work is currently resident. Not a leak, but see the connection section:
this number is what the SQLite connection count is a multiple of, so nobody has chosen the thing that governs it.

## Is there a leak? No — but there is a duplication with the same symptom

### Remounting at the same address: no extra threads

The registry path is hardened against precisely the failure being looked for. `stop_the_volume`
(`crates/cmdr-index/src/indexing/lifecycle/state/teardown.rs:212-283`) takes the instance out **under** the
`INDEX_REGISTRY` lock, publishes `IndexPhase::ShuttingDown`, then **drops the lock before** the up-to-5 s blocking
drain, so a concurrent `get_status` for any volume is not stalled behind it. A start that lands inside that window is
recorded as `ShuttingDown { restart }` and handed back by `retire_the_instance` (`teardown.rs:305-317`), which frees the
slot and reads the request **as one step** — the comment there is explicit that a bare `remove` is wrong because nothing
else watches for the moment the slot becomes free.

The `Initializing` branch (`teardown.rs:243-253`) cancels the in-flight start's root stop signal first, and says why:

> Without it, a fresh start that reserves the freed slot in the window gets its instance overwritten by the old start's
> manager — **two writer threads on one database**, which is exactly what the reservation exists to prevent.

So the "new worker spawned alongside the old one" hypothesis is already a named, defended-against invariant for a given
`volume_id`. Nothing in 25 hours of log contradicts it: every `IndexManager: shut down for volume 'X'` line is followed
by a clean `Indexing stopped for 'X'`, and no volume id appears as `start_indexing: done` twice without an intervening
shutdown **within one process**.

### The real bug: one SMB share reached at three addresses becomes three volumes

`smb_volume_id(server, port, share)` keys a volume on the **server address as the mount records it**
(`apps/desktop/src-tauri/src/network/smb_upgrade.rs:32-38, 51-60`). `identity_from_statfs` canonicalizes by reading
`statfs`, and its doc comment already names this failure class:

> The two travel together because they come from the same `statfs` row and are answers to the same question. Deriving
> them apart is how a mount ends up keyed as one share and addressed as another. (`smb_upgrade.rs:26-29`)

⚠️ **That canonicalization makes the two call sites agree with each other. It does not collapse three addresses onto one
share.** Mounting one NAS share over a VPN, over the LAN, and via its mDNS service name yields three `info.server`
values, hence three volume ids, hence three of everything.

On disk right now (`~/Library/Application Support/com.veszelovszki.cmdr/`):

- `importance-smb-<vpn-ip>-445-<share>-<hash>.db`: 34 MB, the VPN address
- `importance-smb-<lan-ip>-445-<share>-<hash>.db`: 20 MB, the LAN address
- `importance-smb-<nas-name>-smb-tcp-local-4-<hash>.db`: 20 KB, the mDNS service name

Three databases, one NAS share. **Two of them ran concurrently in a single process**, which is the part that settles it
— from `cmdr.log.1`, all on 2026-09-19 with no relaunch marker and no shutdown between them:

```
09:38:38.811  start_indexing: done, 'root' IndexManager is Running
09:57:01.471  start_indexing: done, 'smb-<nas-name>-smb-tcp-local-4-<hash>' IndexManager is Running
09:57:46.626  start_indexing: done, 'smb-<vpn-ip>-445-<share>-<hash>' IndexManager is Running
```

Three volumes live at once, 45 seconds apart, two of them the same physical share. **That is what produced the
`index-writer` × 2 and `importance-writer` × 2 in the earlier sample**: not duplicated workers on one volume, but one
worker each on volumes that should have been one volume.

### Cost per alias

Per extra identity, for as long as it is indexed:

- 1 `index-writer` thread + 1 `importance-writer` thread
- 1 FSEvents stream over the share
- 2 write connections, each contributing its full `WRITE_PAGE_CACHE_KIB` (16 MiB) to `Σ nMax` against a 63 MiB slab —
  see below, this is the one term that still costs real memory
- a duplicate full scan of the share **over the network**, and a duplicate DB on disk

**Per remount cycle:** remounting at the same address costs **0** extra threads. Remounting at a _different_ address
costs **+2 threads**, +1 FSEvents stream, +2 write connections, and a fresh full network scan, persisting until the app
restarts.

### What the importance databases showed (both halves since fixed)

In this run `lsof` showed 8 and 6 open connections on the two SMB **importance** databases while no SMB volume was
indexed at all (the only `periodic full refresh` lines after 2026-09-21T20:22:08 are 24 × `'root'`). Those were
thread-local read connections from the read path, held by tokio blocking threads that had touched the share earlier, and
not a running scheduler. And no `index-smb-*.db` existed on disk any more, while all three `importance-smb-*.db` (34 MB,
20 MB, 20 KB) survived.

Both had one cause each, and neither is how the code behaves now (GitHub issue #327):

- **The files.** Every path that removed an index database unlinked `index-{id}.db` and stopped: forgetting a drive,
  "Clear index", and the retention cap's eviction. Removal is now one door that takes a volume's importance database
  with its index (`crates/cmdr-index/DETAILS.md` § "A volume's files, and the one door they leave by").
- **The connections.** Nothing asked a thread to let go of a connection it had cached to a database that stopped or went
  away. A stop or a removal now retires them, and each thread closes its own at its next read
  (`crates/cmdr-fs/DETAILS.md` § "Retiring cached read connections"). ⚠️ That bound is the honest one: a thread that
  never reads again keeps its connections until it exits.

The three files this run found are still on that machine: nothing sweeps a leftover automatically, and "Clear index" in
settings is what reclaims one. Why there's no sweep is in the first doc above.

## SQLite connections: 119, bounded twice, and the 16 MB figure is stale

**Count today**, `lsof -p 69181` on main `.db` files (each connection holds exactly one fd on the main file;
`EnhancedSecuritySites.db` is WebKit's and excluded):

- 79 × `index-root.db`
- 22 × `importance-root.db`
- 8 × `importance-smb-<vpn-ip>-…db`, 6 × `importance-smb-<lan-ip>-…db`
- 2 × `operation-log.db`, 2 × `main.db`
- **119 total** (was 132 on 2026-08-03, 156 on 2026-07-28)

**What governs it.** Read connections are thread-local and live as long as their thread, and each thread keeps a
three-slot LRU: `THREAD_CONN_SLOTS = 3` (`crates/cmdr-fs/src/sqlite_util.rs:562`, `ThreadConnCache` at `:552-558`). So
the ceiling is `live tokio blocking threads × 3`. At 56 threads that is ~168, and 119 sits under it. The sizing
assumption is `READ_CONNECTION_BUDGET = 256` (`sqlite_util.rs:407`), which the live count respects with room over.

So it is bounded, twice: by `threads × 3`, and by tokio's own default 512-thread blocking ceiling. ⚠️ Neither bound was
chosen for this purpose — the second is a library default nothing in the repo overrides.

### The RAM cost is not 16 MB per connection any more

**Verified rather than trusted, and the repo's own doc is the thing that was stale.** `PRAGMA cache_size = -16384` (16
MiB) is still there as `WRITE_PAGE_CACHE_KIB = 16_384` (`sqlite_util.rs:359`), and `READ_PAGE_CACHE_KIB = 128` (`:392`).
But since the shared-slab work, **those are per-connection ceilings on retained pages, not per-connection allocations**.
Every cached page in the process is served from ONE slab handed to SQLite via
`sqlite3_config(SQLITE_CONFIG_PAGECACHE, …)` before the first connection opens (`sqlite_util.rs:27-60`,
`install_shared_page_cache` at `:116-155`). Total page memory is that one number regardless of connection count.

Confirmed installed in this very run (`cmdr.log`, three launches on 2026-09-21, the last one this process):

```
2026-09-21T20:22:08.346+02:00 DEBUG sqlite  shared page cache installed: 15363 slots x 4368 B = 63 MiB
```

`footprint -s 69181` at 24 h 54 m corroborates it from the outside, against the 2026-07-28 profile at ~10 h:

|                           | 2026-07-28 (v0.36.2) | now                            |
| ------------------------- | -------------------- | ------------------------------ |
| Physical footprint        | 2.5 GB               | **696 MB**                     |
| Malloc Small              | 405 MB               | **65 MB**                      |
| MALLOC_LARGE              | 730 MB               | **absent from the categories** |
| IOAccelerator (Rust heap) | 1386 MB              | 598 MB (+40 MB reclaimable)    |

So: **marginal RAM per additional READ connection is the rusqlite handle plus its prepared-statement cache, and zero
extra page memory.** 119 connections are not 119 × 16 MB, and never were since the slab landed.

**The term that does still cost.** `pGroup->nMaxPage` is `Σ cache_size` over every OPEN connection, and a **write**
connection contributes its full 16 MiB whether or not it is doing anything. `sqlite_util.rs:41-55` records the
measurement: nine idle write connections held 63 of the 64 MiB on their own (2026-08-22), while 132 read connections
scanning continuously held 17 MiB. A slab smaller than `Σ nMax` is not a cap but a treadmill — `pcache1`'s
under-pressure flag latches on and every fetch recycles from the global LRU under the `PGroup` mutex.

**Which is exactly why the SMB aliasing matters beyond thread count:** each duplicate identity adds two write
connections, 32 MiB of `Σ nMax`, against a slab already documented as over-subscribed by the real write population. The
write term is a design target the running app does not meet (`crates/cmdr-index/src/indexing/store/DETAILS.md` § "The
writer term is a design target, not an invariant").

## Recommendation

The current plan for both items below (an alias-adoption layer for the SMB identity, and no tokio cap, which
`allocator-comparison-2026-09-23.md` measured to change nothing) is in `README.md` § "Open follow-ups".

**Canonicalize SMB volume identity so one share is one volume however it is addressed.** Resolve `(server, port, share)`
to a stable server identity before it reaches `smb_volume_id` — the SMB server GUID from the negotiate response, or the
share's own `volume serial`, with the address kept only as a way to reach it. `cmdr-smb` already speaks the protocol, so
the identifier is available without a new round trip.

Size: medium. The id function and its call sites are small and well factored (`smb_upgrade.rs:57`, `:355`,
`file_system/index_provider.rs:202`, `crates/cmdr-smb/src/volume/mod.rs:490`), and `identity_from_statfs` is already the
single canonicalization seam, so the change lands in one place. The real work is migration: three existing DBs per
aliased share have to collapse onto one, or be dropped and re-scanned.

**Tradeoff, not a clear win.** Getting it wrong in the other direction is worse than the current state: two genuinely
different shares that collide onto one id would interleave in one index. Two shares on one server, or a server whose
GUID is unstable across reboots, both need answers first. The current failure costs duplicate work and duplicate disk;
the failure mode of a bad fix corrupts a user's index. Worth a spec before code.

**Cheaper, additive mitigations, either of which stands alone:**

1. **Cap tokio's blocking pool explicitly** rather than inheriting 512. It bounds the connection count at something
   chosen, and it is a one-line `max_blocking_threads` on the runtime builders. Clear win, small.
2. **Detect the alias without renaming anything:** when a new SMB volume's `(port, share)` matches an existing indexed
   volume's and the server resolves to the same host, log it loudly and offer to adopt the existing index. Clear win as
   diagnostics, and it makes the problem visible in user reports before the identity work ships.

❌ **Do not "fix" the sync-status pool, the notify pairs, or the connection count.** All three are bounded by
construction, and the numbers observed are what the designs predict.

## What would settle what is still open

- **Whether the aliasing is still reachable in the current build.** It is proven for 2026-09-19. Reproduce by mounting
  the same share twice, once by IP and once by mDNS name, and checking whether two `start_indexing: done` lines appear.
  That is the one test that says whether this needs fixing now or is already narrowed by other work.
- **Thread count growth over a run.** ⚠️ Nothing here measures it directly: 56 threads at 25 h against 69-71 in the
  2026-08-03 note is suggestive but the two runs had different volume counts and different UI state, so it is not a
  comparison. The honest instrument is a periodic `ps -M | wc -l` logged hourly, which the app could emit itself
  alongside the memory diagnostics it already has.
