# Volume abstraction

This module defines the `Volume` trait — the core abstraction for all storage backends in Cmdr — and the `VolumeManager` registry.

## Purpose

Every file system operation (listing, copy, rename, delete, indexing, watching) goes through a `Volume`. The trait hides the differences between a local POSIX path, an MTP device, an in-memory test fixture, and future backends (SMB, S3, FTP). Callers never touch the filesystem directly; they call `Volume` methods with **paths relative to the volume root**.

## Key files

| File | Role |
|---|---|
| `mod.rs` | `Volume` trait, `VolumeScanner`, `VolumeWatcher`, `VolumeReadStream` traits, `MutationEvent` enum, shared types (`VolumeError`, `SpaceInfo`, `CopyScanResult`, `ScanConflict`, `SourceItemInfo`) |
| `manager.rs` | `VolumeManager` — thread-safe `RwLock<HashMap>` registry; supports a default volume |
| `local_posix.rs` | `LocalPosixVolume` — real filesystem; delegates listing to `file_system::listing`, indexing to `indexing::scanner`, watching to `indexing::watcher` (FSEvents), copy scanning via `walkdir`. Uses `libc::statvfs` FFI for space info. |
| `mtp.rs` | `MtpVolume` — MTP device storage; synchronous `Volume` trait bridged to async MTP calls via `tokio::runtime::Handle::block_on`. Gated with `#[cfg(any(target_os = "macos", target_os = "linux"))]`. |
| `smb.rs` | `SmbVolume` — SMB share storage; synchronous `Volume` trait bridged to async smb2 calls via `Handle::block_on`. Uses `Mutex<Option<(SmbClient, Tree)>>` + `AtomicU8` connection state. Gated with `#[cfg(any(target_os = "macos", target_os = "linux"))]`. |
| `in_memory.rs` | `InMemoryVolume` — `RwLock<HashMap>` store for tests; also used for stress tests (`with_file_count`) |

## Architecture

```
VolumeManager (registry)
  └─ Arc<dyn Volume>
        ├─ LocalPosixVolume   → real FS, FSEvents watcher, jwalk scanner
        ├─ MtpVolume          → async MTP ops via block_on (spawn_blocking context)
        ├─ SmbVolume          → async smb2 ops via block_on (direct protocol, not OS mount)
        └─ InMemoryVolume     → HashMap, test/stress use only
```

`VolumeScanner` and `VolumeWatcher` are separate sub-traits returned by `Volume::scanner()` and `Volume::watcher()`. Only `LocalPosixVolume` implements both today.

## Trait capability model

Optional methods default to `Err(VolumeError::NotSupported)` or `false`, so new volume types can be added incrementally. Key capability flags:

- `supports_watching()` — enables the `notify`-based *listing* file watcher in `operations.rs` (separate from the `VolumeWatcher` trait used for drive indexing). `MtpVolume` returns `false` (it has its own USB event loop).
- `supports_export()` — enables copy/move UI. Local, MTP, and SMB return `true`.
- `supports_streaming()` — enables chunked MTP-to-MTP transfers. Only `MtpVolume` returns `true`.
- `local_path()` — returns `Some` only for local volumes; allows `copyfile(2)` fast-path in copy operations. `SmbVolume` returns `None` so copies go through smb2 instead of the slow OS mount.
- `supports_local_fs_access()` — whether `std::fs` operations (stat, read_dir) work on this volume's paths. Default `true`. `MtpVolume` and `SmbVolume` return `false`. Used to skip the legacy synthetic entry diff path (now superseded by `notify_mutation`).
- `notify_mutation(volume_id, parent_path, mutation)` — called after a successful mutation (create, delete, rename) to update the listing cache immediately. Default impl uses `std::fs` (works for `LocalPosixVolume`). `SmbVolume` and `MtpVolume` override to use their own protocol's `get_metadata`. Fire-and-forget, no error propagation.
- `smb_connection_state()` — returns `Some(SmbConnectionState)` for SMB volumes (green/yellow indicator in volume picker). Default `None`. Only `SmbVolume` implements it.
- `on_unmount()` — lifecycle hook called before unregistration. `SmbVolume` uses it to disconnect its smb2 session. Default is no-op.
- `scanner()` / `watcher()` — drive indexing hooks; `None` by default.

## Path handling gotchas

- **`LocalPosixVolume::resolve`**: accepts empty, `.`, relative, or absolute paths. Three-way branch for absolute paths: (1) already starts with volume root — used as-is, (2) volume root is `/` — absolute path passed through unchanged, (3) otherwise — leading `/` stripped and joined to root. This handles frontend sending full absolute paths.
- **`MtpVolume::to_mtp_path`**: strips the `mtp://{device}/{storage}/` URL prefix and leading slashes, returning the bare relative path the MTP library expects.
- **`InMemoryVolume::normalize`**: always resolves to an absolute path anchored at `/`.

## MTP threading

