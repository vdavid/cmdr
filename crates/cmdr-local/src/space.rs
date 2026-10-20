//! How much room a local volume has: NSURL's important-usage figure on macOS
//! (purgeable space counted, so it matches Finder), `statvfs` on Linux and as the
//! fallback, and the live readout's cheaper derivation of the same figure
//! (`live_space`). `DETAILS.md` § "Local volume space".

use std::path::Path;

use cmdr_fs::volume::{SpaceInfo, VolumeError};

/// Gets space information for a path.
///
/// On macOS, uses `NSURLVolumeAvailableCapacityForImportantUsageKey` which includes purgeable
/// space (APFS snapshots, iCloud caches), matching what Finder reports. Falls back to `statvfs`
/// if the NSURL query fails. On Linux, uses `statvfs` directly (no purgeable space concept).
pub fn get_space_info_for_path(path: &Path) -> Result<SpaceInfo, VolumeError> {
    // On macOS, prefer the NSURL API that accounts for purgeable space.
    #[cfg(target_os = "macos")]
    {
        if let Some(space) = get_volume_space(&path.to_string_lossy()) {
            return Ok(space);
        }
    }

    // Fallback (and Linux primary path): statvfs
    get_space_info_statvfs(path)
}

/// [`get_space_info_for_path`] for a readout that polls: on macOS the same figure, mostly at
/// `statfs` cost (`live_space::live_volume_space`). Linux's `statvfs` is already that cheap.
pub(crate) fn live_space_info_for_path(path: &Path) -> Result<SpaceInfo, VolumeError> {
    #[cfg(target_os = "macos")]
    {
        if let Some(space) = super::live_space::live_volume_space(&path.to_string_lossy()) {
            return Ok(space);
        }
    }

    get_space_info_statvfs(path)
}

/// Gets space information using `statvfs`. Used as the primary method on Linux and as a
/// fallback on macOS, and as macOS's whole answer in a test build (`get_volume_space`).
pub(crate) fn get_space_info_statvfs(path: &Path) -> Result<SpaceInfo, VolumeError> {
    use std::ffi::CString;

    let path_c = CString::new(path.to_string_lossy().as_bytes()).map_err(|e| VolumeError::IoError {
        message: e.to_string().into(),
        raw_os_error: None,
    })?;

    // SAFETY: `path_c` is a valid NUL-terminated C string from `path`; `stat` is a zeroed,
    // correctly-typed `libc::statvfs` out-buffer the kernel fills, and its fields are only read on
    // the `== 0` (success) branch where the kernel initialized them.
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(path_c.as_ptr(), &mut stat) == 0 {
            #[allow(clippy::unnecessary_cast, reason = "statvfs field types vary across platforms")]
            let block_size = stat.f_frsize as u64;
            #[allow(clippy::unnecessary_cast, reason = "statvfs field types vary across platforms")]
            let total_bytes = (stat.f_blocks as u64) * block_size;
            #[allow(clippy::unnecessary_cast, reason = "statvfs field types vary across platforms")]
            let available_bytes = (stat.f_bavail as u64) * block_size;
            #[allow(clippy::unnecessary_cast, reason = "statvfs field types vary across platforms")]
            let used_bytes = total_bytes.saturating_sub((stat.f_bfree as u64) * block_size);

            // ❗ `used` is NOT the complement of `available` here: `statvfs`
            // reserves blocks that are neither free to a normal user nor holding
            // anything, so the two come from different fields and `bounded`
            // (which derives one from the other) would be wrong.
            Ok(SpaceInfo::Bounded {
                total_bytes,
                available_bytes,
                used_bytes,
            })
        } else {
            Err(VolumeError::IoError {
                message: "Failed to get space info".into(),
                raw_os_error: None,
            })
        }
    }
}

/// Get space information for a volume containing the given path.
///
/// A mounted filesystem always has a capacity, so this is always
/// [`SpaceInfo::Bounded`]. NSURL reports no used figure of its own, so
/// [`SpaceInfo::bounded`] derives it.
///
/// ❗ A test build (`crate::TEST_FAST_PATHS`) answers from `statvfs` instead (the same
/// figure minus purgeable space): the first NSURL call in a process `readdir`s the test
/// binary's own directory, which stalled parallel tests for seconds on a bloated
/// `target/debug/deps` (#357). Tests of NSURL itself call `nsurl_volume_space`.
#[cfg(target_os = "macos")]
pub fn get_volume_space(path: &str) -> Option<SpaceInfo> {
    if crate::TEST_FAST_PATHS {
        get_space_info_statvfs(Path::new(path)).ok()
    } else {
        nsurl_volume_space(path)
    }
}

/// [`get_volume_space`]'s real answer, from NSURL.
#[cfg(target_os = "macos")]
fn nsurl_volume_space(path: &str) -> Option<SpaceInfo> {
    #[cfg(test)]
    NSURL_SPACE_QUERIES.with(|n| n.set(n.get() + 1));
    use objc2::rc::autoreleasepool;
    use objc2_foundation::NSURL;

    // Drain autoreleased ObjC objects (NSURL, NSString, NSNumber).
    // Called from spawn_blocking threads that lack AppKit's autorelease pool.
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&objc2_foundation::NSString::from_str(path));

        let total = super::nsurl::get_u64_resource(&url, "NSURLVolumeTotalCapacityKey")?;
        let available = super::nsurl::get_u64_resource(&url, "NSURLVolumeAvailableCapacityForImportantUsageKey")
            .filter(|&v| v > 0)
            .or_else(|| super::nsurl::get_u64_resource(&url, "NSURLVolumeAvailableCapacityKey"))?;

        Some(SpaceInfo::bounded(total, available))
    })
}

#[cfg(all(test, target_os = "macos"))]
thread_local! {
    /// How many NSURL space queries this thread made, so a test can prove it made none.
    static NSURL_SPACE_QUERIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    // Regression anchor for #357: the first NSURL call in a test process `readdir`s the test
    // binary's own directory, which stalled parallel tests for seconds on a bloated
    // `target/debug/deps`. A space query in a test build must never get that far.
    #[test]
    fn a_test_build_answers_a_space_query_without_nsurl() {
        let space = get_volume_space("/").expect("the boot volume has a size");

        assert!(space.total_bytes().is_some_and(|total| total > 0), "got {space:?}");
        assert_eq!(NSURL_SPACE_QUERIES.with(std::cell::Cell::get), 0);
    }

    #[test]
    fn the_boot_volume_reports_a_bounded_capacity_it_has_not_filled() {
        let space = nsurl_volume_space("/").expect("macOS reports space for the boot volume");

        let SpaceInfo::Bounded {
            total_bytes,
            available_bytes,
            ..
        } = space
        else {
            panic!("a mounted filesystem always has a capacity, got {space:?}");
        };
        assert!(total_bytes > 0, "total should be positive");
        assert!(available_bytes > 0, "available should be positive");
        assert!(available_bytes <= total_bytes, "available should be at most total");
    }

    #[test]
    fn test_get_volume_space_home() {
        let home = dirs::home_dir().expect("Should have home dir");
        let space = nsurl_volume_space(home.to_str().unwrap());
        assert!(space.is_some(), "Should get space info for home directory");
    }

    #[test]
    fn test_get_volume_space_nonexistent() {
        // Nonexistent paths return None - the NSURL resource API doesn't resolve to ancestor volumes
        let space = nsurl_volume_space("/nonexistent/path/that/does/not/exist");
        assert!(space.is_none(), "Nonexistent paths should return None");
    }
}
