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
    let raw = xattr::get(path, DOMAIN_ID_XATTR).ok()??;
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
    /// verbatim. Writing the xattr ourselves keeps the test off any real provider.
    #[test]
    fn directory_with_the_xattr_reports_its_domain_id() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().to_string_lossy().into_owned();
        let value = "com.example.provider/2f3c1a90-0000-4000-8000-000000000001";
        xattr::set(&path, DOMAIN_ID_XATTR, value.as_bytes()).expect("set the domain-id xattr");

        assert_eq!(domain_id_for_dir(&path), Some(value.to_string()));
    }
}