`MtpVolume` is called from `tokio::task::spawn_blocking`, so `Handle::block_on` is safe inside its methods. However, `write_from_stream` must collect all chunks **before** entering `block_on` to avoid nested-runtime panics (stream chunks also use `block_on` internally).

## SMB auto-upgrade lifecycle

SMB mounts are automatically upgraded to `SmbVolume` (direct smb2 connection) in two scenarios:

1. **Startup** (`file_system::upgrade_existing_smb_mounts`): Scans registered volumes for `smbfs` type. Waits for mDNS
   discovery to reach `Active` state (polls every 500ms, up to 15s) because Keychain credentials are keyed by hostname
   (from mDNS), not IP (from `statfs`). Uses `tauri::async_runtime::spawn` (not `tokio::spawn` — runs during `setup()`
   before Tokio is fully available). Emits `volumes-changed` after upgrades so the frontend refreshes indicators.

2. **Mount detection** (`volumes/watcher.rs::try_upgrade_smb_mount`): When FSEvents detects a new volume in `/Volumes/`
   and it's `smbfs`, spawns a background upgrade attempt. By this point mDNS is already active.

Both paths check the `network.directSmbConnection` setting (global `AtomicBool`). Both are best-effort — failures log a
warning and the volume stays as `LocalPosixVolume`. The "Connect directly" UI action (`upgrade_to_smb_volume` command)
provides a manual upgrade path.

## Integration status

`LocalPosixVolume` is wired into the indexing subsystem. `VolumeManager` is actively used.

## Key decisions

**Decision**: Trait with optional methods defaulting to `NotSupported`/`false`
**Why**: New volume types (SMB, S3, FTP) will have vastly different capability sets. Forcing every implementor to stub out every method would be noisy and error-prone. Defaults let new backends start with just `list_directory` + `get_metadata` and opt in to capabilities incrementally. The alternative — a capabilities bitfield — would require runtime checks everywhere and couldn't express return-type differences.

**Decision**: `VolumeScanner` and `VolumeWatcher` are separate sub-traits, not part of `Volume`
**Why**: Scanning and watching have their own lifetimes, threading models, and state (handles, channels). Folding them into `Volume` would force every volume to carry scanner/watcher state even if it never indexes. Returning `Option<Box<dyn VolumeScanner>>` keeps the core trait lightweight.

**Decision**: `VolumeManager` uses `RwLock<HashMap>` (not `DashMap` or `Mutex`)
**Why**: Volume registration/unregistration is rare (mount/unmount events); reads are frequent (every file operation resolves a volume). `RwLock` gives concurrent read access without pulling in an extra dependency. `DashMap` would work but is heavier than needed for a registry that rarely exceeds ~10 entries.

**Decision**: `VolumeManager::register_if_absent` for watcher registrations
**Why**: When the mount flow pre-registers an `SmbVolume`, the FSEvents watcher would overwrite it with a `LocalPosixVolume` via `register`. `register_if_absent` is a no-op if a volume is already registered, preserving the `SmbVolume`. The existing `register` (overwrite) is kept for explicit replacement (like SmbVolume replacing itself on reconnect).

**Decision**: `MtpVolume` bridges sync `Volume` trait to async MTP calls via `Handle::block_on`
**Why**: The `Volume` trait is synchronous because local filesystem ops are blocking and shouldn't touch the async executor. MTP operations are inherently async (USB bulk transfers), so `block_on` bridges the gap. This is safe because MTP methods are always called from `spawn_blocking` contexts (separate OS thread pool), avoiding nested-runtime panics.

**Decision**: `VolumeError` stores `String` messages, not the original `std::io::Error`
**Why**: `std::io::Error` is not `Clone`, but `VolumeError` needs to be `Clone` for ergonomic error propagation across thread boundaries and for serialization to the frontend. Storing the formatted message loses the original error type but keeps the information that matters for user-facing error messages.

**Decision**: `LocalPosixVolume` uses `symlink_metadata` for `exists()` instead of `Path::exists()`
**Why**: `Path::exists()` follows symlinks — a dangling symlink returns `false`, which would make the volume claim a file doesn't exist when it visibly does in a directory listing. `symlink_metadata` detects the symlink itself, matching what the user sees.

## Gotchas

**Gotcha**: `write_from_stream` in `MtpVolume` must collect all chunks *before* entering `block_on`
**Why**: `MtpReadStream::next_chunk()` itself calls `block_on` internally to read from the async download stream. If `write_from_stream` entered `block_on` first and then called `next_chunk` inside it, you'd get a nested `block_on` panic. The workaround is to eagerly materialize all chunks into a `Vec<Bytes>`, then do one `block_on` for the upload.

**Gotcha**: `LocalPosixVolume::resolve` has a three-way branch for absolute paths
**Why**: The frontend sometimes sends full absolute paths (like `/Users/alice/Documents`), not paths relative to the volume root. If the volume root is `/Users/alice/Dropbox`, the resolve logic must detect whether the absolute path is already inside the root (pass through), whether the root is `/` (pass through), or neither (strip leading `/` and join). Getting this wrong silently serves the wrong directory.

