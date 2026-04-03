# Volume resolution reliability plan

## The problem we're solving

When the app starts, it needs to figure out which volume each tab's path belongs to. Currently it does this by calling
`findContainingVolume(path)`, which enumerates ALL mounted volumes via
`NSFileManager.mountedVolumeURLsIncludingResourceValuesForKeys`. This macOS API blocks until every volume responds —
including hung network mounts. One stalled SMB share makes the entire call time out, and suddenly a local path like
`~/projects/cmdr` is reported as "unreachable."

**This is wrong.** The app already knows how to reach `~/projects/cmdr` — it's on the local disk. The only thing that
failed is asking macOS for a list of ALL volumes, which is the wrong question to ask. The right question is: "what
volume is THIS path on?" — answerable in microseconds via `statfs()`.

### Secondary problems

- **Retry doesn't sync.** Clicking "Retry" on the unreachable banner fixes the pane but the volume selector still shows
  the stale/broken state. They use different code paths that don't talk to each other.
- **Frontend does too much.** The startup flow has its own 3s `withTimeout` wrapper, volume resolution logic, and
  unreachable state management. This belongs in the backend — the frontend should just ask "resolve this path" and
  render whatever comes back.

### What's already fixed

The recent `volume-store` + `volume_broadcast` work (commit `b0966592`) solved the split-state problem for the volume
_list_: there's now one store, one event, one source of truth. But the per-tab volume resolution at startup and the
retry flow still use the old `findContainingVolume` path — that's what this plan fixes.

## Architecture

### The fix: `statfs()` on the path itself

Instead of enumerating all volumes and finding which one matches, call `statfs()` directly on the path. On macOS, this
returns `f_mntonname` (mount point) and `f_fstypename` (fs type) in microseconds for local filesystems — no network I/O,
no volume enumeration, no dependency on other mounts.

**The backend owns all resolution logic.** The frontend sends a path, gets back a `VolumeInfo` (or a timeout flag). No
frontend timeout wrappers, no fallback logic, no volume-list-dependent resolution.

### Edge cases

**APFS firmlinks:** `statfs("/Users/foo")` returns mount point `/System/Volumes/Data`, not `/`. Normalize this to `/` to
match the main volume entry. (macOS firmlinks `/System/Volumes/Data` to `/` for user content.)

**Symlinks:** `statfs` follows symlinks, so `~/projects` → `/Volumes/External/projects` correctly returns
`/Volumes/External` as the mount point. But the input path string doesn't start with `/Volumes/External`, so cache
lookup fails. Fix: canonicalize the path only when mount point doesn't prefix-match, and only for matching — don't
change the displayed path.

**Deleted directories:** `statfs` fails with `ENOENT`. Walk up parent directories until one succeeds. FilePane
separately handles the missing path via `resolveValidPath`.

**Protocol paths:** `mtp://`, `smb://` — dispatch on prefix, don't call `statfs`. MTP resolves from the connection
manager; network volumes are looked up directly.

**Linux:** `statfs` doesn't have `f_mntonname`. Parse `/proc/self/mountinfo` instead (stable kernel ABI). Cache the
mount table, invalidate on mount watcher events.

### Non-blocking `list_locations()`

Separately from path resolution, `list_locations()` itself should stop blocking on hung mounts. Replace
`NSFileManager.mountedVolumeURLsIncludingResourceValuesForKeys` with:

1. `read_dir("/Volumes/")` — instant, local directory listing
2. Per-entry `statfs()` with 500ms timeout — local entries pass in microseconds, hung network entries are skipped
3. Per-path `NSURL::fileURLWithPath` + `getResourceValue:forKey:error:` for name/icon/ejectable — these are per-path
   queries, not the all-volumes enumeration API

This means the `volumes-changed` broadcast always includes local volumes. The `timed_out` flag (already consumed by
`volume-store.svelte.ts`) reflects whether some entries were skipped.

## All callers of `findContainingVolume` to migrate

| #   | Where                                | What it does                    | Migration                                        |
| --- | ------------------------------------ | ------------------------------- | ------------------------------------------------ |
| 1   | `DualPaneExplorer.svelte` ~line 982  | Startup tab restoration         | `resolve_path_volume` per tab                    |
| 2   | `DualPaneExplorer.svelte` ~line 655  | Retry unreachable               | `resolve_path_volume` + `requestVolumeRefresh()` |
| 3   | `DualPaneExplorer.svelte` ~line 1909 | MCP favorite click              | `resolve_path_volume`                            |
| 4   | `VolumeBreadcrumb.svelte` ~line 108  | `containingVolumeId` derivation | `resolve_path_volume`                            |
| 5   | `VolumeBreadcrumb.svelte` ~line 118  | Favorite click in dropdown      | `resolve_path_volume`                            |
| 6   | `NetworkMountView.svelte` ~line 106  | After SMB mount                 | `resolve_path_volume`                            |

After migration, delete the `find_containing_volume` IPC command from all three platform files + `lib.rs` registration.

## Milestones

### Milestone 1: `resolve_path_volume` backend command

**What it solves:** Local paths resolve in <1ms regardless of network mount health.

**New types:**

```rust
pub struct PathVolumeResolution {
    pub volume: Option<VolumeInfo>,
    pub timed_out: bool, // true = couldn't check (not "checked and not found")
}
```

Intentionally not `TimedOut<Option<VolumeInfo>>` — different semantics. `TimedOut` means "here's a fallback because we
ran out of time." Here, `timed_out: true` means "the filesystem didn't respond, we genuinely don't know."

**Implementation (`src-tauri/src/volumes/mod.rs`):**

