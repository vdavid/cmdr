# Volume abstraction details

Pull-tier docs for `file_system/volume/`: architecture, flows, and decision rationale. Must-know invariants and gotchas
live in [CLAUDE.md](CLAUDE.md).

This module defines the `Volume` trait (the core abstraction for all storage backends in Cmdr) and the `VolumeManager`
registry. Per-backend implementations live in [`backends/`](backends/CLAUDE.md). The friendly-error system (used by
every backend to turn raw OS errors into warm user-facing copy) lives in [`friendly_error/`](friendly_error/CLAUDE.md).

## Purpose

Every file system operation (listing, copy, rename, delete, indexing, watching) goes through a `Volume`. The trait hides
the differences between a local POSIX path, an MTP device, an in-memory test fixture, and future backends (SMB, S3,
FTP). Callers never touch the filesystem directly; they call `Volume` methods with **paths relative to the volume root**.

## Key files

- **`mod.rs`**: `Volume` trait (async: most methods return `Pin<Box<dyn Future>>`; sync: `name`, `root`, `supports_*`, `local_path`, `space_poll_interval`), `VolumeScanner`, `VolumeWatcher`, `VolumeReadStream` traits, `MutationEvent` enum, shared types (`VolumeError`, `SpaceInfo`, `CopyScanResult`, `ScanConflict`, `SourceItemInfo`)
- **`manager.rs`**: `VolumeManager`: thread-safe `RwLock<HashMap>` registry; supports a default volume
- **`backends/`**: Per-backend `Volume` impls (`LocalPosixVolume`, `MtpVolume`, `SmbVolume` + watcher, `InMemoryVolume`). See [`backends/CLAUDE.md`](backends/CLAUDE.md).
- **`friendly_error/`**: User-facing error messages + provider detection. See [`friendly_error/CLAUDE.md`](friendly_error/CLAUDE.md).

## Architecture

```
VolumeManager (registry)
  └─ Arc<dyn Volume>  (async trait: most methods return Pin<Box<dyn Future>>)
        ├─ LocalPosixVolume   → real FS (spawn_blocking for I/O), FSEvents watcher, jwalk scanner
        ├─ MtpVolume          → direct async MTP ops
        ├─ SmbVolume          → direct async smb2 ops (direct protocol, not OS mount)
        └─ InMemoryVolume     → HashMap, test/stress use only
```

`VolumeScanner` and `VolumeWatcher` are separate sub-traits returned by `Volume::scanner()` and `Volume::watcher()`.
Only `LocalPosixVolume` implements both today.

## Trait capability model

Optional methods default to `Err(VolumeError::NotSupported)` or `false`, so new volume types can be added incrementally. Key capability flags:

- `supports_watching()`: enables the `notify`-based *listing* file watcher in `operations.rs` (separate from the `VolumeWatcher` trait used for drive indexing). `MtpVolume` returns `false` (it has its own USB event loop).
- `supports_export()`: "this volume can stream its bytes via `open_read_stream`" (so it can act as a source in a cross-volume copy). Gates the copy dialog's "copy from this volume" UI. Local, MTP, SMB, and InMemory return `true`.
- `supports_streaming()`: enables cross-volume transfers via `open_read_stream` / `write_from_stream`. `LocalPosixVolume`, `MtpVolume`, `SmbVolume`, and `InMemoryVolume` all return `true`. This is the universal byte path for every non-APFS-clone copy. New backends just implement the two streaming methods to get cross-volume copy for free.
- `max_concurrent_ops()`: how many streaming copies the copy engine can drive in parallel against this volume. The batch copy path takes `min(src.max_concurrent_ops(), dst.max_concurrent_ops(), 32)` and spawns that many `FuturesUnordered` tasks. Defaults to `1` (safe for any new backend). Current values: `LocalPosixVolume` returns `available_parallelism()/2` clamped to 4..=16; `SmbVolume` returns 10 (currently hardcoded; eventually wires to `network.smbConcurrency`); `MtpVolume` returns 1 (USB bulk transport is serial); `InMemoryVolume` returns 32.
- `local_path()`: returns `Some` only for local volumes; allows `copyfile(2)` fast-path in copy operations. `SmbVolume` returns `None` so copies go through smb2 instead of the slow OS mount.
- `supports_local_fs_access()`: whether `std::fs` operations (stat, read_dir) work on this volume's paths. Default `true`. `MtpVolume` and `SmbVolume` return `false`. Used to skip the legacy synthetic entry diff path (now superseded by `notify_mutation`).
- `notify_mutation(volume_id, parent_path, mutation)`: called after a successful mutation (create, delete, rename) to update the listing cache immediately. Default impl uses `std::fs` (works for `LocalPosixVolume`). `SmbVolume` and `MtpVolume` override to use their own protocol's `get_metadata`. Fire-and-forget, no error propagation.
- `smb_connection_state()`: returns `Some(SmbConnectionState)` for SMB volumes (green/yellow indicator in volume picker). Default `None`. Only `SmbVolume` implements it.
- `attempt_reconnect()`: tries to rebuild the volume's underlying session in place after a transient connection loss. Default `Err(NotSupported)`. Only `SmbVolume` overrides today; the Tauri command `reconnect_smb_volume` and the FE reconnect manager call this on each backoff tick. Idempotent and single-flight: concurrent callers wait on the same in-flight attempt instead of dog-piling the server.
- `reconnect_with_credentials(username, password)`: reconnect with freshly-entered credentials, replacing whatever was cached. Default `Err(NotSupported)`; `SmbVolume` persists the new password (so the next reconnect is silent) then runs `attempt_reconnect`. Invoked by the Tauri command `reconnect_smb_volume_with_credentials` behind the "Sign in" prompt shown after an auth-failure reconnect give-up.
- `on_unmount()`: lifecycle hook called before unregistration. `SmbVolume` uses it to disconnect its smb2 session. Default is no-op.
- `scanner()` / `watcher()`: drive indexing hooks; `None` by default.
- `space_poll_interval()`: recommended interval for the live disk-space poller (`space_poller.rs`). Default 2 s (local volumes). `SmbVolume` and `MtpVolume` override to 5 s. `InMemoryVolume` returns `None` (no polling). The poller uses this to tick each volume at its own cadence.
- `create_directory_errors_on_existing_dir()`: whether `create_directory` reliably returns `VolumeError::AlreadyExists` for an existing same-name dir. Default `true` (LocalPosix, SMB, InMemory all do). `MtpVolume` overrides to `false` — the MTP protocol allows same-name sibling objects and `create_folder` silently makes a duplicate, so the folder-merge walker (`write_operations/transfer/volume_strategy.rs`) pre-checks existence on MTP instead of trusting the create to error. A blindly-created duplicate would make a merge target the wrong directory.
- `listing_is_watched(path)`: returns `true` when this volume's cached listing for `path` is being kept in sync by a live watcher. Three consumers today:
    1. `file_system::listing::caching::try_get_watched_listing` (the "fresh-listing oracle") — write-op pre-flight scans reuse a cached listing instead of re-reading.
    2. `write_operations::delete::scan_volume_recursive` (the oracle-aware delete walker) — same idea, per-recursion-level.
    3. The `refresh_listing` Tauri command (`commands/file_system/listing.rs`) — short-circuits the post-transfer redundant `list_directory` re-read entirely when the volume is keeping the cache fresh via `notify_mutation`. Without this, a 1k-entry MTP folder paid ~17 s + USB session collision after every transfer outcome, wedging the next user op.
  Default `false` so a new backend without a real watcher won't accidentally claim freshness. **Freshness contract**: a `true` result does NOT mean the cache is byte-perfect with the device right now. Every backend has a debounce or settling window between a real change and the cache reflecting it: local FS ≈ 10 ms (FSEvents coalesce), SMB 200 ms (watcher debounce; > 50 events/dir triggers a `FullRefresh`), MTP 500 ms (event debouncer plus per-device polling; many cameras emit no events at all, so on those `true` means only "the device is reachable"). Callers must treat the result as "fresh as our most recent observation" — the same guarantee a `list_directory` call gives. The MTP and SMB checks are volume-level, not path-level: when the gate flips true, every path on that volume becomes oracle-eligible.

## Conflict classification fields

`scan_for_conflicts` is what powers the upfront Transfer dialog's "N folders will merge / N conflicts" classification, so each `ScanConflict` carries the type of both sides:

- `ScanConflict.source_is_directory` / `dest_is_directory`: let the FE tell a dir-vs-dir collision (a silent merge, never a conflict) from a file clash or a cross-type clash (a real conflict). The FE counts only the latter toward `totalConflictCount` and the bulk-skip set; dir-vs-dir surfaces as the "will merge" info line. The source flag comes from the caller-supplied `SourceItemInfo`; the dest flag from the dest listing entry the scan already lists.
- `SourceItemInfo.is_directory`: the caller (the conflict-scan command) knows each source's type from the `FileEntry` it already holds and passes it in, so backends copy it straight onto `ScanConflict.source_is_directory` without a per-source `is_directory` round-trip.

