# Volumes

macOS volume and location discovery, plus live mount/unmount watching via FSEvents.

## Key files

| File | Purpose |
|---|---|
| `mod.rs` | `LocationInfo` type and `VolumeInfo` type alias (`pub use LocationInfo as VolumeInfo` for backwards compatibility), `LocationCategory` enum. `list_locations()`, `get_volume_space()`, `parse_cloud_provider_name()`, and private helpers using `objc2`/`objc2_foundation`. |
| `watcher.rs` | `notify` (FSEvents) watcher on `/Volumes`. Detects mount/unmount by diffing against `KNOWN_VOLUMES`. Registers/unregisters with `VolumeManager` via `register_volume_with_manager`/`unregister_volume_from_manager` (coupling to `file_system::get_volume_manager()`). Emits `volume-mounted` / `volume-unmounted` Tauri events. |

## Location categories

```
Favorite       — hardcoded: /Applications, ~/Desktop, ~/Documents, ~/Downloads
MainVolume     — root volume at "/"
AttachedVolume — /Volumes/* (skips /System, Preboot, Recovery, CloudStorage)
CloudDrive     — iCloud at ~/Library/Mobile Documents/…, providers at ~/Library/CloudStorage/
Network        — /Network (commented out, pending sidebar implementation)
```

`list_locations()` aggregates all categories in order and deduplicates by path using a `HashSet<String>`.

## Global state in `watcher.rs`

All three statics use `OnceLock` — `start_volume_watcher` is idempotent (second call returns early):

```rust
APP_HANDLE:    OnceLock<AppHandle>
WATCHER:       OnceLock<Mutex<Option<RecommendedWatcher>>>
KNOWN_VOLUMES: OnceLock<Mutex<HashSet<String>>>
```

## `path_to_id` duplication

The `path_to_id` logic (keep only alphanumeric + `-`, lowercase, `/` → `"root"`) is duplicated between `mod.rs` and `watcher.rs`. Keep them in sync if either changes. The constant `DEFAULT_VOLUME_ID = "root"` is defined in `mod.rs` and used in both files.

## Volume space

`get_volume_space(path)` uses `NSURLVolumeTotalCapacityKey` and `NSURLVolumeAvailableCapacityForImportantUsageKey` (falls back to `NSURLVolumeAvailableCapacityKey`). Returns `None` for non-existent paths.

## Cloud provider detection

`parse_cloud_provider_name(dir_name)` maps `~/Library/CloudStorage/` directory names to friendly labels:

| Directory prefix | Display name |
|---|---|
| `Dropbox` | Dropbox |
| `GoogleDrive` | Google Drive |
| `OneDrive` (+ `Business`) | OneDrive / OneDrive for Business |
| `Box` | Box |
| `pCloud` | pCloud |
| anything else | first `-`-delimited segment |

## Key decisions

**Decision**: Use `OnceLock` for all three watcher statics (`APP_HANDLE`, `WATCHER`, `KNOWN_VOLUMES`)
**Why**: `start_volume_watcher` must be idempotent — calling it twice (e.g., if app setup runs again) should not create a second FSEvents stream. `OnceLock::set` failing on the second call is the idempotency gate. `LazyLock` would initialize eagerly, which doesn't work because the `AppHandle` isn't available at static-init time.

**Decision**: Detect mount/unmount by diffing `KNOWN_VOLUMES` against `get_current_volumes()`, not by trusting FSEvents event types
**Why**: FSEvents on `/Volumes` fires `Create`, `Remove`, and `Modify` events, but mount operations can produce multiple events in rapid succession (e.g., a `Modify` followed by a `Create`). Diffing against known state is a reliable debounce — the exact event type doesn't matter; only the before/after difference does.

**Decision**: Use `NSURLVolumeAvailableCapacityForImportantUsageKey` with fallback to `NSURLVolumeAvailableCapacityKey`
**Why**: The "ForImportantUsage" key accounts for purgeable space (iCloud, APFS snapshots) — it reports how much space the OS would make available if needed, which matches what Finder shows. The plain key reports only physically free blocks, which can be misleadingly low on APFS volumes with purgeable data. The fallback handles older macOS versions where the key doesn't exist.

**Decision**: `supports_trash` defaults to `true` for unknown filesystem types
**Why**: Optimistic default. Most local filesystems support trash; the only exceptions are network mounts and FAT-family formats, which are explicitly listed. If an unknown fs type doesn't support trash, the operation fails gracefully at trash time — better than pessimistically disabling trash for a filesystem that actually supports it.

**Decision**: Use `libc::statfs` for filesystem type detection instead of `NSURLVolumeLocalizedFormatDescriptionKey`
**Why**: The NSURL key returns a human-readable string like "APFS (Case-sensitive)" which is locale-dependent and not machine-parseable. `statfs.f_fstypename` returns a stable machine identifier ("apfs", "smbfs", "nfs") that can be matched against a known list of network/non-trash-capable filesystems.

## Gotchas

**Gotcha**: `path_to_id` logic is duplicated between `mod.rs` and `watcher.rs`
**Why**: `watcher.rs` generates volume IDs when registering with `VolumeManager` on mount events. It can't call `mod.rs::path_to_id` because the function is private. Making it `pub(crate)` would work but the duplication predates this refactor. If either copy changes, the other must too — otherwise the watcher registers volumes with IDs that don't match what `list_locations()` returns.

**Gotcha**: `VolumeInfo` is a type alias for `LocationInfo`, not a separate type
**Why**: The module was originally called "volumes" and used `VolumeInfo` everywhere. It was renamed to "locations" (since favorites and cloud drives aren't volumes), but the frontend still sends/receives `VolumeInfo`. The alias preserves backwards compatibility without a migration.

**Gotcha**: Watcher registers/unregisters volumes with `VolumeManager` directly, creating tight coupling to `file_system::get_volume_manager()`
**Why**: When a volume mounts, it must be immediately available for file operations. Emitting just a Tauri event and letting the frontend trigger registration would introduce a race window where operations fail because the volume isn't registered yet. Direct registration ensures atomicity — by the time the frontend receives `volume-mounted`, the volume is already usable.

**Gotcha**: `get_main_volume`, `get_attached_volumes`, and `get_volume_space` wrap their bodies in `objc2::rc::autoreleasepool`
**Why**: These functions are called from `spawn_blocking` threads (via `blocking_with_timeout_flag` in commands). Without an autorelease pool, the `NSFileManager`, `NSURL`, `NSString`, and `NSNumber` objects created per call accumulate in a default pool that is never drained, causing memory leaks over hours.

## Dependencies

External: `notify`, `dirs`, `objc2`, `objc2_foundation`
Internal: `crate::file_system::{get_volume_manager, volume::LocalPosixVolume}`, `crate::icons::get_icon_for_path`
