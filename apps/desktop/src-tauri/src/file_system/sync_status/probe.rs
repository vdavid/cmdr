//! Asking macOS what a single file's cloud sync state is.
//!
//! Three tiers, cheapest first:
//!
//! 1. **Is a File Provider domain anywhere above this path?** An xattr read on the
//!    parent directory's ancestors (~5 µs, memoized per directory,
//!    `cmdr_fs::file_provider`). For a file no provider manages — nearly every
//!    file on the machine — the answer is [`SyncKnowledge::NotCloudManaged`] and
//!    neither of the next two tiers runs at all.
//! 2. `stat`'s `SF_DATALESS` flag answers "is this a stub?" off the inode. No
//!    provider process is involved.
//! 3. An `NSURL` ubiquitous-item resource value answers "is it moving right now?".
//!    This one is a **synchronous XPC round-trip into the provider's daemon**
//!    (`fileproviderd` and the provider's `.appex`), so it has no deadline of its
//!    own and can block for as long as the provider is unwell. Everything in the
//!    parent module exists to keep that fact from reaching the user.

use super::SyncKnowledge;
use cmdr_fs::file_provider::{DomainMembership, FileProviderDomains};
use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

/// macOS `SF_DATALESS` flag indicating a stub/online-only file.
const SF_DATALESS: u32 = 0x40000000;

/// How long a directory's "is it in a cloud domain?" answer stands before it's
/// re-derived. Installing Dropbox or signing into iCloud makes a whole tree
/// cloud-managed without changing any directory's contents, so no invalidation
/// fires and only this bound catches it. Ten minutes costs one xattr read per path
/// component per directory per ten minutes.
const DOMAIN_RECHECK_AFTER: Duration = Duration::from_secs(10 * 60);

/// The process's memo of which directories sit inside a File Provider domain.
/// Shared by every pool worker; its own locking makes it safe to hammer.
static DOMAINS: LazyLock<FileProviderDomains> = LazyLock::new(|| FileProviderDomains::new(DOMAIN_RECHECK_AFTER));

/// What macOS knows about one file's cloud state. Blocks for as long as the file's
/// provider takes.
pub(super) fn sync_status_for(path: &Path) -> SyncKnowledge {
    knowledge_for(path, &DOMAINS)
}

/// The probe with its domain resolver injected, so a test can drive the
/// no-domain-here shortcut without a real File Provider.
fn knowledge_for(path: &Path, domains: &FileProviderDomains) -> SyncKnowledge {
    use std::os::macos::fs::MetadataExt;

    if domains.membership_of(path) == DomainMembership::Outside {
        // Nothing manages this file, so there is no state to ask about — and no
        // reason to spend a `stat` or an `NSURL` on finding that out again.
        return SyncKnowledge::NotCloudManaged;
    }

    let Ok(metadata) = std::fs::metadata(path) else {
        return SyncKnowledge::Indeterminate;
    };
    let is_dir = metadata.is_dir();

    if metadata.st_flags() & SF_DATALESS != 0 {
        // A stub: either purely online, or being fetched right now.
        match ubiquitous_bool(path, is_dir, "NSURLUbiquitousItemIsDownloadingKey") {
            ResourceValue::Answered(true) => SyncKnowledge::Downloading,
            _ => SyncKnowledge::OnlineOnly,
        }
    } else {
        // Local content exists: either settled, or being pushed up.
        match ubiquitous_bool(path, is_dir, "NSURLUbiquitousItemIsUploadingKey") {
            ResourceValue::Answered(true) => SyncKnowledge::Uploading,
            ResourceValue::Answered(false) => SyncKnowledge::Synced,
            ResourceValue::NotApplicable => SyncKnowledge::NotCloudManaged,
            ResourceValue::Unreadable => SyncKnowledge::Indeterminate,
        }
    }
}

/// What one `NSURL` resource-value read came back with.
///
/// The three cases are three different facts, and collapsing them costs the caller
/// the one distinction that matters for caching: a property that doesn't apply is
/// permanent, a read that failed is not.
enum ResourceValue {
    /// The property applies and holds this value.
    Answered(bool),
    /// The read succeeded and the property isn't defined for this URL: no cloud
    /// provider owns the file.
    NotApplicable,
    /// The read itself didn't answer.
    Unreadable,
}

