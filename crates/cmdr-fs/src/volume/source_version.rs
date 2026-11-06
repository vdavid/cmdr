//! [`SourceVersion`]: which version of a file a read stream was reading, kept so
//! a copy can carry on from the byte it got to after a long outage, once the
//! backend proves the file is still that version
//! ([`Volume::resume_read_stream`](super::Volume::resume_read_stream)).

use std::any::Any;
use std::sync::Arc;

/// Which version of a file a [`VolumeReadStream`](super::VolumeReadStream) is
/// reading, in whatever terms its backend can check again later: SMB's
/// identity, length, and write and change times, an object store's ETag, a
/// local file's inode and change time.
///
/// Opaque to everyone but the backend that made it: the engine keeps it and
/// hands it back, and only that backend reads it
/// ([`downcast_ref`](Self::downcast_ref)). A version some other backend made
/// reads as `None` there, and the resume refuses as `NotSupported`.
///
/// In memory only, for one operation: nothing persists it, so it needs no
/// stable shape.
#[derive(Clone)]
pub struct SourceVersion {
    version: Arc<dyn Any + Send + Sync>,
    /// Whether a later stream's version is the same file, for a version that
    /// carries its rule ([`comparable`](Self::comparable)).
    judge: Option<fn(&SourceVersion, &SourceVersion) -> bool>,
}

/// A [`comparable`](SourceVersion::comparable) version: the value, and its
/// backend's rule for "the same file".
struct Comparable<T> {
    value: T,
    same: fn(&T, &T) -> bool,
}

impl SourceVersion {
    /// A version only `T`'s backend can read back: it proves the file itself
    /// in its own `resume_read_stream` (SMB, through smb2's resume).
    pub fn new<T: Any + Send + Sync>(version: T) -> Self {
        Self {
            version: Arc::new(version),
            judge: None,
        }
    }

    /// A version that says whether a later read saw the same file
    /// (`same(was, now)`), so the trait's default
    /// [`resume_read_stream`](super::Volume::resume_read_stream) carries a copy
    /// on for its backend: an offset open, then this rule against the version
    /// the new stream names. For a backend whose offset open learns the
    /// version anyway (S3, WebDAV, NFS, SFTP).
    pub fn comparable<T: Any + Send + Sync>(version: T, same: fn(&T, &T) -> bool) -> Self {
        Self {
            version: Arc::new(Comparable { value: version, same }),
            judge: Some(judge::<T>),
        }
    }

    /// The version as its backend made it, or `None` when another backend did.
    #[must_use]
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.version.downcast_ref::<T>().or_else(|| {
            self.version
                .downcast_ref::<Comparable<T>>()
                .map(|comparable| &comparable.value)
        })
    }

    /// Whether `now`, named by a later stream, is this same file: `None` for a
    /// version that carries no rule ([`new`](Self::new)).
    #[must_use]
    pub fn is_same_as(&self, now: &SourceVersion) -> Option<bool> {
        self.judge.map(|judge| judge(self, now))
    }
}

/// [`SourceVersion::is_same_as`] for a `T` version: `was`'s rule over both
/// values, and never the same across two kinds of version.
fn judge<T: Any>(was: &SourceVersion, now: &SourceVersion) -> bool {
    let Some(was) = was.version.downcast_ref::<Comparable<T>>() else {
        return false;
    };
    now.downcast_ref::<T>().is_some_and(|now| (was.same)(&was.value, now))
}

/// Which version of an HTTP entity a GET read, as the server named it: what an
/// object store or a WebDAV server can prove again on a ranged GET.
///
/// A strong ETag when the server gave one (two answers with the same strong
/// ETag carry the same bytes, RFC 9110 § 8.8.3), else the `Last-Modified`
/// date, and the full length either way. A weak ETag (`W/"…"`) promises only
/// "equivalent", never the same bytes, so it never counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpEntityVersion {
    strong_etag: Option<String>,
    last_modified: Option<String>,
    length: u64,
}