- `get_mount_point(path) -> Option<(String, String)>` — `statfs()`, returns `(mount_point, fs_type)`. Normalizes APFS
  firmlinks. On `ENOENT`, walks up parents.
- `resolve_path_volume_fast(path) -> Option<VolumeInfo>` — calls `get_mount_point`, builds a `VolumeInfo` directly from
  the `statfs` data (mount point → `id` via `path_to_id`, `fs_type`, `supports_trash`). Does NOT call `list_locations()`
  — that would reintroduce the NSFileManager dependency we're escaping. For name/icon, use per-path NSURL resource
  queries (same approach as Milestone 3). The volume selector has the full `VolumeInfo` from the broadcast; this
  function only needs enough to identify the volume and set the tab's `volumeId`.

**Implementation (`src-tauri/src/commands/volumes.rs`):**

- `resolve_path_volume` command. Protocol dispatch (`mtp://` → MTP module, `smb://` → network volume). For filesystem
  paths: wraps `resolve_path_volume_fast` in `blocking_with_timeout_flag` (2s). Returns `PathVolumeResolution`.

**Linux (`src-tauri/src/volumes_linux/` + `commands/volumes_linux.rs`):**

- `get_mount_point(path)` via `/proc/self/mountinfo` parsing. Cache mount table, invalidate on watcher events.
- `resolve_path_volume` command with `spawn_blocking` + timeout (current Linux `find_containing_volume` has NO timeout —
  it runs synchronously on the Tauri command thread).

**Tests:** `get_mount_point("/")` → `("/", "apfs")`. `get_mount_point("/Users/foo")` → `("/", "apfs")` (not
`/System/Volumes/Data`). `get_mount_point("/nonexistent")` walks up → `("/", "apfs")`. MTP path → returns MTP volume.

### Milestone 2: Migrate frontend callers

**What it solves:** The startup flow and retry handler use the fast path. Retry syncs the volume selector.

**`DualPaneExplorer.svelte`:**

- Replace startup `resolveVolumeId()` (lines 976–994) to call `resolve_path_volume` instead of `findContainingVolume`.
  Remove the frontend `withTimeout` wrapper — the backend has its own 2s timeout, and the response carries `timed_out`.
- Replace `handleRetryUnreachable()` (lines 646–671): call `resolve_path_volume` + `requestVolumeRefresh()`. This is the
  fix for "retry fixes pane but not selector."
- Replace MCP `selectVolumeByIndex` favorite handler (line 1909).

**`VolumeBreadcrumb.svelte`:**

- Replace `updateContainingVolume` (line 108) and `handleVolumeSelect` favorite branch (line 118).

**`NetworkMountView.svelte`:**

- Replace line 106. Also remove the `await listVolumes()` call on line 102 — the mount event already triggers a
  `volumes-changed` broadcast via the watcher.

**Tests:** Update `integration.test.ts` and `DualPaneExplorer.test.ts` mocks.

### Milestone 3 (follow-up, lower priority): Non-blocking `list_locations()`

After Milestones 1–2 land, the critical bug is fixed: local tabs always resolve instantly. But the volume _selector_ can
still show incomplete results when network mounts are hung (the 2s broadcast timeout fires and returns an empty list).
This milestone fixes that — it's a real improvement but lower severity than the startup bug.

**What it solves:** The volume selector always shows local volumes, even when network mounts are hung.

**`src-tauri/src/volumes/mod.rs`:**

- Replace `get_main_volume()`: hardcode `/`, `statfs("/")` for fs type, `NSURL::fileURLWithPath("/")` +
  `getResourceValue` for name/icon.
- Replace `get_attached_volumes()`: `read_dir("/Volumes/")` + per-entry `statfs` (500ms timeout) + per-path NSURL
  resource queries. Skip entries that time out, set a `partial` flag.
- `list_locations_uncached` returns `(Vec<LocationInfo>, bool)`. Update cache to include the flag.

**`volume_broadcast.rs`:**

- `do_emit()` already wraps `list_locations()` in a 2s timeout. After this milestone, that timeout is a safety net
  rather than the primary protection — individual entries time out at 500ms, so the overall call should return well
  within 2s even with several slow mounts.

**Linux:** Linux doesn't use NSFileManager, so it's mainly adding per-entry timeouts for network mounts. Also: convert
`list_volumes` from sync to async + `blocking_with_timeout_flag` (it currently has no timeout at all).

### Milestone 4: Delete `find_containing_volume` + update docs

- Remove from `commands/volumes.rs`, `commands/volumes_linux.rs`, `stubs/volumes.rs`, `lib.rs` registration.
- Remove from `tauri-commands/storage.ts` and `tauri-commands/index.ts`.
- Update CLAUDE.md files: `volumes/CLAUDE.md`, `navigation/CLAUDE.md`, `commands/CLAUDE.md`.

## Testing

- **Unit (Rust):** `get_mount_point` for `/`, `~`, nonexistent paths, APFS firmlink normalization. `list_locations`
  always includes root even with simulated slow `/Volumes/` entries.
- **Unit (TS):** Update existing test mocks for `resolve_path_volume`.
- **Manual:** Mount a slow/disconnected SMB share, launch the app. Local tabs load instantly. Volume selector shows
  local volumes. Hit "Retry" — both pane AND selector update.
- **E2E:** Existing tests pass unchanged.

## Out of scope

- FTP/S3 volumes (the `resolve_path_volume` protocol dispatch is ready for them, but we're not building them now)
- Disk space fetching (`get_volume_space`) — already has its own timeout/retry
- Volume watcher (`watcher.rs`) — already works, and the mount-settle poller handles the fsid race
