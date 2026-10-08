//! Volume list broadcast, cross-platform.
//!
//! Provides a single `emit_volumes_changed()` function that computes the full
//! volume list (local + MTP) and emits a `volumes-changed` Tauri event.
//! All volume-list consumers (volume selector, DualPaneExplorer) subscribe to
//! this one event instead of juggling multiple separate events.
//!
//! A 150ms debounce coalesces rapid events (e.g. multiple mounts in quick
//! succession, or MTP connect immediately after USB hotplug).
//!
//! Server rows never wait on local mount discovery: a round whose discovery is
//! late emits the cached local part beside fresh rows first
//! (`discovery_pending: true`), then again when discovery lands. See `round.rs`.

#[cfg(test)]
use crate::ignore_poison::IgnorePoison;
use crate::volume_listing::{self, LocationInfo};
use log::{debug, error};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;
use tauri::AppHandle;
use tauri_specta::Event;

/// Global app handle for emitting events.
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// Generation counter for debounce. Each call to `emit_volumes_changed()` bumps
/// the counter; the spawned task only emits if its generation is still current.
/// This ensures late-arriving triggers always produce an emission with fresh data.
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Debounce window: events within this window are coalesced into one emission.
const DEBOUNCE_MS: u64 = 150;

/// Timeout for listing local volumes. If `list_locations()` takes longer (for example,
/// a hung mount, or a saturated blocking pool the listing can't get a thread from), we
/// emit the LAST GOOD list with `timed_out: true` — see [`round::LocalSnapshot`].
const LIST_TIMEOUT: Duration = Duration::from_secs(2);

/// How long a broadcast waits for local discovery before publishing without it.
///
/// Past this, the round publishes the cached local snapshot beside FRESH server,
/// device, and registry rows (`discovery_pending: true`), then publishes again when
/// discovery lands. A healthy listing beats it, so the common case stays one event.
const PROVISIONAL_AFTER: Duration = Duration::from_millis(100);

/// The one broadcaster every `volumes-changed` goes through.
static BROADCASTER: Broadcaster = Broadcaster::new();

/// Stores the app handle for later use. Call once during app setup.
pub fn init(app: &AppHandle) {
    let _ = APP_HANDLE.set(app.clone());
}

/// Schedules a `volumes-changed` event emission with debouncing.
///
/// Can be called from any thread. Multiple rapid calls within the debounce
/// window result in a single emission after the window expires. The last
/// call always wins: a late trigger re-bumps the generation so the pending
/// task emits fresh data.
pub fn emit_volumes_changed() {
    use std::sync::atomic::Ordering;

    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    debug!("volumes-changed requested (generation {})", generation);

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(DEBOUNCE_MS)).await;
        // Only emit if no newer request arrived during the sleep
        if GENERATION.load(Ordering::SeqCst) == generation {
            do_emit().await;
        } else {
            debug!("volumes-changed skipped (generation {} superseded)", generation);
        }
    });
}

/// How many `volumes-changed` broadcasts have been REQUESTED so far.
///
/// Test-only, and a REQUEST count rather than an emission count: the emission
/// needs a running app, and what a cell about a pin or a forget cares about is
/// that the republish was asked for at all. Without it, "the switcher never
/// learns the row left" is a silent regression.
#[cfg(test)]
pub(crate) fn volumes_changed_requests() -> u64 {
    GENERATION.load(std::sync::atomic::Ordering::SeqCst)
}

/// The `VolumeUnmounted` events emitted so far, each paired with the
/// `volumes-changed` generation that was current when it went out.
///
/// Test-only, and it records the GENERATION rather than a timestamp because that
/// is what the ordering rule is about: a gone event stamped with the generation
/// from before a forget proves the redirect went out ahead of the republish, and
/// no sleep can make that flaky.
#[cfg(test)]
static VOLUMES_GONE: Mutex<Vec<(String, u64)>> = Mutex::new(Vec::new());

/// The last `VolumeUnmounted` this process emitted, as `(volume_id, generation)`.
#[cfg(test)]
pub(crate) fn last_volume_gone() -> Option<(String, u64)> {
    VOLUMES_GONE.lock_ignore_poison().last().cloned()
}

/// The lock every cell that READS or BUMPS the two recorders above must hold for
/// its whole body.
///
/// ❗ `GENERATION` and `VOLUMES_GONE` are process-global, and the cells about
/// ordering assert on an EXACT generation ("the gone event went out before
/// anything asked for a republish") or on "nothing was announced". A sibling cell
/// forgetting or pinning in parallel bumps one and appends to the other, so under
/// a thread-per-test runner those assertions read a neighbour's work. Same shape
/// as `mcp/terminal_ops.rs`'s ring lock.
///
/// ❗ `pnpm check` will never catch a miss here: nextest is process-per-test, so
/// each cell gets its own recorders. A bare `cargo test --lib commands::servers`
/// is the run that fails, a few times in ten.
#[cfg(test)]
pub(crate) fn recorder_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock_ignore_poison()
}