impl HttpEntityVersion {
    /// The version a response's `ETag` and `Last-Modified` headers and its
    /// full `length` name, or `None` when they name too little to prove the
    /// entity unchanged later (no strong ETag and no date).
    #[must_use]
    pub fn of(etag: Option<&str>, last_modified: Option<&str>, length: u64) -> Option<Self> {
        let strong_etag = etag
            .map(str::trim)
            .filter(|etag| !etag.is_empty() && !etag.starts_with("W/"))
            .map(str::to_owned);
        let last_modified = last_modified
            .map(str::trim)
            .filter(|date| !date.is_empty())
            .map(str::to_owned);
        if strong_etag.is_none() && last_modified.is_none() {
            return None;
        }
        Some(Self {
            strong_etag,
            last_modified,
            length,
        })
    }

    /// Whether `now`, read off a later response, is this same entity: the
    /// same length, and the same strong ETag when this one had one, else the
    /// same date.
    #[must_use]
    pub fn is_same_as(&self, now: &Self) -> bool {
        if self.length != now.length {
            return false;
        }
        match (&self.strong_etag, &now.strong_etag) {
            (Some(was), Some(is)) => was == is,
            (Some(_), None) => false,
            (None, _) => self.last_modified.is_some() && self.last_modified == now.last_modified,
        }
    }
}

impl std::fmt::Debug for SourceVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // What's inside is the backend's, and may name a path.
        f.write_str("SourceVersion(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::HttpEntityVersion;

    #[test]
    fn a_strong_etag_decides_and_a_weak_one_never_counts() {
        let was = HttpEntityVersion::of(Some("\"abc\""), Some("Mon, 02 Nov 2026 10:00:00 GMT"), 10).unwrap();
        let same = HttpEntityVersion::of(Some("\"abc\""), Some("Tue, 03 Nov 2026 10:00:00 GMT"), 10).unwrap();
        let rewritten = HttpEntityVersion::of(Some("\"abd\""), Some("Mon, 02 Nov 2026 10:00:00 GMT"), 10).unwrap();
        assert!(was.is_same_as(&same), "the same strong ETag is the same bytes");
        assert!(!was.is_same_as(&rewritten), "a new strong ETag is a new entity");
        assert!(
            HttpEntityVersion::of(Some("W/\"abc\""), None, 10).is_none(),
            "a weak ETag alone proves nothing"
        );
    }

    #[test]
    fn without_a_strong_etag_the_date_and_length_decide() {
        let was = HttpEntityVersion::of(Some("W/\"x\""), Some("Mon, 02 Nov 2026 10:00:00 GMT"), 10).unwrap();
        assert!(was.is_same_as(&HttpEntityVersion::of(None, Some("Mon, 02 Nov 2026 10:00:00 GMT"), 10).unwrap()));
        assert!(!was.is_same_as(&HttpEntityVersion::of(None, Some("Mon, 02 Nov 2026 10:00:01 GMT"), 10).unwrap()));
        assert!(!was.is_same_as(&HttpEntityVersion::of(None, Some("Mon, 02 Nov 2026 10:00:00 GMT"), 11).unwrap()));
        assert!(
            HttpEntityVersion::of(None, None, 10).is_none(),
            "nothing named, nothing to prove"
        );
    }
}

#[cfg(test)]
mod comparable_tests {
    use super::{HttpEntityVersion, SourceVersion};

    fn http(etag: &str) -> SourceVersion {
        SourceVersion::comparable(
            HttpEntityVersion::of(Some(etag), None, 10).unwrap(),
            HttpEntityVersion::is_same_as,
        )
    }

    #[test]
    fn a_comparable_version_judges_a_later_one_by_its_own_rule() {
        assert_eq!(http("\"a\"").is_same_as(&http("\"a\"")), Some(true));
        assert_eq!(http("\"a\"").is_same_as(&http("\"b\"")), Some(false));
        assert!(
            http("\"a\"").downcast_ref::<HttpEntityVersion>().is_some(),
            "still readable by its backend"
        );
    }

    #[test]
    fn another_kind_of_version_is_never_the_same_and_an_opaque_one_has_no_rule() {
        let other = SourceVersion::comparable(7u64, |a: &u64, b: &u64| a == b);
        assert_eq!(http("\"a\"").is_same_as(&other), Some(false));
        assert_eq!(SourceVersion::new(7u64).is_same_as(&other), None);
    }
}