The sibling per-file conflict event (`write_operations::types::WriteConflictEvent`, emitted mid-operation when a deep clash needs a human) carries `source_size: Option<u64>` for the same reason: a cross-type clash can now surface on the same-volume fast path, where no pre-flight scan ran, so a folder source's size is genuinely unknown. The FE renders `(unknown)` for a `None` source size, and `size_difference` collapses to `None` when either side is unknown. (`ScanConflict.source_size` itself stays a plain `u64` — the upfront scan always has a size for the items it lists.)

Folders always merge (see `write_operations/transfer/CLAUDE.md` § "Dir-vs-dir is NEVER a conflict"), so these flags exist purely to classify, never to gate a folder behind a prompt.

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
- [ ] Register the volume via `VolumeManager::register_if_absent` (not `register`; see "Key decisions" below).
- [ ] Add unit tests using a fake/in-memory harness or real fixtures.

### Tier 2: make it writable (recommended for real-world use)

Everything below is optional per the trait (methods default to `Err(NotSupported)` or `false`), but a read-only volume is rarely useful:

- [ ] Implement `create_directory`, `create_file`, `delete`, `rename`.
- [ ] After each successful mutation, call `self.notify_mutation(&volume_id, parent_path, MutationEvent::...)` so the listing cache updates immediately. Override `notify_mutation` on the trait if your backend can answer `get_metadata` faster than `std::fs::metadata` would (MTP and SMB do this).
- [ ] Return `supports_streaming() = true` and implement `open_read_stream` + `write_from_stream`. These are the byte path for every cross-volume copy. The Copy dialog uses them for "this volume ↔ anywhere" transfers.
- [ ] Return `supports_export() = true` if the volume should appear as a copy source in the UI.
- [ ] Implement `scan_for_copy` (count + bytes) and `scan_for_conflicts` (destination collision detection). These feed the Copy dialog's pre-flight. `scan_for_conflicts` takes a `SourceItemInfo` per source and emits a `ScanConflict` per collision; see "Conflict classification fields" above for the `is_directory` flags it must populate.
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
- [ ] Add a branch to `detect_provider` / `provider_suggestion` in [`friendly_error/provider.rs`](friendly_error/CLAUDE.md) if there's a recognizable path shape or fs type worth calling out in friendly errors.
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
| `create_directory_errors_on_existing_dir` | ✅ (default) | ❌ (protocol allows dup names) | ✅ (default) | ✅ (default) |
| `scanner` / `watcher` (indexing) | ✅ / ✅          | ❌                      | ❌                        | ❌                 |
| `on_unmount`                | default              | default                 | ✅ drops smb2 session     | default            |
| `smb_connection_state`      | `None`               | `None`                  | ✅                        | `None`             |
| `space_poll_interval`       | 2 s (default)        | 5 s                     | 5 s                       | `None`             |
| `max_concurrent_ops`        | 4..=16 (core-based)  | 1 (USB bulk serial)     | 10 (eventually setting)   | 32                 |

Legend: ✅ = implemented, ❌ = opted out (default or explicitly), ⚠️ = implemented but suboptimal (memory-heavy or otherwise worth revisiting).

When adding a new volume, add a column for it and fill in each row. The matrix doubles as a self-review: gaps will stare back at you.

## Streaming patterns

Reads and writes have different shapes because the consumer relationship is different:

- **Reads** return a `VolumeReadStream` that an external caller polls. The download handle has to live past the function call and cross async contexts. That's where the lifetime/ownership gymnastics below come from.
- **Writes** consume a stream (or a local file) inside the method itself. The chunk loop is the consumer, so there's nothing to hand off. For backends with a `'static` writer (smb2 0.9's owned `FileWriter`, mtp-rs's `upload_stream`), drive the writer directly on a cloned session handle — no lock held across I/O. For backends whose writer borrows from the session, hold the session lock for the chunk loop's duration. `SmbVolume::write_from_stream` is the reference implementation: clone the session once, open the smb2 `FileWriter` on the clone, loop `write_chunk`, call `finish()` on success or `abort()` on cancel. No task spawn, no channel, no self-referential struct, no client mutex held while WRITEs are in flight.

The rest of this section is about **read-side** lifetime handling. Which pattern to pick depends on whether your protocol SDK's download handle is `'static` or borrowed.

### Pattern A: own the download (use when the SDK's download type is `'static`)

If the SDK gives you a download handle that owns its session internally and doesn't borrow from anything, store it directly in your stream struct. **Example: `MtpReadStream`** (`backends/mtp.rs`).

```rust
struct MtpReadStream {
    download: Option<mtp_rs::FileDownload>,  // 'static, no lifetime parameter
    total_size: u64,
    bytes_read: u64,
}
```