**Gotcha**: `MtpVolume::get_metadata` is expensive — it lists the entire parent directory
**Why**: MTP has no single-file stat call — `get_metadata` lists the parent directory and searches for the entry by name. This is used by `notify_mutation` after each self-mutation (create, delete, rename) and is acceptable because those are infrequent, but avoid calling it in hot paths.

**Decision**: `notify_mutation` lives on the Volume trait, not in Tauri commands
**Why**: Every mutation method (`create_file`, `create_directory`, `delete`, `rename`) knows what changed. Adding the notification call at the end of each method keeps it colocated with the mutation. The alternative (notification calls in every Tauri command) is fragile — easy to miss a call site.

**Decision**: `SmbVolume` and `MtpVolume` store `volume_id: String` for listing cache lookups
**Why**: `notify_mutation` needs to call `notify_directory_changed(volume_id, ...)` to find the right cached listings. The volume_id is computed at creation time (`path_to_id(mount_path)` for SMB, `"{device_id}:{storage_id}"` for MTP) and stored on the struct rather than recomputed on every mutation.

**Decision**: `SmbVolume::supports_local_fs_access()` returns `false`
**Why**: `SmbVolume` now handles listing updates via `notify_mutation` using its own smb2 `get_metadata`. The old `std::fs`-based synthetic diff path (`emit_synthetic_entry_diff`) is redundant and goes through the slow OS mount. Returning `false` skips it.

**Decision**: `SmbVolume` uses `Mutex<Option<(SmbClient, Tree)>>`, not `RwLock`
**Why**: Every `SmbClient` method takes `&mut self` — there is no read-only access path. An `RwLock` where you only ever take write locks is strictly worse than a `Mutex` (higher overhead). The `Option` allows graceful cleanup on disconnect (set to `None`).

**Decision**: `SmbVolume::local_path()` returns `None`
**Why**: `local_path()` is checked in `volume_copy.rs` to decide whether to use native OS copy APIs. If SmbVolume returned `Some(mount_path)`, copies would go through the slow OS mount — exactly what we're trying to avoid. `root()` still returns the mount path for frontend path resolution.

**Decision**: `on_unmount()` trait method instead of `Any` downcasting
**Why**: Avoids runtime type checking, extensible for future volume types (S3, FTP might also need cleanup), consistent with the trait's design of optional methods with default no-ops.

**Decision**: SmbVolume background watcher uses a dedicated smb2 connection, not the main one
**Why**: `smb2::Watcher<'a>` borrows `&'a mut Connection` for its lifetime (long-poll blocks until server reports changes). Using the main client would block all file operations. The watcher task owns its own `SmbClient` + `Tree`, and stats new/modified files through the main client via `VolumeManager::get(volume_id)`.

**Decision**: Watcher task is not stored on `SmbVolume`, only the cancel sender is
**Why**: `Watcher<'a>` borrows `&'a mut Connection`. Storing both the client and watcher on the struct would require self-referential types. Instead, the `tokio::spawn`ed task owns the client, creates the watcher, and runs the loop. The `watcher_cancel: Mutex<Option<oneshot::Sender<()>>>` on the struct provides clean shutdown.

**Decision**: Watcher debounces 200ms per batch, `FullRefresh` above 50 events per directory
**Why**: Prevents 1000 individual stat calls when 1000 files are copied. The 200ms window collects events that arrive in rapid succession. The 50-event threshold for `FullRefresh` avoids O(n) stat calls for bulk operations.

**Gotcha**: Watcher filenames from SMB use backslashes; must normalize to forward slashes
**Why**: SMB servers send paths like `papers\new-file.txt`. The watcher normalizes these to `papers/new-file.txt` before extracting parent directories and constructing display paths.

**Gotcha**: Watcher filenames are NFC (from server) but macOS mount paths are NFD
**Why**: SMB servers return NFC-normalized filenames. macOS filesystem paths use NFD. The watcher NFD-normalizes filenames before constructing display paths used for cache lookups.

## Testing

- `in_memory_test.rs` — unit tests for `InMemoryVolume` (CRUD, sorting, concurrency, stress 50k entries)
- `inmemory_test.rs` — integration tests combining `InMemoryVolume` + `VolumeManager`, streaming state, sort helpers
- `local_posix_test.rs` — real-FS tests (write ops, symlinks, copy, space info) using `std::env::temp_dir()`
- `manager.rs` inline tests — concurrent registration/read/write-mix scenarios
- `mtp.rs` inline tests — path conversion and capability flags (no device needed)
- `smb.rs` inline tests — type mapping (DirectoryEntry→FileEntry, FsInfo→SpaceInfo, Error→VolumeError), connection state transitions, path conversion, capability flags (no server needed)
