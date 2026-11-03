//! The loss note, through the engine: every pair of real backends' metadata
//! caps either keeps a class, reports it lost on the completion event, or
//! never had it to lose. None is a silent drop.
//!
//! Both ends are `InMemoryVolume` doubles configured to a real backend's caps
//! (`with_metadata_caps`), so the expectations are COMPUTED from the caps,
//! never hand-written, and the whole matrix runs at in-memory speed. Two real
//! cells pin the ends the doubles stand in for: a local source (its stream's
//! name-only xattr outline) onto SFTP's caps here, and a real SFTP server in
//! `backend_suites`. The contract: `DETAILS.md` § "Copies keep the source's
//! metadata", invariant 7.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use cmdr_fs::metadata::{CarriedMetadata, MetadataCaps, MetadataClasses, UserFlags, Xattr};

use super::copy_volumes_with_progress;
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use crate::types::{LostMetadata, LostMetadataClass, MetadataLeftBehind, VolumeCopyConfig, WriteCompleteEvent};
use cmdr_fs::testing::TestDir;
use cmdr_fs::volume::{InMemoryVolume, Volume};
use cmdr_local::LocalPosixVolume;

/// 2015-03-01: every seeded item's creation date, years before its last edit.
const BORN: u64 = 1_425_168_000;
/// 2021-01-29: every seeded item's modification date.
const MODIFIED: u64 = 1_611_909_015;
/// A mode no umask gives a fresh file.
const EXECUTABLE: u32 = 0o755;

/// Every backend as a SOURCE. MTP and WebDAV report nothing beyond mtime.
fn sources() -> Vec<(&'static str, MetadataCaps)> {
    vec![
        ("local", LocalPosixVolume::METADATA_CAPS),
        ("SMB without named streams", cmdr_smb::volume::SmbVolume::METADATA_CAPS),
        ("SMB with named streams", cmdr_smb::volume::SmbVolume::NAMED_STREAM_CAPS),
        ("SFTP", cmdr_sftp::SftpVolume::METADATA_CAPS),
        ("ADB", cmdr_adb::AdbVolume::METADATA_CAPS),
        ("S3", cmdr_s3::S3Volume::METADATA_CAPS),
        ("archive", archive_caps()),
        // The git portal answers this shape inline (pinned in its own crate's
        // volume tests).
        ("git", MetadataCaps::MODE_REPORTER),
        ("MTP / WebDAV", MetadataCaps::NONE),
    ]
}

/// Every backend a copy can write to (git and archives are read-only here).
fn destinations() -> Vec<(&'static str, MetadataCaps)> {
    vec![
        ("local", LocalPosixVolume::METADATA_CAPS),
        ("SMB without named streams", cmdr_smb::volume::SmbVolume::METADATA_CAPS),
        ("SMB with named streams", cmdr_smb::volume::SmbVolume::NAMED_STREAM_CAPS),
        ("SFTP", cmdr_sftp::SftpVolume::METADATA_CAPS),
        ("ADB", cmdr_adb::AdbVolume::METADATA_CAPS),
        ("S3", cmdr_s3::S3Volume::METADATA_CAPS),
        (
            "WebDAV keeping dates (Nextcloud)",
            cmdr_webdav::WebdavVolume::DATE_KEEPING_CAPS,
        ),
        ("MTP / WebDAV", MetadataCaps::NONE),
    ]
}

/// What a live `ArchiveVolume` claims: it answers its caps inline, with no
/// public const (`cmdr-archive` sits at its public-surface ceiling).
fn archive_caps() -> MetadataCaps {
    let parent: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Parent"));
    cmdr_archive::ArchiveVolume::new(
        parent,
        "/a.zip".into(),
        cmdr_archive::ArchiveFormat::Zip,
        cmdr_fs::volume::host::VolumeHost::detached(),
    )
    .metadata_caps()
}

fn at(secs: u64) -> std::time::SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

fn tag() -> Xattr {
    Xattr {
        name: "com.apple.metadata:_kMDItemUserTags".into(),
        value: b"red".to_vec(),
    }
}

