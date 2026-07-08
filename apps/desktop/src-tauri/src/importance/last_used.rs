//! Sampled `kMDItemLastUsedDate` (Spotlight last-used) for the recency-of-use
//! signal, macOS-local only (plan Decision 5, agent-spec §5.1 / §18.4).
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

/// The most folders to query per pass. A guess until measured on a real home
/// (plan open-question 2 / agent-spec §18.4); measured cost goes in
/// `docs/notes/`. Kept modest so a pass never spends unbounded time in MDItem.
#[cfg(target_os = "macos")]
pub const SAMPLE_CAP: usize = 500;

/// Whether the Spotlight last-used signal can be produced on this platform.
/// `true` on macOS (MDItem available), `false` elsewhere — the scheduler uses
/// this to set the `SignalSet` availability so the weight redistributes off
/// non-macOS rather than being fabricated.
pub fn is_available() -> bool {
    cfg!(target_os = "macos")
}

/// Sample `kMDItemLastUsedDate` for up to `SAMPLE_CAP` of the given folder
/// paths, returning `path → last-used Unix seconds`. macOS only; a stub on other
/// platforms returns an empty map. Runs the MDItem queries on a dedicated OS
/// thread with an autoreleasepool and joins it, so the caller (a blocking
/// recompute task) stays off the framework thread-stack hazard.
#[cfg(target_os = "macos")]
pub fn sample_last_used(paths: &[String]) -> HashMap<String, u64> {
    // Cap the sample. Taking the first N is fine: folder order from the index walk
    // isn't meaningful, and the cap is about bounding cost, not fairness. A future
    // refinement could bias toward recently-listed folders.
    let sample: Vec<String> = paths.iter().take(SAMPLE_CAP).cloned().collect();
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
                for path in &sample {
                    if let Some(secs) = macos::last_used_secs(path) {
                        out.insert(path.clone(), secs);
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
pub fn sample_last_used(_paths: &[String]) -> HashMap<String, u64> {
    HashMap::new()
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
