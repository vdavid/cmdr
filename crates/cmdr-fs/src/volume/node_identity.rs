//! [`NodeIdentity`]: which filesystem node sits at a path, beyond its name.

/// Which filesystem node an entry is, so "the same name" and "the same thing" can
/// be told apart ([`Volume::node_identity`](super::Volume::node_identity)).
///
/// A path can't answer it alone: the macOS Trash hands a freed name to the next
/// item trashed under it, so the folder at `~/.Trash/Invoices` today may be a
/// stranger to the one Cmdr put there last week.
///
/// Inode plus birth time, ❌ no device number: `st_dev` can change across a reboot
/// or a replug while the node stays put. Why, and the evidence:
/// `crates/cmdr-operation-log/DETAILS.md` § "A trashed item must still be the one trashed".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeIdentity {
    /// The node's inode number (`st_ino`).
    pub inode: u64,
    /// When the node was created (`st_birthtime`), in nanoseconds since the Unix
    /// epoch.
    pub birth_nanos: i64,
}

impl NodeIdentity {
    /// The identity `metadata` describes, read off an `lstat`. `None` when the
    /// filesystem keeps no birth time (or one before 1970), since an inode alone
    /// can be reused once its node is gone.
    #[cfg(unix)]
    pub fn of(metadata: &std::fs::Metadata) -> Option<Self> {
        use std::os::unix::fs::MetadataExt;

        let born = metadata.created().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
        Some(Self {
            inode: metadata.ino(),
            birth_nanos: i64::try_from(born.as_nanos()).ok()?,
        })
    }
}
