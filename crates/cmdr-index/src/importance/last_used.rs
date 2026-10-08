//! Sampled `kMDItemLastUsedDate` (Spotlight last-used) for the recency-of-use
//! signal, macOS-local only.
//!
//! Per-item MDItem queries are slow, so we SAMPLE rather than sweep: cap the
//! number of folders queried per pass ([`SAMPLE_CAP`]) and skip the rest (their
//! `last_used` stays `None`, contributing 0 — available-but-unsampled, distinct
//! from an SMB folder where the signal is unavailable and its weight
//! redistributes). The sampling runs on a DEDICATED OS thread with an
//! `objc2::rc::autoreleasepool` — never rayon, whose 2 MB worker stack can't
//! absorb the synchronous framework round-trips (`src-tauri/CLAUDE.md`), and
//! never inline on the caller.
//!
//! Returns a `path → last-used-seconds` map for the sampled folders. A folder
//! with no `kMDItemLastUsedDate` (never opened, or Spotlight has no record) is
//! simply absent from the map.

use std::collections::HashMap;

/// The most folders to query per pass. A guess until measured on a real home;
/// measured cost goes in `docs/notes/`. Kept modest so a pass never spends
/// unbounded time in MDItem.
///
/// Defined on every platform (the sample itself is macOS-only) so a caller can hand
/// [`sample_last_used`] the paths it can actually use rather than the whole volume's.
pub const SAMPLE_CAP: usize = 500;

/// Whether the Spotlight last-used signal can be produced on this platform.
/// `true` on macOS (MDItem available), `false` elsewhere — the scheduler uses
/// this to set the `SignalSet` availability so the weight redistributes off
/// non-macOS rather than being fabricated.
pub fn is_available() -> bool {
    cfg!(target_os = "macos")
}

