//! Volume manager for registering and accessing volumes.
//!
//! The VolumeManager is the central registry for all mounted volumes.
//! It tracks both the available volumes and which one is the current default.
//! The one instance the app runs on lives here too, behind [`get_volume_manager`].

use super::Volume;
use crate::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::log_private::LogPrivate;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex, RwLock};

mod archive_routing;
mod git_routing;
mod restic_routing;

/// Which volume serves a path: `resolve` and what it answers.
mod routing;

/// The mount-root set an ID owns, and the promotion rules over it.
mod roots;

/// Swapping an id's volume across a root change a person asked for.
mod root_replace;

/// A registered volume reconnecting in place announces itself like an arrival.
mod reconnects;

/// Following a mount root that moved while its filesystem stayed mounted (a rename).
/// Only the macOS mount watcher sees a rename.
#[cfg(any(target_os = "macos", test))]
mod root_move;

pub use reconnects::{ArrivalOnReconnect, ConnectionNews, connection_news};
#[cfg(target_os = "macos")]
pub use root_move::RootMove;
pub use root_replace::RootReplacement;
use roots::Registration;
pub use roots::{RootRemoval, StaleRootOutcome, is_stale_mount_errno};
pub use routing::path_routes_over_its_parent;

#[cfg(test)]
pub(crate) mod test_support;

/// Manages registered volumes and provides access to them.
///
/// Thread-safe registry storing volumes by ID, with support for a default volume.
///
/// An entry is a [`Registration`]: the volume plus EVERY mount root known to
/// carry its ID, one of which is active (`volume.root()`). See `roots.rs`.
pub struct VolumeManager {
    volumes: RwLock<HashMap<String, Registration>>,
    default_volume_id: RwLock<Option<String>>,
    /// Registration recency of the on-demand `ArchiveVolume`s (front = oldest).
    /// A value store: recovering on poison is safe (a lost reorder at worst
    /// evicts slightly early). See [`routing::touch_routed_lru`].
    archive_lru: Mutex<VecDeque<String>>,
    /// The same, for the on-demand `GitPortalVolume`s. Separate from the
    /// archive LRU on purpose: browsing a folder full of zips must not evict
    /// the portal of the repo the other pane is sitting in.
    git_portal_lru: Mutex<VecDeque<String>>,
    /// The on-demand `ResticVolume`s: their recency, and the parent each was
    /// minted over. Separate from the other two LRUs for the same reason.
    restic_routes: Mutex<restic_routing::ResticRoutes>,
    /// Who wants to hear that a volume has become available. See
    /// [`Self::on_volume_arrival`].
    arrival_listeners: RwLock<Vec<VolumeArrivalListener>>,
}

/// Notified with the ID of a volume the registry has just taken on.
///
/// Deliberately only the ID: a listener that needs the handle asks the registry
/// for it, so it can never act on a volume a racing registration has already
/// replaced. It also keeps the dependency pointing one way — the registry knows
/// nothing about who listens, which is what lets the in-flight temp ledger
/// (`cmdr_operations::in_flight_temps`) subscribe without welding the two
/// subtrees into a cycle.
type VolumeArrivalListener = Box<dyn Fn(&str) + Send + Sync>;

/// How [`VolumeManager::register`] and [`VolumeManager::register_if_absent`]
/// resolve an identity conflict, for the log line.
const ROOT_RECORDED_RESOLUTION: &str =
    "the existing registration stays active and the new root is recorded as a fallback";

