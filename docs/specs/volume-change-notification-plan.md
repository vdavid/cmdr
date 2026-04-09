# Volume change notification: unified watching for SMB, MTP, and local volumes

Fix the "listing doesn't update after mutations" bug for SmbVolume and MtpVolume. Establish a unified change
notification architecture that works for all volume types, handling both self-initiated mutations and external changes.

## Problem

When Cmdr creates, deletes, or renames files through a non-local volume (SmbVolume or MtpVolume), the file listing
doesn't update reliably. Multiple interacting causes:

1. **Self-initiated mutations bypass FSEvents.** `SmbVolume` writes via smb2 (bypasses kernel VFS, so FSEvents never
   fires). `MtpVolume` writes via USB (same — kernel doesn't know). The listing watcher relies on FSEvents for change
   detection on local volumes.

2. **`handle_directory_change()` (watcher.rs:360) bails out** with `if !vol.supports_watching() { return; }` for
   non-watchable volumes. Even if called, it uses `list_directory_core` (`std::fs::read_dir`) which doesn't go through
   the Volume trait — it only works for local volumes.

3. **`emit_synthetic_entry_diff` partially works for SMB.** `SmbVolume` doesn't override `supports_local_fs_access()`
   (defaults to `true`), so `should_emit_synthetic_diff` returns `true`, and the synthetic diff fires. But it calls
   `get_single_entry` which uses `std::fs::symlink_metadata` on the OS mount path — this works but goes through the slow
   mount path and may have stale directory caches. For MTP, `supports_local_fs_access()` returns `false`, so the
   synthetic diff is skipped entirely.

4. **`rename_file` has no listing notification at all.** `commands/rename.rs` doesn't call `emit_synthetic_entry_diff`
   or any other cache update after a successful rename. All volume types (including local) rely entirely on FSEvents or
   the watcher catching up. This is a pre-existing bug.

5. **`delete` through `write_operations` has no listing cache update.** The write_operations module emits
   `write-complete` events but doesn't patch the listing cache. The frontend is expected to re-read the listing on
   `write-complete`. Verify this happens correctly for non-local volumes — if the frontend calls `refreshListing` which
   goes through `handle_directory_change`, it will bail out for non-watchable volumes (cause #2).

## Design

### Principle: volume mutations should trigger listing updates through the Volume trait, not through filesystem side effects

The current architecture for local volumes works by accident: `std::fs::create_dir` → kernel VFS → FSEvents → listing
refresh. This is an indirect side effect. For non-local volumes, there's no equivalent side effect. Rather than trying
to create one (polling, fake events), we make the Volume trait explicitly notify the listing system after mutations.

### Two layers

**Layer 1: Self-mutation notification (immediate, reliable)**

After each successful Volume mutation (`create_file`, `create_directory`, `delete`, `rename`), the volume notifies the
listing system directly. This provides instant UI feedback — no watcher delay.

**Layer 2: Background watcher (external changes)**

For changes made by other clients (another user on the NAS, another app on the MTP device), a background watcher detects
and reports them. For SMB, this is smb2's `CHANGE_NOTIFY`. For MTP, this is the existing USB event loop.

Both layers feed into the same mechanism: `notify_directory_changed()`.

### Why not just rely on the watcher?

The smb2 watcher would eventually see self-mutations (the watch connection detects changes from the main connection),
but there's a delay (server batches notifications, network round-trip). For the best UX, self-mutations should update
the listing instantly. The watcher is a safety net for external changes and a deduplication layer.

## Implementation

### Milestone 1: `notify_directory_changed` — unified entry point

#### 1. New function in `file_system/listing/caching.rs`

```rust
/// Notifies the listing system that a directory's contents changed on the given volume.
///
/// Finds all active listings matching the volume_id and path, and triggers a targeted update.
/// For single-file events (add/remove/modify), uses the cache helpers for O(1) patch.
/// For bulk or unknown events, falls back to a full re-read via the Volume trait.
pub fn notify_directory_changed(
    volume_id: &str,
    parent_path: &Path,
    change: DirectoryChange,
)
```

Where `DirectoryChange` is:

```rust
pub enum DirectoryChange {
    /// A single entry was added. Includes the FileEntry to insert.
    Added(FileEntry),
    /// A single entry was removed by name.
    Removed(String),
    /// A single entry was modified. Includes the updated FileEntry.
    Modified(FileEntry),
    /// An entry was renamed within the same directory.
    Renamed { old_name: String, new_entry: FileEntry },
    /// Unknown or bulk change — trigger a full re-read via the Volume trait.
    FullRefresh,
}
```

**Why a full `FileEntry` for `Added`/`Modified`**: The listing cache stores `FileEntry` objects. To insert or update, we
need the complete entry (name, size, timestamps, icon_id). The caller constructs this from the Volume's `get_metadata`.

**Why `FullRefresh` variant**: SMB's `CHANGE_NOTIFY` can return `STATUS_NOTIFY_ENUM_DIR` when too many changes occurred.
MTP events may be unreliable. A full refresh is the safe fallback.

**Listing lookup must filter by `volume_id`**: `find_listings_for_path` currently matches by path only. Two volumes
could theoretically serve overlapping paths. Add `volume_id` filtering to `find_listings_for_path` (the `CachedListing`
already stores `volume_id`).

**Implementation for each variant:**

- `Added(entry)`: Call `insert_entry_sorted(listing_id, entry)` for each matching listing. Emit `directory-diff` event
  with the insertion. This is exactly what `emit_synthetic_entry_diff` does today, but going through the Volume trait
  instead of `std::fs`.
- `Removed(name)`: Call `remove_entry_by_path(listing_id, path)` for each matching listing. Emit `directory-diff`.
- `Modified(entry)`: Call `update_entry_sorted(listing_id, entry)`. Emit `directory-diff`.
- `Renamed { old_name, new_entry }`: `remove_entry_by_path` for old name, `insert_entry_sorted` for new entry. This
  handles same-directory renames. Cross-directory renames (moves) need two notifications — `Removed` in the source dir
  and `Added` in the dest dir. The caller is responsible for emitting both.
- `FullRefresh`: For each matching listing, re-read the directory via `Volume::list_directory`, compute diff against
  cache, emit `directory-diff`. This replaces `handle_directory_change`'s `list_directory_core` call with the Volume
  trait's `list_directory`.

**Deduplication**: If a self-mutation notification and a watcher event arrive for the same file within a short window,
the second one is a no-op (`insert_entry_sorted` returns `None` if the entry already exists, `remove_entry_by_path`
returns `None` if already removed). No explicit dedup logic needed.

### Milestone 2: Self-mutation notification from SmbVolume and MtpVolume

#### 2. Add `notify_mutation` to the Volume trait

```rust
/// Called after a successful mutation to update the listing cache.
///
/// Default implementation uses `std::fs` to stat the entry and calls `notify_directory_changed`.
/// Non-local volumes override to use their own protocol for metadata.
fn notify_mutation(&self, parent_path: &Path, mutation: MutationEvent) {
    // Default: construct FileEntry via std::fs and call notify_directory_changed
}

pub enum MutationEvent {
    Created(String),       // file/dir name
    Deleted(String),       // file/dir name
    Modified(String),      // file/dir name
    Renamed { from: String, to: String },
}
```

**Why on the Volume trait**: Every Volume mutation method (`create_file`, `create_directory`, `delete`, `rename`)
already knows what changed. Adding the notification call at the end of each method keeps it colocated with the mutation.
The alternative (adding notification calls in every Tauri command that calls Volume methods) is fragile — easy to miss a
call site.

**Why a default implementation**: `LocalPosixVolume` mutations go through `std::fs`, so the default can use
`std::fs::symlink_metadata` to construct the `FileEntry`. This means existing local volume behavior doesn't change —
it's belt-and-suspenders alongside FSEvents.

**Coupling note**: `notify_mutation` needs to call `notify_directory_changed`, which touches `LISTING_CACHE` — this
pulls listing concerns into the Volume layer. This is acceptable because the notification is fire-and-forget (no return
value, no error propagation). The Volume doesn't depend on the listing system; it just calls a global function.

#### 3. SmbVolume implementation

Add `volume_id: String` field to `SmbVolume`. Set during `connect_smb_volume` or registration.

After each mutation in `SmbVolume`, call `self.notify_mutation(parent_path, event)`. The implementation:

1. For `Created`/`Modified`: call `self.get_metadata(entry_path)` to get the `FileEntry`, then
   `notify_directory_changed(volume_id, parent_path, DirectoryChange::Added(entry))`.
2. For `Deleted`: `notify_directory_changed(volume_id, parent_path, DirectoryChange::Removed(name))`.
3. For `Renamed`: get metadata for the new path. For same-directory rename:
   `notify_directory_changed(volume_id, parent_path, DirectoryChange::Renamed { old_name, new_entry })`. For
   cross-directory rename (move): emit `Removed(old_name)` for source dir, `Added(new_entry)` for dest dir.

**Mutex contention note**: The watcher task stats new files via the main client (`SmbVolume::get_metadata`), sharing the
same `Mutex<Option<(SmbClient, Tree)>>`. This means watcher stat calls queue behind user-initiated operations (and vice
versa). This is acceptable — watcher events are debounced, so stat calls are infrequent. Large ongoing operations (like
a copy) will delay watcher notifications, but the self-mutation notification provides instant feedback for
Cmdr-initiated changes anyway.

#### 4. MtpVolume implementation

Same pattern. After each MtpVolume mutation, call `self.notify_mutation()`. The MTP implementation uses its own
`connection_manager().stat()` to construct the `FileEntry`.

#### 5. Update `create_directory` and `create_file` Tauri commands

Remove the `should_emit_synthetic_diff` / `emit_synthetic_entry_diff` path. The Volume's `notify_mutation` handles it
now. The commands become cleaner — they just call the Volume method and return.

**Migration note**: Keep `emit_synthetic_entry_diff` for one release cycle as a fallback for any volume types that don't
implement `notify_mutation`. Remove once all volume types are migrated.

#### 6. Update `handle_directory_change`

Remove the `!vol.supports_watching()` bail-out. Replace `list_directory_core` (raw `std::fs`) with
`volume.list_directory` (Volume trait). This makes the full-refresh path work for all volume types.

This change also means `handle_directory_change` works for SMB volumes, which is needed by the smb2 watcher
(milestone 3) for the `FullRefresh` fallback.

### Milestone 3: smb2 background watcher (external changes)

#### 7. Dedicated watch connection in SmbVolume

`smb2::Watcher<'a>` borrows `&'a mut Connection` for its lifetime (long-poll — blocks until the server reports changes).
This means the main `SmbClient` can't be used for watching and operations simultaneously.

The watcher task owns its own connection — not stored on the struct:

```
SmbVolume {
    smb: Mutex<Option<(SmbClient, Tree)>>,          // For file operations (existing)
    volume_id: String,                                // For notification routing (new)
    // Watch connection is owned by the background task (not stored on the struct)
    watcher_cancel: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,  // To stop the watcher
}
```

**Why not store the watch client on the struct**: The `Watcher<'a>` borrows `&'a mut Connection` for its lifetime.
Storing both the client and the watcher on the struct would require self-referential types. Instead, the background task
owns the client, creates the watcher, and runs it in an async loop.

#### 8. Background watcher task

Spawned during `connect_smb_volume`:

```rust
async fn run_smb_watcher(
    server: &str, share: &str, port: u16,
    username: Option<&str>, password: Option<&str>,
    volume_id: String, mount_path: PathBuf,
    cancel: oneshot::Receiver<()>,
) {
    // 1. Establish a separate smb2 connection
    // 2. Connect to the same share
    // 3. Call client.watch(&tree, "", true) — recursive from share root
    // 4. Loop: select! { next_events() => process, cancel => break }
    // 5. On exit: call watcher.close() to release the SMB directory handle
}
```

**Event processing:**

For each `FileNotifyEvent { action, filename }`:

1. Extract parent directory from `filename` (for example, `papers/new-file.txt` → parent is `papers`).
2. Convert to the mount-path-based parent: `mount_path.join(parent_dir)`.
3. Determine the `DirectoryChange`:
   - `Added` → stat the new file via the MAIN client (`SmbVolume::get_metadata`), construct `FileEntry`, emit
     `DirectoryChange::Added(entry)`.
   - `Removed` → emit `DirectoryChange::Removed(filename)`.
   - `Modified` → stat via main client, emit `DirectoryChange::Modified(entry)`.
   - `RenamedOldName` + `RenamedNewName` → buffer the old name, on the new name stat and emit
     `DirectoryChange::Renamed`.
4. Call `notify_directory_changed(volume_id, parent_path, change)`.

**Debouncing:** Batch events that arrive within 200ms into a single notification per directory. If more than 50 events
arrive for the same directory in one batch, emit `FullRefresh` instead of individual events. This handles bulk
operations (copying 1000 files) without 1000 individual stat calls.

**`STATUS_NOTIFY_ENUM_DIR`:** The server returns this when too many changes occurred and the buffer overflowed. Emit
`FullRefresh` for the share root — the listing system will re-read the currently displayed directory.

**Cleanup on exit:** The smb2 `Watcher` has a `close(self)` method that releases the SMB directory handle. The watcher
task must call `watcher.close().await` before exiting (on cancellation, connection loss, or any other exit path).
Failing to close leaks the directory handle on the server.

**Reconnection:** If the watch connection drops, wait 5 seconds and try to reconnect. Log the reconnection. If it fails
3 times, stop trying (the main connection's state transitions will handle the UX).

#### 9. Start/stop the watcher

- **Start**: After `SmbVolume` is created and registered, spawn the watcher task. Pass a `oneshot::Sender` for
  cancellation.
- **Stop**: `SmbVolume::on_unmount()` sends on the cancel channel. The watcher task receives it via `select!`, calls
  `watcher.close().await`, and exits.

### Milestone 4: MTP event loop integration

#### 10. Wire MTP USB events into `notify_directory_changed`

The MTP event loop (`mtp/event_loop.rs`) already polls for USB events. When it receives `ObjectAdded`, `ObjectRemoved`,
or `ObjectInfoChanged`, it should call `notify_directory_changed` with the appropriate `DirectoryChange`.

This is a smaller change since the event loop infrastructure already exists. The main work is:

- Mapping MTP event types to `DirectoryChange` variants
- Determining the parent directory from the MTP object ID (may need a parent lookup)
- Constructing `FileEntry` from MTP metadata

**Note**: Many Android devices don't emit USB events for host-initiated changes. The self-mutation notification from
milestone 2 handles this. The MTP event loop enhancement is for device-initiated changes (user takes a photo, downloads
a file on the device).

### Milestone 5: Testing and docs

#### 11. Integration tests

- **SmbVolume self-mutation**: Create file via `SmbVolume::create_file` → verify listing cache updated, `directory-diff`
  event emitted. Same for delete, rename.
- **SmbVolume watcher**: Create a file on the Docker SMB share via a second smb2 connection (simulating external change)
  → verify the watcher detects it and the listing updates.
- **Cross-pane**: Both panes show the same SMB directory. Create file in pane A → verify pane B updates.

#### 12. Update CLAUDE.md files

- `file_system/listing/CLAUDE.md`: Document `notify_directory_changed`, `DirectoryChange`, and the new notification
  flow.
- `file_system/volume/CLAUDE.md`: Document `notify_mutation` trait method and `volume_id` field on SmbVolume.
- `file_system/watcher.rs` CLAUDE.md (if exists): Update `handle_directory_change` documentation.

#### 13. Run full checks

- `./scripts/check.sh`
- `cargo nextest run` — all tests
- SMB integration tests with Docker containers

## Risks

- **smb2 watcher reliability**: Some SMB servers (especially older Samba versions) may not support `CHANGE_NOTIFY` or
  may have quirks. The `FullRefresh` fallback and self-mutation notification provide resilience.
- **Debounce tuning**: Too aggressive debouncing misses events. Too little overwhelms the listing system. Start with
  200ms / 50 events and tune based on testing.
- **Second connection resource cost**: Each SmbVolume now has two TCP connections to the server. Most NAS devices handle
  this fine (tested with QNAP and Raspberry Pi). Servers with very low connection limits might be affected.
- **MTP parent directory lookup**: MTP events report object IDs, not paths. Resolving the parent directory path may
  require walking the MTP object tree. This might be slow on devices with many files.
- **Rename event ordering**: SMB sends `RenamedOldName` and `RenamedNewName` as separate events. If they arrive in
  different batches (unlikely but possible), the rename would be processed as a delete + add instead of a rename. This
  is functionally correct but loses the rename animation in the UI.
- **Mutex contention between watcher stat and user operations**: Watcher stat calls share the main client mutex. This is
  acceptable for debounced watcher events but means stat calls queue behind large ongoing operations.

## Not changing

- **Local volume watching**: FSEvents continues to work as before. `notify_mutation` has a default implementation that
  works for local volumes, but FSEvents remains the primary mechanism.
- **Frontend listing rendering**: No frontend changes. The `directory-diff` event format is unchanged.
- **Write operations module**: Copy/move/delete through `write_operations/` already emit `write-complete` events. The
  frontend handles these by refreshing the listing. The new notification supplements this for individual operations (⌘N,
  rename, etc.). Verify during implementation that the frontend's `write-complete` handler works correctly for non-local
  volumes — if it calls `refreshListing` which goes through `handle_directory_change`, the M1 fix (removing the
  `supports_watching` bail-out) is needed.
