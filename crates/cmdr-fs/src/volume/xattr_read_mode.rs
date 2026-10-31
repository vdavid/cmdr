//! The caller's statement about the paths it hands `Volume::read_xattr_many`.

/// One path's answer from [`Volume::read_xattr_many`](super::Volume::read_xattr_many): the
/// value, or `None` for a file with no such attribute.
pub type XattrAnswer = Result<Option<Vec<u8>>, super::VolumeError>;

/// What a missing file answers when [`Volume::read_xattr_many`](super::Volume::read_xattr_many)
/// asks about it, and with that, how the read goes on a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XattrReadMode {
    /// A missing file is [`NotFound`](super::VolumeError::NotFound). For a read that decides a write.
    Verified,
    /// A missing file reads as `Ok(None)`. For paths a listing just named, where it's
    /// the same answer ("no tags on that row") and a server spends less to give it.
    AssumePathsExist,
}
