# MTP file watching spec

**Goal**: Auto-refresh MTP directory listings when files change on the device, matching the UX of local file watching.

## Current state

- **Local volumes**: Use `notify` crate with debouncing. When files change, `directory-diff` event is emitted with add/remove/modify changes.
- **MTP volumes**: `supports_watching()` returns `false`. No auto-refresh when files change on device.

## mtp-rs library capabilities

The library supports device events via `device.next_event().await`:

```rust
pub enum DeviceEvent {
    ObjectAdded { handle: ObjectHandle },      // File/folder created
    ObjectRemoved { handle: ObjectHandle },    // File/folder deleted
    ObjectInfoChanged { handle: ObjectHandle }, // File modified
    StoreAdded { storage_id: StorageId },      // SD card inserted
    StoreRemoved { storage_id: StorageId },    // SD card removed
    StorageInfoChanged { storage_id: StorageId }, // Free space changed
    DeviceInfoChanged,
    DeviceReset,
}
```

**Key limitation**: Events only give `ObjectHandle`, not paths. To know which directory changed, we need to either:
1. Call `get_object_info(handle)` to get parent handle, then resolve to path
2. Maintain a handle→path cache
3. Simply emit "something changed" and let frontend re-fetch

## Design decision: Simple vs. granular

### Option A: Granular diffs (like local watcher)
- Map handle → parent directory path
- Compute exact add/remove/modify diffs
- Emit `directory-diff` events with specific changes

**Pros**: Consistent with local, efficient UI updates
**Cons**: Complex, requires handle→path mapping, extra MTP calls per event

### Option B: Simple refresh signal (recommended)
- On any ObjectAdded/Removed/Changed event, emit `mtp-directory-changed`
- Frontend re-fetches the currently viewed directory if it matches
- No handle→path mapping needed

**Pros**: Simple, reliable, works with MTP protocol limitations
**Cons**: Slightly less efficient (full re-fetch vs. incremental diff)

**Recommendation**: Start with Option B. MTP directories typically have <1000 files, re-fetch is fast (~100ms). Can optimize later if needed.

## Implementation plan

### Phase 1: Backend event loop

**File**: `apps/desktop/src-tauri/src/mtp/connection.rs`

Add event polling to `MtpConnectionManager`:

```rust
use mtp_rs::mtp::DeviceEvent;
use tokio::sync::broadcast;

pub struct MtpConnectionManager {
    // ... existing fields ...

    /// Channel to signal event loop shutdown
    event_loop_shutdown: RwLock<HashMap<String, broadcast::Sender<()>>>,
}

impl MtpConnectionManager {
    /// Start the event polling loop for a connected device.
    fn start_event_loop(
        &self,
        device_id: String,
        device: Arc<MtpDevice>,
        app: AppHandle,
    ) {
        let (shutdown_tx, mut shutdown_rx) = broadcast::channel(1);

        // Store shutdown sender
        self.event_loop_shutdown.write().unwrap()
            .insert(device_id.clone(), shutdown_tx);

        // Spawn event polling task
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Check for shutdown signal
                    _ = shutdown_rx.recv() => {
                        debug!("MTP event loop shutting down: {}", device_id);
                        break;
                    }

                    // Poll for next event (with timeout)
                    result = device.next_event() => {
                        match result {
                            Ok(event) => {
                                Self::handle_device_event(&device_id, event, &app);
                            }
                            Err(mtp_rs::Error::Timeout) => {
                                // No event, continue polling
                            }
                            Err(mtp_rs::Error::Disconnected) => {
                                info!("MTP device disconnected (event loop): {}", device_id);
                                break;
                            }
                            Err(e) => {
                                warn!("MTP event error: {}", e);
                                // Continue polling, device might recover
                            }
                        }
                    }
                }
            }
        });
    }

    /// Handle a device event and emit to frontend.
    fn handle_device_event(device_id: &str, event: DeviceEvent, app: &AppHandle) {
        match event {
            DeviceEvent::ObjectAdded { handle } => {
                debug!("MTP object added: {:?} on {}", handle, device_id);
                Self::emit_directory_changed(device_id, app);
            }
            DeviceEvent::ObjectRemoved { handle } => {
                debug!("MTP object removed: {:?} on {}", handle, device_id);
                Self::emit_directory_changed(device_id, app);
            }
            DeviceEvent::ObjectInfoChanged { handle } => {
                debug!("MTP object changed: {:?} on {}", handle, device_id);
                Self::emit_directory_changed(device_id, app);
            }
            DeviceEvent::StorageInfoChanged { storage_id } => {
                debug!("MTP storage info changed: {:?} on {}", storage_id, device_id);
                // Could emit storage space update event
            }
            DeviceEvent::StoreAdded { storage_id } => {
                info!("MTP storage added: {:?} on {}", storage_id, device_id);
                // Could emit storage list update
            }
            DeviceEvent::StoreRemoved { storage_id } => {
                info!("MTP storage removed: {:?} on {}", storage_id, device_id);
                // Could emit storage list update
            }
            _ => {}
        }
    }

    /// Emit directory changed event to frontend.
    fn emit_directory_changed(device_id: &str, app: &AppHandle) {
        let _ = app.emit("mtp-directory-changed", serde_json::json!({
            "deviceId": device_id
        }));
    }

    /// Stop the event loop when disconnecting.
    fn stop_event_loop(&self, device_id: &str) {
        if let Some(tx) = self.event_loop_shutdown.write().unwrap().remove(device_id) {
            let _ = tx.send(()); // Signal shutdown
        }
    }
}
```

