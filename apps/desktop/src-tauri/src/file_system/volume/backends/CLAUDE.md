# Volume backends

Per-backend `Volume` implementations. The trait shape, capability matrix, streaming patterns, and "Building a new
volume" checklist live in the parent [`volume/CLAUDE.md`](../CLAUDE.md). This file is the per-backend decisions /
gotchas reference: when you're modifying `SmbVolume`, `MtpVolume`, `LocalPosixVolume`, the SMB watcher, or
`InMemoryVolume`, read here.

## Key files

| File | Role |
|---|---|
| `local_posix.rs` | `LocalPosixVolume`: real filesystem; delegates listing to `file_system::listing`, indexing to `indexing::scanner`, watching to `indexing::watcher` (FSEvents), copy scanning via `walkdir`. Uses `libc::statvfs` FFI for space info. |
| `mtp.rs` | `MtpVolume`: MTP device storage; async `Volume` trait with direct async MTP calls. Uses `MtpReadStream` for streaming (calls `FileDownload::next_chunk().await` directly). Gated with `#[cfg(any(target_os = "macos", target_os = "linux"))]`. |
| `smb.rs` | `SmbVolume`: SMB share storage; async `Volume` trait with direct async smb2 calls. Splits session storage into `Arc<Mutex<Option<SmbClient>>>` + `Arc<RwLock<Option<Arc<Tree>>>>` so the hot read/write paths can clone `Connection` under a brief lock and drive compound / download ops without serializing on the client mutex. `AtomicU8` connection state. Caches `SmbConnectionParams` (host, share, port, credentials) so `attempt_reconnect` can rebuild the session in place after a transient disconnect, single-flighted via `reconnect_lock`. Holds a global `AppHandle` (`set_app_handle` in `lib.rs::setup`) for emitting `smb-connection-changed` events. Also contains `connect_smb_volume()`. Gated with `#[cfg(any(target_os = "macos", target_os = "linux"))]`. |
| `smb_watcher.rs` | Background SMB change watcher (`run_smb_watcher`). Owns a dedicated smb2 session (separate TCP connection from the volume's primary client) and uses smb2 0.10's `'static` `Watcher` with pipelined CHANGE_NOTIFY (one request kept pre-issued on the wire so events arriving during consumer processing don't fall in a re-arm gap). Debounces events, feeds `notify_directory_changed`. Spawned by `connect_smb_volume()` and respawned by `attempt_reconnect`. No internal reconnect — bails on `next_events` errors and lets `attempt_reconnect` handle session recovery. |
| `in_memory.rs` | `InMemoryVolume`: `RwLock<HashMap>` store for tests; also used for stress tests (`with_file_count`) |

## SMB auto-upgrade lifecycle

SMB mounts are automatically upgraded to `SmbVolume` (direct smb2 connection) in two scenarios:

1. **Startup** (`file_system::upgrade_existing_smb_mounts(app_handle)`): Scans registered volumes for `smbfs` type. If
   any are found, calls `network::ensure_mdns_started` to kick off mDNS itself (creds are keyed by hostname, not IP),
   then waits for mDNS to reach `Active` state (polls every 500ms, up to 15s). Uses `tauri::async_runtime::spawn` (not
   `tokio::spawn`; runs during `setup()` before Tokio is fully available). Emits `volumes-changed` after upgrades so
   the frontend refreshes indicators. **No `firstTriggerDone` gate**: the function is a no-op when no SMB mounts are
   present (no network activity, no macOS Local Network prompt). When mounts are present AND `network.directSmbConnection`
   is on (default `true`), it kicks off mDNS — that's when the macOS prompt fires, once per app per data dir. Without
   this, dev profiles with auto-reconnected SMB shares would stay on the slow OS-mount path forever.

2. **Mount detection** (`volumes/watcher.rs::try_upgrade_smb_mount`): When FSEvents detects a new volume in `/Volumes/`
   and it's `smbfs`, spawns a background upgrade attempt. Calls `ensure_mdns_started` to kick off mDNS too.

Both paths check the `network.directSmbConnection` setting (global `AtomicBool`). Both are best-effort. Failures log a
warning and the volume stays as `LocalPosixVolume`. The "Connect directly" UI action (`upgrade_to_smb_volume` command)
and the MCP `upgrade_smb_to_direct` tool provide manual upgrade paths.

## SMB live-reconnect lifecycle

When a hot-path op hits `ConnectionLost` / `SessionExpired`, `handle_smb_result` flips state to `Disconnected` and
`transition_to_disconnected` emits `smb-connection-changed { volumeId, state: "disconnected" }`. The frontend reconnect
manager listens for this event and runs a per-volume backoff cycle (timer-driven, calling the
`reconnect_smb_volume(volumeId)` Tauri command on each tick).

`SmbVolume::do_attempt_reconnect` is the single source of truth for re-establishing the session:

1. Acquires `reconnect_lock` (single-flight: concurrent FE-cycle and lazy-nav callers wait here).
2. If state is already `Direct`, returns Ok cheaply.
3. Tries `build_session()` with the cached `SmbConnectionParams` (the credentials that worked at original connect).
4. If that fails with an auth error, calls `refresh_credentials_from_store` (which re-reads from `keychain::get_credentials`) and retries once with the fresh creds. On success, the new credentials replace the cached ones via `params.write()`.
5. On success: installs the new client + tree, restarts the watcher with `spawn_watcher` (the prior watcher is cancelled via `stop_watcher` first), then `transition_to_direct` flips state and emits `smb-connection-changed { state: "direct" }`. Doing the state flip last means observers wake up to a fully-installed session.
6. On failure: state stays `Disconnected`. The FE backoff cycle decides whether to retry.

Credentials are kept in memory for the lifetime of the `SmbVolume` (no security concern: they're already in the
process's address space for every smb2 call). Only re-pulled from the secret store on auth failure, in case the user
updated them.

## Per-backend decisions

**Decision**: `SmbVolume` and `MtpVolume` store `volume_id: String` for listing cache lookups
**Why**: `notify_mutation` needs to call `notify_directory_changed(volume_id, ...)` to find the right cached listings. The volume_id is computed at creation time (`smb_volume_id(server, port, share)` for SMB so two same-named shares on different servers don't collide — see `volumes/CLAUDE.md` § "Volume IDs"; `"{device_id}:{storage_id}"` for MTP) and stored on the struct rather than recomputed on every mutation.

**Decision**: `SmbVolume::supports_local_fs_access()` returns `false`
**Why**: `SmbVolume` handles listing updates via `notify_mutation` using its own smb2 `get_metadata`. A `std::fs`-based synthetic diff path (`emit_synthetic_entry_diff`) would be redundant and would go through the slow OS mount. Returning `false` skips it.

**Decision**: `SmbVolume` splits session storage: `Arc<Mutex<Option<SmbClient>>>` + `Arc<RwLock<Option<Arc<Tree>>>>`
**Why**: Keeping the session in one `Mutex<Option<(SmbClient, Tree)>>` would force the streaming-read producer and the compound read/write fast-paths to hold the mutex for the entire transfer, serializing every concurrent copy through it. `smb2::Connection` is `Clone` (cheap `Arc::clone`, all clones multiplex frames over one SMB session), so splitting the Tree out lets us briefly lock the client, clone its `Connection`, and release the lock, then drive `Tree::download` / `Tree::read_file_compound` / `Tree::write_file_compound` on the cloned `Connection` with no lock held. N concurrent copies on one `SmbVolume` pipeline N operations over the single session instead of queuing on the mutex. Tree lives in a `RwLock` because we only take read locks in the hot path (cloning an `Arc<Tree>`) and only write on disconnect. The streaming-write path uses the same clone-and-release shape (see the `write_from_stream` Decision below), so the client mutex is never held across I/O.

**Decision**: `SmbVolume::local_path()` returns `None`
**Why**: `local_path()` is checked in `volume_copy.rs` to decide whether to use native OS copy APIs. If SmbVolume returned `Some(mount_path)`, copies would go through the slow OS mount, which is exactly what we're trying to avoid. `root()` still returns the mount path for frontend path resolution.

**Decision**: SmbVolume background watcher runs on a dedicated smb2 session, not a clone of the volume's main connection
**Why**: smb2 0.10 made `Watcher` `'static` (owns a `Connection` clone), so technically the watcher could share the volume's session via `clone_session`. Empirically it can't: stacking the watcher's CHANGE_NOTIFY long-polls on the same TCP session as heavy concurrent writes wedges Samba — `smb_integration_concurrent_streaming_writes_no_deadlock` hangs against `smb-consumer-maxreadsize` (64 KB max read/write, 8 concurrent writers, 200 × 1 MB files). The dedicated session keeps the watcher's traffic out of the writers' way at the cost of a separate TCP+auth. What we *do* keep from the new API: the watcher is `'static` (no borrow on the watcher task's `client`), and the pipelining (one CHANGE_NOTIFY pre-issued so events during consumer processing don't fall in a re-arm gap). Stat calls for new/modified files still go through `VolumeManager::get(volume_id).get_metadata(...)` (the main session), so the cmdr-side `notify_mutation` cache patch from our own writes lands first regardless.

**Decision**: Watcher task is not stored on `SmbVolume`, only the cancel sender is
**Why**: The spawned task owns its own `Watcher` and `SmbClient`. Storing them on the struct alongside the cancel sender would just duplicate ownership without buying anything — `watcher.next_events()` is `&mut self`, so the task is the only thing that can drive it anyway. The `watcher_cancel: Mutex<Option<oneshot::Sender<()>>>` on the struct provides clean shutdown.

**Decision**: Watcher doesn't reconnect itself; it bails on connection errors
**Why**: When `next_events` errors with anything but `NOTIFY_ENUM_DIR`, the watcher's task returns. The next hot-path op on the volume hits the dead main session, `handle_smb_result` flips to `Disconnected`, the FE backoff cycle calls `attempt_reconnect`, which respawns the watcher (with a fresh dedicated session). Don't give the watcher its own reconnect-with-backoff loop: two state machines tracking the same "is the session alive" question is a recipe for divergence — the watcher's internal retries swallow real disconnections the FE reconnect manager would have surfaced. One reconnect path, one source of truth. The watcher's session being separate from the main session means a watcher-only failure (e.g., a TCP hiccup on the watcher's connection) doesn't surface as a volume disconnect until the next mutation; that's the trade-off for keeping the connections independent.

**Decision**: Watcher debounces 200ms per batch, `FullRefresh` above 50 events per directory
**Why**: Prevents 1000 individual stat calls when 1000 files are copied. The 200ms window collects events that arrive in rapid succession. The 50-event threshold for `FullRefresh` avoids O(n) stat calls for bulk operations.

**Decision**: `write_from_stream` uses a cloned `Connection` + `Arc<Tree>` (owned `FileWriter`)
**Why**: `FileWriter` owns its `Connection` (cheap `Arc::clone`) and `Arc<Tree>` rather than borrowing `&'a mut Connection`. `write_from_stream` calls `clone_session` once up front and drives both the compound fast-path AND the streaming fallback on the same owned `Connection` clone. The client mutex is held only for the few microseconds of `clone_session()`, never across I/O. **Don't switch back to a borrowed `FileWriter<'a>` that holds the client mutex across the upload**: that shape deadlocks under sustained concurrent pressure (the two-phase brief-clone-then-long-hold pattern is the QNAP deadlock reproducer). The regression is pinned by `smb_integration_concurrent_streaming_writes_no_deadlock`. The architectural property we get from owned `FileWriter`: N concurrent streaming writes on one `SmbVolume` pipeline N WRITE chains over a single SMB session, multiplexed by `MessageId` in smb2's receiver task. No external locking, no mutex contention on the hot copy path.

**Decision**: `SmbVolume` overrides `scan_for_copy_batch` to pipeline per-path stats over a single SMB session
**Why**: A naive scan phase that loops `scan_for_copy` per top-level source costs N sequential RTTs before the copy phase can start. For a 100-file copy over a ~60 ms Tailscale link that's ~5 s of serial stats. The override clones `smb2::Connection` per path under a brief client-mutex acquire (cheap `Arc::clone`, all clones multiplex over the same SMB session), releases the lock, then drives `tree.stat(&mut conn, path)` on each clone inside a `FuturesUnordered`. Empty root paths skip the stat. Single-path batches fall through to `scan_recursive` so one-file drag-drops don't pay the batch machinery cost. Directories found during the stat phase recurse sequentially afterward; parallel directory recursion is a future enhancement. Measured 6.5× wall-clock win at 100 × 10 KB: 6.11 s → 947 ms. See `docs/notes/phase4-rtt-investigation.md` for the wire trace. **Oracle layered on top**: before the pipelined-stat block runs, every input path's parent is checked against the fresh-listing oracle (`try_get_watched_listing(volume_id, parent)`). Oracle-served paths get their size + `is_directory` from the cached `FileEntry` and are removed from the leftover set; only the leftover paths go through the pipelined stat. Decision is per-parent: one batch can mix oracle-served and pipelined-stat paths, and if every path resolves via the oracle the stat pipeline is skipped entirely.

**Decision**: `MtpVolume` overrides `scan_for_copy_batch_with_progress` to group selected paths by parent and list each parent once
**Why**: MTP has no single-file stat call: `get_metadata(path)` lists the parent directory and searches by name. A naive scan that called `get_metadata` per path would re-list `/DCIM/Camera` (15k entries, ~17 s over USB) for every selected photo. The override groups the input paths by parent, calls `list_directory(parent, on_progress)` once per unique parent, and indexes the entries by name for O(1) lookups. **Oracle layered on top**: before listing a parent, the override consults `try_get_watched_listing(volume_id, parent)`; on hit, the cached entries replace the listing call entirely (no USB I/O for that parent). On miss the single-listing-per-parent path runs, so cold-cache perf is preserved. Decision is per-parent; one batch can mix watcher-fresh and cold parents.

**Decision**: `SmbVolume` has a compound fast-path in `open_read_stream_with_hint` and `write_from_stream` for files ≤ `max_read_size` / `max_write_size`
**Why**: The streaming open+read+close sequence costs 3 RTTs per file. For small files (typical 10 KB copies on a NAS) that dominates wall-clock at high-latency links (~60 ms RTT → ~180 ms/file just for protocol overhead, not data). `smb2` already exposes `Tree::read_file_compound` (CREATE+READ+CLOSE in a single compound frame = 1 RTT) and `Tree::write_file_compound` (CREATE+WRITE+FLUSH+CLOSE = 1 RTT). The copy pipeline feeds per-file size hints from the pre-copy scan; when the size is known and fits in one READ/WRITE, we take the compound path. Falls back cleanly to the streaming reader/writer when the hint is missing or the file is too big. Small compound reads return a `Vec<u8>` wrapped as a single-chunk `InlineReadStream` so the consumer API stays shaped the same. See `docs/notes/phase4-rtt-investigation.md` for the measurement.

## Gotchas

**Gotcha**: `MtpReadStream::Drop` spawns a detached cancel task
**Why**: When a download is cancelled mid-stream (user presses Cancel during MTP copy), the `MtpReadStream` is dropped
before the `FileDownload` is fully consumed. mtp-rs's `ReceiveStream` panics on drop if not consumed or cancelled
(to prevent USB session corruption). The `Drop` impl calls `download.cancel(DEFAULT_CANCEL_TIMEOUT).await` on a
spawned detached task. This is safe because the stream always lives in an async context (tokio worker thread), so
`Handle::try_current()` succeeds. The detached task runs independently; the drop returns immediately.

**Gotcha**: `MtpVolume::get_metadata` is expensive: it lists the entire parent directory
**Why**: MTP has no single-file stat call. `get_metadata` lists the parent directory and searches for the entry by name. This is used by `notify_mutation` after each self-mutation (create, delete, rename) and is acceptable because those are infrequent, but avoid calling it in hot paths.

**Gotcha**: Watcher filenames from SMB use backslashes; must normalize to forward slashes
**Why**: SMB servers send paths like `papers\new-file.txt`. The watcher normalizes these to `papers/new-file.txt` before extracting parent directories and constructing display paths.

**Gotcha**: Watcher filenames are NFC (from server) but macOS mount paths are NFD
**Why**: SMB servers return NFC-normalized filenames. macOS filesystem paths use NFD. The watcher NFD-normalizes filenames before constructing display paths used for cache lookups.

## Testing

- `in_memory_test.rs`: unit tests for `InMemoryVolume` (CRUD, sorting, concurrency, stress 50k entries)
- `local_posix_test.rs`: real-FS tests (write ops, symlinks, copy, space info) using `std::env::temp_dir()`
- `mtp.rs` inline tests: path conversion and capability flags (no device needed)
- `smb.rs` inline tests: type mapping (DirectoryEntry→FileEntry, FsInfo→SpaceInfo, Error→VolumeError), connection state transitions, path conversion, capability flags (no server needed)
- **Docker SMB integration tests**: `smb.rs` contains `#[ignore]` tests that require Docker SMB containers (start with
  `apps/desktop/test/smb-servers/start.sh`). Connect via `smb2::testing::guest_port()` (10480, guest/no-auth),
  `auth_port()` (10481, `testuser`/`testpass`), `readonly_port()` (10488), `slow_port()` (10493, 200ms latency). Use
  these for testing real SMB protocol behavior (streaming, error paths, network edge cases). See
  `apps/desktop/test/smb-servers/README.md` for the full container list and env var overrides.
- **SMB soak test** (`smb_soak_copy_loop` in `smb.rs`): Repeats the SMB→Local copy pipeline for hundreds to thousands
  of iterations and watches RSS, open FDs, SMB credits, and per-iteration wall-clock drift. Catches accumulating bugs
  the single-shot integration tests can't see (credit leak, FD leak, memory growth, slowdown). Default mode:
  `CMDR_SOAK_ITERATIONS=100` (≈5 s against Docker). Long mode: `CMDR_SOAK_DURATION_SECS=1800` (30 min, via
  `./scripts/soak-smb.sh`). CI has a `workflow_dispatch`-only job in `slow-checks.yml`.
