# Volume abstraction

This module defines the `Volume` trait (the core abstraction for all storage backends in Cmdr) and the `VolumeManager` registry.

## Purpose

Every file system operation (listing, copy, rename, delete, indexing, watching) goes through a `Volume`. The trait hides the differences between a local POSIX path, an MTP device, an in-memory test fixture, and future backends (SMB, S3, FTP). Callers never touch the filesystem directly; they call `Volume` methods with **paths relative to the volume root**.

## Key files

| File | Role |
|---|---|
| `mod.rs` | `Volume` trait (async: most methods return `Pin<Box<dyn Future>>`; sync: `name`, `root`, `supports_*`, `local_path`, `space_poll_interval`), `VolumeScanner`, `VolumeWatcher`, `VolumeReadStream` traits, `MutationEvent` enum, shared types (`VolumeError`, `SpaceInfo`, `CopyScanResult`, `ScanConflict`, `SourceItemInfo`) |
| `friendly_error.rs` | User-facing error messages: `FriendlyError`, `ErrorCategory`, errno mapping. See [Friendly error system](#friendly-error-system) below. |
| `provider.rs` | Provider detection and enrichment: `Provider` enum (19 variants), `detect_provider()`, `provider_suggestion()`, `enrich_with_provider()`. Re-exported via `friendly_error.rs`. |
| `manager.rs` | `VolumeManager`: thread-safe `RwLock<HashMap>` registry; supports a default volume |
| `local_posix.rs` | `LocalPosixVolume`: real filesystem; delegates listing to `file_system::listing`, indexing to `indexing::scanner`, watching to `indexing::watcher` (FSEvents), copy scanning via `walkdir`. Uses `libc::statvfs` FFI for space info. |
| `mtp.rs` | `MtpVolume`: MTP device storage; async `Volume` trait with direct async MTP calls. Uses `MtpReadStream` for streaming (calls `FileDownload::next_chunk().await` directly). Gated with `#[cfg(any(target_os = "macos", target_os = "linux"))]`. |
| `smb.rs` | `SmbVolume`: SMB share storage; async `Volume` trait with direct async smb2 calls. Splits session storage into `Arc<Mutex<Option<SmbClient>>>` + `Arc<RwLock<Option<Arc<Tree>>>>` so the hot read/write paths can clone `Connection` under a brief lock and drive compound / download ops without serializing on the client mutex. `AtomicU8` connection state. Caches `SmbConnectionParams` (host, share, port, credentials) so `attempt_reconnect` can rebuild the session in place after a transient disconnect, single-flighted via `reconnect_lock`. Holds a global `AppHandle` (`set_app_handle` in `lib.rs::setup`) for emitting `smb-connection-changed` events. Also contains `connect_smb_volume()`. Gated with `#[cfg(any(target_os = "macos", target_os = "linux"))]`. |
| `smb_watcher.rs` | Background SMB change watcher (`run_smb_watcher`). Owns a dedicated smb2 session (separate TCP connection from the volume's primary client) and uses smb2 0.10's `'static` `Watcher` with pipelined CHANGE_NOTIFY (one request kept pre-issued on the wire so events arriving during consumer processing don't fall in a re-arm gap). Debounces events, feeds `notify_directory_changed`. Spawned by `connect_smb_volume()` and respawned by `attempt_reconnect`. No internal reconnect — bails on `next_events` errors and lets `attempt_reconnect` handle session recovery. |
| `in_memory.rs` | `InMemoryVolume`: `RwLock<HashMap>` store for tests; also used for stress tests (`with_file_count`) |

## Architecture

```
VolumeManager (registry)
  └─ Arc<dyn Volume>  (async trait: most methods return Pin<Box<dyn Future>>)
        ├─ LocalPosixVolume   → real FS (spawn_blocking for I/O), FSEvents watcher, jwalk scanner
        ├─ MtpVolume          → direct async MTP ops
        ├─ SmbVolume          → direct async smb2 ops (direct protocol, not OS mount)
        └─ InMemoryVolume     → HashMap, test/stress use only
```

`VolumeScanner` and `VolumeWatcher` are separate sub-traits returned by `Volume::scanner()` and `Volume::watcher()`. Only `LocalPosixVolume` implements both today.

## Trait capability model

Optional methods default to `Err(VolumeError::NotSupported)` or `false`, so new volume types can be added incrementally. Key capability flags:

- `supports_watching()`: enables the `notify`-based *listing* file watcher in `operations.rs` (separate from the `VolumeWatcher` trait used for drive indexing). `MtpVolume` returns `false` (it has its own USB event loop).
- `supports_export()`: "this volume can stream its bytes via `open_read_stream`" (so it can act as a source in a cross-volume copy). Gates the copy dialog's "copy from this volume" UI. Local, MTP, SMB, and InMemory return `true`.
- `supports_streaming()`: enables cross-volume transfers via `open_read_stream` / `write_from_stream`. `LocalPosixVolume`, `MtpVolume`, `SmbVolume`, and `InMemoryVolume` all return `true`. Since Phase 4 this is the universal byte path for every non-APFS-clone copy. New backends just implement the two streaming methods to get cross-volume copy for free.
- `max_concurrent_ops()`: how many streaming copies the copy engine can drive in parallel against this volume. The batch copy path takes `min(src.max_concurrent_ops(), dst.max_concurrent_ops(), 32)` and spawns that many `FuturesUnordered` tasks. Defaults to `1` (safe for any new backend). Current values: `LocalPosixVolume` returns `available_parallelism()/2` clamped to 4..=16; `SmbVolume` returns 10 (hardcoded in Phase 4.2; Phase 4.3 will wire it to `network.smbConcurrency`); `MtpVolume` returns 1 (USB bulk transport is serial); `InMemoryVolume` returns 32.
- `local_path()`: returns `Some` only for local volumes; allows `copyfile(2)` fast-path in copy operations. `SmbVolume` returns `None` so copies go through smb2 instead of the slow OS mount.
- `supports_local_fs_access()`: whether `std::fs` operations (stat, read_dir) work on this volume's paths. Default `true`. `MtpVolume` and `SmbVolume` return `false`. Used to skip the legacy synthetic entry diff path (now superseded by `notify_mutation`).
- `notify_mutation(volume_id, parent_path, mutation)`: called after a successful mutation (create, delete, rename) to update the listing cache immediately. Default impl uses `std::fs` (works for `LocalPosixVolume`). `SmbVolume` and `MtpVolume` override to use their own protocol's `get_metadata`. Fire-and-forget, no error propagation.
- `smb_connection_state()`: returns `Some(SmbConnectionState)` for SMB volumes (green/yellow indicator in volume picker). Default `None`. Only `SmbVolume` implements it.
- `attempt_reconnect()`: tries to rebuild the volume's underlying session in place after a transient connection loss. Default `Err(NotSupported)`. Only `SmbVolume` overrides today; the Tauri command `reconnect_smb_volume` and the FE reconnect manager call this on each backoff tick. Idempotent and single-flight: concurrent callers wait on the same in-flight attempt instead of dog-piling the server.
- `on_unmount()`: lifecycle hook called before unregistration. `SmbVolume` uses it to disconnect its smb2 session. Default is no-op.
- `scanner()` / `watcher()`: drive indexing hooks; `None` by default.
- `space_poll_interval()`: recommended interval for the live disk-space poller (`space_poller.rs`). Default 2 s (local volumes). `SmbVolume` and `MtpVolume` override to 5 s. `InMemoryVolume` returns `None` (no polling). The poller uses this to tick each volume at its own cadence.
- `listing_is_watched(path)`: returns `true` when this volume's cached listing for `path` is being kept in sync by a live watcher. Three consumers today:
    1. `file_system::listing::caching::try_get_watched_listing` (the "fresh-listing oracle") — write-op pre-flight scans reuse a cached listing instead of re-reading.
    2. `write_operations::delete::scan_volume_recursive` (the oracle-aware delete walker) — same idea, per-recursion-level.
    3. The `refresh_listing` Tauri command (`commands/file_system/listing.rs`) — short-circuits the post-transfer redundant `list_directory` re-read entirely when the volume is keeping the cache fresh via `notify_mutation`. Without this, a 1k-entry MTP folder paid ~17 s + USB session collision after every transfer outcome, wedging the next user op.
  Default `false` so a new backend without a real watcher won't accidentally claim freshness. **Freshness contract**: a `true` result does NOT mean the cache is byte-perfect with the device right now. Every backend has a debounce or settling window between a real change and the cache reflecting it: local FS ≈ 10 ms (FSEvents coalesce), SMB 200 ms (watcher debounce; > 50 events/dir triggers a `FullRefresh`), MTP 500 ms (event debouncer plus per-device polling; many cameras emit no events at all, so on those `true` means only "the device is reachable"). Callers must treat the result as "fresh as our most recent observation" — the same guarantee a `list_directory` call gives. The MTP and SMB checks are volume-level, not path-level: when the gate flips true, every path on that volume becomes oracle-eligible.

## Cancel-aware variants

`list_directory_with_cancel(path, on_progress, cancel)` and
`delete_with_cancel(path, cancel)` accept an
`Option<&Arc<AtomicBool>>` that backends interpret as a cooperative cancel
flag. Default impls delegate to the non-cancel `list_directory` / `delete`,
dropping the flag — so adding a new backend doesn't have to implement them
unless its operations are interruptible at a meaningful boundary.

- `MtpVolume` overrides both. The flag wraps a fresh `mtp_rs::CancelToken` via
  `CancelToken::from_arc(Arc::clone(...))` (shared atomic, no polling task) and
  threads through to mtp-rs's `list_objects_with_cancel` /
  `delete_with_cancel`. That bails the per-handle `GetObjectInfo` loop within
  one USB roundtrip's latency.
- `LocalPosixVolume`, `SmbVolume`, and `InMemoryVolume` inherit the default
  (ignore the flag). Local listings are effectively atomic; SMB cancel
  propagation is a follow-up.

The write-op layer hands `Some(&state.backend_cancel)` (a clone of the same
`Arc<AtomicBool>` that `cancel_write_operation` flips when intent leaves
`Running`). Volumes that ignore the flag are unaffected; volumes that consume
it stop their wire activity, not just the loop above.

See `apps/desktop/src-tauri/src/mtp/CLAUDE.md` § "Cancel propagation" for the
MTP-specific wiring and the rationale for "between-roundtrip" cancel vs PTP
`CancelTransaction`.

## Building a new volume

Adding a new backend (say, FTP, WebDAV, S3, or a new device protocol) is a matter of implementing the `Volume` trait and opting into the capability flags that make sense for your backend. The checklist below walks the path in the order you'd hit each concern.

Work through it top-to-bottom. Each tier depends on the previous being solid. Ship to users only after tier 3.

### Tier 1: make it listable (mandatory)

Without these, the volume can't even appear in the UI:

- [ ] Implement `name()` and `root()` (return the display name and the path everything is relative to).
- [ ] Implement `list_directory(path, on_progress)`: the core read. Call `on_progress(count)` at least once.
- [ ] Implement `get_metadata(path)`: per-entry stat.
- [ ] Implement `exists(path)` and `is_directory(path)`. On backends where these would issue two round-trips, implement them in terms of `get_metadata` to share the cost.
- [ ] Implement `get_space_info()`: for the volume usage bar and pre-copy space checks. Return zeros if the backend doesn't report it.
- [ ] Register the volume via `VolumeManager::register_if_absent` (not `register`; see "Key decisions" above).
- [ ] Add unit tests using a fake/in-memory harness or real fixtures.

### Tier 2: make it writable (recommended for real-world use)

Everything below is optional per the trait (methods default to `Err(NotSupported)` or `false`), but a read-only volume is rarely useful:

- [ ] Implement `create_directory`, `create_file`, `delete`, `rename`.
- [ ] After each successful mutation, call `self.notify_mutation(&volume_id, parent_path, MutationEvent::...)` so the listing cache updates immediately. Override `notify_mutation` on the trait if your backend can answer `get_metadata` faster than `std::fs::metadata` would (MTP and SMB do this).
- [ ] Return `supports_streaming() = true` and implement `open_read_stream` + `write_from_stream`. These are the byte path for every cross-volume copy (Phase 4 collapsed the old `export_to_local` / `import_from_local` onto this pair). The Copy dialog uses them for "this volume ↔ anywhere" transfers.
- [ ] Return `supports_export() = true` if the volume should appear as a copy source in the UI.
- [ ] Implement `scan_for_copy` (count + bytes) and `scan_for_conflicts` (destination collision detection). These feed the Copy dialog's pre-flight.
- [ ] Map your backend's errors through a `map_*_error` function that returns `VolumeError`. Connection-loss errors should trigger a state transition (see `SmbVolume::handle_smb_result` as a reference) so subsequent calls fail fast.
- [ ] **No full-file buffering in per-file transfer paths.** Don't drain the incoming `VolumeReadStream` into a `Vec<u8>` before writing, and don't collect the remote file into a `Vec<u8>` before yielding. An 8 GB copy would allocate 8 GB of RAM. See the "Streaming requirement" section on each trait method's doc comment: `open_read_stream`, `write_from_stream`.

### Tier 3: integrate with the wider app (optional, but mostly expected)

- [ ] If the backend has its own change-notification channel, set `supports_watching() = true` and implement a watcher task that calls `notify_directory_changed` when things move. If you rely on the OS mount's FSEvents (like SmbVolume currently does), leave it `false`.
- [ ] Implement `listing_is_watched(path)` if your backend can answer "is the cached listing for this path being kept in sync by a live watcher?" cheaply. Returning `true` from this gate opts the volume into the fresh-listing oracle: write-op pre-flight scans (copy/move scan preview) reuse cached entries from `LISTING_CACHE` for any path your volume reports as watched, skipping the redundant `list_directory` round-trip. Default `false` is the safe choice — without a real watcher, the cache may be arbitrarily stale. Path-level (LocalPosixVolume) is the most accurate signal; volume-level (MTP "device connected", SMB "Direct + watcher running") is fine when the underlying notification channel is volume-wide. Be honest about the per-backend debounce window in the doc comment; see `try_get_watched_listing` for the freshness contract.
- [ ] If `std::fs` operations work on the volume's paths (you're a local FS with extra flavor), leave `supports_local_fs_access()` at the default `true`. Otherwise override to `false` so the legacy synthetic-diff path is skipped.
- [ ] If `std::fs::copy` can target this volume's paths directly, return `Some(root)` from `local_path()`. The copy path will prefer `copyfile(3)` / `copy_file_range(2)` for same-device copies. Otherwise return `None` (the default).
- [ ] Override `space_poll_interval()` to whatever polling cadence your backend can afford (local 2 s, network 5 s, none = don't poll).
- [ ] If the volume needs async teardown (session close, handle drop), implement `on_unmount`. The default is a no-op.
- [ ] If the backend participates in drive indexing, implement `scanner()` and `watcher()`. Today only `LocalPosixVolume` does.
- [ ] Add a branch to `detect_provider` / `provider_suggestion` in `provider.rs` if there's a recognizable path shape or fs type worth calling out in friendly errors.
- [ ] Add a capability-matrix row below and update the `docs/architecture.md` volume line if the shape changes meaningfully.

### Tier 4: E2E and friendly-error polish

- [ ] Add integration tests (real fixtures if possible; see the Docker SMB containers for inspiration).
- [ ] Verify that `FriendlyError` messages come out well for your backend's common failure modes. Test the `error_messages_never_contain_error_or_failed` rule: it's enforced by existing unit tests.
- [ ] Stress-test concurrent reads and writes (the `stress_tests_*` modules in indexing are the reference pattern).

## Capability matrix

At-a-glance view of which capabilities each current volume opts into. Use this when picking a reference implementation for your new volume.

| Capability                  | Local                | MTP                     | SMB                       | InMemory           |
| --------------------------- | -------------------- | ----------------------- | ------------------------- | ------------------ |
| `list_directory` / metadata | ✅                   | ✅                      | ✅                        | ✅                 |
| Mutations (create/delete/rename) | ✅              | ✅                      | ✅                        | ✅                 |
| `supports_export`           | ✅                   | ✅                      | ✅                        | ✅                 |
| `supports_streaming`        | ✅                   | ✅                      | ✅                        | ✅                 |
| `open_read_stream`          | ✅ spawn_blocking    | ✅ owned download       | ✅ channel-backed         | ✅ in-memory       |
| `write_from_stream`         | ✅ spawn_blocking    | ✅ streaming            | ✅ streaming              | ✅ in-memory       |
| `supports_watching`         | ✅ FSEvents/inotify  | ❌ (own USB watcher)    | ❌ (OS-mount FSEvents)    | ❌                 |
| `listing_is_watched`        | ✅ path-level (WATCHER_MANAGER) | ✅ volume-level (device connected) | ✅ volume-level (watcher + Direct) | ❌ (default) |
| `supports_local_fs_access`  | ✅ (default)         | ❌                      | ❌                        | ❌                 |
| `local_path`                | ✅ `Some(root)`      | `None`                  | `None`                    | `None`             |
| `notify_mutation`           | default (std::fs)    | ✅ MTP `get_metadata`   | ✅ smb2 `get_metadata`    | ✅ in-memory       |
| `scanner` / `watcher` (indexing) | ✅ / ✅          | ❌                      | ❌                        | ❌                 |
| `on_unmount`                | default              | default                 | ✅ drops smb2 session     | default            |
| `smb_connection_state`      | `None`               | `None`                  | ✅                        | `None`             |
| `space_poll_interval`       | 2 s (default)        | 5 s                     | 5 s                       | `None`             |
| `max_concurrent_ops`        | 4..=16 (core-based)  | 1 (USB bulk serial)     | 10 (P4.3 will tune)       | 32                 |

Legend: ✅ = implemented, ❌ = opted out (default or explicitly), ⚠️ = implemented but suboptimal (memory-heavy or otherwise worth revisiting).

When adding a new volume, add a column for it and fill in each row. The matrix doubles as a self-review: gaps will stare back at you.

## Streaming patterns

Reads and writes have different shapes because the consumer relationship is different:

- **Reads** return a `VolumeReadStream` that an external caller polls. The download handle has to live past the function call and cross async contexts. That's where the lifetime/ownership gymnastics below come from.
- **Writes** consume a stream (or a local file) inside the method itself. The chunk loop is the consumer, so there's nothing to hand off. For backends with a `'static` writer (smb2 0.9's owned `FileWriter`, mtp-rs's `upload_stream`), drive the writer directly on a cloned session handle — no lock held across I/O. For backends whose writer borrows from the session, hold the session lock for the chunk loop's duration. `SmbVolume::write_from_stream` is the reference implementation: clone the session once, open the smb2 `FileWriter` on the clone, loop `write_chunk`, call `finish()` on success or `abort()` on cancel. No task spawn, no channel, no self-referential struct, no client mutex held while WRITEs are in flight.

The rest of this section is about **read-side** lifetime handling. Which pattern to pick depends on whether your protocol SDK's download handle is `'static` or borrowed.

### Pattern A: own the download (use when the SDK's download type is `'static`)

If the SDK gives you a download handle that owns its session internally and doesn't borrow from anything, store it directly in your stream struct. **Example: `MtpReadStream`** (`mtp.rs:704-729`).

```rust
struct MtpReadStream {
    download: Option<mtp_rs::FileDownload>,  // 'static, no lifetime parameter
    total_size: u64,
    bytes_read: u64,
}
```

`next_chunk()` calls `download.as_mut()?.next_chunk().await` directly, no task spawn, no channel. `Drop` cancels the transfer (see the MTP gotcha in this file for the detached-task cancel pattern).

### Pattern B: channel-backed stream (use when the SDK's download type borrows `&mut Connection`)

If the SDK's download handle holds a borrow against the session (like `smb2::FileDownload<'a>` borrowing `&'a mut Connection`), you can't stuff it into a `'static` struct. Use a background producer task that holds an `OwnedMutexGuard` over the session, drives the download, and feeds chunks through a bounded mpsc channel. **Example: `SmbReadStream`** (`smb.rs` → `open_smb_download_stream`).

Key building blocks:
- `Arc<tokio::sync::Mutex<Session>>` so the task can call `lock_owned()` and own the guard until done.
- Bounded mpsc channel (capacity ~4) for backpressure. Peak memory is `capacity × chunk_size`, a few MB regardless of file size.
- Oneshot channel for the total size (reported before the first chunk so the consumer sees the correct `total_size()` synchronously).
- Oneshot channel for cancellation. `Drop` on the stream sends the signal, producer breaks its loop and releases the guard.
- If the session state (connection health) can transition on protocol errors, wrap the state atomic in `Arc<AtomicU8>` so the task can update it from outside `&self` context.

### Anti-pattern: pre-buffering the whole file

The pre-refactor `SmbReadStream` read the entire file into a `Vec<u8>` via `read_file_pipelined` and yielded slices. For an 8 GB file that meant an 8 GB allocation. Don't do this. If the consumer API is stream-shaped, the producer should stream too.

The same rule applies to write paths: `write_from_stream` must drive the backend's chunk-by-chunk writer (for example, smb2's `FileWriter`) rather than slurping the source into a `Vec<u8>` first. See the "Streaming requirement" section on each Volume trait method's doc comment.

## Path handling gotchas

- **`LocalPosixVolume::resolve`**: accepts empty, `.`, relative, or absolute paths. Three-way branch for absolute paths: (1) already starts with volume root, used as-is; (2) volume root is `/`, absolute path passed through unchanged; (3) otherwise, leading `/` stripped and joined to root. This handles frontend sending full absolute paths.
- **`MtpVolume::to_mtp_path`**: strips the `mtp://{device}/{storage}/` URL prefix and leading slashes, returning the bare relative path the MTP library expects.
- **`InMemoryVolume::normalize`**: always resolves to an absolute path anchored at `/`.

## SMB auto-upgrade lifecycle

SMB mounts are automatically upgraded to `SmbVolume` (direct smb2 connection) in two scenarios:

1. **Startup** (`file_system::upgrade_existing_smb_mounts(app_handle)`): Scans registered volumes for `smbfs` type. If
   any are found, calls `network::ensure_mdns_started` to kick off mDNS itself (creds are keyed by hostname, not IP),
   then waits for mDNS to reach `Active` state (polls every 500ms, up to 15s). Uses `tauri::async_runtime::spawn` (not
   `tokio::spawn`; runs during `setup()` before Tokio is fully available). Emits `volumes-changed` after upgrades so
   the frontend refreshes indicators. **No `firstTriggerDone` gate**: the function is a no-op when no SMB mounts are
   present (no network activity, no macOS Local Network prompt). When mounts are present AND `network.directSmbConnection`
   is on (default `true`), it kicks off mDNS — that's when the macOS prompt fires, once per app per data dir. Before
   this change the upgrade was gated behind `firstTriggerDone`, so dev profiles with auto-reconnected SMB shares stayed
   on the slow OS-mount path forever.

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

## Friendly error system

`friendly_error.rs` turns raw OS errors into warm, actionable messages so the user feels supported when something goes
wrong. This is one of Cmdr's UX differentiators: where other file managers show "I/O error: Operation timed out (os
error 60)", we show a friendly title, a plain-language explanation, and provider-specific advice ("This folder is managed
by **MacDroid**. Here's what to try: ...").

### Philosophy

**The user should never feel alone with a broken state.** Every error message should feel like the app is putting its
hand on the user's shoulder and saying "Here's what happened, and here's what you can do." We go above and beyond: we
detect which cloud provider or mount tool manages the path, and tailor the suggestion to that specific app. A timeout on
a Dropbox folder gets different advice than a timeout on an SSHFS mount.

Power users also need the raw details (errno name, code) for debugging or bug reports. These are available in a
collapsible "Technical details" section, never hidden but never in your face either.

### Architecture

Three-layer mapping across two files, plus a third path for "succeeded but suspiciously empty":

**Layer 0**: typed git pass-through. `VolumeError::FriendlyGit(FriendlyGitError)` is a dedicated variant the git
module's volume hooks (`try_route_listing`, `try_route_metadata`, `try_open_blob_stream`) return when they detect a
git-shaped failure. `friendly_error_from_volume_error` matches it first and calls `to_friendly_error()` on the carried
payload, returning a fully-shaped `FriendlyError` with the right title, explanation, suggestion, and category, with no
errno mapping needed, no provider enrichment downstream. Keeps git-specific copy from getting clobbered by the generic
I/O fallback, end-to-end type-checked, no string parsing.

1. **`friendly_error_from_volume_error(err, path)`** (`friendly_error.rs`): maps `VolumeError` variants and macOS errno
   codes (37 codes) to a `FriendlyError` with category (Transient/NeedsAction/Serious), title, explanation, suggestion,
   and raw detail.
2. **`enrich_with_provider(error, path)`** (`provider.rs`, re-exported from `friendly_error.rs`): detects 19
   cloud/mount providers from path patterns and `statfs` filesystem type, then overwrites the suggestion with
   provider-specific advice.
3. **`friendly_error_for_restricted_empty_root(volume_id, path)`** (`friendly_error.rs`): for the case where the OS
   returns a successful empty listing at a volume root that's commonly hidden by macOS TCC (currently iCloud Drive
   without Full Disk Access). The streaming listing path (`file_system/listing/streaming.rs`) checks this after a
   successful empty read at the volume root and emits `listing-error` with the hint instead of `listing-complete`.
   Returns `None` for any other volume / non-root path so genuine empty directories don't get the warning.
