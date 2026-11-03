//! What `Volume`'s extended attributes promise, on every backend that stores them.
//!
//! The fixture is two names in one writable folder: one a file with no attributes
//! yet, the other nothing at all.

use std::path::{Path, PathBuf};

use super::modification_date::DatedSource;
use crate::metadata::{CarriedMetadata, ClassOutcome, MetadataAsk, MetadataClasses, Xattr};
use crate::volume::{StreamLength, Volume, VolumeError, WriteMode, XattrReadMode};

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

/// What a copy hands a destination: a Finder tag, FinderInfo (the custom-icon
/// bit and a type and creator), and an app's own attribute, sorted by name.
fn copied_xattrs() -> Vec<Xattr> {
    // macOS's own plist for one red tag.
    const RED: &[u8] = &[
        0x62, 0x70, 0x6C, 0x69, 0x73, 0x74, 0x30, 0x30, 0xA1, 0x01, 0x55, 0x52, 0x65, 0x64, 0x0A, 0x36, 0x08, 0x0A,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10,
    ];
    let mut finder_info = vec![0u8; 32];
    finder_info[..8].copy_from_slice(b"TEXTttxt");
    finder_info[8] = 0x04;
    sorted(vec![
        Xattr {
            name: "com.apple.metadata:_kMDItemUserTags".into(),
            value: RED.to_vec(),
        },
        Xattr {
            name: "com.apple.FinderInfo".into(),
            value: finder_info,
        },
        Xattr {
            name: NAME.into(),
            value: b"an app's own note".to_vec(),
        },
    ])
}

fn sorted(mut xattrs: Vec<Xattr>) -> Vec<Xattr> {
    xattrs.sort_by(|a, b| a.name.cmp(&b.name));
    xattrs
}

/// Each of `expected` reads back from `path` byte for byte.
async fn assert_reads_back(volume: &dyn Volume, path: &Path, expected: &[Xattr]) {
    for xattr in expected {
        let name = xattr.name.to_str().expect("a UTF-8 name");
        let read = volume
            .read_xattr_many(&[path.to_path_buf()], name, XattrReadMode::Verified)
            .await
            .pop()
            .expect("one result per path");
        assert_eq!(
            read.unwrap_or_else(|e| panic!("{name} on {} must read, got {e:?}", path.display()))
                .as_deref(),
            Some(xattr.value.as_slice()),
            "{name} on {} must read back as the copy wrote it",
            path.display()
        );
    }
}