/// Complain loudly when one ID is about to cover two different mount roots.
///
/// A volume ID is identity: it keys the index DB, `lastUsedPaths`, tab state, and
/// operation routing, so two volumes sharing one would send reads (and writes) to
/// the wrong disk. `cmdr_fs::volume::ids` makes that unreachable for every
/// derived ID, which leaves exactly two ways here: a byte-for-byte volume clone
/// (two disks really do report one UUID) and a filesystem mounted twice. Both are
/// genuinely ambiguous, so the registry picks the deterministic answer (keep the
/// incumbent, remember the other root) and says so rather than resolving the
/// ambiguity quietly. What it must never be again is silent.
///
/// The fallback root inherits the same ambiguity: for a double mount it's the
/// same filesystem and promoting to it is exactly right, while for a genuine
/// clone it's a second disk. Neither case is silent, and for the clone the two
/// already share every per-volume store, so which one the ID points at is not
/// the wound.
///
/// **Warn, not error, and deliberately so.** Error level is the auto-report
/// threshold (`error_reporter/CLAUDE.md`), and the common case here is a share
/// macOS mounted twice: recoverable, expected (the message says so), and not
/// user-impacting, which is exactly the rule's definition of a warning. At error
/// level one machine's remount loop uploaded 98 bundles in a week and drowned the
/// channel it was supposed to alert. The rarer clone deserves louder, but nothing
/// here can tell the two apart, so this stays a warning that names both. The one
/// exception is [`identity_conflict_level`]'s nested case, which can only be a
/// double mount.
fn report_identity_conflict(id: &str, existing: &Arc<dyn Volume>, incoming: &Arc<dyn Volume>, resolution: &str) {
    if !is_identity_conflict(existing.root(), incoming.root()) {
        return;
    }
    log::log!(
        target: "cmdr_lib::file_system::volume",
        identity_conflict_level(existing.root(), incoming.root()),
        "Two different mount roots ({} and {}) claim volume ID {id}; {resolution}, so the two share per-volume state. Expected only for a cloned volume or a doubly-mounted filesystem.",
        LogPrivate::dir_path(existing.root()),
        LogPrivate::dir_path(incoming.root()),
    );
}

/// How loud an identity conflict between two different roots is.
///
/// A root mounted INSIDE the other one's tree is the same filesystem mounted a
/// second time, never a cloned disk: macOS does exactly this with
/// `/System/Volumes/Update` and its `mnt1` snapshot mount during an OS update
/// (seen 10 times in a week of dev logs, 2026-11-03). That's routine, so it logs at
/// info. Two sibling roots stay a warning, since one of them could be a clone.
fn identity_conflict_level(existing_root: &Path, incoming_root: &Path) -> log::Level {
    if existing_root.starts_with(incoming_root) || incoming_root.starts_with(existing_root) {
        log::Level::Info
    } else {
        log::Level::Warn
    }
}

/// Whether re-registering `id` means two DIFFERENT mounts, rather than the same
/// mount changing backends (which is routine: that's the SMB upgrade).
fn is_identity_conflict(existing_root: &Path, incoming_root: &Path) -> bool {
    existing_root != incoming_root
}

impl VolumeManager {
    /// Creates a new empty volume manager.
    pub fn new() -> Self {
        Self {
            volumes: RwLock::new(HashMap::new()),
            default_volume_id: RwLock::new(None),
            archive_lru: Mutex::new(VecDeque::new()),
            git_portal_lru: Mutex::new(VecDeque::new()),
            restic_routes: Mutex::new(restic_routing::ResticRoutes::default()),
            arrival_listeners: RwLock::new(Vec::new()),
        }
    }

    /// Asks to be told, by ID, whenever a volume becomes available here.
    ///
    /// For work that has to wait for a volume rather than go looking for one:
    /// the in-flight temp ledger holds the partials an interrupted transfer left
    /// on a share and can only remove them once that share is back. A listener
    /// runs INSIDE the registration, so it must return immediately — hand real
    /// work to a task.
    pub fn on_volume_arrival(&self, listener: impl Fn(&str) + Send + Sync + 'static) {
        self.arrival_listeners.write_ignore_poison().push(Box::new(listener));
    }

    /// Tells the listeners `id` is now served.
    ///
    /// ❌ Never call this while holding the `volumes` lock: a listener may ask
    /// the registry for the volume it was just told about.
    fn announce_arrival(&self, id: &str) {
        for listener in self.arrival_listeners.read_ignore_poison().iter() {
            listener(id);
        }
    }

    /// Tells the listeners a registered `id` reconnected in place: the same
    /// news as an arrival, for work waiting on that volume to be reachable
    /// ([`ArrivalOnReconnect`]). Silent for an id nothing registered.
    pub fn announce_reconnected(&self, id: &str) {
        // The read guard is a temporary of this statement, released before any
        // listener may ask the registry back.
        let registered = self.volumes.read_ignore_poison().contains_key(id);
        if registered {
            self.announce_arrival(id);
        }
    }

    /// Registers a volume with the given ID.
    ///
    /// Replacing the volume at the SAME root is routine, and this is how it's
    /// done: that's an OS-mounted SMB share being upgraded in place to a direct
    /// `smb2` session. An IDENTITY CONFLICT (a different root under an ID that's
    /// already taken) keeps the INCUMBENT instead, so registration order stops
    /// deciding where a doubly-mounted filesystem is rooted. Discovery collapses
    /// those mounts before they get here (`volumes::mounts::collapse_by_volume_id`);
    /// this is the registry's own guard, and it stays loud either way.
    pub fn register(&self, id: &str, volume: Arc<dyn Volume>) {
        self.register_locked(id, volume);
        // Outside the `volumes` guard, and unconditional: every arm above leaves
        // a usable volume serving `id`, whether it's the newcomer or a kept
        // incumbent.
        self.announce_arrival(id);
    }