/// `/file.bin`, carrying something worth keeping in every class, and `/album`,
/// an empty folder born years before its last edit (when the source can say
/// so: a backend that doesn't report creation dates lists none).
async fn seed(source: &InMemoryVolume, caps: MetadataCaps) {
    let file = Path::new("/file.bin");
    source.create_file(file, b"bytes").await.unwrap();
    source.set_modified_at(file, Some(MODIFIED));
    source.set_carried_metadata(
        file,
        CarriedMetadata {
            created: Some(at(BORN)),
            mode: Some(EXECUTABLE),
            flags: UserFlags::HIDDEN,
            xattrs: vec![tag()],
        },
    );
    let album = Path::new("/album");
    source.create_directory(album).await.unwrap();
    source.set_modified_at(album, Some(MODIFIED));
    // What the source can report about the folder beyond its date: its
    // creation date off the listing, its tags through `folder_metadata`.
    source.set_carried_metadata(
        album,
        CarriedMetadata {
            created: caps.reports.contains(MetadataClasses::CREATED).then(|| at(BORN)),
            xattrs: if caps.reports.contains(MetadataClasses::XATTRS) {
                vec![tag()]
            } else {
                Vec::new()
            },
            ..CarriedMetadata::default()
        },
    );
}

async fn copy(
    engine: &TestOperations,
    source: Arc<dyn Volume>,
    paths: &[PathBuf],
    dest: Arc<dyn Volume>,
    dest_dir: &Path,
) -> WriteCompleteEvent {
    let events = Arc::new(CollectorEventSink::new());
    copy_volumes_with_progress(
        Arc::clone(&events) as _,
        "op-metadata-loss",
        &Arc::new(engine.new_operation_state(Duration::from_millis(50))),
        source,
        paths,
        dest,
        dest_dir,
        &VolumeCopyConfig::default(),
    )
    .await
    .unwrap_or_else(|failure| panic!("the copy succeeds, got {:?}", failure.error));
    let complete = events.complete.lock().expect("the collector's lock").clone();
    complete.into_iter().next().expect("one completion event")
}

/// The engine classes a FILE carries, with the person-facing class each one
/// reports as.
const FILE_CLASSES: [(MetadataClasses, LostMetadataClass); 4] = [
    (MetadataClasses::CREATED, LostMetadataClass::CreationDates),
    (MetadataClasses::MODE, LostMetadataClass::Permissions),
    (MetadataClasses::FLAGS, LostMetadataClass::LockedAndHidden),
    (MetadataClasses::XATTRS, LostMetadataClass::ExtendedAttributes),
];

/// What the caps say the completion must report: per class, the file if its
/// source reports the class and the destination can't store it, plus the folder
/// for its creation date, its tags, and its modification date by the same rule.
fn expected_losses(source: MetadataCaps, dest: MetadataCaps) -> Option<MetadataLeftBehind> {
    let mut counts = std::collections::BTreeMap::<u8, (LostMetadataClass, u32)>::new();
    let mut bump = |order: u8, class: LostMetadataClass| counts.entry(order).or_insert((class, 0)).1 += 1;
    for (order, (engine, class)) in (0u8..).zip(FILE_CLASSES) {
        if source.reports.contains(engine) && !dest.stores.contains(engine) {
            bump(order, class);
        }
    }
    if source.reports.contains(MetadataClasses::CREATED) && !dest.folder_stores.contains(MetadataClasses::CREATED) {
        bump(0, LostMetadataClass::CreationDates);
    }
    if source.reports.contains(MetadataClasses::XATTRS) && !dest.folder_stores.contains(MetadataClasses::XATTRS) {
        bump(3, LostMetadataClass::ExtendedAttributes);
    }
    if !dest.folder_dates {
        bump(4, LostMetadataClass::FolderDates);
    }
    let classes: Vec<LostMetadata> = counts
        .into_values()
        .map(|(class, item_count)| LostMetadata { class, item_count })
        .collect();
    (!classes.is_empty()).then_some(MetadataLeftBehind { classes })
}

/// The matrix: for every (source, destination) pair, each class the source
/// reports either lands on the destination or is on the note, and nothing
/// else is.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_pair_keeps_or_reports_each_class_its_source_had() {
    let engine = TestOperations::new();
    for (source_name, source_caps) in sources() {
        for (dest_name, dest_caps) in destinations() {
            let pair = format!("{source_name} → {dest_name}");
            let source = Arc::new(InMemoryVolume::new("Source").with_metadata_caps(source_caps));
            seed(&source, source_caps).await;
            let dest = Arc::new(InMemoryVolume::new("Dest").with_metadata_caps(dest_caps));
            dest.create_directory(Path::new("/to")).await.unwrap();

            let complete = copy(
                &engine,
                source,
                &[PathBuf::from("/file.bin"), PathBuf::from("/album")],
                Arc::clone(&dest) as _,
                Path::new("/to"),
            )
            .await;

            assert_eq!(
                complete.metadata_left_behind,
                expected_losses(source_caps, dest_caps),
                "{pair}: the note"
            );
            let landed = dest
                .carried_metadata(Path::new("/to/file.bin"))
                .expect("the file landed");
            for (engine, _) in FILE_CLASSES {
                if !(source_caps.reports.contains(engine) && dest_caps.stores.contains(engine)) {
                    continue;
                }
                let kept = match engine {
                    MetadataClasses::CREATED => landed.created == Some(at(BORN)),
                    MetadataClasses::MODE => landed.mode == Some(EXECUTABLE),
                    MetadataClasses::FLAGS => landed.flags == UserFlags::HIDDEN,
                    _ => landed.xattrs.contains(&tag()),
                };
                assert!(kept, "{pair}: {engine:?} is kept, so it must land; got {landed:?}");
            }
        }
    }
}