/// Tauri command: triggers a fresh `volumes-changed` broadcast.
/// The result arrives via the event, not as a return value.
/// Used by the frontend retry button when the initial listing timed out.
#[tauri::command]
#[specta::specta]
pub fn refresh_volumes() {
    emit_volumes_changed_now();
}

/// Emits immediately, bypassing debounce. Used for the initial startup emission.
pub fn emit_volumes_changed_now() {
    tauri::async_runtime::spawn(async {
        do_emit().await;
    });
}

/// Typed `volumes-changed` Tauri event. The struct name kebab-cases to the wire
/// event name (`volumes-changed`) via `tauri_specta::Event`. The TS payload type
/// and a typed `events.volumesChanged.listen(...)` helper are generated into
/// `apps/desktop/src/lib/ipc/bindings.ts`.
#[derive(Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct VolumesChanged {
    /// The full volume list (local + MTP).
    pub data: Vec<LocationInfo>,
    /// Whether the latest finished local listing timed out, so the local part is
    /// the last complete list standing in (some volumes may be missing).
    pub timed_out: bool,
    /// Whether a local discovery is still running, so the local part is the cached
    /// snapshot and another `volumes-changed` follows. Server, device, and registry
    /// rows are fresh either way.
    ///
    /// ❗ Like `timed_out`, it means "don't retire anything by its absence from the
    /// local part". Unlike it, it's no verdict on the listing: the UI keeps its
    /// "may be missing" state and a pending retry until a non-pending event.
    pub discovery_pending: bool,
}

/// Typed `volume-mounted` Tauri event (per-volume, carries the mount path).
/// Emitted by both the macOS (`NSWorkspace`) and Linux (`/proc/mounts` + GVFS)
/// watchers. The struct name kebab-cases to `volume-mounted`.
#[derive(Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct VolumeMounted {
    /// The volume path (like "/Volumes/MyDrive").
    pub volume_path: String,
}

/// Typed `volume-unmounted` Tauri event: one volume is gone, go home if you are
/// standing on it. `DualPaneExplorer` listens and redirects both panes.
#[derive(Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct VolumeUnmounted {
    /// The volume path (like "/Volumes/MyDrive").
    pub volume_path: String,
    /// The volume's id, when the emitter knows it.
    ///
    /// ❗ What the consumer acts on, because a "Forget server" takes the row out
    /// of the store and a path lookup would then find nothing. The mount
    /// watchers leave it `None`: they speak in paths, and the id they resolve
    /// doesn't always mean "gone" (a promoted volume keeps serving from another
    /// mount).
    pub volume_id: Option<String>,
}

/// Typed `volume-root-changed` Tauri event: a volume's root, its start folder, or
/// both moved, and the registry already serves the new root. Two causes, named by
/// [`RootChangeKind`]: saving an edit to a CONNECTED place, or a mounted drive
/// being renamed.
///
/// Every path is an APP path (`sftp://ada@nas.local:22/srv/data`, `/Volumes/New`),
/// and a landing is where opening the place lands: its start folder, else its root.
/// Emitted only when the root or the landing actually moved. What a pane does with
/// it: `network/DETAILS.md` § "Editing a connected place".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct VolumeRootChanged {
    /// The place's volume id, which an edit never changes.
    pub volume_id: String,
    /// The root the place had until this edit.
    pub old_root: String,
    /// The root it has now.
    pub new_root: String,
    /// Where opening the place landed until this edit.
    pub old_landing: String,
    /// Where it lands now.
    pub new_landing: String,
    /// Why it moved, which decides where a path inside the old root goes.
    pub kind: RootChangeKind,
}

/// Why a volume's root moved, which decides where a path inside the old root goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RootChangeKind {
    /// Someone edited a saved place: the new root is a different folder, so a path
    /// inside the old one has no counterpart under it and goes to the new landing.
    Edited,
    /// The same tree is reached at a new root (a renamed drive): a path inside the
    /// old root keeps its place under the new one.
    Moved,
}