4. **`friendly_from_write_error(err)`** (`friendly_error.rs`): variant-by-variant mapping from
   `WriteOperationError` (post-`map_volume_error`) to a `FriendlyError`. Used by `WriteErrorEvent::new` so every
   `write-error` event the FE receives carries a friendly payload, even on local-FS paths where the original
   `VolumeError` is no longer in scope. `TransferErrorDialog` renders this directly with category-based styling
   (mirrors the listing-error path's treatment).

The frontend receives the fully-baked `FriendlyError` struct via the `listing-error` and `write-error` Tauri events
and renders it with category-based visual styling. The frontend never sees errno codes or does OS-specific logic.

### Adding a new error message

When you need to handle a new errno or `VolumeError` variant:

1. Add the match arm in `friendly_error_from_volume_error`
2. Pick the right `ErrorCategory`: **Transient** (retry might work), **NeedsAction** (user must do something),
   **Serious** (something is genuinely broken)
3. Write the message following the rules below
4. Build `explanation` / `suggestion` with the `md!(...)` macro (see `friendly_error/markdown.rs`). Templates are
   trusted markdown; positional `{}` args route through `MarkdownArg::render_arg` which escapes plain strings
   (paths, OS messages, names) and passes a `Markdown` value through unescaped. **Use positional `{}` only** —
   captured-identifier syntax (`md!("foo {bar}")`) bypasses escaping and renders the literal `{bar}` in the UI.
5. Add a unit test asserting the category and that the text follows the style rules
6. Run the existing `error_messages_never_contain_error_or_failed` test to catch violations

### Adding a new provider

When a new cloud storage or mount tool becomes popular enough to detect:

1. Add a variant to the `Provider` enum with `display_name()` and `app_name()`
2. Add path detection in `detect_provider` (CloudStorage prefix, specific path, or `statfs` type)
3. Write provider-specific suggestions in `provider_suggestion` for each `ErrorCategory`
4. Add a unit test for path detection and suggestion content
5. Update `volumes/CLAUDE.md` provider table to keep the two lists in sync

### Writing rules for error messages

These are non-negotiable. The existing test suite enforces some of them automatically.

- **NEVER use "error" or "failed"** in titles, explanations, or suggestions. Say "Couldn't read" not "Read error". The
  automated test `error_messages_never_contain_error_or_failed` catches this.
- **Active voice, contractions**: "Cmdr couldn't..." not "The operation was unable to..."
- **No trivializing**: no "just", "simply", "easy", "all you have to do"
- **No permissive language**: "Check your connection" not "You might want to check..."
- **Direct and warm**: "Here's what to try:" not "Please attempt the following remediation steps:"
- **No em dashes**: use parentheses, commas, or new sentences
- **Sentence case in titles**: "Connection timed out" not "Connection Timed Out"
- **Bold key terms** with `**` only when it helps scanning (for example, provider names)
- **Platform-native terms**: "System Settings" on macOS, "Finder", "Trash"
- **Keep it short**: max two sentences for explanation, bullets for suggestions

Good example:
```
title: "Connection timed out"
explanation: "Cmdr tried to read this folder but the connection didn't respond in time."
suggestion: "Here's what to try:\n- Check that the device or server is reachable\n- ..."
```

Bad example (every rule violated):
```
title: "I/O Error: Operation Timed Out"   // "Error", Title Case
explanation: "An error occurred while the system attempted to access the directory."  // passive, "error"
suggestion: "You may want to try simply reconnecting the device."  // permissive, trivializing
```

### Provider detection strategies

| Strategy | Providers covered |
|---|---|
| `~/Library/CloudStorage/<Prefix>*` | Dropbox, GoogleDrive, OneDrive, Box, pCloud, Nextcloud, SynologyDrive, Tresorit, ProtonDrive, Sync, Egnyte, MacDroid, plus a generic fallback for unrecognized providers |
| `~/Library/Mobile Documents/` | iCloud Drive |
| `/Volumes/pCloudDrive` | pCloud (FUSE virtual drive) |
| `/Volumes/veracrypt*` | VeraCrypt |
| `~/.CMVolumes/` | CloudMounter |
| `statfs` `f_fstypename` (macOS) | macFUSE/SSHFS/Cryptomator/rclone (`macfuse`, `osxfuse`), pCloud (`pcloudfs`) |

The `statfs` check runs only at error time (not on every listing), so the syscall cost is negligible.

## Integration status

`LocalPosixVolume` is wired into the indexing subsystem. `VolumeManager` is actively used.

## Git delegation hooks (M2)

`LocalPosixVolume` delegates three read-side methods to the git module after `resolve()`:

- `list_directory` calls `git::try_route_listing(resolved_path)`. Returns the virtual listing for `.git/`, `.git/branches/...`, `.git/tags/...`, `.git/commits/...`, `.git/stash/...`, `.git/worktrees/...`, or `.git/submodules/...`. Real `.git/*` entries (HEAD, config, hooks/, objects/, refs/, etc.) get `None` from the hook and fall through to real-FS listing. The portal root (`.git/`) returns a mixed listing: real entries plus the six virtual categories.
- `get_metadata` calls `git::try_route_metadata(resolved_path)`.
- `open_read_stream` calls `git::try_open_blob_stream(resolved_path)`. Returns a `GitBlobReadStream` for blobs inside refs; real `.git/*` files fall through to the LocalPosixVolume real-FS reader.

All mutation methods (`create_file`, `create_directory`, `delete`, `rename`, `write_from_stream`) detect virtual paths via `git::is_virtual(path)` and return `VolumeError::NotSupported` immediately. `notify_mutation` early-returns for virtual paths since git mutations happen out-of-band (the user runs `git` in a terminal); state changes flow through the `.git`-watcher pipeline (`file_system/git/watcher.rs`) instead.

The hook order is fixed: `resolve()` first (normalizes the path), then `try_route_*`. This lets the user open `.git` from any volume-rooted path and get the portal regardless of whether the frontend sent an absolute or relative path.

## Key decisions

**Decision**: Trait with optional methods defaulting to `NotSupported`/`false`
**Why**: New volume types (SMB, S3, FTP) will have vastly different capability sets. Forcing every implementor to stub out every method would be noisy and error-prone. Defaults let new backends start with just `list_directory` + `get_metadata` and opt in to capabilities incrementally. The alternative (a capabilities bitfield) would require runtime checks everywhere and couldn't express return-type differences.

**Decision**: `VolumeScanner` and `VolumeWatcher` are separate sub-traits, not part of `Volume`
**Why**: Scanning and watching have their own lifetimes, threading models, and state (handles, channels). Folding them into `Volume` would force every volume to carry scanner/watcher state even if it never indexes. Returning `Option<Box<dyn VolumeScanner>>` keeps the core trait lightweight.

**Decision**: `VolumeManager` uses `RwLock<HashMap>` (not `DashMap` or `Mutex`)
**Why**: Volume registration/unregistration is rare (mount/unmount events); reads are frequent (every file operation resolves a volume). `RwLock` gives concurrent read access without pulling in an extra dependency. `DashMap` would work but is heavier than needed for a registry that rarely exceeds ~10 entries.

**Decision**: `VolumeManager::register_if_absent` for watcher registrations
**Why**: When the mount flow pre-registers an `SmbVolume`, the FSEvents watcher would overwrite it with a `LocalPosixVolume` via `register`. `register_if_absent` is a no-op if a volume is already registered, preserving the `SmbVolume`. The existing `register` (overwrite) is kept for explicit replacement (like SmbVolume replacing itself on reconnect).

**Decision**: `Volume` trait is async (methods return `Pin<Box<dyn Future>>`)
**Why**: MTP and SMB operations are inherently async (USB bulk transfers, network I/O). The previous sync trait required `block_on` bridges that risked nested-runtime panics in cross-volume streaming. The async trait lets MTP and SMB call their async backends directly. `LocalPosixVolume` wraps its blocking I/O in `spawn_blocking`. Sync-only methods (`name()`, `root()`, `supports_*()`, capability flags) remain non-async.

**Decision**: `VolumeError` stores `String` messages, not the original `std::io::Error`
**Why**: `std::io::Error` is not `Clone`, but `VolumeError` needs to be `Clone` for ergonomic error propagation across thread boundaries and for serialization to the frontend. Storing the formatted message loses the original error type but keeps the information that matters for user-facing error messages. The `IoError` variant also carries `raw_os_error: Option<i32>` so the friendly error mapper can match on platform-specific errno codes.

**Decision**: Friendly error mapping is two layers: errno mapping, then provider enrichment
**Why**: Not every provider+errno combination needs custom copy. The base errno message is always useful. Provider enrichment is additive, making the suggestion more specific when we recognize who manages the mount. Keeping them separate avoids a combinatorial explosion of messages.

**Decision**: Friendly error mapping lives in Rust, not the frontend
**Why**: The mapping needs access to the full path (for provider detection) and platform-specific errno codes. Doing it in Rust keeps the frontend thin (principle: smart backend, thin frontend) and avoids duplicating errno knowledge in TypeScript. The frontend receives a ready-to-render `FriendlyError` struct with markdown strings.

**Decision**: `FriendlyError.explanation` / `.suggestion` are typed `Markdown`, not `String`; built via the `md!` macro
**Why**: Raw OS messages and provider names contain markdown specials (the bug was `STATUS_DELETE_PENDING` rendering as italics because `format!()` baked the underscores straight into the explanation). The `Markdown` newtype + `md!` macro escape every interpolated runtime value via the `MarkdownArg` trait while leaving the trusted template literal alone. `#[serde(transparent)]` keeps the wire format identical to the old `String`, and the FE bindings.ts post-processing brands the type as `string & { readonly __markdown: unique symbol }` so the single `renderErrorMarkdown` call site only accepts wire-supplied markdown values. See `friendly_error/markdown.rs` for the macro, the trait, the HTML-entity escape strategy (snarkdown doesn't honor CommonMark `\` escapes, so `\_` would render literally — we emit `&#95;` instead, which snarkdown ignores and the browser decodes), the conservative escape set (line-start chars like `.` / `-` / `#` left alone so paths render naturally), and the captured-identifier footgun warning.

**Decision**: `LocalPosixVolume` uses `symlink_metadata` for `exists()` instead of `Path::exists()`
**Why**: `Path::exists()` follows symlinks. A dangling symlink returns `false`, which would make the volume claim a file doesn't exist when it visibly does in a directory listing. `symlink_metadata` detects the symlink itself, matching what the user sees.

## Gotchas

**Gotcha**: `LocalPosixVolume::resolve` has a three-way branch for absolute paths
**Why**: The frontend sometimes sends full absolute paths (like `/Users/alice/Documents`), not paths relative to the volume root. If the volume root is `/Users/alice/Dropbox`, the resolve logic must detect whether the absolute path is already inside the root (pass through), whether the root is `/` (pass through), or neither (strip leading `/` and join). Getting this wrong silently serves the wrong directory.

**Gotcha**: `MtpReadStream::Drop` spawns a detached cancel task
**Why**: When a download is cancelled mid-stream (user presses Cancel during MTP copy), the `MtpReadStream` is dropped
before the `FileDownload` is fully consumed. mtp-rs's `ReceiveStream` panics on drop if not consumed or cancelled
(to prevent USB session corruption). The `Drop` impl calls `download.cancel(DEFAULT_CANCEL_TIMEOUT).await` on a
spawned detached task. This is safe because the stream always lives in an async context (tokio worker thread), so
`Handle::try_current()` succeeds. The detached task runs independently; the drop returns immediately.

**Gotcha**: `MtpVolume::get_metadata` is expensive: it lists the entire parent directory
**Why**: MTP has no single-file stat call. `get_metadata` lists the parent directory and searches for the entry by name. This is used by `notify_mutation` after each self-mutation (create, delete, rename) and is acceptable because those are infrequent, but avoid calling it in hot paths.

**Decision**: `notify_mutation` lives on the Volume trait, not in Tauri commands
**Why**: Every mutation method (`create_file`, `create_directory`, `delete`, `rename`) knows what changed. Adding the notification call at the end of each method keeps it colocated with the mutation. The alternative (notification calls in every Tauri command) is fragile, easy to miss a call site.

**Decision**: `SmbVolume` and `MtpVolume` store `volume_id: String` for listing cache lookups
**Why**: `notify_mutation` needs to call `notify_directory_changed(volume_id, ...)` to find the right cached listings. The volume_id is computed at creation time (`smb_volume_id(server, port, share)` for SMB so two same-named shares on different servers don't collide — see `volumes/CLAUDE.md` § "Volume IDs"; `"{device_id}:{storage_id}"` for MTP) and stored on the struct rather than recomputed on every mutation.

**Decision**: `SmbVolume::supports_local_fs_access()` returns `false`
**Why**: `SmbVolume` now handles listing updates via `notify_mutation` using its own smb2 `get_metadata`. The old `std::fs`-based synthetic diff path (`emit_synthetic_entry_diff`) is redundant and goes through the slow OS mount. Returning `false` skips it.

**Decision**: `SmbVolume` splits session storage: `Arc<Mutex<Option<SmbClient>>>` + `Arc<RwLock<Option<Arc<Tree>>>>`
**Why**: Phase 4 Fix 2 unblocks concurrency on the hot copy path. Previously the session lived in one `Mutex<Option<(SmbClient, Tree)>>`, which the streaming-read producer and the compound read/write fast-paths held for the entire transfer, serializing every concurrent copy through the mutex. With smb2 0.7.x, `Connection` is `Clone` (cheap `Arc::clone`, all clones multiplex frames over one SMB session). Splitting the Tree out lets us briefly lock the client, clone its `Connection`, and release the lock, then drive `Tree::download` / `Tree::read_file_compound` / `Tree::write_file_compound` on the cloned `Connection` with no lock held. N concurrent copies on one `SmbVolume` now pipeline N operations over the single session instead of queuing on the mutex. Tree lives in a `RwLock` because we only take read locks in the hot path (cloning an `Arc<Tree>`) and only write on disconnect. Since smb2 0.9, the streaming-write path also uses this clone-and-release shape (see the `write_from_stream` Decision below), so the client mutex is no longer held across any I/O.

**Decision**: `SmbVolume::local_path()` returns `None`
**Why**: `local_path()` is checked in `volume_copy.rs` to decide whether to use native OS copy APIs. If SmbVolume returned `Some(mount_path)`, copies would go through the slow OS mount, which is exactly what we're trying to avoid. `root()` still returns the mount path for frontend path resolution.

**Decision**: `on_unmount()` trait method instead of `Any` downcasting
**Why**: Avoids runtime type checking, extensible for future volume types (S3, FTP might also need cleanup), consistent with the trait's design of optional methods with default no-ops.

**Decision**: SmbVolume background watcher runs on a dedicated smb2 session, not a clone of the volume's main connection
**Why**: smb2 0.10 made `Watcher` `'static` (owns a `Connection` clone), so technically the watcher could share the volume's session via `clone_session`. Empirically it can't: stacking the watcher's CHANGE_NOTIFY long-polls on the same TCP session as heavy concurrent writes wedges Samba — `smb_integration_concurrent_streaming_writes_no_deadlock` hangs against `smb-consumer-maxreadsize` (64 KB max read/write, 8 concurrent writers, 200 × 1 MB files). The dedicated session keeps the watcher's traffic out of the writers' way at the cost of a separate TCP+auth, which is the same shape we had pre-0.10. What we *do* keep from the new API: the watcher is now `'static` (no borrow on the watcher task's `client`), and the pipelining (one CHANGE_NOTIFY pre-issued so events during consumer processing don't fall in a re-arm gap). Stat calls for new/modified files still go through `VolumeManager::get(volume_id).get_metadata(...)` (the main session), so the cmdr-side `notify_mutation` cache patch from our own writes lands first regardless.

**Decision**: Watcher task is not stored on `SmbVolume`, only the cancel sender is
**Why**: The spawned task owns its own `Watcher` and `SmbClient`. Storing them on the struct alongside the cancel sender would just duplicate ownership without buying anything — `watcher.next_events()` is `&mut self`, so the task is the only thing that can drive it anyway. The `watcher_cancel: Mutex<Option<oneshot::Sender<()>>>` on the struct provides clean shutdown.

**Decision**: Watcher doesn't reconnect itself; it bails on connection errors (changed in 0.10 bump)
**Why**: Pre-0.10 the watcher had its own reconnect-with-backoff loop, separate from `SmbVolume::attempt_reconnect`. Two state machines tracking the same "is the session alive" question is a recipe for divergence — the watcher's internal retries swallowed real disconnections the FE reconnect manager would have surfaced. New model: when `next_events` errors with anything but `NOTIFY_ENUM_DIR`, the watcher's task returns. The next hot-path op on the volume hits the dead main session, `handle_smb_result` flips to `Disconnected`, the FE backoff cycle calls `attempt_reconnect`, which respawns the watcher (with a fresh dedicated session). One reconnect path, one source of truth. The watcher's session being separate from the main session means a watcher-only failure (e.g., a TCP hiccup on the watcher's connection) doesn't surface as a volume disconnect until the next mutation; that's the trade-off for keeping the connections independent.

**Decision**: Watcher debounces 200ms per batch, `FullRefresh` above 50 events per directory
**Why**: Prevents 1000 individual stat calls when 1000 files are copied. The 200ms window collects events that arrive in rapid succession. The 50-event threshold for `FullRefresh` avoids O(n) stat calls for bulk operations.

**Gotcha**: Watcher filenames from SMB use backslashes; must normalize to forward slashes
**Why**: SMB servers send paths like `papers\new-file.txt`. The watcher normalizes these to `papers/new-file.txt` before extracting parent directories and constructing display paths.

**Gotcha**: Watcher filenames are NFC (from server) but macOS mount paths are NFD
**Why**: SMB servers return NFC-normalized filenames. macOS filesystem paths use NFD. The watcher NFD-normalizes filenames before constructing display paths used for cache lookups.

**Gotcha (no longer applicable as of smb2 0.9)**: SMB write streaming fallback used to hold the client mutex for the whole upload
**Why**: Historically `FileWriter<'a>` borrowed `&'a mut Connection` from the `SmbClient`, so `write_from_stream` had to hold the client mutex for the duration of the streaming write. Under sustained concurrent pressure this two-phase pattern (brief `clone_session` fast-path probe → drop → long mutex-guarded streaming fallback) deadlocked. smb2 0.9 rebuilt `FileWriter` to own its `Connection` and `Arc<Tree>`, removing the borrow. The regression is pinned by `smb_integration_concurrent_streaming_writes_no_deadlock`. See the new Decision below (`write_from_stream` uses a cloned Connection + Arc<Tree>) for the current design.

**Decision**: `write_from_stream` uses a cloned `Connection` + `Arc<Tree>` via smb2 0.9's owned `FileWriter`
**Why**: smb2 0.9 made `FileWriter` own its `Connection` (cheap `Arc::clone`) and `Arc<Tree>` instead of borrowing `&'a mut Connection`. `write_from_stream` now calls `clone_session` once up front and drives both the compound fast-path AND the streaming fallback on the same owned `Connection` clone. The client mutex is held only for the few microseconds of `clone_session()`, never across I/O. The previous shape — fast-path on a clone, then drop and re-acquire the client for the streaming fallback — was the deadlock reproducer in Phase C against QNAP. The architectural property we get for free: N concurrent streaming writes on one `SmbVolume` pipeline N WRITE chains over a single SMB session, multiplexed by `MessageId` in smb2's receiver task. No external locking, no mutex contention on the hot copy path.

**Decision**: `SmbVolume` overrides `scan_for_copy_batch` to pipeline per-path stats over a single SMB session
**Why**: The copy pipeline's scan phase used to loop `scan_for_copy` per top-level source, N sequential RTTs on the wire before the copy phase could even start. For a 100-file copy over a ~60 ms Tailscale link that's ~5 s of serial stats. Fix 4 overrides `scan_for_copy_batch` to clone `smb2::Connection` per path under a brief client-mutex acquire (cheap `Arc::clone`, all clones multiplex over the same SMB session), release the lock, then drive `tree.stat(&mut conn, path)` on each clone inside a `FuturesUnordered`. Empty root paths skip the stat. Single-path batches fall through to `scan_recursive` so one-file drag-drops don't pay the batch machinery cost. Directories found during the stat phase recurse sequentially afterward. Parallel directory recursion is a future "Fix 5". Measured 6.5× wall-clock win at 100 × 10 KB: 6.11 s → 947 ms. See `docs/notes/phase4-rtt-investigation.md` for the wire trace. **Oracle layered on top (M2b of fresh-listing-reuse)**: before the pipelined-stat block runs, every input path's parent is checked against the fresh-listing oracle (`try_get_watched_listing(volume_id, parent)`). Oracle-served paths get their size + `is_directory` from the cached `FileEntry` and are removed from the leftover set; only the leftover paths go through the pipelined stat. Decision is per-parent: one batch can mix oracle-served and pipelined-stat paths, and if every path resolves via the oracle the stat pipeline is skipped entirely.

**Decision**: `MtpVolume` overrides `scan_for_copy_batch_with_progress` to group selected paths by parent and list each parent once
**Why**: MTP has no single-file stat call: `get_metadata(path)` lists the parent directory and searches by name. A naive scan that called `get_metadata` per path would re-list `/DCIM/Camera` (15k entries, ~17 s over USB) for every selected photo. The override groups the input paths by parent, calls `list_directory(parent, on_progress)` once per unique parent, and indexes the entries by name for O(1) lookups. **Oracle layered on top (M2b of fresh-listing-reuse)**: before listing a parent, the override consults `try_get_watched_listing(volume_id, parent)`; on hit, the cached entries replace the listing call entirely (no USB I/O for that parent). On miss the existing single-listing-per-parent path runs, so cold-cache perf is preserved. Decision is per-parent; one batch can mix watcher-fresh and cold parents.

**Decision**: `Volume::scan_for_copy_batch` returns `BatchScanResult { aggregate, per_path }` (changed in Phase 4 Fix 4)
**Why**: The copy engine needs per-source type+size hints (`is_directory`, `total_bytes`) for its `source_hints` map, which seeds conflict detection and feeds the SMB compound fast-path's size hint. Pre-Fix-4 it paid N separate `scan_for_copy` calls to collect both aggregate stats and per-path info. Returning a `BatchScanResult` lets the batch scan surface both at once: one trait call, one round-trip to each backend. Scan-preview callers that only want the aggregate just read `.aggregate`. `LocalPosixVolume` and `InMemoryVolume` inherit the default (serial per-path loop, cheap); `MtpVolume` preserves its "group by parent dir" batch; `SmbVolume` overrides with the pipelined stat path.

**Decision**: `SmbVolume` has a compound fast-path in `open_read_stream_with_hint` and `write_from_stream` for files ≤ `max_read_size` / `max_write_size`
**Why**: The streaming open+read+close sequence costs 3 RTTs per file. For small files (typical 10 KB copies on a NAS) that dominates wall-clock at high-latency links (~60 ms RTT → ~180 ms/file just for protocol overhead, not data). `smb2` already exposes `Tree::read_file_compound` (CREATE+READ+CLOSE in a single compound frame = 1 RTT) and `Tree::write_file_compound` (CREATE+WRITE+FLUSH+CLOSE = 1 RTT). The copy pipeline feeds per-file size hints from the pre-copy scan; when the size is known and fits in one READ/WRITE, we take the compound path. Falls back cleanly to the streaming reader/writer when the hint is missing or the file is too big. Small compound reads return a `Vec<u8>` wrapped as a single-chunk `InlineReadStream` so the consumer API stays shaped the same. See `docs/notes/phase4-rtt-investigation.md` for the measurement.

**Decision**: Phase 4 collapsed `export_to_local` / `import_from_local` onto `open_read_stream` / `write_from_stream`
**Why**: The three pre-Phase-4 copy paths (local↔local, local↔volume, volume↔volume) duplicated the same "open a reader, pipe to a writer" logic in three different shapes. The APFS clonefile fast path is the only one with a real capability difference. Collapsing the other two to a single streaming path means new backends (S3, WebDAV, FTP) implement two methods instead of four, concurrency lives in one place (`volume_copy.rs`, Phase 4.2), and features like resume / checksum / progress benefit every direction at once. See `docs/notes/phase4-volume-copy-unification.md`.

**Decision**: Progress callbacks use `&dyn Fn(u64, u64) -> ControlFlow<()>`, not `FnMut`
**Why**: The Volume trait is object-safe (`dyn Volume`), so callbacks must be `Fn` (not `FnMut`). Callers use `AtomicU64` for byte counters and `Cell<Instant>` for timestamps to mutate state inside a `Fn` closure. This avoids needing `RefCell` or `Mutex` in the hot path.

**Gotcha**: `write_from_stream` is a mutation; call `notify_mutation` on success on backends with unreliable out-of-band notifications
**Why**: `write_from_stream` originally relied on the SMB CHANGE_NOTIFY watcher / MTP USB event loop to patch `LISTING_CACHE` after a cross-volume copy. Both are lossy under load: the smb2 watcher keeps one outstanding `CHANGE_NOTIFY` request at a time, and Samba drops events that arrive between consecutive responses (real reproduction: 9 files copied, 4 events delivered, destination pane showed 4 files until the user navigated away and back — files written fine, only the cache was stale). Many MTP devices emit no self-mutation events at all. The other mutation methods (`create_file`, `create_directory`, `delete`, `rename`) already call `self.notify_mutation(...)` after success; `write_from_stream` must too. `LocalPosixVolume` is the exception: FSEvents is reliable, so local mutations don't need the extra patch. The "After each successful mutation, call `self.notify_mutation(...)`" rule in the Tier 2 checklist includes `write_from_stream`.

**Gotcha**: On macOS, never use `statvfs` alone for disk space. Use `NSURLVolumeAvailableCapacityForImportantUsageKey`
**Why**: `statvfs` reports only physically free blocks and ignores purgeable space (APFS snapshots, iCloud caches), which can be tens of GB. This causes inconsistent numbers between the status bar (NSURL API) and copy validation (`statvfs`), and prematurely blocks copies that would succeed. `get_space_info_for_path` calls `crate::volumes::get_volume_space()` on macOS and falls back to `statvfs` on Linux.

## Testing

- **E2E error injection**: The `Volume` trait has an `inject_error(&self, errno: i32)` method behind the `playwright-e2e` feature flag. `LocalPosixVolume` and `InMemoryVolume` implement it. The next `list_directory` call returns the injected errno, then clears it (single-shot, so retry tests work). Default is no-op.
- `in_memory_test.rs`: unit tests for `InMemoryVolume` (CRUD, sorting, concurrency, stress 50k entries)
- `inmemory_test.rs`: integration tests combining `InMemoryVolume` + `VolumeManager`, streaming state, sort helpers
- `local_posix_test.rs`: real-FS tests (write ops, symlinks, copy, space info) using `std::env::temp_dir()`
- `manager.rs` inline tests: concurrent registration/read/write-mix scenarios
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
