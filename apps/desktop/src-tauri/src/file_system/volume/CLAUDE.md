# Volume abstraction

This module defines the `Volume` trait — the core abstraction for all storage backends in Cmdr — and the `VolumeManager` registry.

## Purpose

Every file system operation (listing, copy, rename, delete, indexing, watching) goes through a `Volume`. The trait hides the differences between a local POSIX path, an MTP device, an in-memory test fixture, and future backends (SMB, S3, FTP). Callers never touch the filesystem directly; they call `Volume` methods with **paths relative to the volume root**.

## Key files

| File | Role |
|---|---|
| `mod.rs` | `Volume` trait, `VolumeScanner`, `VolumeWatcher`, `VolumeReadStream` traits, shared types (`VolumeError`, `SpaceInfo`, `CopyScanResult`, `ScanConflict`, `SourceItemInfo`) |
| `manager.rs` | `VolumeManager` — thread-safe `RwLock<HashMap>` registry; supports a default volume |
| `local_posix.rs` | `LocalPosixVolume` — real filesystem; delegates listing to `file_system::listing`, indexing to `indexing::scanner`, watching to `indexing::watcher` (FSEvents), copy scanning via `walkdir`. Uses `libc::statvfs` FFI for space info. |
| `mtp.rs` | `MtpVolume` — MTP device storage; synchronous `Volume` trait bridged to async MTP calls via `tokio::runtime::Handle::block_on`. Gated with `#[cfg(any(target_os = "macos", target_os = "linux"))]`. |
| `in_memory.rs` | `InMemoryVolume` — `RwLock<HashMap>` store for tests; also used for stress tests (`with_file_count`) |

## Architecture

```
VolumeManager (registry)
  └─ Arc<dyn Volume>
        ├─ LocalPosixVolume   → real FS, FSEvents watcher, jwalk scanner
        ├─ MtpVolume          → async MTP ops via block_on (spawn_blocking context)
        └─ InMemoryVolume     → HashMap, test/stress use only
```

`VolumeScanner` and `VolumeWatcher` are separate sub-traits returned by `Volume::scanner()` and `Volume::watcher()`. Only `LocalPosixVolume` implements both today.

## Trait capability model

Optional methods default to `Err(VolumeError::NotSupported)` or `false`, so new volume types can be added incrementally. Key capability flags:

- `supports_watching()` — enables the `notify`-based *listing* file watcher in `operations.rs` (separate from the `VolumeWatcher` trait used for drive indexing). `MtpVolume` returns `false` (it has its own USB event loop).
- `supports_export()` — enables copy/move UI. Both local and MTP return `true`.
- `supports_streaming()` — enables chunked MTP-to-MTP transfers. Only `MtpVolume` returns `true`.
- `local_path()` — returns `Some` only for local volumes; allows `copyfile(2)` fast-path in copy operations.
- `scanner()` / `watcher()` — drive indexing hooks; `None` by default.

## Path handling gotchas

- **`LocalPosixVolume::resolve`**: accepts empty, `.`, relative, or absolute paths. Three-way branch for absolute paths: (1) already starts with volume root — used as-is, (2) volume root is `/` — absolute path passed through unchanged, (3) otherwise — leading `/` stripped and joined to root. This handles frontend sending full absolute paths.
- **`MtpVolume::to_mtp_path`**: strips the `mtp://{device}/{storage}/` URL prefix and leading slashes, returning the bare relative path the MTP library expects.
- **`InMemoryVolume::normalize`**: always resolves to an absolute path anchored at `/`.

## MTP threading

`MtpVolume` is called from `tokio::task::spawn_blocking`, so `Handle::block_on` is safe inside its methods. However, `write_from_stream` must collect all chunks **before** entering `block_on` to avoid nested-runtime panics (stream chunks also use `block_on` internally).

## Integration status

`LocalPosixVolume` is wired into the indexing subsystem. `VolumeManager` is actively used.

## Key decisions

**Decision**: Trait with optional methods defaulting to `NotSupported`/`false`
**Why**: New volume types (SMB, S3, FTP) will have vastly different capability sets. Forcing every implementor to stub out every method would be noisy and error-prone. Defaults let new backends start with just `list_directory` + `get_metadata` and opt in to capabilities incrementally. The alternative — a capabilities bitfield — would require runtime checks everywhere and couldn't express return-type differences.

**Decision**: `VolumeScanner` and `VolumeWatcher` are separate sub-traits, not part of `Volume`
**Why**: Scanning and watching have their own lifetimes, threading models, and state (handles, channels). Folding them into `Volume` would force every volume to carry scanner/watcher state even if it never indexes. Returning `Option<Box<dyn VolumeScanner>>` keeps the core trait lightweight.

**Decision**: `VolumeManager` uses `RwLock<HashMap>` (not `DashMap` or `Mutex`)
**Why**: Volume registration/unregistration is rare (mount/unmount events); reads are frequent (every file operation resolves a volume). `RwLock` gives concurrent read access without pulling in an extra dependency. `DashMap` would work but is heavier than needed for a registry that rarely exceeds ~10 entries.

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

**Gotcha**: `MtpVolume::get_metadata` returns `NotSupported`
**Why**: MTP has no single-file stat call — you must list the parent directory and search for the entry by name. The listing pipeline doesn't use `get_metadata` during normal browsing (it gets metadata from `list_directory` results), so implementing it would add an expensive round-trip for a code path that's currently unused.

## Testing

- `in_memory_test.rs` — unit tests for `InMemoryVolume` (CRUD, sorting, concurrency, stress 50k entries)
- `inmemory_test.rs` — integration tests combining `InMemoryVolume` + `VolumeManager`, streaming state, sort helpers
- `local_posix_test.rs` — real-FS tests (write ops, symlinks, copy, space info) using `std::env::temp_dir()`
- `manager.rs` inline tests — concurrent registration/read/write-mix scenarios
- `mtp.rs` inline tests — path conversion and capability flags (no device needed)