/// The action vocabulary of a volume, server, or favorite row: what the user can
/// pick from its menu.
///
/// Rust emits only `Eject`, from the native breadcrumb menu. Every other variant
/// is picked in the frontend, from a row's in-app menu (the volume switcher's and
/// the favorites menu's → submenus, the servers hub's right-click:
/// `apps/desktop/src/lib/file-explorer/navigation/row-menu.ts`), and from the
/// palette. It lives here so the one vocabulary those surfaces and the handlers
/// share is generated, not retyped.
///
/// ❗ A typed enum, ❌ never a free string: the frontend branches on every one of
/// these, and a misspelling would go to the one place a compiler never looks. The
/// wire spelling is kebab-case, which is what the existing consumers already
/// match on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum VolumeContextActionKind {
    /// Navigate the focused pane to the row.
    Open,
    /// Unmount a removable disk, or retire a device session.
    Eject,
    /// Drop a server's session and leave it as a `saved` row.
    Disconnect,
    /// Put a saved place in the switcher.
    Pin,
    /// Take it back out. ❗ The place stays saved; the hub still lists it.
    Unpin,
    /// Open the sign-in sheet on this server's stored fields.
    Edit,
    /// Stop remembering a place's credential, keeping the place.
    ForgetSecret,
    /// Drop the server, its places, and their pins.
    ForgetServer,
    /// Rename a favorite row.
    RenameFavorite,
    /// Assign or change a favorite's single-letter menu shortcut.
    EditFavoriteShortcut,
    /// Remove a favorite row.
    RemoveFavorite,
}

/// Typed `volume-context-action` Tauri event. Emitted to the `main` window when
/// the user picks Eject from the native breadcrumb context menu. Window-scoped,
/// so it's emitted via `Event::emit_to`.
#[derive(Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct VolumeContextAction {
    /// Which item was picked.
    pub action: VolumeContextActionKind,
    /// The target volume's ID.
    pub volume_id: String,
    /// The target volume's display name (for confirmation copy).
    pub volume_name: String,
}

/// Tells the panes that `volume_id` is gone, so a pane standing on it goes home.
///
/// ❗ Call this BEFORE [`emit_volumes_changed`] when the same action also removes
/// the row: `volumes-changed` is debounced and this is not, so the order only
/// holds if it is written this way round.
pub fn emit_volume_gone(volume_id: &str, volume_path: &str) {
    #[cfg(test)]
    VOLUMES_GONE
        .lock_ignore_poison()
        .push((volume_id.to_string(), volumes_changed_requests()));
    let Some(app) = APP_HANDLE.get() else {
        // No app in a unit test; the recording above is what a cell reads.
        return;
    };
    let payload = VolumeUnmounted {
        volume_path: volume_path.to_string(),
        volume_id: Some(volume_id.to_string()),
    };
    if let Err(e) = payload.emit(app) {
        error!("Failed to emit volume-unmounted for {volume_id}: {e}");
    }
}

/// The `VolumeRootChanged` events emitted so far. Test-only; a cell reads its
/// own through [`volume_root_changes`], by a volume id no other cell uses.
#[cfg(test)]
static VOLUME_ROOTS_CHANGED: Mutex<Vec<VolumeRootChanged>> = Mutex::new(Vec::new());

/// Every `VolumeRootChanged` this process emitted for `volume_id`, in order.
#[cfg(test)]
pub(crate) fn volume_root_changes(volume_id: &str) -> Vec<VolumeRootChanged> {
    VOLUME_ROOTS_CHANGED
        .lock_ignore_poison()
        .iter()
        .filter(|change| change.volume_id == volume_id)
        .cloned()
        .collect()
}

/// Tells the panes a connected place's root or landing moved, so a pane standing
/// on the old one follows. Not debounced: the registry already serves the new
/// root when this goes out.
pub fn emit_volume_root_changed(change: VolumeRootChanged) {
    #[cfg(test)]
    VOLUME_ROOTS_CHANGED.lock_ignore_poison().push(change.clone());
    let Some(app) = APP_HANDLE.get() else {
        // No app in a unit test; the recording above is what a cell reads.
        return;
    };
    if let Err(e) = change.emit(app) {
        error!("Failed to emit volume-root-changed for {}: {e}", change.volume_id);
    }
}

// ============================================================================
// Emission
// ============================================================================

/// Runs one broadcast round: discovers the local volumes and emits the list.
///
/// Discovery gets a timeout of its own here rather than going through
/// `volume_listing::list_with_timeout`, because this caller has somewhere to fall back
/// to: the cached [`round::LocalSnapshot`] takes the place of the empty list a bare timeout
/// would publish. How a round orders its events: [`Broadcaster::round`].
async fn do_emit() {
    let Some(app) = APP_HANDLE.get() else {
        error!("volumes-changed: no app handle (broadcast not initialized)");
        return;
    };

    let discovery = volume_listing::discover_local(LIST_TIMEOUT);
    BROADCASTER
        .round(discovery, volume_listing::complete, |payload| {
            debug!(
                "Emitting volumes-changed ({} volumes, timed_out={}, discovery_pending={})",
                payload.data.len(),
                payload.timed_out,
                payload.discovery_pending
            );
            if let Err(e) = payload.emit(app) {
                error!("Failed to emit volumes-changed: {}", e);
            }
        })
        .await;
}

mod round;
use round::Broadcaster;

#[cfg(test)]
mod tests;
