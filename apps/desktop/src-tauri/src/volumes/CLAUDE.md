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

## Dependencies

External: `notify`, `dirs`, `objc2`, `objc2_foundation`
Internal: `crate::file_system::{get_volume_manager, volume::LocalPosixVolume}`, `crate::icons::get_icon_for_path`
