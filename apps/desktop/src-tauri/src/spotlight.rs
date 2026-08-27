//! One question for Spotlight: which folders has this user been working in?
//!
//! Spotlight already knows when every file was last opened (`kMDItemLastUsedDate`).
//! This asks for the recently-opened ones under a scope and reports the folders they
//! sit in, busiest first, so a caller can rank folders by how much of the user's
//! recent activity happened in each.
//!
//! **Deliberately uncoupled.** It knows about Spotlight and nothing else: no volumes,
//! no index, no settings, no app state. One function in, plain data out. The one
//! caller today ranks indexing priority ([`crate::priority::roots`]), but nothing
//! here is shaped for that.
//!
//! ⚠️ **Every failure is an empty answer, never an error.** A machine with Spotlight
//! turned off, a scope that isn't indexed, a permission we don't have, or a macOS
//! release that changes the query language all produce "no folders" and a log line.
//! Callers treat the result as a hint they may not get.
//!
//! macOS only. Every other platform gets an empty vec from a stub, so callers need no
//! `cfg` of their own.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// A folder, and how many of the user's recently-opened files sit directly in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentFolder {
    /// The folder itself. Absolute, exactly as Spotlight spelled its files' paths.
    pub path: PathBuf,
    /// Recently-opened files found DIRECTLY in this folder, never in its subfolders:
    /// a folder earns its count by holding the files, not by containing a folder that
    /// does. That is what keeps `$HOME` from outranking everything under it.
    pub files: usize,
}

/// The folders under `scope` holding files opened within `window`, busiest first.
///
/// Ties break on the path so the order is stable between calls, which matters because
/// a caller may compare or cache it.
///
/// `max_files` bounds the cost, ❌ not the answer: it caps how many Spotlight results
/// are read, and when it bites, the most recently used ones win. A window wide enough
/// to matter can match tens of thousands of files on a working machine, and each one
/// read costs a framework round-trip.
///
/// ⚠️ Blocking, and slow enough to matter: a synchronous Spotlight query plus one
/// attribute read per result. ❌ Never call it on a UI thread or from anything holding
/// a lock; give it a thread of its own.
#[cfg(target_os = "macos")]
pub fn folders_with_recent_files(scope: &Path, window: Duration, max_files: usize) -> Vec<RecentFolder> {
    let files = macos::recently_used_files(scope, window, max_files);
    fold_into_folders(files)
}

#[cfg(not(target_os = "macos"))]
pub fn folders_with_recent_files(_scope: &Path, _window: Duration, _max_files: usize) -> Vec<RecentFolder> {
    Vec::new()
}

/// Count the files per parent folder and order by that count, busiest first.
///
/// Split out from the query so the ranking is exercisable on every platform, with no
/// Spotlight and no machine state. Gated on `any(test, macOS)` for exactly that reason:
/// its only non-test reader is the macOS `folders_with_recent_files`, so a plain Linux
/// build has none and the workspace denies `unused`.
#[cfg(any(test, target_os = "macos"))]
fn fold_into_folders(files: impl IntoIterator<Item = PathBuf>) -> Vec<RecentFolder> {
    use std::collections::HashMap;

    let mut counts: HashMap<PathBuf, usize> = HashMap::new();
    for file in files {
        // A path with no parent is `/` or a bare relative name; neither is a folder
        // somebody works in.
        if let Some(parent) = file.parent().filter(|p| !p.as_os_str().is_empty()) {
            *counts.entry(parent.to_path_buf()).or_default() += 1;
        }
    }
    let mut folders: Vec<RecentFolder> = counts
        .into_iter()
        .map(|(path, files)| RecentFolder { path, files })
        .collect();
    folders.sort_by(|a, b| b.files.cmp(&a.files).then_with(|| a.path.cmp(&b.path)));
    folders
}

/// How many whole days `window` covers, as the query language's `$time.today()`
/// argument wants it. At least one: a sub-day window would otherwise become
/// `$time.today(0)`, which is midnight tonight and matches nothing.
///
/// Gated like [`fold_into_folders`]: only `mod macos` and the tests read it.
#[cfg(any(test, target_os = "macos"))]
fn window_in_days(window: Duration) -> u64 {
    (window.as_secs() / 86_400).max(1)
}

#[cfg(target_os = "macos")]
mod macos {
    use super::window_in_days;
    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::{CFIndex, CFRelease, CFTypeRef, TCFType};
    use core_foundation::string::{CFString, CFStringRef};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    /// Run the query synchronously and return before dispatching results live. Without
    /// it the query is a live, self-updating object we would have to run a run loop for.
    const K_MD_QUERY_SYNCHRONOUS: usize = 1;

