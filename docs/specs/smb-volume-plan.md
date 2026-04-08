# SmbVolume: Direct SMB I/O through the Volume trait

Replace OS-level filesystem calls with direct `smb2` protocol operations for mounted SMB shares. The share remains
OS-mounted (for Finder/Terminal/drag-drop compatibility), but all Cmdr file operations go through `smb2`'s pipelined I/O
for ~4x faster performance. **macOS first** — Linux (`gio mount`, `/run/user/<uid>/gvfs/`) follows with the same
pattern but different mount paths and watcher.

## Why

- **~4x faster** directory listing and file transfers on network shares (smb2's pipelined I/O vs macOS's sequential SMB
  client).
- **Fail-fast instead of hang.** Current `LocalPosixVolume` on a dead SMB mount blocks indefinitely (which is why we
  have timeout wrappers everywhere). `SmbVolume` returns `Error::Disconnected` immediately.
- **Better progress.** smb2 has byte-level streaming with `Progress` callbacks. OS-level copies on SMB mounts have
  buffering issues where cancellation is delayed (documented in `write_operations/CLAUDE.md`).
- **Unblocks future backends.** The `Volume` trait was explicitly designed for "future backends (SMB, S3, FTP)." This is
  the first non-local, non-MTP backend. The patterns we establish here will be reused.

## Design

### The "sneaky mount" approach

When the user connects to an SMB share:

1. **Authenticate** via smb2 (existing guest → keychain → prompt flow).
2. **Mount** via `NetFSMountURLSync` with the same credentials → OS mount at `/Volumes/ShareName`.
3. **Hold** the smb2 session alongside the mount.
4. **Register** as `SmbVolume` (not `LocalPosixVolume`) in `VolumeManager`.
5. **Route** all Cmdr file operations through smb2 internally.

The user sees a normal mounted share. Finder, Terminal, Quick Look, drag & drop all work via the OS mount point. But
Cmdr's own operations are fast.

### Connection states

`SmbVolume` tracks its connection health and can degrade gracefully:

| State | Meaning | Behavior |
|-------|---------|----------|
| `Direct` | smb2 session active | All ops through smb2 (fast path) |
| `OsMount` | smb2 down, OS mount alive | Fall through to filesystem calls on mount path (slow but works) |
| `Disconnected` | Both down | Return errors immediately |

**Why include OS mount fallback**: There may be exotic auth scenarios, protocol extensions, or edge cases where smb2
fails but the OS client handles it. The fallback is cheap to implement (delegate to `std::fs` calls on the mount path,
reusing helpers from `reading.rs`) and eliminates a class of "works in Finder but not in Cmdr" bugs. The state is
exposed to the frontend so it can show connection quality (green/yellow/red indicator).

**Implementation note**: The `OsMount` fallback can be added after the core `Direct` path is solid. For v1, the state
machine can be `Direct` or `Disconnected` only. Adding `OsMount` is a follow-up once the direct path is validated.

**State transitions:**
- `Direct` → `OsMount`: smb2 operation fails with `Disconnected`/`SessionExpired`. Try `auto_reconnect` first. If
  reconnect fails and the mount path still exists, degrade to `OsMount`.
- `OsMount` → `Direct`: Periodic reconnect attempt succeeds (background task, exponential backoff).
- `Direct`/`OsMount` → `Disconnected`: OS mount disappears (FSEvents watcher detects removal from `/Volumes/`).
- `Disconnected` → cleanup: Unregister volume from `VolumeManager`, emit `volume-unmounted`.

### External unmount handling

When the user unmounts externally (Finder eject, `umount` CLI, network loss):

1. The existing FSEvents watcher on `/Volumes/` fires a remove event.
2. `check_for_volume_changes()` in `volumes/watcher.rs` detects the path is gone.
3. Currently, it calls `unregister_volume_from_manager()` → removes the `LocalPosixVolume`.
4. **New behavior**: Before unregistering, check if the volume needs cleanup. Add `fn on_unmount(&self)` to the `Volume`
   trait (default no-op). `SmbVolume` implements it to disconnect the smb2 session and transition to `Disconnected`.