`next_chunk()` calls `download.as_mut()?.next_chunk().await` directly, no task spawn, no channel. `Drop` cancels the transfer (see the MtpReadStream Drop gotcha in `backends/CLAUDE.md` for the detached-task cancel pattern).

### Pattern B: channel-backed stream (use when the SDK's download type borrows `&mut Connection`)

If the SDK's download handle holds a borrow against the session (like `smb2::FileDownload<'a>` borrowing `&'a mut Connection`), you can't stuff it into a `'static` struct. Use a background producer task that holds an `OwnedMutexGuard` over the session, drives the download, and feeds chunks through a bounded mpsc channel. **Example: `SmbReadStream`** (`backends/smb.rs` → `open_smb_download_stream`).

Key building blocks:
- `Arc<tokio::sync::Mutex<Session>>` so the task can call `lock_owned()` and own the guard until done.
- Bounded mpsc channel (capacity ~4) for backpressure. Peak memory is `capacity × chunk_size`, a few MB regardless of file size.
- Oneshot channel for the total size (reported before the first chunk so the consumer sees the correct `total_size()` synchronously).
- Oneshot channel for cancellation. `Drop` on the stream sends the signal, producer breaks its loop and releases the guard.
- If the session state (connection health) can transition on protocol errors, wrap the state atomic in `Arc<AtomicU8>` so the task can update it from outside `&self` context.

### Anti-pattern: pre-buffering the whole file

Don't slurp the whole file into a `Vec<u8>` before yielding chunks. For an 8 GB file that means an 8 GB allocation. If the consumer API is stream-shaped, the producer should stream too.

The same rule applies to write paths: `write_from_stream` must drive the backend's chunk-by-chunk writer (for example, smb2's `FileWriter`) rather than slurping the source into a `Vec<u8>` first. See the "Streaming requirement" section on each Volume trait method's doc comment.

## Path handling gotchas

- **`LocalPosixVolume::resolve`**: accepts empty, `.`, relative, or absolute paths. Three-way branch for absolute paths: (1) already starts with volume root, used as-is; (2) volume root is `/`, absolute path passed through unchanged; (3) otherwise, leading `/` stripped and joined to root. This handles frontend sending full absolute paths.
- **`MtpVolume::to_mtp_path`**: strips the `mtp://{device}/{storage}/` URL prefix and leading slashes, returning the bare relative path the MTP library expects.
- **`InMemoryVolume::normalize`**: always resolves to an absolute path anchored at `/`.

## Integration status

`LocalPosixVolume` is wired into the indexing subsystem. `VolumeManager` is actively used.

## Git delegation hooks

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

**Decision**: `LocalPosixVolume` uses `symlink_metadata` for `exists()` instead of `Path::exists()`
**Why**: `Path::exists()` follows symlinks. A dangling symlink returns `false`, which would make the volume claim a file doesn't exist when it visibly does in a directory listing. `symlink_metadata` detects the symlink itself, matching what the user sees.

**Decision**: `notify_mutation` lives on the Volume trait, not in Tauri commands
**Why**: Every mutation method (`create_file`, `create_directory`, `delete`, `rename`) knows what changed. Adding the notification call at the end of each method keeps it colocated with the mutation. The alternative (notification calls in every Tauri command) is fragile, easy to miss a call site.

**Decision**: `Volume::scan_for_copy_batch` returns `BatchScanResult { aggregate, per_path }`
**Why**: The copy engine needs per-source type+size hints (`is_directory`, `total_bytes`) for its `source_hints` map, which seeds conflict detection and feeds the SMB compound fast-path's size hint. Returning both at once (one trait call, one round-trip per backend) avoids the N separate `scan_for_copy` calls that an aggregate-only batch API would force. Scan-preview callers that only want the aggregate just read `.aggregate`. `LocalPosixVolume` and `InMemoryVolume` inherit the default (serial per-path loop, cheap); `MtpVolume` preserves its "group by parent dir" batch; `SmbVolume` overrides with the pipelined stat path. See [`backends/CLAUDE.md`](backends/CLAUDE.md) for the per-backend overrides.

**Decision**: All cross-volume copy flows through `open_read_stream` / `write_from_stream`
**Why**: The three plausible copy paths (local↔local, local↔volume, volume↔volume) all reduce to "open a reader, pipe to a writer." The APFS clonefile fast path is the only one with a real capability difference. Routing the other two through a single streaming path means new backends (S3, WebDAV, FTP) implement two methods instead of four, concurrency lives in one place (`volume_copy.rs`), and features like resume / checksum / progress benefit every direction at once. Don't reintroduce `export_to_local` / `import_from_local`. See `docs/notes/phase4-volume-copy-unification.md`.