    /// [`register`](Self::register)'s body, so the announcement happens after the
    /// `volumes` guard is gone.
    fn register_locked(&self, id: &str, volume: Arc<dyn Volume>) {
        let mut volumes = self.volumes.write_ignore_poison();
        if let Some(existing) = volumes.get_mut(id) {
            report_identity_conflict(id, &existing.volume, &volume, ROOT_RECORDED_RESOLUTION);
            if is_identity_conflict(existing.volume.root(), volume.root()) {
                existing.record_root(volume.root());
                return;
            }
            existing.replace_volume(volume);
            return;
        }
        volumes.insert(id.to_string(), Registration::new(volume));
    }

    /// Registers a volume under `id`, replacing whatever is there even across a
    /// root change.
    ///
    /// Only for restoring a registration a test remembered
    /// (`test_support::TestVolumeRegistration`): putting back the previous value
    /// has to be unconditional, since [`register`]'s conflict guard would
    /// otherwise strand the test's own volume in the process-global registry.
    ///
    /// [`register`]: Self::register
    #[cfg(test)]
    pub(crate) fn force_register(&self, id: &str, volume: Arc<dyn Volume>) {
        self.volumes
            .write_ignore_poison()
            .insert(id.to_string(), Registration::new(volume));
        self.announce_arrival(id);
    }

    /// Whether [`register`] would REFUSE a volume rooted at `root` under `id`,
    /// keeping the incumbent active and recording `root` as a fallback.
    ///
    /// A caller that retires the incumbent before handing over (the SMB upgrade's
    /// `register_replacing_predecessor`) has to ask FIRST. Retiring stops the
    /// volume's watcher, and on a refused registration the registry keeps that
    /// same volume serving: the ID ends up pointing at a live share that no longer
    /// sees its own changes.
    ///
    /// [`register`]: Self::register
    pub fn would_keep_incumbent(&self, id: &str, root: &Path) -> bool {
        self.volumes
            .read_ignore_poison()
            .get(id)
            .is_some_and(|entry| is_identity_conflict(entry.volume.root(), root))
    }

    /// Registers a volume only if no volume with this ID exists yet.
    ///
    /// Returns `true` if the volume was registered, `false` if a volume
    /// with this ID already exists (the existing volume is kept).
    ///
    /// This is the mount watcher's entry point, so a second mount of an
    /// already-registered filesystem arrives here: the incumbent keeps the ID
    /// and the new mount point is recorded as a fallback root.
    pub fn register_if_absent(&self, id: &str, volume: Arc<dyn Volume>) -> bool {
        use std::collections::hash_map::Entry;
        let registered = match self.volumes.write_ignore_poison().entry(id.to_string()) {
            Entry::Occupied(mut existing) => {
                let entry = existing.get_mut();
                report_identity_conflict(id, &entry.volume, &volume, ROOT_RECORDED_RESOLUTION);
                entry.record_root(volume.root());
                false
            }
            Entry::Vacant(e) => {
                e.insert(Registration::new(volume));
                true
            }
        };
        if registered {
            self.announce_arrival(id);
        }
        registered
    }

    /// Unregisters a volume by ID, dropping every mount root it owned.
    ///
    /// If this was the default volume, the default is cleared.
    pub fn unregister(&self, id: &str) {
        let removed = self.volumes.write_ignore_poison().remove(id);
        if let Some(entry) = removed {
            retire(&entry.volume);
        }
        self.clear_default_if(id);
    }

    /// Clears the default volume when it was `id`. Touches only
    /// `default_volume_id`, so it's safe to call while holding `volumes`.
    fn clear_default_if(&self, id: &str) {
        // The read guard is a temporary of this statement, so it's released
        // before the write below asks for the same lock.
        let is_default = self.default_volume_id.read_ignore_poison().as_deref() == Some(id);
        if is_default {
            *self.default_volume_id.write_ignore_poison() = None;
        }
    }

    /// Gets a volume by ID.
    pub fn get(&self, id: &str) -> Option<Arc<dyn Volume>> {
        Some(self.volumes.read_ignore_poison().get(id)?.volume.clone())
    }