5. The frontend receives `volume-unmounted` and navigates away — same as today.

**Why `on_unmount` trait method instead of downcast**: Avoids `Any` downcasting, extensible for future volume types
(S3, FTP might also need cleanup), consistent with the trait's design of optional methods with default no-ops.

**Why disconnect smb2 on external unmount**: The user's mental model is "I ejected the share." Keeping a hidden smb2
connection alive would violate that expectation and waste resources. We respect the user's action.

## Implementation

### Milestone 1: `SmbVolume` struct and core read ops

#### 1. Create `apps/desktop/src-tauri/src/file_system/volume/smb.rs`

The `SmbVolume` struct:

```
SmbVolume {
    name: String,                              // Display name (share name)
    mount_path: PathBuf,                       // OS mount point, e.g. /Volumes/Documents
    server: String,                            // Server hostname or IP
    share_name: String,                        // SMB share name
    smb: Mutex<Option<(SmbClient, Tree)>>,     // smb2 session + tree (None when disconnected)
    state: AtomicU8,                           // ConnectionState enum as atomic
    runtime_handle: tokio::runtime::Handle,    // For block_on bridging
}
```

**Why `Mutex`, not `RwLock`**: Every `SmbClient` method takes `&mut self` — there is no read-only access path. An
`RwLock` where you only ever take write locks is strictly worse than a `Mutex` (higher overhead due to writer starvation
prevention). `Mutex` is the correct choice.

**Why `(SmbClient, Tree)` together**: `Tree` is a plain data struct (`tree_id`, `share_name`, etc.) passed by `&Tree`
to all `SmbClient` methods. It doesn't need its own lock. Storing client and tree together avoids lock-ordering
concerns and simplifies the code.

**Why `Option`**: Allows graceful cleanup on disconnect — set to `None` when the connection is dropped.

**Threading model**: Same as MtpVolume — `Volume` trait methods are synchronous, called from `spawn_blocking` contexts.
Use `Handle::block_on` to bridge to smb2's async API. This is safe because `spawn_blocking` runs on a separate OS
thread pool.

Implement `Volume` trait:

- `name()` → share name
- `root()` → mount path (so the frontend's path resolution works unchanged)
- `list_directory(path)` → acquire mutex, call `client.list_directory(&tree, path)` via `block_on`, map
  `smb2::DirectoryEntry` to `FileEntry`.
- `list_directory_with_progress(path, on_progress)` → same, but call `on_progress` with entry count after the call
  completes. (smb2's `list_directory` returns all entries at once, so progress is reported as a batch, not
  incrementally. Fine for now — still faster than the OS path.)
- `get_metadata(path)` → `client.stat(&tree, path)` via `block_on`, map to `FileEntry`.
- `exists(path)` → `client.stat()`, return `true` on success, `false` on `NotFound` error.
- `is_directory(path)` → `client.stat()`, check `is_directory` field.
- `local_path()` → **`None`**. See copy strategy discussion below. OS integration features (Quick Look, "Reveal in
  Finder", drag & drop) don't use `local_path()` — they construct full paths from `root()`, which returns the mount
  path. So returning `None` from `local_path()` only affects the copy fast-path, which is the desired behavior.
- `supports_watching()` → `false` initially. Use the existing FSEvents watcher on the mount path (which already works
  for mounted SMB shares). Add smb2-native watching later.
- `get_space_info()` → `client.fs_info(&tree)` via `block_on`, map to `SpaceInfo`.

**Why `local_path()` returns `None`**: `local_path()` is checked in `volume_copy.rs` to decide whether to use native
OS copy APIs (`copyfile(3)`). If `SmbVolume` returned `Some(mount_path)`, copies would go through the slow OS mount —
exactly what we're trying to avoid. By returning `None`, the copy system uses `export_to_local`/`import_from_local`
(streaming through smb2). No new trait method is needed — `root()` already returns the mount path, and OS integration
features (Quick Look, Finder reveal, drag & drop) construct paths from `root()`, not `local_path()`.

**Fallback behavior**: Every smb2 operation should be wrapped in a helper that catches `smb2::Error::Disconnected` /
`SessionExpired` and attempts reconnect. If reconnect fails, transition to `OsMount` state and delegate to the
equivalent filesystem operation on `mount_path`. This means every `Volume` method has the shape:

```
fn list_directory(&self, path) {
    match self.state {
        Direct => match self.try_smb2_list(path) {
            Ok(entries) => Ok(entries),
            Err(e) if e.is_retryable() => {
                self.try_reconnect();
                // If reconnect succeeded, retry once
                // If failed, degrade to OsMount and delegate
            }
            Err(e) => Err(classify(e))
        }
        OsMount => list_directory_on_local_path(mount_path, path),
        Disconnected => Err(VolumeError::Disconnected)
    }
}
```

#### 2. Map smb2 types to Cmdr types

`smb2::DirectoryEntry` → `FileEntry` mapping function:

- `name` → `name`
- `size` → `size` (u64)
- `is_directory` → `entry_type` (File vs Directory)
- `modified` → `modified` (convert `smb2::FileTime` to `SystemTime` — FileTime is Windows FILETIME format, 100ns
  intervals since 1601-01-01. smb2 may provide a conversion method; if not, implement carefully to avoid epoch offset
  bugs)
- `created` → `created`

`smb2::FsInfo` → `SpaceInfo` mapping:
- `total_bytes` → `total`
- `free_bytes` → `available`

`smb2::Error` → `VolumeError` mapping: Use `ErrorKind` for clean classification, similar to the share listing code.

**Path translation note**: `SmbVolume` receives paths relative to the volume root (like `Documents/report.pdf`). smb2's
`Tree` methods also expect relative paths with `/` separators. smb2 normalizes `/` to `\\` internally
(`normalize_path` in `tree.rs`), so no explicit translation is needed on Cmdr's side.

#### 3. Register `SmbVolume` on mount

Modify the mount flow in `commands/network.rs`:

Currently: `mount_share()` → OS mount → FSEvents watcher registers `LocalPosixVolume`.

New flow:
1. `mount_share()` → OS mount → get `mount_path`.
2. **Before** the FSEvents watcher fires, connect smb2 to the same server/share.
3. Create `SmbVolume` with the mount path, smb2 client, and tree.
4. Register it in `VolumeManager` with the same volume ID that the watcher would use.
5. When the FSEvents watcher fires, it calls `register_volume_with_manager()` — this should **skip** registration if
   the volume ID already exists (it's already an `SmbVolume`).

**Why register before the watcher**: Race condition prevention. If we let the watcher register a `LocalPosixVolume`
first, it would **replace** our `SmbVolume` (because `VolumeManager::register` does `HashMap::insert` which overwrites).
Registering the `SmbVolume` first means the watcher's registration is a no-op.

**Modification to `VolumeManager`**: Add `register_if_absent(id, volume)` → returns `true` if registered, `false` if
ID already exists. The watcher calls this instead of `register` for mount events. The existing `register` (which
overwrites) is kept for explicit re-registration (like `SmbVolume` replacing itself on reconnect).

**Modification to `volumes/watcher.rs`**: `register_volume_with_manager()` calls `register_if_absent` instead of
`register`. This is a safe, backwards-compatible change — the only scenario where a volume is pre-registered is our
explicit `SmbVolume` registration.

**Race window note**: If the FSEvents watcher fires before the smb2 connection completes (step 2), the watcher would
register a `LocalPosixVolume` via `register_if_absent`. Then when smb2 connects, the `SmbVolume` registration uses
`register` (overwrite) to replace it. This is the correct behavior — the `SmbVolume` always wins. The brief window
where a `LocalPosixVolume` exists is harmless (it works, just slower).

#### 4. Handle external unmount

Add `fn on_unmount(&self)` to the `Volume` trait (default empty implementation).

Modify `volumes/watcher.rs` `emit_volume_unmounted()`: Before calling `unregister_volume_from_manager()`, look up the
volume from `VolumeManager::get(id)` and call `volume.on_unmount()`.

`SmbVolume::on_unmount()`:
1. Transition state to `Disconnected` (atomic store).
2. Take the mutex, set `smb` to `None` (drops `SmbClient` and `Tree`, disconnects gracefully).
3. Cancel any background reconnection task.

Then `unregister_volume_from_manager()` removes it from the registry as normal.

### Milestone 2: Write operations

#### 5. Implement write ops on `SmbVolume`

- `create_file(path, content)` → `client.write_file(&tree, path, content)`. Note: smb2's `write_file` creates or
  overwrites. The `Volume` trait's `create_file` has the same semantics in practice (no existing callers depend on
  create-only behavior).
- `create_directory(path)` → `client.create_directory(&tree, path)`
- `delete(path)` → `client.delete_file(&tree, path)` or `client.delete_directory(&tree, path)` based on a `stat`
  check. Note: `Volume::delete` says "file or empty directory." Recursive deletion is handled by the caller
  (`delete_volume_files_with_progress` in `delete.rs`), which calls `list_directory` + `delete` for each item
  bottom-up. This already works with the `Volume` trait.
- `rename(from, to, force)` → `client.rename(&tree, from, to)`. Handle `force` by deleting dest first if needed.
- `supports_export()` → `true`
- `export_to_local(source, local_dest)` → smb2 `read_file` or `read_file_pipelined`, write to local file. For large
  files, use `read_file_pipelined` for best throughput. Note: the streaming `download` API borrows `&mut self` for the
  download's lifetime, which holds the mutex for the entire transfer and blocks other operations. For v1, use
  `read_file_pipelined` (which reads the full file into memory and releases the lock). Streaming downloads can be
  optimized later with a dedicated transfer session.
- `import_from_local(local_source, dest)` → read local file into memory, `client.write_file_pipelined`. Same
  trade-off as above.
- `scan_for_copy(path)` → recursive smb2 listing to count files/dirs/bytes.
- `scan_for_conflicts(source_items, dest_path)` → smb2 `stat` each item at dest.

**Copy strategy interaction**: Since `local_path()` returns `None` for `SmbVolume`, the copy strategy in
`volume_copy.rs` won't take the "both local" fast path. It will use `export_to_local`/`import_from_local` — the
volume-aware copy path that streams through smb2. This is the desired behavior.

#### 6. File watching

**Start with FSEvents on the mount path** — it already works for mounted SMB shares. The existing listing watcher in
`streaming.rs` uses `Volume::supports_watching()` to decide whether to set up a watcher. Since we start with `false`,
the listing will rely on manual refresh and the FSEvents watcher that the OS mount provides.

smb2-native watching (`client.watch()`) can be added later as an optimization. It would provide faster change
notifications (direct from the server vs the OS mount's FSEvents delay).

### Milestone 3: Frontend connection indicator

#### 7. Expose connection state to frontend

Add a `connection_quality` field to the volume info that the frontend already receives via `list_volumes` / volume
events. Three states:

- `"direct"` — full smb2 connection (green indicator)
- `"os_mount"` — degraded, using OS mount (yellow indicator)
- `"disconnected"` — nothing works (red indicator, but this state is brief before the volume is unregistered)

**FE changes**: Minimal — add an optional indicator dot/icon next to network volume names in the sidebar and breadcrumb.
Only shown for SMB volumes (local volumes don't have this field). This is the only FE change needed.

**Why expose this**: Radical transparency (design principle). The user should understand what's happening. If they see
yellow, they know Cmdr is working but slower than usual.

### Milestone 4: Reconnection logic

#### 8. Background reconnection

When `SmbVolume` transitions from `Direct` to `OsMount`:

1. Log the transition with the error that caused it.
2. Spawn a background task that attempts reconnection with exponential backoff (1s, 2s, 4s, 8s, 16s, 30s max).
3. On success: transition back to `Direct`, emit a state-change event to frontend.
4. On repeated failure: stay in `OsMount` (the OS mount is still working).
5. If the volume is unmounted externally during this, the unmount handler cancels the reconnection task.

**Why exponential backoff**: Server might be restarting, network might be flaky. Hammering reconnections wastes
resources and could overwhelm the server.

### Milestone 5: Testing and docs

#### 9. Unit tests

- `SmbVolume` with a mock smb2 connection (smb2 has `Connection::from_transport` for mock transports).
- State transitions: `Direct` → `OsMount` → `Direct`, `Direct` → `Disconnected`.
- Type mapping: `DirectoryEntry` → `FileEntry`, `FsInfo` → `SpaceInfo`.
- Fallback behavior: verify that `OsMount` state delegates to filesystem calls.

#### 10. Integration tests with Docker SMB containers

- Start smb-guest container.
- Register `SmbVolume` pointing at it.
- List directory, create file, read file, delete file, rename file.
- Test connection loss: stop the container, verify graceful degradation to `OsMount` (if mounted) or `Disconnected`.
- Test external unmount: unmount the volume, verify smb2 cleanup.

#### 11. Manual testing

- Connect to a real NAS, browse directories, copy files in both directions.
- Kill the network connection (WiFi off), verify degradation.
- Reconnect (WiFi on), verify recovery.
- Eject the share from Finder, verify Cmdr handles it.

#### 12. Update docs

- Update `apps/desktop/src-tauri/src/file_system/volume/CLAUDE.md`: Add `SmbVolume` to the architecture diagram and
  capability table.
- Update `apps/desktop/src-tauri/src/network/CLAUDE.md`: Document the "sneaky mount" approach and the mount → register
  flow.
- Update `docs/architecture.md` if it references the volume system.

#### 13. Run full checks

- `./scripts/check.sh` — all checks must pass.
- `cargo nextest run` — all tests.

## Risks

- **smb2 `Tree` lifetime**: The `Tree` struct holds a `tree_id` that the server can invalidate (idle timeout,
  reconnection). Operations after invalidation return `STATUS_NETWORK_NAME_DELETED`. Handle by reconnecting the tree.
  smb2's `auto_reconnect` may handle this, but needs testing.
- **Concurrent access**: Multiple Cmdr operations can hit `SmbVolume` concurrently (listing in one pane, copy in
  another). `SmbClient` takes `&mut self`, so the `Mutex` serializes operations. This is fine for correctness but means
  operations don't pipeline across Cmdr features. Future optimization: maintain multiple smb2 sessions (one for
  browsing, one for transfers).
- **Large file transfers hold the mutex**: `export_to_local` / `import_from_local` hold the mutex for the entire
  transfer duration, blocking other operations. For v1 this matches MTP's behavior (same limitation). For v2, use a
  dedicated transfer session or the streaming API with a separate `SmbClient`.
- **Large directory listings**: smb2 returns all entries at once from `list_directory`. For directories with 100k+
  files, this could be slow. The progress callback helps, but smb2 may need a streaming listing API for truly huge
  directories.
- **FileTime conversion**: smb2's `FileTime` is Windows FILETIME (100ns intervals since 1601-01-01). Need correct
  conversion to `SystemTime`. Off-by-one in the epoch offset would silently corrupt timestamps.
- **Linux**: This plan is macOS-first. Linux uses `gio mount` (different mount paths, no FSEvents). The `SmbVolume`
  itself is cross-platform, but the mount + watcher integration needs platform-specific work. Linux support follows as
  a separate effort.

## Not changing

- **Share discovery and listing**: Already migrated to smb2 (phase 1, done).
- **Mount mechanism**: Still `NetFSMountURLSync` on macOS, `gio mount` on Linux. The mount is still needed for OS
  integration.
- **Frontend navigation model**: Same volume_id + path system. No new concepts.
- **MTP volumes**: Unaffected.