    // Opaque CoreServices handles.
    #[repr(C)]
    struct __MDQuery(std::ffi::c_void);
    type MDQueryRef = *mut __MDQuery;
    #[repr(C)]
    struct __MDItem(std::ffi::c_void);
    type MDItemRef = *mut __MDItem;

    // SAFETY: the standard CoreServices MDQuery/MDItem C signatures. `MDQueryCreate`
    // returns a +1 (Create-rule) reference the caller releases; `MDItemCopyAttribute`
    // likewise. `MDQueryGetResultAtIndex` follows the GET rule, so its MDItem is
    // borrowed from the query and must NOT be released. A null allocator means the
    // default one.
    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn MDQueryCreate(
            allocator: CFTypeRef,
            query_string: CFStringRef,
            value_list_attrs: CFArrayRef,
            sorting_attrs: CFArrayRef,
        ) -> MDQueryRef;
        fn MDQuerySetSearchScope(query: MDQueryRef, scope_directories: CFArrayRef, scope_options: u32);
        fn MDQueryExecute(query: MDQueryRef, option_flags: usize) -> u8;
        fn MDQueryGetResultCount(query: MDQueryRef) -> CFIndex;
        fn MDQueryGetResultAtIndex(query: MDQueryRef, idx: CFIndex) -> MDItemRef;
        fn MDItemCopyAttribute(item: MDItemRef, name: CFStringRef) -> CFTypeRef;
    }

    /// Paths of files under `scope` whose `kMDItemLastUsedDate` falls inside `window`,
    /// most recently used LAST, capped at `max_files`.
    ///
    /// Folders are excluded at the query, so every path returned has a parent worth
    /// counting. Bundles (`.app`, `.rtfd`) are files as far as this is concerned, which
    /// is the useful answer: opening one says something about the folder it lives in.
    pub fn recently_used_files(scope: &Path, window: Duration, max_files: usize) -> Vec<PathBuf> {
        if max_files == 0 {
            return Vec::new();
        }
        // Whole-batch autoreleasepool: the CF objects the attribute reads allocate are
        // drained once at the end rather than accumulating for the process's life.
        objc2::rc::autoreleasepool(|_| run_query(scope, window, max_files))
    }

    fn run_query(scope: &Path, window: Duration, max_files: usize) -> Vec<PathBuf> {
        let days = window_in_days(window);
        // `kMDItemContentTypeTree` rather than `kMDItemContentType`: it carries the
        // whole conformance chain, so one clause excludes every kind of directory
        // instead of just the ones typed exactly `public.folder`.
        let query_text =
            format!("kMDItemLastUsedDate >= $time.today(-{days}) && kMDItemContentTypeTree != \"public.folder\"");
        let cf_query = CFString::new(&query_text);

        // Sort by last-used so a cap that bites keeps the most recent files. The
        // default order is ascending, so the tail is what we want; a macOS that ignored
        // the sort would leave us an arbitrary slice, which is a worse answer and still
        // a usable one.
        let sort_attrs = CFArray::from_CFTypes(&[CFString::from_static_string("kMDItemLastUsedDate")]);

        // SAFETY: both CF objects outlive the call (`as_concrete_TypeRef` borrows
        // them), a null allocator is valid, and a null `value_list_attrs` asks for no
        // value lists. The result is a +1 Create reference, released below.
        let query = unsafe {
            MDQueryCreate(
                std::ptr::null(),
                cf_query.as_concrete_TypeRef(),
                std::ptr::null(),
                sort_attrs.as_concrete_TypeRef(),
            )
        };
        if query.is_null() {
            log::warn!(target: "spotlight", "Spotlight rejected the recency query, so no folders are ranked by it: {query_text}");
            return Vec::new();
        }

        let paths = collect_paths(query, scope, max_files);
        // SAFETY: `query` is the non-null +1 reference from `MDQueryCreate`, not used
        // after this. Its borrowed MDItems die with it, which is why `collect_paths`
        // copies every path out before returning.
        unsafe { CFRelease(query as CFTypeRef) };
        paths
    }

    /// Scope, execute, and read `kMDItemPath` off the last `max_files` results.
    fn collect_paths(query: MDQueryRef, scope: &Path, max_files: usize) -> Vec<PathBuf> {
        let cf_scope = CFArray::from_CFTypes(&[CFString::new(&scope.to_string_lossy())]);
        // SAFETY: `query` is live and non-null; `cf_scope` outlives the call. No scope
        // options: the array names the only directory we want searched.
        unsafe { MDQuerySetSearchScope(query, cf_scope.as_concrete_TypeRef(), 0) };

        // SAFETY: `query` is live and non-null. Synchronous, so it returns with every
        // result already gathered and no run loop needed.
        let executed = unsafe { MDQueryExecute(query, K_MD_QUERY_SYNCHRONOUS) };
        if executed == 0 {
            log::warn!(target: "spotlight", "Spotlight wouldn't run the recency query (is indexing off for this volume?)");
            return Vec::new();
        }

        // SAFETY: `query` is live, non-null, and has executed.
        let total = unsafe { MDQueryGetResultCount(query) };
        if total <= 0 {
            return Vec::new();
        }
        // The tail is the most recently used slice, per the sort in `run_query`.
        let take = (total as usize).min(max_files);
        let first = total - take as CFIndex;

        let path_attr = CFString::from_static_string("kMDItemPath");
        let mut paths = Vec::with_capacity(take);
        for idx in first..total {
            // SAFETY: `idx` is in `0..total` and the query is live, so the GET-rule
            // MDItem it hands back is valid for as long as the query is. ❌ Not
            // released: we don't own it.
            let item = unsafe { MDQueryGetResultAtIndex(query, idx) };
            if item.is_null() {
                continue;
            }
            // SAFETY: `item` is live and non-null, `path_attr` outlives the call. The
            // value is a +1 Copy reference, handed straight to `wrap_under_create_rule`
            // so the CFString owns and releases it.
            let value = unsafe { MDItemCopyAttribute(item, path_attr.as_concrete_TypeRef()) };
            if value.is_null() {
                continue;
            }
            // SAFETY: `kMDItemPath` is documented as a CFString, and the reference is
            // the +1 this loop just took.
            let path = unsafe { CFString::wrap_under_create_rule(value as CFStringRef) };
            paths.push(PathBuf::from(path.to_string()));
        }
        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_busiest_folder_comes_first() {
        let folders = fold_into_folders([
            PathBuf::from("/Users/x/quiet/a.txt"),
            PathBuf::from("/Users/x/busy/a.txt"),
            PathBuf::from("/Users/x/busy/b.txt"),
            PathBuf::from("/Users/x/busy/c.txt"),
        ]);
        assert_eq!(folders[0].path, PathBuf::from("/Users/x/busy"));
        assert_eq!(folders[0].files, 3);
        assert_eq!(folders[1].path, PathBuf::from("/Users/x/quiet"));
        assert_eq!(folders[1].files, 1);
    }

    #[test]
    fn a_file_counts_for_its_own_folder_and_no_ancestor() {
        // The whole reason the count is direct: were it recursive, `/Users/x` would
        // hold every file below it and outrank every folder that actually matters.
        let folders = fold_into_folders([
            PathBuf::from("/Users/x/deep/nest/a.txt"),
            PathBuf::from("/Users/x/deep/nest/b.txt"),
        ]);
        assert_eq!(folders.len(), 1, "only the immediate parent is counted");
        assert_eq!(folders[0].path, PathBuf::from("/Users/x/deep/nest"));
    }

    #[test]
    fn folders_with_equal_counts_come_back_in_a_stable_order() {
        let ordered = |files: [&str; 2]| -> Vec<PathBuf> {
            fold_into_folders(files.iter().map(PathBuf::from))
                .into_iter()
                .map(|folder| folder.path)
                .collect()
        };
        // Same two folders, opposite input order: the answer must not move, or a
        // caller that caches or compares the ranking sees phantom churn.
        assert_eq!(
            ordered(["/b/one.txt", "/a/one.txt"]),
            ordered(["/a/one.txt", "/b/one.txt"])
        );
    }

    #[test]
    fn a_parentless_path_is_dropped_rather_than_counted_as_root() {
        let folders = fold_into_folders([PathBuf::from("/"), PathBuf::from("bare.txt")]);
        assert!(
            folders.is_empty(),
            "neither has a folder worth ranking, got {folders:?}"
        );
    }

    #[test]
    fn a_window_shorter_than_a_day_still_asks_for_a_whole_day() {
        // `$time.today(0)` is midnight tonight, which matches nothing.
        assert_eq!(window_in_days(Duration::from_secs(60)), 1);
        assert_eq!(window_in_days(Duration::ZERO), 1);
    }

    #[test]
    fn a_window_in_days_is_the_number_of_whole_days_it_spans() {
        assert_eq!(window_in_days(Duration::from_secs(30 * 86_400)), 30);
        // Partial days round down: 29.5 days asks for 29, never 30.
        assert_eq!(window_in_days(Duration::from_secs(30 * 86_400 - 43_200)), 29);
    }
}