/// A copy onto `volume` keeps a source's xattrs, and a copy off it reports them,
/// on a backend whose caps say it stores them
/// ([`MetadataCaps::stores`](crate::metadata::MetadataCaps::stores)): the
/// class `assert_write_from_stream_keeps_metadata` can't see, since a listing
/// has no field for it.
///
/// Writes a small file and a 1 MiB one into `dir` (a backend that streams the
/// big one and sends the small in one frame covers both paths) from a source
/// carrying a Finder tag, FinderInfo, and an app's attribute, and insists each
/// reads back. Then, where the caps say the read streams report xattrs, that a
/// copy asking for their values gets them, one asking only for an outline gets
/// their names and no values, and a file with none reports none. Then the same
/// on a folder through `apply_metadata` and `folder_metadata`, where
/// `folder_stores` claims them. `dir` must not exist yet.
pub async fn assert_a_copy_keeps_xattrs(volume: &dyn Volume, dir: &Path) {
    let caps = volume.metadata_caps();
    assert!(
        caps.stores.contains(MetadataClasses::XATTRS),
        "this contract is for a backend that claims to keep a copy's xattrs"
    );
    assert!(
        !volume.exists(dir).await,
        "fixture precondition: {} must not exist yet",
        dir.display()
    );
    volume
        .create_directory(dir)
        .await
        .unwrap_or_else(|e| panic!("{} must be creatable, got {e:?}", dir.display()));
    let expected = copied_xattrs();
    let carried = CarriedMetadata {
        xattrs: expected.clone(),
        ..CarriedMetadata::default()
    };
    let values = MetadataAsk {
        values: MetadataClasses::XATTRS,
        outline: MetadataClasses::NONE,
    };
    let outline = MetadataAsk {
        values: MetadataClasses::NONE,
        outline: MetadataClasses::XATTRS,
    };

    for (name, size) in [("small.txt", 64usize), ("large.bin", 1024 * 1024)] {
        let path = dir.join(name);
        let written = volume
            .write_from_stream(
                &path,
                WriteMode::CreateNew,
                StreamLength::Known(size as u64),
                Box::new(DatedSource::new(vec![b'x'; size]).carrying(carried.clone())),
                &|_| std::ops::ControlFlow::Continue(()),
            )
            .await
            .unwrap_or_else(|e| panic!("the write onto {} must land, got {e:?}", path.display()));
        assert!(
            !written.refused().contains(MetadataClasses::XATTRS),
            "a copy onto {} must keep its xattrs; the write reports them refused",
            path.display()
        );
        assert_reads_back(volume, &path, &expected).await;

        if caps.reports.contains(MetadataClasses::XATTRS) {
            let stream = volume
                .open_read_stream_for_copy(&path, Some(size as u64), values)
                .await
                .unwrap_or_else(|e| panic!("{} must be readable, got {e:?}", path.display()));
            assert_eq!(
                sorted(stream.metadata(MetadataClasses::XATTRS).xattrs),
                expected,
                "a copy off {} must report the xattrs it carries",
                path.display()
            );
            let stream = volume
                .open_read_stream_for_copy(&path, Some(size as u64), outline)
                .await
                .unwrap_or_else(|e| panic!("{} must be readable, got {e:?}", path.display()));
            assert!(
                stream
                    .metadata(MetadataClasses::XATTRS)
                    .xattrs
                    .iter()
                    .all(|xattr| !xattr.value.is_empty()),
                "a value the read didn't fetch must never reach `metadata` from {}",
                path.display()
            );
            let mut names: Vec<_> = stream
                .metadata_outline(MetadataClasses::XATTRS)
                .xattrs
                .into_iter()
                .map(|xattr| xattr.name)
                .collect();
            names.sort();
            let expected_names: Vec<_> = expected.iter().map(|xattr| xattr.name.clone()).collect();
            assert_eq!(
                names,
                expected_names,
                "the outline of {} names its xattrs",
                path.display()
            );
        }
    }

    let plain = dir.join("plain.txt");
    volume
        .write_from_stream(
            &plain,
            WriteMode::CreateNew,
            StreamLength::Known(5),
            Box::new(DatedSource::new(b"plain".to_vec())),
            &|_| std::ops::ControlFlow::Continue(()),
        )
        .await
        .unwrap_or_else(|e| panic!("the write onto {} must land, got {e:?}", plain.display()));
    if caps.reports.contains(MetadataClasses::XATTRS) {
        let stream = volume
            .open_read_stream_for_copy(&plain, Some(5), values)
            .await
            .unwrap_or_else(|e| panic!("{} must be readable, got {e:?}", plain.display()));
        assert!(
            stream.metadata(MetadataClasses::XATTRS).xattrs.is_empty(),
            "a file with no xattrs reports none"
        );
    }

    if caps.folder_stores.contains(MetadataClasses::XATTRS) {
        let folder = dir.join("folder");
        volume
            .create_directory(&folder)
            .await
            .unwrap_or_else(|e| panic!("{} must be creatable, got {e:?}", folder.display()));
        let report = volume.apply_metadata(&folder, &carried, None).await;
        assert_eq!(report.xattrs, ClassOutcome::Kept, "the folder's xattrs; got {report:?}");
        assert_reads_back(volume, &folder, &expected).await;
        if caps.reports.contains(MetadataClasses::XATTRS) {
            assert_eq!(
                sorted(volume.folder_metadata(&folder, MetadataClasses::XATTRS).await.xattrs),
                expected,
                "a copy off {} must report the folder's xattrs",
                folder.display()
            );
        }
    }
}
