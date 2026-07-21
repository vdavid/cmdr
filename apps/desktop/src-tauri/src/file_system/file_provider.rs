//! Recognizing File Provider domain roots (macOS).
//!
//! A File Provider domain (Dropbox, Google Drive, iCloud Drive, MacDroid, …) is
//! NOT a mount point: its root reports the same `st_dev` as `$HOME` and never
//! appears in `mount`, so the usual volume-boundary detectors are blind to it.
//! What DOES mark it is an extended attribute, [`DOMAIN_ID_XATTR`], present on the
//! domain root only — not on its children, not on `~/Library/CloudStorage` itself,
//! and not on ordinary folders.
//!
//! Reading it costs ~5 µs: a plain APFS xattr read, no XPC, no provider process,
//! so it works while the provider is offline and can't hang. It needs no
//! entitlement.
//!
//! **This is a private Apple xattr and an OPTIMIZATION, never a safety guarantee.**
//! It's an implementation detail of `fileproviderd`, undocumented and not
//! contractual, so any code that must stay correct when it disappears needs its own
//! backstop; a `None` here means "not recognized", never "proven ordinary folder".
//! Evidence and the authoritative-but-costly `NSFileProviderManager` alternative:
//! [`/docs/notes/fileprovider-domain-detection.md`](../../../../../docs/notes/fileprovider-domain-detection.md)
//! (verified on macOS 26.5.2, build 25F84, 2026-07-20).

/// The extended attribute `fileproviderd` writes on every File Provider domain
/// root. Its value is `<provider extension bundle id>/<domain identifier>`.
const DOMAIN_ID_XATTR: &str = "com.apple.file-provider-domain-id";

/// The File Provider domain identifier for `path`, or `None` when `path` isn't a
/// domain root (the overwhelming majority of directories).
///
/// The returned string is the raw xattr value,
/// `<provider extension bundle id>/<domain identifier>` (for example
/// `com.getdropbox.dropbox.fileprovider/c840514d-…`). Callers that only need the
/// yes/no answer can `.is_some()` it.
///
/// Doesn't follow symlinks (`XATTR_NOFOLLOW`), so a symlink pointing INTO a domain
/// isn't mistaken for the domain root. A read failure, a missing attribute, and a
/// non-UTF-8 value all collapse to `None`: this is a hint, so an unreadable path is
/// simply "not recognized".
pub(crate) fn domain_id_for_dir(path: &str) -> Option<String> {
    read_domain_id_xattr(path, DOMAIN_ID_XATTR)
}

/// The read itself, with the attribute name injected so tests can exercise it
/// against a name they're allowed to write. macOS refuses `com.apple.*` xattrs to
/// an unentitled process, so a test that builds its fixture with the real constant
/// can never pass; see the tests below.
fn read_domain_id_xattr(path: &str, name: &str) -> Option<String> {
    let raw = xattr::get(path, name).ok()??;
    String::from_utf8(raw).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An ordinary directory carries no domain-id xattr, and a path that doesn't
    /// exist reads as "not recognized" rather than panicking.
    #[test]
    fn ordinary_directory_is_not_a_domain_root() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().to_string_lossy().into_owned();
        assert_eq!(domain_id_for_dir(&path), None, "a plain temp dir is no domain root");
        assert_eq!(
            domain_id_for_dir(&format!("{path}/nope")),
            None,
            "a missing path is None"
        );
    }

    /// A directory carrying the domain-id xattr reads back as that domain, value
    /// verbatim.
    ///
    /// ❌ Do NOT write `DOMAIN_ID_XATTR` here to build the fixture. macOS refuses
    /// `com.apple.*` extended attributes to an unentitled process with EPERM, so a
    /// test that sets it fails on a real machine no matter what this module does
    /// (verified on macOS 26.5.2, 2026-07-21: `xattr -w com.apple.file-provider-domain-id`
    /// → "Operation not permitted", while a `com.example.*` name succeeds). The
    /// read path is exercised against a name we ARE allowed to write; the constant
    /// itself is covered by `the_domain_id_xattr_name_is_the_one_macos_uses`.
    #[test]
    fn a_directory_carrying_the_xattr_reports_its_value_verbatim() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().to_string_lossy().into_owned();
        let value = "com.example.provider/2f3c1a90-0000-4000-8000-000000000001";
        let writable_name = "com.example.file-provider-domain-id";
        xattr::set(&path, writable_name, value.as_bytes()).expect("set the stand-in xattr");

        assert_eq!(
            read_domain_id_xattr(&path, writable_name),
            Some(value.to_string()),
            "the reader returns the attribute's value unchanged"
        );
    }

    /// The production constant is the name macOS actually uses. Split out because
    /// the fixture above cannot use it (see that test's note), so without this the
    /// name itself would be untested and a typo would silently disable detection.
    #[test]
    fn the_domain_id_xattr_name_is_the_one_macos_uses() {
        assert_eq!(DOMAIN_ID_XATTR, "com.apple.file-provider-domain-id");
    }
}