**Decision**: `Volume::list_directory` / `scan_for_copy_batch_with_progress` callbacks take a `ListingProgress { files, dirs, bytes }` struct (not `Fn(usize)` — files-only).
**Why**: A files-only count makes MTP and Direct SMB scan previews show "0 bytes / N files / 0 dirs" climbing through the scan, because `run_volume_scan_preview` has nothing else to forward to the mid-stream `scan-preview-progress` event. The struct lets each backend track running file count, dir count, and byte total as it enumerates entries (MTP per-handle in `mtp/connection/directory_ops.rs`, SMB in a single tally pass after `list_directory_impl`, the default trait impl in `scan_for_copy_batch_with_progress`). Self-documenting field semantics; room to grow (symlinks, special files). Streaming-listing UI callers (`commands/file_system/listing.rs`) read `progress.entries()` (= `files + dirs`) which preserves their "Loaded N entries…" display. The baseline-shift logic in `run_oracle_aware_batch_scan` shifts files / dirs / bytes together so cross-group accumulation stays cumulative. Pinned by `scan_preview_listing_progress_tests`.

**Decision**: Progress callbacks use `&dyn Fn(u64, u64) -> ControlFlow<()>`, not `FnMut`
**Why**: The Volume trait is object-safe (`dyn Volume`), so callbacks must be `Fn` (not `FnMut`). Callers use `AtomicU64` for byte counters and `Cell<Instant>` for timestamps to mutate state inside a `Fn` closure. This avoids needing `RefCell` or `Mutex` in the hot path.

**Decision**: `on_unmount()` trait method instead of `Any` downcasting
**Why**: Avoids runtime type checking, extensible for future volume types (S3, FTP might also need cleanup), consistent with the trait's design of optional methods with default no-ops.

## Gotchas

**Gotcha**: `LocalPosixVolume::resolve` has a three-way branch for absolute paths
**Why**: The frontend sometimes sends full absolute paths (like `/Users/alice/Documents`), not paths relative to the volume root. If the volume root is `/Users/alice/Dropbox`, the resolve logic must detect whether the absolute path is already inside the root (pass through), whether the root is `/` (pass through), or neither (strip leading `/` and join). Getting this wrong silently serves the wrong directory.

**Gotcha**: `write_from_stream` is a mutation; call `notify_mutation` on success on backends with unreliable out-of-band notifications
**Why**: `write_from_stream` originally relied on the SMB CHANGE_NOTIFY watcher / MTP USB event loop to patch `LISTING_CACHE` after a cross-volume copy. Both are lossy under load: the smb2 watcher keeps one outstanding `CHANGE_NOTIFY` request at a time, and Samba drops events that arrive between consecutive responses (real reproduction: 9 files copied, 4 events delivered, destination pane showed 4 files until the user navigated away and back — files written fine, only the cache was stale). Many MTP devices emit no self-mutation events at all. The other mutation methods (`create_file`, `create_directory`, `delete`, `rename`) already call `self.notify_mutation(...)` after success; `write_from_stream` must too. `LocalPosixVolume` is the exception: FSEvents is reliable, so local mutations don't need the extra patch. The "After each successful mutation, call `self.notify_mutation(...)`" rule in the Tier 2 checklist includes `write_from_stream`.

**Gotcha**: On macOS, never use `statvfs` alone for disk space. Use `NSURLVolumeAvailableCapacityForImportantUsageKey`
**Why**: `statvfs` reports only physically free blocks and ignores purgeable space (APFS snapshots, iCloud caches), which can be tens of GB. This causes inconsistent numbers between the status bar (NSURL API) and copy validation (`statvfs`), and prematurely blocks copies that would succeed. `get_space_info_for_path` calls `crate::volumes::get_volume_space()` on macOS and falls back to `statvfs` on Linux.

## Testing

- **E2E error injection**: The `Volume` trait has an `inject_error(&self, errno: i32)` method behind the `playwright-e2e` feature flag. `LocalPosixVolume` and `InMemoryVolume` implement it. The next `list_directory` call returns the injected errno, then clears it (single-shot, so retry tests work). Default is no-op.
- `inmemory_test.rs`: integration tests combining `InMemoryVolume` + `VolumeManager`, streaming state, sort helpers
- `manager.rs` inline tests: concurrent registration/read/write-mix scenarios
- `mtp_scan_oracle_tests.rs`, `smb_scan_oracle_tests.rs`: oracle-aware batch-scan integration tests for MTP and SMB

Per-backend tests live colocated with their backend in `backends/`. See [`backends/DETAILS.md`](backends/DETAILS.md) §
"Testing".
