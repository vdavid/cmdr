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
| `mtp.rs` | `MtpVolume` — MTP device storage; synchronous `Volume` trait bridged to async MTP calls via `tokio::runtime::Handle::block_on`. Gated with `#[cfg(target_os = "macos")]`. |
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

Both `VolumeManager` and parts of `Volume` are gated with `#[allow(dead_code)]` pending Phase 2/4 integration into `operations.rs` and `lib.rs`. `LocalPosixVolume` is already wired into the indexing subsystem.

## Testing

- `in_memory_test.rs` — unit tests for `InMemoryVolume` (CRUD, sorting, concurrency, stress 50k entries)
- `inmemory_test.rs` — integration tests combining `InMemoryVolume` + `VolumeManager`, streaming state, sort helpers
- `local_posix_test.rs` — real-FS tests (write ops, symlinks, copy, space info) using `std::env::temp_dir()`
- `manager.rs` inline tests — concurrent registration/read/write-mix scenarios
- `mtp.rs` inline tests — path conversion and capability flags (no device needed)