/// Reads a boolean ubiquitous-item property off `NSURL`.
///
/// `is_dir` comes from the caller's own `stat` and is passed to
/// `fileURLWithPath:isDirectory:`, which removes the `stat` `NSURL` would
/// otherwise do for itself (measured on macOS 26.6, 2026-08-21: 4.08 µs → 0.53 µs
/// to build the URL, and the same ~1 µs gap survives all the way through
/// `getResourceValue`, so the syscall is gone rather than deferred). ⚠️ It must be
/// the real value: `isDirectory:` decides whether the URL keeps a trailing slash,
/// which changes the path the File Provider machinery matches on.
fn ubiquitous_bool(path: &Path, is_dir: bool, key: &str) -> ResourceValue {
    use objc2::rc::{Retained, autoreleasepool};
    use objc2_foundation::{NSNumber, NSString, NSURL};

    // Drain autoreleased ObjC objects (NSURL, NSString) created per call. Pool
    // workers are plain OS threads and have no AppKit autorelease pool of their own.
    autoreleasepool(|_| {
        let Some(path_str) = path.to_str() else {
            return ResourceValue::Unreadable;
        };
        let ns_path = NSString::from_str(path_str);
        let url = NSURL::fileURLWithPath_isDirectory(&ns_path, is_dir);

        let key = NSString::from_str(key);
        let mut value: Option<Retained<objc2::runtime::AnyObject>> = None;
        // SAFETY: `url` is a valid NSURL, `key` a valid NSString, and `&mut value` a valid
        // `&mut Option<Retained<_>>` out-param. On success objc2 stores an already-retained
        // object there per its out-param convention, so the `Retained` owns one reference.
        let success = unsafe { url.getResourceValue_forKey_error(&mut value, &key) };

        if success.is_err() {
            return ResourceValue::Unreadable;
        }
        match value {
            // A nil value on a successful read is Cocoa's "this property isn't
            // defined for this URL", which for a ubiquitous key means the file
            // isn't in anybody's cloud.
            None => ResourceValue::NotApplicable,
            Some(object) => match object.downcast::<NSNumber>() {
                Ok(number) => ResourceValue::Answered(number.boolValue()),
                Err(_) => ResourceValue::Unreadable,
            },
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmdr_fs::testing::TestDir;

    /// A resolver that says "no domain anywhere", which is the truth for a scratch
    /// directory and for nearly every path on a real machine. Scripted rather than
    /// real so the test doesn't depend on which cloud apps the machine running it
    /// happens to have installed.
    fn no_domains() -> FileProviderDomains {
        FileProviderDomains::with_domain_roots(Vec::new(), DOMAIN_RECHECK_AFTER)
    }

    /// A resolver that treats `root` as a cloud provider's domain root, so the
    /// probe takes the full `stat` + `NSURL` path for anything under it.
    fn domain_at(root: &Path) -> FileProviderDomains {
        // Canonical, because the walk resolves symlinks before it climbs and a
        // scratch directory lives under the `/var` → `/private/var` link.
        let root = root.canonicalize().expect("canonical fixture root");
        FileProviderDomains::with_domain_roots(vec![root], DOMAIN_RECHECK_AFTER)
    }

    /// The shortcut is the whole point: outside every domain, a path is answered
    /// without a `stat` or an `NSURL` at all.
    ///
    /// A path that doesn't exist is what makes that observable: the shortcut
    /// answers `NotCloudManaged`, while the same path INSIDE a domain reaches the
    /// `stat`, fails it, and answers `Indeterminate`. Same path, two answers, and
    /// only one of them can have touched the filesystem.
    #[test]
    fn a_path_outside_every_domain_is_answered_without_touching_the_filesystem() {
        let dir = TestDir::new("sync-status-probe");
        let missing = dir.join("not-there.txt");

        assert_eq!(
            knowledge_for(&missing, &no_domains()),
            SyncKnowledge::NotCloudManaged,
            "no domain above it, so no provider owns it and nothing was read"
        );
        assert_eq!(
            knowledge_for(&missing, &domain_at(&dir)),
            SyncKnowledge::Indeterminate,
            "inside a domain the probe really looks, and a vanished file is no answer"
        );
    }

    /// A real file outside every domain takes the same shortcut, so an ordinary
    /// folder costs one memoized xattr read per row instead of a `stat` plus an
    /// `NSURL` resource-value read.
    #[test]
    fn an_ordinary_file_is_not_cloud_managed() {
        let dir = TestDir::new("sync-status-probe");
        let file = dir.join("notes.txt");
        std::fs::write(&file, b"hello").expect("write the fixture");

        assert_eq!(knowledge_for(&file, &no_domains()), SyncKnowledge::NotCloudManaged);
    }

    /// The authoritative negative, from macOS rather than from the xattr: a file
    /// the probe DOES ask about, which no provider owns, comes back
    /// `NotCloudManaged` rather than "we couldn't tell". That's what makes the
    /// structural TTL safe even when the domain marker isn't believed.
    #[test]
    fn a_file_the_provider_has_no_state_for_is_not_cloud_managed_either() {
        let dir = TestDir::new("sync-status-probe");
        let file = dir.join("notes.txt");
        std::fs::write(&file, b"hello").expect("write the fixture");

        assert_eq!(
            knowledge_for(&file, &domain_at(&dir)),
            SyncKnowledge::NotCloudManaged,
            "the resource value doesn't apply, which is Cocoa for 'not a cloud file'"
        );
    }
}
