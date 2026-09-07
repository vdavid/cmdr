//! Volume list broadcast, cross-platform.
//!
//! Provides a single `emit_volumes_changed()` function that computes the full
//! volume list (local + MTP) and emits a `volumes-changed` Tauri event.
//! All volume-list consumers (volume selector, DualPaneExplorer) subscribe to
//! this one event instead of juggling multiple separate events.
//!
//! A 150ms debounce coalesces rapid events (e.g. multiple mounts in quick
//! succession, or MTP connect immediately after USB hotplug).

use crate::ignore_poison::IgnorePoison;
use crate::volume_listing::{self, ListingOutcome, LocationInfo};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};
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
/// emit the LAST GOOD list with `timed_out: true` — see [`LAST_GOOD_LOCAL`].
const LIST_TIMEOUT: Duration = Duration::from_secs(2);

/// The most recent SUCCESSFUL local volume listing, re-emitted when a later one times
/// out.
///
/// **Why a timeout must not publish an empty list.** `timed_out: true` means "this list
/// may be missing volumes", and the frontend voices exactly that. Pairing it with an
/// empty list said "you have no volumes" instead: the picker went blank, and its
/// refresh button re-ran the same listing into the same timeout, so nothing the user
/// could do brought the volumes back. A transient 2 s stall on one hung mount left the
/// app looking like it had lost every drive, permanently.
///
/// A stale entry is the right trade against a blank picker: it's flagged stale, an
/// unmount arrives on its own `volume-unmounted` event regardless, and picking a volume
/// that has since gone reports a normal missing-path error. ❌ Don't "simplify" this
/// back to emitting `vec![]` on timeout.
static LAST_GOOD_LOCAL: Mutex<Vec<LocationInfo>> = Mutex::new(Vec::new());

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
    /// Whether the local volume listing timed out (some volumes may be missing).
    pub timed_out: bool,
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

/// What the user picked in a volume row's context menu.
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
    /// Remove a favorite row.
    RemoveFavorite,
}

/// Typed `volume-context-action` Tauri event. Emitted to the `main` window when
/// the user picks an item from the native breadcrumb / volume-selector row
/// context menu. Window-scoped, so it's emitted via `Event::emit_to`.
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

// ============================================================================
// Emission
// ============================================================================

/// The local volumes to publish for one `outcome`, and whether the result is flagged
/// incomplete — folding [`LAST_GOOD_LOCAL`] in. Split out of [`do_emit`] so the rule
/// that a failed listing never publishes an empty list is directly testable, without an
/// `AppHandle` or a hung mount.
///
/// A panic reports `timed_out: false`: the frontend's flag drives a retry affordance
/// for a slow listing, and a panicked one isn't slow. The last-good set still carries,
/// for the same reason it does on a timeout.
fn publishable(outcome: ListingOutcome, last_good: &mut Vec<LocationInfo>) -> (Vec<LocationInfo>, bool) {
    match outcome {
        ListingOutcome::Listed(volumes) => {
            last_good.clone_from(&volumes);
            (volumes, false)
        }
        ListingOutcome::TimedOut => (last_good.clone(), true),
        ListingOutcome::Panicked => (last_good.clone(), false),
    }
}

/// Computes the full volume list and emits the event.
async fn do_emit() {
    let app = match APP_HANDLE.get() {
        Some(a) => a,
        None => {
            error!("volumes-changed: no app handle (broadcast not initialized)");
            return;
        }
    };

    // Discovery gets a timeout of its own here rather than going through
    // `volume_listing::list_with_timeout`, because this caller has somewhere to fall
    // back to: [`LAST_GOOD_LOCAL`] takes the place of the empty list a bare timeout
    // would publish.
    let outcome = volume_listing::discover_local(LIST_TIMEOUT).await;
    let (local_volumes, timed_out) = publishable(outcome, &mut LAST_GOOD_LOCAL.lock_ignore_poison());
    let volumes = volume_listing::complete(local_volumes).await;

    debug!(
        "Emitting volumes-changed ({} volumes, timed_out={})",
        volumes.len(),
        timed_out
    );
    let payload = VolumesChanged {
        data: volumes,
        timed_out,
    };
    if let Err(e) = payload.emit(app) {
        error!("Failed to emit volumes-changed: {}", e);
    }
}

#[cfg(test)]
mod tests;
