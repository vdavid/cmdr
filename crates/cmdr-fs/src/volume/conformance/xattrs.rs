//! What `Volume`'s extended attributes promise, on every backend that stores them.
//!
//! The fixture is two names in one writable folder: one a file with no attributes
//! yet, the other nothing at all.

use std::path::{Path, PathBuf};

use crate::volume::{Volume, VolumeError, XattrReadMode};

/// An ordinary xattr name, which every store takes and deletes. Not a Finder tag:
/// a QNAP keeps that one when told to delete it, which is a server's business and
/// not this contract's.
const NAME: &str = "user.cmdr-conformance";

/// Reads, writes, and removes `NAME` on `file` (seeded, no attributes), asking about
/// `missing` (nothing there) along the way.
pub async fn assert_xattrs_keep_their_promises(volume: &dyn Volume, file: &Path, missing: &Path) {
    assert!(
        volume.supports_xattrs(),
        "this contract is for a backend that claims to store them"
    );
    let read_one = async |path: &Path, mode| {
        let paths = [path.to_path_buf()];
        volume
            .read_xattr_many(&paths, NAME, mode)
            .await
            .pop()
            .expect("one result per path")
    };

    assert_eq!(
        read_one(file, XattrReadMode::Verified)
            .await
            .expect("a file with none reads"),
        None,
        "a file with no such attribute is Ok(None), never an error"
    );

    // A missing file is told apart only when asked to.
    assert!(
        matches!(
            read_one(missing, XattrReadMode::Verified).await,
            Err(VolumeError::NotFound(_))
        ),
        "Verified: a missing file is NotFound"
    );
    assert_eq!(
        read_one(missing, XattrReadMode::AssumePathsExist)
            .await
            .expect("assumed"),
        None,
        "AssumePathsExist: a missing file reads as no attribute"
    );

    // A write lands byte for byte, and a shorter one leaves nothing of the longer.
    volume.write_xattr(file, NAME, b"a longer value").await.expect("write");
    assert_eq!(
        read_one(file, XattrReadMode::Verified)
            .await
            .expect("read back")
            .as_deref(),
        Some(&b"a longer value"[..])
    );
    volume.write_xattr(file, NAME, b"short").await.expect("overwrite");
    assert_eq!(
        read_one(file, XattrReadMode::AssumePathsExist)
            .await
            .expect("read back")
            .as_deref(),
        Some(&b"short"[..])
    );

    // A batch answers per path, in order, each on its own.
    let batch: Vec<PathBuf> = vec![missing.to_path_buf(), file.to_path_buf(), missing.to_path_buf()];
    let answers = volume.read_xattr_many(&batch, NAME, XattrReadMode::Verified).await;
    assert_eq!(answers.len(), 3, "one result per path");
    assert!(matches!(answers[0], Err(VolumeError::NotFound(_))));
    assert_eq!(answers[1].as_ref().expect("the file").as_deref(), Some(&b"short"[..]));
    assert!(matches!(answers[2], Err(VolumeError::NotFound(_))));

    // Writing never creates the file, and removing asks for a file too.
    assert!(
        matches!(
            volume.write_xattr(missing, NAME, b"x").await,
            Err(VolumeError::NotFound(_))
        ),
        "a write to a missing file is NotFound"
    );
    assert!(!volume.exists(missing).await, "a refused write creates nothing");
    assert!(
        matches!(volume.remove_xattr(missing, NAME).await, Err(VolumeError::NotFound(_))),
        "a removal on a missing file is NotFound"
    );

    // Removal, and removing what's already gone.
    volume.remove_xattr(file, NAME).await.expect("remove");
    assert_eq!(
        read_one(file, XattrReadMode::Verified).await.expect("after removal"),
        None
    );
    volume
        .remove_xattr(file, NAME)
        .await
        .expect("removing what's gone succeeds");
}
