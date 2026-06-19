//! Caching structures for MTP path resolution and directory listings,
//! plus event debouncing for directory change notifications.

use mtp_rs::ObjectHandle;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use crate::file_system::FileEntry;
use crate::ignore_poison::RwLockIgnorePoison;

/// Cache mapping virtual paths to MTP object handles, and back.
///
/// Both directions are populated together (see [`insert`](Self::insert)) at the
/// same sites that list a directory, so they never drift. The forward map
/// (`path_to_handle`) backs [`resolve_path_to_handle`](super::MtpConnectionManager::resolve_path_to_handle)
/// for browsing; the reverse map (`handle_to_path`) lets the pathless PTP change
/// events ([`DeviceEvent::ObjectAdded`](mtp_rs::mtp::DeviceEvent) and friends, which carry only an
/// opaque handle) short-circuit a parent-walk the moment they hit a cached
/// ancestor instead of always walking to the storage root over USB.
#[derive(Default)]
pub(super) struct PathHandleCache {
    /// Maps virtual path -> MTP object handle.
    pub(super) path_to_handle: HashMap<PathBuf, ObjectHandle>,
    /// Maps MTP object handle -> virtual path. The reverse of `path_to_handle`,
    /// kept in lockstep with it.
    pub(super) handle_to_path: HashMap<ObjectHandle, PathBuf>,
}

impl PathHandleCache {
    /// Records a `(path, handle)` pair in both directions.
    ///
    /// Always insert through this (never `path_to_handle.insert` directly) so the
    /// reverse map can't fall out of sync. `ObjectHandle` is `Copy` and `PathBuf`
    /// is cheap to clone for a single map entry.
    pub(super) fn insert(&mut self, path: PathBuf, handle: ObjectHandle) {
        self.path_to_handle.insert(path.clone(), handle);
        self.handle_to_path.insert(handle, path);
    }
}

/// Cache for directory listings.
#[derive(Default)]
pub(super) struct ListingCache {
    /// Maps directory path -> cached file entries.
    pub(super) listings: HashMap<PathBuf, CachedListing>,
}

/// A cached directory listing with timestamp for invalidation.
pub(super) struct CachedListing {
    /// The cached file entries.
    pub(super) entries: Vec<FileEntry>,
    /// When this listing was cached (for TTL checks).
    pub(super) cached_at: Instant,
}

/// How long to keep cached listings (5 seconds).
pub(super) const LISTING_CACHE_TTL_SECS: u64 = 5;

/// Debounce duration for MTP directory change events (500ms).
/// MTP devices can emit rapid events during bulk operations (like copying many files).
pub(super) const EVENT_DEBOUNCE_MS: u64 = 500;

/// Debouncer for MTP directory change events.
///
/// Prevents flooding the frontend with events during rapid operations like
/// bulk copy/delete. Each device has its own last-emit timestamp.
pub(super) struct EventDebouncer {
    /// Last emit time per device ID.
    last_emit: RwLock<HashMap<String, Instant>>,
    /// Debounce duration.
    debounce_duration: Duration,
}

impl EventDebouncer {
    /// Creates a new debouncer with the given duration.
    pub(super) fn new(debounce_duration: Duration) -> Self {
        Self {
            last_emit: RwLock::new(HashMap::new()),
            debounce_duration,
        }
    }

    /// Checks if we should emit an event for the given device.
    /// Updates the last emit time if we should emit.
    pub(super) fn should_emit(&self, device_id: &str) -> bool {
        let now = Instant::now();
        let mut last_emit = self.last_emit.write_ignore_poison();

        if let Some(last) = last_emit.get(device_id)
            && now.duration_since(*last) < self.debounce_duration
        {
            return false;
        }

        last_emit.insert(device_id.to_string(), now);
        true
    }

    /// Clears the debounce state for a device (called on disconnect).
    pub(super) fn clear(&self, device_id: &str) {
        let mut last_emit = self.last_emit.write_ignore_poison();
        last_emit.remove(device_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_debouncer_allows_first_event() {
        let debouncer = EventDebouncer::new(Duration::from_millis(500));

        // First event for a device should always be allowed
        assert!(debouncer.should_emit("device-1"));

        // First event for a different device should also be allowed
        assert!(debouncer.should_emit("device-2"));
    }

    #[test]
    fn test_event_debouncer_throttles_rapid_events() {
        let debouncer = EventDebouncer::new(Duration::from_millis(100));

        // First event should be allowed
        assert!(debouncer.should_emit("device-1"));

        // Immediate second event should be throttled
        assert!(!debouncer.should_emit("device-1"));

        // Third rapid event should also be throttled
        assert!(!debouncer.should_emit("device-1"));
    }

    #[test]
    fn test_event_debouncer_allows_after_timeout() {
        let debouncer = EventDebouncer::new(Duration::from_millis(10));

        // First event should be allowed
        assert!(debouncer.should_emit("device-1"));

        // Wait for debounce period to elapse
        std::thread::sleep(Duration::from_millis(20));

        // Event after timeout should be allowed
        assert!(debouncer.should_emit("device-1"));
    }

    #[test]
    fn test_event_debouncer_clear() {
        let debouncer = EventDebouncer::new(Duration::from_millis(500));

        // First event allowed
        assert!(debouncer.should_emit("device-1"));

        // Second event should be throttled
        assert!(!debouncer.should_emit("device-1"));

        // Clear the device state
        debouncer.clear("device-1");

        // After clear, next event should be allowed immediately
        assert!(debouncer.should_emit("device-1"));
    }

    #[test]
    fn test_event_debouncer_per_device_isolation() {
        let debouncer = EventDebouncer::new(Duration::from_millis(500));

        // First event for device-1
        assert!(debouncer.should_emit("device-1"));

        // Rapid event for device-1 should be throttled
        assert!(!debouncer.should_emit("device-1"));

        // But event for device-2 should be allowed (independent)
        assert!(debouncer.should_emit("device-2"));

        // And rapid event for device-2 should be throttled independently
        assert!(!debouncer.should_emit("device-2"));
    }
}