/// Ordinary files report nothing, wherever they go: a plain file has nothing
/// worth keeping beyond its bytes and its mtime.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_plain_file_reports_nothing_anywhere() {
    let engine = TestOperations::new();
    for (dest_name, dest_caps) in destinations() {
        let source = Arc::new(InMemoryVolume::new("Source"));
        let plain = Path::new("/plain.txt");
        source.create_file(plain, b"plain").await.unwrap();
        source.set_modified_at(plain, Some(MODIFIED));
        source.set_carried_metadata(
            plain,
            CarriedMetadata {
                created: Some(at(MODIFIED)),
                mode: Some(cmdr_fs::metadata::default_file_mode()),
                ..CarriedMetadata::default()
            },
        );
        let dest = Arc::new(InMemoryVolume::new("Dest").with_metadata_caps(dest_caps));
        dest.create_directory(Path::new("/to")).await.unwrap();

        let complete = copy(&engine, source, &[plain.to_path_buf()], dest, Path::new("/to")).await;

        assert_eq!(complete.metadata_left_behind, None, "a plain file onto {dest_name}");
    }
}

/// A local file through its real stream (the name-only xattr outline) onto
/// SFTP's caps: a tagged file puts the tags on the note, an untagged one puts
/// nothing there.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tagged_local_file_copied_to_sftp_reports_its_tags_and_an_untagged_one_nothing() {
    let engine = TestOperations::new();
    let dir = TestDir::new("metadata_loss_local_to_sftp");
    let tagged = dir.join("tagged.txt");
    let plain = dir.join("plain.txt");
    for path in [&tagged, &plain] {
        let file = std::fs::File::create(path).unwrap();
        // An mtime before its birth drags APFS's birth down to it, so neither
        // file has a creation date worth reporting: the tags are the only
        // difference between them.
        file.set_modified(at(MODIFIED)).unwrap();
    }
    xattr::set(&tagged, "user.cmdr.tag", b"red").unwrap();
    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "Local",
        &*dir,
        crate::host::testing::pane_order_host(),
    ));
    let sftp = || Arc::new(InMemoryVolume::new("SFTP").with_metadata_caps(cmdr_sftp::SftpVolume::METADATA_CAPS));

    let tagged_copy = copy(
        &engine,
        Arc::clone(&source),
        &[PathBuf::from("tagged.txt")],
        sftp(),
        Path::new("/"),
    )
    .await;
    let plain_copy = copy(&engine, source, &[PathBuf::from("plain.txt")], sftp(), Path::new("/")).await;

    assert_eq!(
        tagged_copy.metadata_left_behind,
        Some(MetadataLeftBehind {
            classes: vec![LostMetadata {
                class: LostMetadataClass::ExtendedAttributes,
                item_count: 1
            }]
        })
    );
    assert_eq!(plain_copy.metadata_left_behind, None);
}

/// A folder whose only xattr is the quarantine had nothing a person would miss,
/// so a destination that keeps no folder xattrs reports nothing for it
/// (invariant 10's "worth keeping").
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_with_only_a_quarantine_reports_nothing() {
    let engine = TestOperations::new();
    let source = Arc::new(InMemoryVolume::new("Source"));
    let folder = Path::new("/downloaded");
    source.create_directory(folder).await.unwrap();
    source.set_carried_metadata(
        folder,
        CarriedMetadata {
            xattrs: vec![Xattr {
                name: "com.apple.quarantine".into(),
                value: b"0083;".to_vec(),
            }],
            ..CarriedMetadata::default()
        },
    );
    let dest = Arc::new(InMemoryVolume::new("Dest").with_metadata_caps(cmdr_sftp::SftpVolume::METADATA_CAPS));
    dest.create_directory(Path::new("/to")).await.unwrap();

    let complete = copy(&engine, source, &[folder.to_path_buf()], dest, Path::new("/to")).await;

    assert_eq!(complete.metadata_left_behind, None);
}