    /// Finds a registered volume by a mount path, matching ANY known root of an
    /// entry, not only the active one.
    ///
    /// Used by the unmount path: when `NSWorkspaceDidUnmount` (macOS) or the
    /// `/proc/mounts` watcher (Linux) fires, `statfs` on the now-gone path can no
    /// longer recover the SMB mount info, so we can't rederive the volume ID from
    /// the path. Looking up by root instead lets us find the entry we registered,
    /// whatever ID it was keyed under. It has to see the fallback roots too: a
    /// second mount of one share is registered under the SAME ID at a DIFFERENT
    /// path, and a sibling the lookup can't see is a sibling nothing can act on.
    ///
    /// The returned volume is the entry's ACTIVE one, which for a fallback-root
    /// hit is rooted somewhere else. Callers that care compare `volume.root()`.
    pub fn find_by_root(&self, root: &Path) -> Option<(String, Arc<dyn Volume>)> {
        self.volumes
            .read_ignore_poison()
            .iter()
            .find(|(_, entry)| entry.knows_root(root))
            .map(|(id, entry)| (id.clone(), Arc::clone(&entry.volume)))
    }

    /// Gets the default volume.
    #[cfg(test)]
    pub fn default_volume(&self) -> Option<Arc<dyn Volume>> {
        let default_id = self.default_volume_id.read_ignore_poison().clone()?;
        self.get(&default_id)
    }

    /// Gets the default volume ID.
    #[cfg(any(target_os = "macos", test))]
    pub fn default_volume_id(&self) -> Option<String> {
        self.default_volume_id.read_ignore_poison().clone()
    }

    /// Sets the default volume by ID.
    ///
    /// Returns true if the volume exists and was set as default.
    pub fn set_default(&self, id: &str) -> bool {
        // Verify the volume exists
        if !self.volumes.read_ignore_poison().contains_key(id) {
            return false;
        }
        *self.default_volume_id.write_ignore_poison() = Some(id.to_string());
        true
    }

    /// Lists all registered volumes as (id, name) pairs.
    pub fn list_volumes(&self) -> Vec<(String, String)> {
        self.volumes
            .read_ignore_poison()
            .iter()
            .map(|(id, entry)| (id.clone(), entry.volume.name().to_string()))
            .collect()
    }

    /// Returns all registered volumes as (id, handle) pairs. Unlike [`list_volumes`]
    /// (which returns display names), this hands back the `Volume` handles so callers
    /// can inspect capabilities (`root`, `supports_local_fs_access`, `backend_kind`).
    /// Used by the file viewer's locality check, which asks the last of those:
    /// ❗ `connection_state()` answers for four backends now, so it can't say
    /// whether a POSIX-visible path is a share.
    ///
    /// [`list_volumes`]: Self::list_volumes
    pub fn list_volumes_with_handles(&self) -> Vec<(String, Arc<dyn Volume>)> {
        self.volumes
            .read_ignore_poison()
            .iter()
            .map(|(id, entry)| (id.clone(), entry.volume.clone()))
            .collect()
    }

    /// Returns the number of registered volumes.
    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.volumes.read_ignore_poison().len()
    }
}

/// Tell a volume the registry no longer serves it, so whatever it runs in the
/// background (a watcher, a reconnect backoff loop) stands down.
///
/// Called from the two ways out of the registry, [`VolumeManager::unregister`]
/// and `roots::remove_root`'s last-mount arm, and from nowhere else. ❌ Not on a
/// REPLACE: a re-root hands the ID to another instance of a share that is still
/// live and still watching, and retiring there stops a healthy volume. An
/// upgrade's predecessor is retired by the hand-over itself, through
/// `Volume::on_superseded`.
///
/// A backend with nothing running between calls keeps no flag and this is a
/// no-op. See `cmdr_fs::volume::Retirement`.
pub(super) fn retire(volume: &Arc<dyn Volume>) {
    if let Some(retirement) = volume.retirement() {
        retirement.retire();
    }
}

impl Default for VolumeManager {
    fn default() -> Self {
        Self::new()
    }
}

/// The process-wide volume registry, created on first access.
static VOLUME_MANAGER: LazyLock<VolumeManager> = LazyLock::new(VolumeManager::new);

/// Returns a reference to the global volume manager.
///
/// It lives beside the type, not in the `file_system` facade, so that reaching
/// the registry never means importing a module that knows every backend. Which
/// volumes get registered at startup is the facade's job (`init_volume_manager`).
pub(crate) fn get_volume_manager() -> &'static VolumeManager {
    &VOLUME_MANAGER
}

#[cfg(test)]
#[path = "manager/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "manager/arrival_tests.rs"]
mod arrival_tests;