Integrate into `connect()` and `disconnect()`:

```rust
pub async fn connect(&self, device_id: &str, app: &AppHandle) -> Result<...> {
    // ... existing connection logic ...

    // After successful connection, start event loop
    self.start_event_loop(
        device_id.to_string(),
        device.clone(),
        app.clone(),
    );

    Ok(...)
}

pub async fn disconnect(&self, device_id: &str, app: Option<&AppHandle>) -> Result<...> {
    // Stop event loop before disconnecting
    self.stop_event_loop(device_id);

    // ... existing disconnect logic ...
}
```

### Phase 2: Frontend event handling

**File**: `apps/desktop/src/lib/file-explorer/FilePane.svelte`

Listen for MTP directory changed events:

```typescript
import { listen, type UnlistenFn } from '@tauri-apps/api/event'

let mtpChangeUnlisten: UnlistenFn | undefined

onMount(async () => {
    // ... existing onMount ...

    // Listen for MTP directory changes
    mtpChangeUnlisten = await listen<{ deviceId: string }>(
        'mtp-directory-changed',
        (event) => {
            // Check if we're viewing this device
            if (isMtpVolume && volumeId.startsWith(event.payload.deviceId)) {
                // Re-fetch current directory
                void refreshDirectory()
            }
        }
    )
})

onDestroy(() => {
    mtpChangeUnlisten?.()
    // ... existing cleanup ...
})

async function refreshDirectory() {
    // Re-trigger the listing for current path
    // This will use the existing streaming listing pipeline
    await listDirectoryStartStreaming(volumeId, currentPath, sortColumn, sortOrder)
}
```

**Alternative**: Handle in `DualPaneExplorer.svelte` if it has better access to refresh both panes.

### Phase 3: Debouncing (optional enhancement)

MTP devices can emit rapid events (e.g., multiple files being copied). Add debouncing:

```rust
use std::time::{Duration, Instant};

struct EventDebouncer {
    last_emit: RwLock<HashMap<String, Instant>>,
    debounce_duration: Duration,
}

impl EventDebouncer {
    fn should_emit(&self, device_id: &str) -> bool {
        let mut last_emit = self.last_emit.write().unwrap();
        let now = Instant::now();

        if let Some(last) = last_emit.get(device_id) {
            if now.duration_since(*last) < self.debounce_duration {
                return false;
            }
        }

        last_emit.insert(device_id.to_string(), now);
        true
    }
}
```

### Phase 4: Update Volume trait

**File**: `apps/desktop/src-tauri/src/file_system/volume/mtp.rs`

```rust
impl Volume for MtpVolume {
    fn supports_watching(&self) -> bool {
        true  // Now we support it!
    }
}
```

## Frontend TypeScript types

**File**: `apps/desktop/src/lib/tauri-commands.ts`

```typescript
export interface MtpDirectoryChangedEvent {
    deviceId: string
}

export function onMtpDirectoryChanged(
    callback: (event: MtpDirectoryChangedEvent) => void
): Promise<UnlistenFn> {
    return listen('mtp-directory-changed', (e) => callback(e.payload as MtpDirectoryChangedEvent))
}
```

## Testing

### Unit tests (Rust)

1. `test_event_loop_starts_on_connect` - Verify event loop spawns
2. `test_event_loop_stops_on_disconnect` - Verify clean shutdown
3. `test_debouncer_throttles_rapid_events` - If debouncing added

### Integration tests

1. Mock MTP device that emits events
2. Verify `mtp-directory-changed` event reaches frontend

### Manual testing (with real device)

- [ ] Connect device, view folder, create file on device → view refreshes
- [ ] Connect device, view folder, delete file on device → view refreshes
- [ ] Connect device, view folder, rename file on device → view refreshes
- [ ] Disconnect device during event loop → clean shutdown, no crash
- [ ] Rapid file changes (copy 100 files) → debounced, doesn't overwhelm UI

## Complexity estimate

| Phase | Effort | Risk |
|-------|--------|------|
| Phase 1: Backend event loop | Medium | Low - straightforward async task |
| Phase 2: Frontend handling | Small | Low - simple event listener |
| Phase 3: Debouncing | Small | Low - optional enhancement |
| Phase 4: Update trait | Trivial | None |

**Total estimate**: Medium effort, low risk

## Future enhancements

1. **Granular diffs**: Map ObjectHandle → path, compute exact diffs
2. **Storage events**: Update volume list when SD card inserted/removed
3. **Space updates**: Show real-time free space changes
4. **Selective refresh**: Only refresh if changed object is in current view

## Out of scope

- Watching specific files (not directories)
- Cross-device notifications (device A watching device B)
- Offline change detection (changes while app not running)