/// Sample `kMDItemLastUsedDate` for up to `SAMPLE_CAP` of `volume_id`'s folders,
/// given as the INDEX-relative paths a walk produces, returning `path → last-used
/// Unix seconds` keyed by those same paths. macOS only; a stub on other platforms
/// returns an empty map. Runs the MDItem queries on a dedicated OS thread with an
/// autoreleasepool and joins it, so the caller (a blocking recompute task) stays
/// off the framework thread-stack hazard.
///
/// ❗ Spotlight is asked about each folder where it sits on disk
/// (`paths::routing::local_path_of`): an external drive's index stores `/photos`
/// for `/Volumes/Ext/photos`, and asking about `/photos` asks about a boot-disk
/// folder that isn't there. A volume the host can't place samples nothing.
#[cfg(target_os = "macos")]
pub fn sample_last_used(volume_id: &str, paths: &[String]) -> HashMap<String, u64> {
    // Cap the sample. Taking the first N is fine: folder order from the index walk
    // isn't meaningful, and the cap is about bounding cost, not fairness. A future
    // refinement could bias toward recently-listed folders.
    let sample: Vec<(String, String)> = paths
        .iter()
        .take(SAMPLE_CAP)
        .filter_map(|indexed| {
            crate::indexing::paths::routing::local_path_of(volume_id, indexed).map(|on_disk| (indexed.clone(), on_disk))
        })
        .collect();
    if sample.is_empty() {
        return HashMap::new();
    }

    // Dedicated 8 MB-stack OS thread (never rayon): the MDItem calls are
    // synchronous macOS-framework round-trips. Wrap the whole batch in one
    // autoreleasepool so the CF objects each query allocates are drained.
    let handle = std::thread::Builder::new()
        .name("importance-mditem-sample".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            objc2::rc::autoreleasepool(|_| {
                let mut out = HashMap::with_capacity(sample.len());
                for (indexed, on_disk) in &sample {
                    if let Some(secs) = query(on_disk) {
                        out.insert(indexed.clone(), secs);
                    }
                }
                out
            })
        });

    match handle {
        Ok(h) => h.join().unwrap_or_default(),
        Err(e) => {
            log::warn!(target: "importance", "kMDItemLastUsedDate sampler thread failed to spawn: {e}");
            HashMap::new()
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn sample_last_used(_volume_id: &str, _paths: &[String]) -> HashMap<String, u64> {
    HashMap::new()
}

/// One `kMDItemLastUsedDate` lookup, through the stand-in a test installed if there is one.
#[cfg(target_os = "macos")]
fn query(path: &str) -> Option<u64> {
    #[cfg(test)]
    if let Some(stand_in) = *tests::QUERY.lock().unwrap_or_else(std::sync::PoisonError::into_inner) {
        return stand_in(path);
    }
    macos::last_used_secs(path)
}

#[cfg(target_os = "macos")]
mod macos {
    use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
    use core_foundation::date::CFDateGetAbsoluteTime;
    use core_foundation::string::{CFString, CFStringRef};

    /// Offset between the CF absolute-time epoch (2001-01-01) and the Unix epoch
    /// (1970-01-01), in seconds. `CFDateGetAbsoluteTime` returns seconds since the
    /// CF epoch; add this to get Unix seconds.
    const CF_TO_UNIX_EPOCH_OFFSET: f64 = 978_307_200.0;

    // Opaque MDItem handle.
    #[repr(C)]
    struct __MDItem(std::ffi::c_void);
    type MDItemRef = *mut __MDItem;

    // SAFETY: these are the standard CoreServices MDItem C signatures. `MDItemCreate`
    // returns a +1 (Create-rule) reference the caller must release;
    // `MDItemCopyAttribute` likewise returns a +1 reference. Both accept a null
    // allocator (the default).
    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn MDItemCreate(allocator: CFTypeRef, path: CFStringRef) -> MDItemRef;
        fn MDItemCopyAttribute(item: MDItemRef, name: CFStringRef) -> CFTypeRef;
    }

    /// Query `kMDItemLastUsedDate` for one path, as Unix seconds. `None` when the
    /// item can't be created (path gone, not indexed by Spotlight) or has no
    /// last-used date.
    pub fn last_used_secs(path: &str) -> Option<u64> {
        let cf_path = CFString::new(path);
        // SAFETY: `cf_path` is a live CFString for the duration of this call
        // (`as_concrete_TypeRef` borrows it); a null allocator is valid. The
        // returned MDItemRef is a +1 Create reference we release below.
        let item = unsafe { MDItemCreate(std::ptr::null(), cf_path.as_concrete_TypeRef()) };
        if item.is_null() {
            return None;
        }

        let attr_name = CFString::from_static_string("kMDItemLastUsedDate");
        // SAFETY: `item` is a live, non-null MDItemRef (checked above); `attr_name`
        // is a live CFString for the call. The returned value is a +1 Copy
        // reference (a CFDate here) we release below.
        let value = unsafe { MDItemCopyAttribute(item, attr_name.as_concrete_TypeRef()) };

        // Release the MDItem now; we're done with it.
        // SAFETY: `item` is the non-null +1 reference from `MDItemCreate`; releasing
        // it balances that Create. Not used after this.
        unsafe { CFRelease(item as CFTypeRef) };

        if value.is_null() {
            return None;
        }

        // The attribute is a CFDate. Read its absolute time, then release it.
        // SAFETY: `value` is the non-null +1 reference from `MDItemCopyAttribute`.
        // `kMDItemLastUsedDate` is documented as a CFDate, so treating it as a
        // CFDateRef is sound; `CFDateGetAbsoluteTime` reads a live CFDate.
        let abs = unsafe { CFDateGetAbsoluteTime(value as _) };
        // SAFETY: balancing the +1 Copy reference from `MDItemCopyAttribute`.
        unsafe { CFRelease(value) };

        let unix = abs + CF_TO_UNIX_EPOCH_OFFSET;
        if unix < 0.0 { None } else { Some(unix as u64) }
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::indexing::host::volumes::{FakeVolumeProvider, install_for_test};

    /// One `kMDItemLastUsedDate` lookup a test answers in place of Spotlight.
    pub(super) type StandIn = fn(&str) -> Option<u64>;

    /// What [`query`] answers in place of Spotlight while a test holds it.
    pub(super) static QUERY: Mutex<Option<StandIn>> = Mutex::new(None);

    /// Puts Spotlight back however the test ends.
    struct RealSpotlight;

    impl Drop for RealSpotlight {
        fn drop(&mut self) {
            *QUERY.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        }
    }

    /// Spotlight as a test sees it: one folder opened, on an external drive.
    fn spotlight_knows_the_drives_photos(path: &str) -> Option<u64> {
        (path == "/Volumes/Ext/photos").then_some(1_700_000_000)
    }

    /// An external drive's index stores `/photos`, and Spotlight knows the folder
    /// as `/Volumes/Ext/photos`. The sample asks about the folder ON THE DRIVE and
    /// answers under the index's own path, which is what the scorer looks it up by.
    /// Asking about `/photos` instead asks about a boot-disk folder that isn't there.
    #[test]
    fn an_external_drive_is_sampled_at_its_mount_point() {
        let _serialized = crate::indexing::handle::test_lock();
        let volumes = FakeVolumeProvider::shared();
        volumes.register(
            "ext-drive",
            Arc::new(cmdr_fs::volume::InMemoryVolume::new("Ext").with_root("/Volumes/Ext")),
        );
        let _host = install_for_test(volumes);
        *QUERY.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(spotlight_knows_the_drives_photos);
        let _restore = RealSpotlight;

        let sampled = sample_last_used("ext-drive", &["/photos".to_string(), "/music".to_string()]);

        assert_eq!(sampled, HashMap::from([("/photos".to_string(), 1_700_000_000)]));
    }
}
