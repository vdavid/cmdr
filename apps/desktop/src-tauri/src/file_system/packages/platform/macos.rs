//! macOS: asks `NSURLIsPackageKey` (a registered package type, or the `kHasBundle` bit), on its
//! own pool and under a budget.

use std::sync::LazyLock;
use std::time::Duration;

use crate::file_system::framework_pool::{Pool, PoolConfig};

/// How long one visible range's lookups may take before the pane keeps the
/// list's answer. Each lookup is a local metadata read; a hung mount is what
/// this bounds.
const LOOKUP_BUDGET: Duration = Duration::from_secs(2);

/// The lookups' own pool: NSURL resource reads can reach File Provider over XPC
/// (an iCloud-synced Desktop), which needs an 8 MB stack and may never return.
static LOOKUP_POOL: LazyLock<Pool> = LazyLock::new(|| {
    Pool::new(PoolConfig {
        name: "package_lookup",
        target_workers: 1,
        max_workers: 4,
        wedged_after: Duration::from_secs(10),
    })
});

/// macOS's answer for each candidate, or `None` when the lookups outlived [`LOOKUP_BUDGET`].
pub(crate) async fn look_up(candidates: Vec<String>) -> Option<Vec<(String, bool)>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    LOOKUP_POOL.submit(Box::new(move || {
        let answers = candidates
            .into_iter()
            .map(|path| {
                let is_package = is_package_to_macos(&path);
                (path, is_package)
            })
            .collect();
        let _ = tx.send(answers);
    }));
    tokio::time::timeout(LOOKUP_BUDGET, rx).await.ok()?.ok()
}

/// The seam's name for [`is_package_to_macos`]: one blocking NSURL read, for a
/// caller already on an 8 MB-stack pool thread.
pub(crate) fn is_package_to_the_os(path: &str) -> bool {
    is_package_to_macos(path)
}

/// Whether macOS presents the folder at `path` as one item
/// (`NSURLIsPackageKey`). `false` for a missing path, a file, or a failed read:
/// the row then stays a folder.
pub(crate) fn is_package_to_macos(path: &str) -> bool {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{NSString, NSURL};
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        cmdr_local::nsurl::get_bool_resource(&url, "NSURLIsPackageKey").unwrap_or(false)
    })
}
