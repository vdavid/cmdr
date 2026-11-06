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
pub struct SourceVersion(Arc<dyn Any + Send + Sync>);

impl SourceVersion {
    /// A version only `T`'s backend can read back.
    pub fn new<T: Any + Send + Sync>(version: T) -> Self {
        Self(Arc::new(version))
    }

    /// The version as its backend made it, or `None` when another backend did.
    #[must_use]
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.0.downcast_ref::<T>()
    }
}

impl std::fmt::Debug for SourceVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // What's inside is the backend's, and may name a path.
        f.write_str("SourceVersion(..)")
    }
}
