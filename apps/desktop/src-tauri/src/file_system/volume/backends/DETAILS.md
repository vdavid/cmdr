# Volume backends details

Pull-tier docs for `file_system/volume/backends/`: per-backend architecture, lifecycle flows, and decision rationale.
Must-know invariants and gotchas live in [CLAUDE.md](CLAUDE.md). The trait shape, capability matrix, streaming
patterns, and "Building a new volume" checklist live in the parent [`volume/DETAILS.md`](../DETAILS.md). When you're
modifying `SmbVolume`, `MtpVolume`, `LocalPosixVolume`, the SMB watcher, or `InMemoryVolume`, read here.

## Key files

- **`local_posix.rs`**: `LocalPosixVolume`: real filesystem; delegates listing to `file_system::listing`, indexing to `indexing::scanner`, watching to `indexing::watcher` (FSEvents), copy scanning via `walkdir`. Uses `libc::statvfs` FFI for space info.
- **`mtp.rs`**: `MtpVolume`: MTP device storage; async `Volume` trait with direct async MTP calls. Uses `MtpReadStream`, which reads in bounded `GetPartialObject64` windows over a cached `MtpReadSession` (mtp-rs `WindowedDownload`; the window/offset bookkeeping lives in mtp-rs, the per-window device lock in `mtp/connection`). Gated with `#[cfg(any(target_os = "macos", target_os = "linux"))]`.
- **`smb.rs`**: `SmbVolume`: SMB share storage; async `Volume` trait with direct async smb2 calls. Splits session storage into `Arc<Mutex<Option<SmbClient>>>` + `Arc<RwLock<Option<Arc<Tree>>>>` so the hot read/write paths can clone `Connection` under a brief lock and drive compound / download ops without serializing on the client mutex. `AtomicU8` connection state. Caches `SmbConnectionParams` (host, share, port, credentials) so `attempt_reconnect` can rebuild the session in place after a transient disconnect, single-flighted via `reconnect_lock`. Holds a global `AppHandle` (`set_app_handle` in `lib.rs::setup`) for emitting `smb-connection-changed` events (the typed `tauri_specta::Event` struct `SmbConnectionChanged` lives in the always-compiled `network/mod.rs`, not here, so `collect_events!` in `ipc.rs` can reference it on every platform; `emit_state_change` just builds and `.emit()`s it). Also contains `connect_smb_volume()`. Gated with `#[cfg(any(target_os = "macos", target_os = "linux"))]`.
- **`smb_watcher.rs`**: Background SMB change watcher (`run_smb_watcher`). Owns a dedicated smb2 session (separate TCP connection from the volume's primary client) and uses smb2 0.10's `'static` `Watcher` with pipelined CHANGE_NOTIFY (one request kept pre-issued on the wire so events arriving during consumer processing don't fall in a re-arm gap). Debounces events, feeds `notify_directory_changed`. Spawned by `connect_smb_volume()` and respawned by `attempt_reconnect`. No internal reconnect — bails on `next_events` errors and lets `attempt_reconnect` handle session recovery.
- **`in_memory.rs`**: `InMemoryVolume`: `RwLock<HashMap>` store for tests; also used for stress tests (`with_file_count`)

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
6. On failure: state stays `Disconnected`. The FE backoff cycle decides whether to retry. **Auth give-up is special**: when the failure is an auth error and the refreshed store creds also fail (or there are none), `do_attempt_reconnect` emits `smb-connection-changed { state: "needs_auth" }` before returning Err. `"needs_auth"` is a transient FE-only signal (not a `ConnectionState` variant — the backend state stays binary Direct/Disconnected); the reconnect manager flips to `needs-auth`, stops the futile backoff, and FilePane shows a "Sign in" prompt (`SmbReauthView`) instead of the generic "unreachable" banner. The user signs in via `Volume::reconnect_with_credentials` (Tauri `reconnect_smb_volume_with_credentials`), which persists the new password server-level (so the next reconnect is silent), updates the in-memory params, and runs `do_attempt_reconnect`. If the new creds are also wrong, it re-emits `needs_auth` — a bad retry re-prompts rather than dead-ending.

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

**Decision**: `write_from_stream` ERROR paths delete the partial file, mirroring the cancel branch
**Why**: Once the streaming `FileWriter` is open and bytes have streamed into it, an early error (mid-stream source-read error, `write_chunk` failure, `finish` failure, the compound-fallback writer's `write_chunk`/`finish`) would otherwise leave a half-written file at the user's intended destination name — corrupt bytes presented as a real file (violates AGENTS.md principle #4). The cancel branch already cleaned up (`writer.abort()` + best-effort `delete_file` on a fresh cloned session); every owned-writer error site now does the same. **`abort()` before delete is load-bearing**: dropping a `FileWriter` without `finish()`/`abort()` leaks the SMB handle (smb2's `FileWriter::Drop` only logs, never sends CLOSE), so a fresh-session `delete_file` (CREATE-with-delete-on-close) hits a sharing violation against the still-open handle and the partial lingers. So: `write_chunk`/source-read errors `writer.abort().await` first (writer still owned), then `delete_partial()`. `finish()` consumes the writer, so on its failure the handle is already gone — best-effort `delete_partial()` only. The compound FAST-path (`write_file_compound`) is atomic CREATE+WRITE+FLUSH+CLOSE and the compound DRAIN loop buffers in memory before any handle opens, so neither leaves a streamed partial — those propagate their error unchanged. The original error always propagates (never `Cancelled`); cleanup is best-effort and never masks it. Pinned by `smb_integration_write_from_stream_source_error_deletes_partial` (source errors after the first chunk; asserts the propagated `IoError` and that no file remains at the destination). Don't refactor the owned-writer error sites into a post-block catch-all that loses the writer — you'd lose the `abort()` and the delete would no-op against the leaked handle.

**Decision**: `SmbVolume` overrides `scan_for_copy_batch` to pipeline per-path stats over a single SMB session
**Why**: A naive scan phase that loops `scan_for_copy` per top-level source costs N sequential RTTs before the copy phase can start. For a 100-file copy over a ~60 ms Tailscale link that's ~5 s of serial stats. The override clones `smb2::Connection` per path under a brief client-mutex acquire (cheap `Arc::clone`, all clones multiplex over the same SMB session), releases the lock, then drives `tree.stat(&mut conn, path)` on each clone inside a `FuturesUnordered`. Empty root paths skip the stat. Single-path batches fall through to `scan_recursive` so one-file drag-drops don't pay the batch machinery cost. Directories found during the stat phase recurse sequentially afterward; parallel directory recursion is a future enhancement. Measured 6.5× wall-clock win at 100 × 10 KB: 6.11 s → 947 ms. See `docs/notes/phase4-rtt-investigation.md` for the wire trace. **Oracle layered on top**: before the pipelined-stat block runs, every input path's parent is checked against the fresh-listing oracle (`try_get_watched_listing(volume_id, parent)`). Oracle-served paths get their size + `is_directory` from the cached `FileEntry` and are removed from the leftover set; only the leftover paths go through the pipelined stat. Decision is per-parent: one batch can mix oracle-served and pipelined-stat paths, and if every path resolves via the oracle the stat pipeline is skipped entirely.

**Decision**: `MtpVolume` overrides `scan_for_copy_batch_with_progress` to group selected paths by parent and list each parent once
**Why**: MTP has no single-file stat call: `get_metadata(path)` lists the parent directory and searches by name. A naive scan that called `get_metadata` per path would re-list `/DCIM/Camera` (15k entries, ~17 s over USB) for every selected photo. The override groups the input paths by parent, calls `list_directory(parent, on_progress)` once per unique parent, and indexes the entries by name for O(1) lookups. **Oracle layered on top**: before listing a parent, the override consults `try_get_watched_listing(volume_id, parent)`; on hit, the cached entries replace the listing call entirely (no USB I/O for that parent). On miss the single-listing-per-parent path runs, so cold-cache perf is preserved. Decision is per-parent; one batch can mix watcher-fresh and cold parents.

**Decision**: `SmbVolume` has a compound fast-path in `open_read_stream_with_hint` and `write_from_stream` for files ≤ `max_read_size` / `max_write_size`
**Why**: The streaming open+read+close sequence costs 3 RTTs per file. For small files (typical 10 KB copies on a NAS) that dominates wall-clock at high-latency links (~60 ms RTT → ~180 ms/file just for protocol overhead, not data). `smb2` already exposes `Tree::read_file_compound` (CREATE+READ+CLOSE in a single compound frame = 1 RTT) and `Tree::write_file_compound` (CREATE+WRITE+FLUSH+CLOSE = 1 RTT). The copy pipeline feeds per-file size hints from the pre-copy scan; when the size is known and fits in one READ/WRITE, we take the compound path. Falls back cleanly to the streaming reader/writer when the hint is missing or the file is too big. Small compound reads return a `Vec<u8>` wrapped as a single-chunk `InlineReadStream` so the consumer API stays shaped the same. See `docs/notes/phase4-rtt-investigation.md` for the measurement.

**Decision**: `LocalPosixVolume::write_from_stream` `sync_data`s each file (+ best-effort parent-dir fsync) before it returns
**Why**: Every cross-volume copy/move that lands on a local disk (MTP → Local, SMB → Local, USB import) flows through this one method. A bare `file.flush()` finish is a userspace no-op on a raw `std::fs::File`, so the bytes would sit only in the OS page cache when the op reports "complete" — letting the user eject / sleep and lose data (on a move, from both sides, since the source delete runs after the copy reports Ok). The `sync_data` (fdatasync) gives the "durable as each file completes" property the local-FS chunked copy already has (`transfer/chunked_copy.rs`), so a crash mid-batch leaves earlier files safe. The parent-dir fsync makes the file's directory entry durable too. Both are best-effort on error: a failure logs under `target: "write_durability"` and continues rather than failing a completed multi-GB transfer at the final fsync (matching `durability::flush_created_destinations`). Non-local backends (MTP/SMB/InMemory) need no equivalent — durability there is the device/server's concern. Pinned by `local_posix_test::test_write_from_stream_multichunk_is_durable_and_correct` (content-correctness regression guard; the fdatasync itself isn't observable from a unit test).

## SMB archive push-refresh

The recursive share watcher already refreshes the DIRECTORY listing showing a changed `.zip` (its new size/mtime). On top of that, `process_event_batch`'s Modified and RenamedNewName handlers call `maybe_refresh_archive_listings(volume_id, entry_path)`: when `entry_path`'s name is a supported archive (`archive::has_supported_archive_extension`, the single-source predicate `format_for_name` backs), it fires the same `caching::refresh_archive_listings` the local `archive::watch` fires, pushing an out-of-band edit of the `.zip` to any open archive-INNER listing.

Why this is the whole fix, cheaply:

- **Same consumer, same key.** `refresh_archive_listings` scans `LISTING_CACHE` for keys at/inside the archive path and re-reads them; `volume_id` here is the parent DRIVE id, which is exactly what archive listings key on, so no rekeying. It's a no-op when the path isn't an archive or no inner listing is open, and the watcher already runs for the whole volume lifetime — so the only added cost is a re-parse when a `.zip` actually changes AND an inner pane is open.
- **`entry_path` is already normalized.** It's the `to_nfd_display_path` result, so it went through the same backslash→slash + NFC→NFD normalization every other cache-facing path in `smb_watcher.rs` uses. Passing the raw event filename would miss the cache.
- **Fires independent of the stat.** The refresh runs even when the pre-refresh `get_metadata` fails (a mid-write, truncated `.zip`): `refresh_archive_listings` keeps the previous inner listing on an unreadable parse rather than blanking the pane, and the next change event retries.
- **NOT a freshness claim.** This is a visible-listing UX nicety, a SEPARATE consumer from the write-op fresh-listing oracle. `ArchiveVolume::listing_is_watched` stays `false` for a remote parent regardless (the SMB watcher is lossy under load, so the oracle must keep re-reading pre-flight scans honestly). The remote-archive freshness decision and the guardrail test are in [`archive/watch/DETAILS.md`](archive/watch/DETAILS.md) § "remote archives have NO live watch". MTP keeps manual refresh (F5) as its contract.

Tests: `smb_watcher/archive_refresh_test.rs` (a Modified `.zip` event refreshes the inner listing; a non-archive change doesn't — the extension gate).

## Gotchas

**Gotcha**: `MtpReadStream` holds nothing scarce between windows, so dropping it mid-read is safe and needs no `Drop` impl
**Why**: It reads in bounded `GetPartialObject64(offset, MTP_READ_WINDOW)` windows (the windowing + offset accounting live in `mtp/connection`; see that module's DETAILS § "Bounded-window reads"). Between windows nothing is in flight — no held `FileDownload`, no pinned PTP session — so a cancel/pause/drop has nothing to abort or drain (`cancel_and_release` is the trait default no-op). If the stream is dropped WHILE a window read is in flight, mtp-rs's `TransactionScope` flags the pipe and the next op drains it under the operation lock (one ~300 ms self-heal), so an aborted window never desyncs the session. ❌ Don't re-add a `Drop`/cancel here: there's no held `FileDownload`, so mtp-rs's `ReceiveStream` unconsumed-drop panic (the reason a `Drop` cancel was once needed) can't apply.

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
- `smb_test.rs`: SMB unit tests (no server needed): type mapping (DirectoryEntry→FileEntry, FsInfo→SpaceInfo,
  Error→VolumeError), connection state transitions, path conversion, capability flags, and the channel-backed
  `SmbReadStream` consumer. These run by default.
- The SMB test suites live in sibling files wired as `#[cfg(test)] #[path = "..."] mod`s of `smb` (so `super::*`
  still reaches the backend's private items), split by theme: `smb_test.rs` (unit, above), `smb_integration_test.rs`
  (connection management, core CRUD, basic streaming smoke, scan/conflict preview), `smb_streaming_integration_test.rs`
  (the full read/write streaming surface: progress, cancel, large multi-chunk files, plus the error/cleanup paths with
  the `ErroringReadStream` double), `smb_transfer_semantics_test.rs` (high-level merge/move contracts driven through
  the transfer pipelines), `smb_stress_test.rs` (concurrency: the no-deadlock guard with its `MutexCaptureLogger`
  machinery, and the 100-file content-integrity test), and `smb_soak_test.rs` (below). Cross-suite helpers
  (`make_docker_volume`, `test_dir_name`, `ensure_clean`, `hash_bytes`, `hash_volume_file`, `TEST_PREFIX_ROOT`,
  `cleanup_test_prefix`) live in `smb_test_support.rs` as `pub(super)` items.
- **Docker SMB integration tests** (the four themed `smb_*_test.rs` Docker suites above): `#[ignore]` tests that require Docker SMB containers
  (start with `apps/desktop/test/smb-servers/start.sh`). Run with `cargo nextest run smb_integration --run-ignored all`.
  Connect via `smb2::testing::guest_port()` (10480, guest/no-auth), `auth_port()` (10481, `testuser`/`testpass`),
  `readonly_port()` (10488), `slow_port()` (10493, 200ms latency). Use these for testing real SMB protocol behavior
  (streaming, error paths, network edge cases). See `apps/desktop/test/smb-servers/README.md` for the full container
  list and env var overrides.
- **SMB soak test** (`smb_soak_copy_loop` in `smb_soak_test.rs`): Repeats the SMB→Local copy pipeline for hundreds to
  thousands of iterations and watches RSS, open FDs, SMB credits, and per-iteration wall-clock drift. Catches accumulating bugs
  the single-shot integration tests can't see (credit leak, FD leak, memory growth, slowdown). Default mode:
  `CMDR_SOAK_ITERATIONS=100` (≈5 s against Docker). Long mode: `CMDR_SOAK_DURATION_SECS=1800` (30 min, via
  `./scripts/soak-smb.sh`). CI has a `workflow_dispatch`-only job in `slow-checks.yml`.
