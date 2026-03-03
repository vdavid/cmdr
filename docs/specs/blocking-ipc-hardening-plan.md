# Blocking IPC hardening plan

## Problem

Synchronous `#[tauri::command]` functions block the Tauri IPC handler thread. If one such command calls
a filesystem syscall (`statfs`, `readdir`, `metadata`, NSURL queries) on a slow or hung network mount,
ALL subsequent IPC calls from the frontend queue behind it. The app appears frozen.

Currently only `path_exists` is protected with `blocking_with_timeout`. At least 16 other commands
touch the filesystem without timeout protection.

## Intent

Wrap every filesystem-touching Tauri command in `blocking_with_timeout` (or convert to async with
`spawn_blocking` + `tokio::time::timeout`) so that no single slow mount can freeze the app. The goal
is defense in depth: even if one syscall hangs, the IPC thread stays responsive.

## The pattern

Reference implementation: `path_exists` in `commands/file_system.rs`.

```rust
pub async fn path_exists(path: String) -> bool {
    blocking_with_timeout(Duration::from_secs(2), false, move || {
        Path::new(&path).exists()
    }).await
}
```

For commands that return `Option` or `Result`, use `None` / `Err` as the timeout fallback.

## P0 — Critical (sync commands + user-controlled network-mountable paths)

These block the IPC thread and are called on hot paths (startup, volume switching, dropdown open).

| Command | File | Syscalls | Called from |
|---------|------|----------|------------|
| `list_volumes` | `commands/volumes.rs:7` | `statfs`, NSURL, `readdir`, `Path::exists` on every volume | Startup, mount/unmount events, volume dropdown |
| `get_volume_space` | `commands/volumes.rs:48` | NSURL capacity queries / `statvfs` | `VolumeBreadcrumb.fetchVolumeSpaces` — `Promise.all` across all physical volumes |
| `find_containing_volume` | `commands/volumes.rs:21` | Same as `list_volumes` (calls `list_locations`) | Startup tab restoration — `Promise.all` per tab |

**Fix**: Convert all three to `async` + `blocking_with_timeout(2s)`. For `list_volumes`, consider
per-volume timeouts so one hung mount doesn't prevent discovering the others.

## P1 — High (sync commands on user paths, less frequent hot paths)

| Command | File | Syscalls | Notes |
|---------|------|----------|-------|
| `get_sync_status` | `sync_status.rs:13` | `metadata` + NSURL per path, rayon parallel | Cloud-synced directories; rayon threads also get consumed |
| `get_icons` | `icons.rs:14` | NSWorkspace/Launch Services queries by path | For `path:*` icons on network paths |
| `refresh_directory_icons` | `icons.rs:25` | Same as `get_icons`, rayon parallel | Multiple rayon threads can block simultaneously |
| `viewer_open` | `file_viewer.rs:11` | `exists`, `is_dir`, `metadata`, `File::open` | User can open files on any mount |
| `viewer_get_lines` | `file_viewer.rs:23` | File read/seek | Same |
| `list_directory_start` | `file_system.rs:132` | `readdir` via volume | Sync variant; streaming variant exists. Consider removing this one |
| `refresh_listing` | `file_system.rs:250` | `readdir` via watcher | Re-reads directory synchronously |

**Fix**: Convert to `async` + `blocking_with_timeout`. For `get_sync_status` and icon commands,
consider skipping network-mounted paths entirely or using a shorter timeout (500ms).

## P2 — Medium (async + spawn_blocking but no timeout)

These don't block the IPC thread (already use `spawn_blocking`) but can consume blocking pool threads
indefinitely on hung mounts.

| Command | File | Notes |
|---------|------|-------|
| `create_directory` | `file_system.rs:69` | Fallback path also calls `create_dir` directly |
| `scan_volume_for_copy` | `file_system.rs:518` | Recursive FS scan |
| `scan_volume_for_conflicts` | `file_system.rs:548` | Metadata checks per file |
| `move_to_trash` | `rename.rs:14` | NSFileManager on network volumes |
| `check_rename_permission` | `rename.rs:25` | `lstat`, `access` |
| `check_rename_validity` | `rename.rs:65` | `symlink_metadata` |
| `rename_file` | `rename.rs:79` | `symlink_metadata`, `rename` |

**Fix**: Add `tokio::time::timeout` wrapper around the existing `spawn_blocking` calls.

## Frontend fan-out risks

These `Promise.all` patterns fan out to at-risk commands:

| Pattern | File | Commands called | Risk |
|---------|------|----------------|------|
| Volume space fetch | `VolumeBreadcrumb.svelte:378` | `get_volume_space` per physical volume | P0 — one hung mount blocks all |
| Startup tab restore | `DualPaneExplorer.svelte:879` | `find_containing_volume` per tab | P0 — startup freeze |

**Fix**: After backend commands are protected, also add frontend `withTimeout` wrappers as defense
in depth (like `path-navigation.ts` already does for `pathExists`).

## Implementation order

1. P0 commands first — these cause the reported "volume switch doesn't load" bug
2. P1 commands — cover the remaining sync hot paths
3. P2 commands — add timeouts to existing `spawn_blocking` calls
4. Frontend `withTimeout` wrappers — defense in depth
5. Update `volumes/CLAUDE.md` and `commands/CLAUDE.md` with the new gotcha entries

## Test plan

- Mount a slow/hung SMB share (or use a FUSE mock that delays `statfs` by 30 seconds)
- Open volume dropdown while the hung share is mounted — should not freeze
- Switch to Macintosh HD — should load immediately
- Open a file on a slow mount in the viewer — IPC should stay responsive
- Verify all protected commands return their timeout fallback within 2 seconds
