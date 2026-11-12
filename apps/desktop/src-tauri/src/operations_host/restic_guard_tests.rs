//! Every write entry point refuses a path inside a restic repo's
//! `Browse snapshots` view, through the app's own registry, and leaves the
//! drive untouched; a copy OUT is routed, not refused.
//!
//! The engine's half (any routed path refuses) is pinned on fakes in
//! `crates/cmdr-operations/src/routed_writes_tests.rs`. What only the app can
//! pin is that the restic route CONFIRMS: the gate is lexical, `resolve` asks
//! the hub, and a real folder that merely shares the row's name stays writable.
//!
//! Each cell's drive holds a repo-shaped folder AND a real file at a path in
//! the view, which is the file the drive underneath would touch if a guard let
//! the write through.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use crate::file_system::volume::manager::get_volume_manager;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{InMemoryVolume, ListingProgress, RoutedKind, Volume, VolumeError};
use cmdr_operation_log::types::Initiator;
use cmdr_operations::testing::CollectorEventSink;
use cmdr_operations::{
    BulkRenameRow, MutationError, ReadOnlySide, RenameStartError, SourceFingerprint, VolumeCopyConfig,
    WriteOperationConfig, WriteOperationError,
};

const REPO: &str = "/share/backups/test-restic";
const IN_VIEW: &str = "/share/backups/test-restic/Browse snapshots/latest";
const IN_VIEW_FILE: &str = "/share/backups/test-restic/Browse snapshots/latest/a.txt";
const LOOKALIKE: &str = "/share/photos/Browse snapshots";

/// A drive under a fresh id in the app's registry, holding a repo at [`REPO`]
/// with a real file at [`IN_VIEW_FILE`], a real non-repo folder at
/// [`LOOKALIKE`] holding `x.txt`, and `/share/top.txt`.
async fn drive(tag: &str) -> (String, Arc<InMemoryVolume>) {
    let drive = Arc::new(InMemoryVolume::new("Share"));
    drive
        .create_file(&Path::new(REPO).join("config"), b"encrypted")
        .await
        .expect("config");
    for dir in ["data", "index", "keys", "snapshots"] {
        drive
            .create_directory(&Path::new(REPO).join(dir))
            .await
            .expect("repo folder");
    }
    drive
        .create_file(Path::new(IN_VIEW_FILE), b"a")
        .await
        .expect("a file the drive would touch");
    drive
        .create_file(&Path::new(LOOKALIKE).join("x.txt"), b"x")
        .await
        .expect("a real folder named like the row");
    drive.create_file(Path::new("/share/top.txt"), b"t").await.expect("top");
    let id = cmdr_fs::testing::unique_test_id(tag);
    get_volume_manager().register(&id, Arc::clone(&drive) as Arc<dyn Volume>);
    (id, drive)
}

fn refused_as(result: &Result<impl std::fmt::Debug, WriteOperationError>, side: ReadOnlySide) -> bool {
    matches!(result, Err(WriteOperationError::ReadOnlyDevice { side: s, .. }) if *s == side)
}