/// A local FOLDER through `LocalPosixVolume::folder_metadata` onto SFTP's caps:
/// a tagged one puts its tags on the note, an untagged one puts nothing there.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tagged_local_folder_copied_to_sftp_reports_its_tags_and_an_untagged_one_nothing() {
    let engine = TestOperations::new();
    let dir = TestDir::new("metadata_loss_local_folder_to_sftp");
    for name in ["tagged", "plain"] {
        std::fs::create_dir(dir.join(name)).unwrap();
    }
    xattr::set(dir.join("tagged"), "user.cmdr.tag", b"red").unwrap();
    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "Local",
        &*dir,
        crate::host::testing::pane_order_host(),
    ));
    let sftp = || Arc::new(InMemoryVolume::new("SFTP").with_metadata_caps(cmdr_sftp::SftpVolume::METADATA_CAPS));

    let tagged_copy = copy(
        &engine,
        Arc::clone(&source),
        &[PathBuf::from("tagged")],
        sftp(),
        Path::new("/"),
    )
    .await;
    let plain_copy = copy(&engine, source, &[PathBuf::from("plain")], sftp(), Path::new("/")).await;

    assert_eq!(
        tagged_copy.metadata_left_behind,
        Some(MetadataLeftBehind {
            classes: vec![LostMetadata {
                class: LostMetadataClass::ExtendedAttributes,
                item_count: 1
            }]
        })
    );
    assert_eq!(plain_copy.metadata_left_behind, None);
}

/// A destination that claims a class and refuses it at runtime (a NAS
/// rejecting a creation date) puts that class on the note: its caps promised
/// it, so only the write's own report can tell.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_class_the_destination_refuses_at_runtime_is_on_the_note() {
    let engine = TestOperations::new();
    let source = Arc::new(InMemoryVolume::new("Source"));
    seed(&source, InMemoryVolume::REFERENCE_CAPS).await;
    // The double's own caps keep every class on every OS, so the runtime refusal
    // is the only loss (Linux's local caps would add the flags to the note).
    let dest = Arc::new(InMemoryVolume::new("NAS").with_write_refusing(MetadataClasses::CREATED));
    dest.create_directory(Path::new("/to")).await.unwrap();

    let complete = copy(&engine, source, &[PathBuf::from("/file.bin")], dest, Path::new("/to")).await;

    assert_eq!(
        complete.metadata_left_behind,
        Some(MetadataLeftBehind {
            classes: vec![LostMetadata {
                class: LostMetadataClass::CreationDates,
                item_count: 1
            }]
        })
    );
}

/// A refusal counts only what the source had worth keeping: a creation date
/// equal to the last edit is refused without a note.
/// Refusing only the date keeps the test off the process umask, which decides
/// whether the double's default file mode counts as worth keeping.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_runtime_refusal_of_nothing_worth_keeping_is_not_on_the_note() {
    let engine = TestOperations::new();
    let source = Arc::new(InMemoryVolume::new("Source"));
    let plain = Path::new("/plain.txt");
    source.create_file(plain, b"plain").await.unwrap();
    source.set_modified_at(plain, Some(MODIFIED));
    source.set_carried_metadata(
        plain,
        CarriedMetadata {
            created: Some(at(MODIFIED)),
            ..CarriedMetadata::default()
        },
    );
    let dest = Arc::new(InMemoryVolume::new("NAS").with_write_refusing(MetadataClasses::CREATED));
    dest.create_directory(Path::new("/to")).await.unwrap();

    let complete = copy(&engine, source, &[plain.to_path_buf()], dest, Path::new("/to")).await;

    assert_eq!(complete.metadata_left_behind, None);
}

/// Local disk onto local disk through the volume engine keeps every class, so
/// its write's report refuses nothing and the note stays silent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_local_to_local_volume_copy_reports_nothing() {
    let engine = TestOperations::new();
    let dir = TestDir::new("metadata_loss_local_to_local");
    let from = dir.join("from");
    let to = dir.join("to");
    std::fs::create_dir_all(&from).unwrap();
    std::fs::create_dir_all(&to).unwrap();
    let file = from.join("tagged.sh");
    std::fs::write(&file, b"#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&file, std::os::unix::fs::PermissionsExt::from_mode(EXECUTABLE)).unwrap();
    xattr::set(&file, "user.cmdr.tag", b"red").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(at(MODIFIED))
        .unwrap();
    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "From",
        &*from,
        crate::host::testing::pane_order_host(),
    ));
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "To",
        &*to,
        crate::host::testing::pane_order_host(),
    ));

    let complete = copy(&engine, source, &[PathBuf::from("tagged.sh")], dest, Path::new("")).await;

    assert_eq!(complete.metadata_left_behind, None);
    assert_eq!(
        xattr::get(to.join("tagged.sh"), "user.cmdr.tag").unwrap(),
        Some(b"red".to_vec())
    );
}