fn refused_read_only<T>(result: &Result<T, MutationError>) -> bool {
    matches!(
        result,
        Err(MutationError::Volume {
            error: VolumeError::ReadOnly(_)
        })
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_in_the_view_refuses_and_one_in_a_lookalike_folder_runs() {
    let engine = super::test_engine();
    let (id, drive) = drive("restic-guard-delete").await;

    let refused = engine
        .delete_files_start(
            Arc::new(CollectorEventSink::new()),
            vec![PathBuf::from(IN_VIEW_FILE)],
            WriteOperationConfig::default(),
            Some(id.clone()),
            Initiator::User,
            None,
        )
        .await;
    assert!(refused_as(&refused, ReadOnlySide::Source), "{refused:?}");
    assert!(drive.exists(Path::new(IN_VIEW_FILE)).await);

    let allowed = engine
        .delete_files_start(
            Arc::new(CollectorEventSink::new()),
            vec![Path::new(LOOKALIKE).join("x.txt")],
            WriteOperationConfig::default(),
            Some(id),
            Initiator::User,
            None,
        )
        .await;
    assert!(allowed.is_ok(), "a real folder of that name is ordinary: {allowed:?}");
}

/// The row selected beside the repo's real files refuses the whole delete,
/// before anything goes: the same answer as a git category row in a
/// multi-selection (`first_routed` asks every source).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_that_selects_the_row_refuses_whole() {
    let engine = super::test_engine();
    let (id, drive) = drive("restic-guard-delete-row").await;
    let row = Path::new(REPO).join("Browse snapshots");

    let refused = engine
        .delete_files_start(
            Arc::new(CollectorEventSink::new()),
            vec![Path::new(REPO).join("config"), row],
            WriteOperationConfig::default(),
            Some(id),
            Initiator::User,
            None,
        )
        .await;
    assert!(refused_as(&refused, ReadOnlySide::Source), "{refused:?}");
    assert!(drive.exists(&Path::new(REPO).join("config")).await, "nothing deleted");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn renames_and_new_entries_in_the_view_refuse() {
    let engine = super::test_engine();
    let (id, drive) = drive("restic-guard-rename").await;

    let renamed = engine
        .rename_managed(
            PathBuf::from(IN_VIEW_FILE),
            Path::new(IN_VIEW).join("b.txt"),
            false,
            id.clone(),
            Initiator::User,
        )
        .await;
    assert!(refused_read_only(&renamed), "{renamed:?}");

    let folder = engine
        .create_directory_managed(
            Some(id.clone()),
            IN_VIEW.to_string(),
            "new".to_string(),
            Initiator::User,
        )
        .await;
    assert!(refused_read_only(&folder), "{folder:?}");

    let file = engine
        .create_file_managed(
            Some(id.clone()),
            IN_VIEW.to_string(),
            "new.txt".to_string(),
            Initiator::User,
        )
        .await;
    assert!(refused_read_only(&file), "{file:?}");

    #[cfg(target_os = "macos")]
    {
        let numbered = engine
            .create_numbered_file(
                Some(id.clone()),
                Path::new(IN_VIEW),
                "pasted",
                "txt",
                b"x",
                Initiator::User,
            )
            .await;
        assert!(refused_read_only(&numbered), "{numbered:?}");
    }

    assert!(drive.exists(Path::new(IN_VIEW_FILE)).await);
    assert!(!drive.exists(&Path::new(IN_VIEW).join("new")).await);
    assert!(!drive.exists(&Path::new(IN_VIEW).join("new.txt")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bulk_rename_and_a_rename_as_move_in_the_view_refuse() {
    let engine = super::test_engine();
    let (id, drive) = drive("restic-guard-bulk").await;
    let row = BulkRenameRow {
        row_id: "1".to_string(),
        source: PathBuf::from(IN_VIEW_FILE),
        destination: Path::new(IN_VIEW).join("b.txt"),
        expected_fingerprint: SourceFingerprint::capture_remote(drive.as_ref(), Path::new(IN_VIEW_FILE))
            .await
            .expect("a fingerprint"),
    };

    let bulk = engine
        .start_renames(
            Arc::new(CollectorEventSink::new()),
            id.clone(),
            vec![row],
            Initiator::Agent,
        )
        .await;
    assert!(
        matches!(
            &bulk,
            Err(RenameStartError::Engine {
                error: WriteOperationError::ReadOnlyDevice {
                    side: ReadOnlySide::Source,
                    ..
                }
            })
        ),
        "{bulk:?}"
    );

    let out = engine
        .start_rename_by_move(
            Arc::new(CollectorEventSink::new()),
            id.clone(),
            vec![(PathBuf::from(IN_VIEW_FILE), "b.txt".to_string())],
            "/share".to_string(),
            VolumeCopyConfig::default(),
            Initiator::User,
            None,
        )
        .await;
    assert!(refused_as(&out, ReadOnlySide::Source), "{out:?}");

    let into = engine
        .start_rename_by_move(
            Arc::new(CollectorEventSink::new()),
            id,
            vec![(PathBuf::from("/share/top.txt"), "top.txt".to_string())],
            IN_VIEW.to_string(),
            VolumeCopyConfig::default(),
            Initiator::User,
            None,
        )
        .await;
    assert!(refused_as(&into, ReadOnlySide::Destination), "{into:?}");

    assert!(drive.exists(Path::new(IN_VIEW_FILE)).await);
    assert!(drive.exists(Path::new("/share/top.txt")).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_or_move_into_the_view_and_a_move_out_refuse() {
    let engine = super::test_engine();
    let (id, drive) = drive("restic-guard-transfer").await;

    let copy_in = engine
        .start_volume_copy(
            Arc::new(CollectorEventSink::new()),
            id.clone(),
            vec![PathBuf::from("/share/top.txt")],
            id.clone(),
            IN_VIEW.to_string(),
            VolumeCopyConfig::default(),
            Initiator::User,
            None,
        )
        .await;
    assert!(refused_as(&copy_in, ReadOnlySide::Destination), "{copy_in:?}");

    let move_in = engine
        .start_volume_move(
            Arc::new(CollectorEventSink::new()),
            id.clone(),
            vec![PathBuf::from("/share/top.txt")],
            id.clone(),
            IN_VIEW.to_string(),
            VolumeCopyConfig::default(),
            Initiator::User,
            None,
        )
        .await;
    assert!(refused_as(&move_in, ReadOnlySide::Destination), "{move_in:?}");

    let move_out = engine
        .start_volume_move(
            Arc::new(CollectorEventSink::new()),
            id.clone(),
            vec![PathBuf::from(IN_VIEW_FILE)],
            id,
            "/share".to_string(),
            VolumeCopyConfig::default(),
            Initiator::User,
            None,
        )
        .await;
    assert!(refused_as(&move_out, ReadOnlySide::Source), "{move_out:?}");

    assert!(drive.exists(Path::new("/share/top.txt")).await);
    assert!(drive.exists(Path::new(IN_VIEW_FILE)).await);
    assert!(
        !drive.exists(Path::new("/share/a.txt")).await,
        "nothing was copied out by the move"
    );
}

/// ❗ A hand-picked selection (or an agent's) of the `Browse snapshots` view
/// beside the repo's real files can't run as one transfer: it reads through one
/// volume, so the first item would decide and every other one would fail (or a
/// move would end half-done). Refused whole, before anything is read, naming the
/// first item served by another volume, whichever comes first.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_transfer_whose_sources_span_the_drive_and_the_view_refuses_whole() {
    let engine = super::test_engine();
    let (dest_id, dest) = drive("restic-guard-span-dest").await;
    let (id, drive) = drive("restic-guard-span").await;
    let real = Path::new(REPO).join("config");
    let in_view = PathBuf::from(IN_VIEW_FILE);

    for (sources, odd_one) in [
        (vec![real.clone(), in_view.clone()], &in_view),
        (vec![in_view.clone(), real.clone()], &real),
    ] {
        let copy = engine
            .start_volume_copy(
                Arc::new(CollectorEventSink::new()),
                id.clone(),
                sources.clone(),
                dest_id.clone(),
                "/share/out".to_string(),
                VolumeCopyConfig::default(),
                Initiator::User,
                None,
            )
            .await;
        assert!(
            matches!(&copy, Err(WriteOperationError::SourcesSpanVolumes { path }) if Path::new(path) == odd_one.as_path()),
            "{copy:?}"
        );
        let moved = engine
            .start_volume_move(
                Arc::new(CollectorEventSink::new()),
                id.clone(),
                sources,
                dest_id.clone(),
                "/share/out".to_string(),
                VolumeCopyConfig::default(),
                Initiator::User,
                None,
            )
            .await;
        assert!(
            matches!(&moved, Err(WriteOperationError::SourcesSpanVolumes { .. })),
            "{moved:?}"
        );
    }

    assert!(drive.exists(&real).await, "nothing moved");
    assert!(!dest.exists(Path::new("/share/out/config")).await, "nothing copied");
}

/// A copy OUT reads through the view: the source resolves to the restic route,
/// the volume a copy reads with `open_read_stream`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_out_resolves_its_source_to_the_view() {
    let engine = super::test_engine();
    let (id, _drive) = drive("restic-guard-copy-out").await;

    let (volume, route) = engine
        .resolve_source_volume(&id, Some(&PathBuf::from(IN_VIEW_FILE)))
        .await
        .expect("a source volume");
    assert_eq!(route, Some(RoutedKind::Restic));
    assert_eq!(volume.root(), Path::new(REPO).join("Browse snapshots"));

    let (parent, real) = engine
        .resolve_source_volume(&id, Some(&Path::new(REPO).join("config")))
        .await
        .expect("a source volume");
    assert_eq!(real, None, "the repo's real files are the drive's");
    assert_eq!(parent.name(), "Share");
}

/// A drive whose listing of the repo folder fails (a NAS that won't answer),
/// so the restic route can't decide whether `REPO` is a repo. Everything else
/// is the in-memory drive's, writes included.
struct UndecidedDrive {
    inner: Arc<InMemoryVolume>,
}

impl Volume for UndecidedDrive {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn is_writable(&self) -> bool {
        true
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        if path == Path::new(REPO) {
            return Box::pin(async {
                Err(VolumeError::IoError {
                    message: "the NAS didn't answer".into(),
                    raw_os_error: None,
                })
            });
        }
        self.inner.list_directory(path, on_progress)
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }

    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.create_directory(path)
    }
}

/// ❗ When the route can't tell whether the folder holds a repo (its listing
/// failed or outran the deadline), a write under `Browse snapshots` refuses:
/// letting it through would put a real `Browse snapshots/` folder inside what may
/// be a repo, which the view then hides forever. A read passes through to the
/// drive.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_write_refuses_when_the_route_cant_decide() {
    let engine = super::test_engine();
    let (_, inner) = drive("restic-guard-undecided-backing").await;
    let id = cmdr_fs::testing::unique_test_id("restic-guard-undecided");
    get_volume_manager().register(
        &id,
        Arc::new(UndecidedDrive {
            inner: Arc::clone(&inner),
        }) as Arc<dyn Volume>,
    );

    let resolved = get_volume_manager().resolve(&id, Path::new(IN_VIEW)).await;
    assert_eq!(resolved.routed, None, "a read goes to the drive");
    assert_eq!(resolved.unconfirmed, Some(RoutedKind::Restic));

    let folder = engine
        .create_directory_managed(Some(id), IN_VIEW.to_string(), "new".to_string(), Initiator::User)
        .await;
    assert!(refused_read_only(&folder), "{folder:?}");
    assert!(!inner.exists(&Path::new(IN_VIEW).join("new")).await);
}
