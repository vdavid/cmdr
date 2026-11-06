//! Planning a fresh compress: aliases, counting, duplicate names, the sidecar
//! folder, and tree order.

use std::future::Future;

use super::*;
use crate::testing::TestOperations;
use crate::transfer::volume::forward_volume_methods;
use cmdr_fs::volume::InMemoryVolume;
use cmdr_local::LocalPosixVolume;

#[tokio::test]
async fn distinct_handles_for_one_remote_resource_still_reject_containment() {
    let lane = "same-remote-resource";
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("source").with_lane_key(lane));
    let destination: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("destination").with_lane_key(lane));
    source
        .create_directory(Path::new("/folder"))
        .await
        .expect("create source folder");

    assert!(matches!(
        validate_aliases(
            &Handle::current(),
            &source,
            &[PathBuf::from("/folder")],
            &destination,
            Path::new("/folder/archive.zip"),
        )
        .await,
        Err(WriteOperationError::DestinationInsideSource { .. })
    ));
}

/// A remote-shaped source that counts its single-entry stats, each one a
/// network round trip on SMB and SFTP (and a whole parent listing on MTP).
struct CountingStats {
    inner: InMemoryVolume,
    stats: std::sync::atomic::AtomicUsize,
}

impl Volume for CountingStats {
    forward_volume_methods!(inner => name, root, list_directory, exists, is_directory, get_space_info);

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        self.stats.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.get_metadata(path)
    }
}

#[tokio::test]
async fn planning_a_remote_tree_stats_only_the_selected_items() {
    let engine = TestOperations::new();
    let inner = InMemoryVolume::new("remote");
    inner.create_directory(Path::new("/tree")).await.expect("mkdir");
    inner.create_directory(Path::new("/tree/sub")).await.expect("mkdir sub");
    for index in 0..20 {
        inner
            .create_file(&PathBuf::from(format!("/tree/sub/f{index}.txt")), b"x")
            .await
            .expect("seed file");
    }
    let source = Arc::new(CountingStats {
        inner,
        stats: Default::default(),
    });
    let as_volume: Arc<dyn Volume> = Arc::clone(&source) as Arc<dyn Volume>;
    let state = Arc::new(engine.new_operation_state(Duration::ZERO));

    let plan = plan_sources_with_context(
        &as_volume,
        &[PathBuf::from("/tree")],
        ConflictResolution::Stop,
        &state,
        None,
        None,
    )
    .await
    .unwrap_or_else(|_| panic!("plan"));

    assert_eq!(plan.entries.len(), 22, "two directories and 20 files");
    assert_eq!(plan.source_bytes, 20);
    // The selected item costs a kind check (`entry_kind`, here the trait
    // default over `get_metadata`) and a stat; its 21 descendants cost none.
    assert_eq!(
        source.stats.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "children come from their parent's listing, never a stat each"
    );
}

/// A remote whose `get_metadata` FOLLOWS links, the way SFTP's `stat` does:
/// `/link` answers as the folder it points at, and only `entry_kind` (an
/// `lstat`) says it's a link.
struct LinkFollowingStat {
    inner: InMemoryVolume,
}

impl LinkFollowingStat {
    fn target(path: &Path) -> PathBuf {
        match path.strip_prefix("/link") {
            Ok(rest) => Path::new("/target").join(rest),
            Err(_) => path.to_path_buf(),
        }
    }
}

impl Volume for LinkFollowingStat {
    forward_volume_methods!(inner => name, root, exists, is_directory, get_space_info);

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(cmdr_fs::volume::ListingProgress) + Sync)>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move { self.inner.list_directory(&Self::target(path), on_progress).await })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move { self.inner.get_metadata(&Self::target(path)).await })
    }

    fn entry_kind<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<EntryKind, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if path == Path::new("/link") {
                Ok(EntryKind::Symlink)
            } else {
                self.inner.entry_kind(path).await
            }
        })
    }
}

#[tokio::test]
async fn a_selected_symlink_is_skipped_even_where_stat_follows_links() {
    let engine = TestOperations::new();
    // A link inside a selected folder is skipped off its listing entry; a
    // selected link has to be skipped too, or SFTP (whose `stat` follows)
    // packs the target that the same link would never pack one level down.
    let inner = InMemoryVolume::new("remote");
    inner.create_directory(Path::new("/target")).await.expect("mkdir");
    inner
        .create_file(Path::new("/target/inside.txt"), b"x")
        .await
        .expect("seed");
    let source: Arc<dyn Volume> = Arc::new(LinkFollowingStat { inner });
    let state = Arc::new(engine.new_operation_state(Duration::ZERO));

    let plan = plan_sources_with_context(
        &source,
        &[PathBuf::from("/link")],
        ConflictResolution::Stop,
        &state,
        None,
        None,
    )
    .await
    .unwrap_or_else(|_| panic!("plan"));

    assert!(plan.entries.is_empty(), "the link's target is never packed");
    assert_eq!(plan.skipped, 1);
}

#[cfg(unix)]
#[tokio::test]
async fn planning_skips_local_symlinks_and_special_files_without_a_stat_each() {
    let engine = TestOperations::new();
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(temp.path().join("tree")).expect("mkdir");
    std::fs::write(temp.path().join("tree/real.txt"), b"real").expect("write");
    std::os::unix::fs::symlink("real.txt", temp.path().join("tree/link")).expect("symlink");
    let _socket = std::os::unix::net::UnixListener::bind(temp.path().join("tree/sock")).expect("bind a socket file");
    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "source",
        temp.path().to_path_buf(),
        crate::host::testing::pane_order_host(),
    ));
    let state = Arc::new(engine.new_operation_state(Duration::ZERO));

    let plan = plan_sources_with_context(
        &source,
        &[PathBuf::from("tree")],
        ConflictResolution::Stop,
        &state,
        None,
        None,
    )
    .await
    .unwrap_or_else(|_| panic!("plan"));

    assert_eq!(
        plan.entries.iter().map(|entry| entry.name.as_str()).collect::<Vec<_>>(),
        vec!["tree/", "tree/real.txt"]
    );
    assert_eq!(plan.skipped, 2, "the symlink and the socket are skipped, never lost");
}

#[tokio::test]
async fn duplicate_top_names_honor_rename_and_skip_policies() {
    let engine = TestOperations::new();
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(temp.path().join("a")).expect("create a");
    std::fs::create_dir_all(temp.path().join("b")).expect("create b");
    std::fs::write(temp.path().join("a/report.txt"), b"first").expect("write first");
    std::fs::write(temp.path().join("b/report.txt"), b"second").expect("write second");
    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "source",
        temp.path().to_path_buf(),
        crate::host::testing::pane_order_host(),
    ));
    let paths = [PathBuf::from("a/report.txt"), PathBuf::from("b/report.txt")];
    let state = Arc::new(engine.new_operation_state(Duration::ZERO));

    let renamed = plan_sources_with_context(&source, &paths, ConflictResolution::Rename, &state, None, None)
        .await
        .unwrap_or_else(|_| panic!("rename plan"));
    assert_eq!(
        renamed
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        // In tree order, by the names the zip holds: the space sorts first.
        vec!["report (1).txt", "report.txt"]
    );

    let skipped = plan_sources_with_context(&source, &paths, ConflictResolution::Skip, &state, None, None)
        .await
        .unwrap_or_else(|_| panic!("skip plan"));
    assert_eq!(skipped.entries.len(), 1);
    assert_eq!(skipped.skipped, 1);
}

/// A zip's root `__MACOSX` holds its sidecars and is hidden from every
/// listing, so a selected folder of that name is skipped and counted
/// instead of landing where it could never be seen (and failing the stage's
/// read-back). A local folder entry carries its path, for its sidecar.
#[tokio::test]
async fn a_selected_macosx_folder_is_skipped_and_a_local_folder_is_its_own_source() {
    let engine = TestOperations::new();
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(temp.path().join("__MACOSX")).expect("mkdir");
    std::fs::write(temp.path().join("__MACOSX/._a"), b"x").expect("write");
    std::fs::create_dir_all(temp.path().join("docs")).expect("mkdir");
    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "source",
        temp.path().to_path_buf(),
        crate::host::testing::pane_order_host(),
    ));
    let state = Arc::new(engine.new_operation_state(Duration::ZERO));

    let plan = plan_sources_with_context(
        &source,
        &[PathBuf::from("__MACOSX"), PathBuf::from("docs")],
        ConflictResolution::Stop,
        &state,
        None,
        None,
    )
    .await
    .unwrap_or_else(|_| panic!("plan"));

    assert_eq!(
        plan.entries.iter().map(|entry| entry.name.as_str()).collect::<Vec<_>>(),
        vec!["docs/"]
    );
    assert_eq!(plan.skipped, 1);
    assert!(matches!(&plan.entries[0].source, PlannedSource::Local(path) if path.ends_with("docs")));
}

/// A compress that leaves Mac-only files out (#314) packs none below the
/// selection, and bridges every local entry without its `__MACOSX` sidecar,
/// the same `._` clutter on another system. A folder's only use as a local
/// source is its sidecar, so it becomes an empty entry.
#[tokio::test]
async fn a_compress_skipping_mac_only_files_packs_neither_them_nor_sidecars() {
    use super::super::fresh_zip::FreshZipSource;
    use cmdr_fs::mac_only_files::MacOnlyFiles;

    let engine = TestOperations::new();
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(temp.path().join("docs/sub")).expect("mkdir");
    std::fs::write(temp.path().join("docs/.DS_Store"), b"finder").expect("write");
    std::fs::write(temp.path().join("docs/._a.txt"), b"xattrs").expect("write");
    std::fs::write(temp.path().join("docs/a.txt"), b"a").expect("write");
    std::fs::write(temp.path().join("docs/sub/.DS_Store"), b"finder").expect("write");
    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder(
        "source",
        temp.path().to_path_buf(),
        crate::host::testing::pane_order_host(),
    ));
    let state = Arc::new(
        engine
            .new_operation_state(Duration::ZERO)
            .leaving_out(MacOnlyFiles::Skip),
    );

    let plan = plan_sources_with_context(
        &source,
        &[PathBuf::from("docs")],
        ConflictResolution::Stop,
        &state,
        None,
        None,
    )
    .await
    .unwrap_or_else(|_| panic!("plan"));

    let mut names: Vec<&str> = plan.entries.iter().map(|entry| entry.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, vec!["docs/", "docs/a.txt", "docs/sub/"]);
    assert_eq!(plan.source_bytes, 1, "the totals leave them out too");
    assert!(!plan.sidecars);
    let (entries, _feeds) = plan.bridged();
    for entry in &entries {
        assert!(
            !matches!(entry.source, FreshZipSource::Local(_)),
            "{} would carry a sidecar",
            entry.name
        );
    }
}

/// A source whose listings come back backwards, the way an SMB or MTP
/// listing comes back in no particular order.
struct ReversedListing {
    inner: InMemoryVolume,
}

impl Volume for ReversedListing {
    forward_volume_methods!(inner => name, root, exists, is_directory, get_metadata, get_space_info);

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(cmdr_fs::volume::ListingProgress) + Sync)>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        use futures_util::TryFutureExt;
        Box::pin(
            self.inner
                .list_directory(path, on_progress)
                .map_ok(|entries| entries.into_iter().rev().collect()),
        )
    }
}

/// The plan is the order the zip is written in, so it's the order the
/// compress's tree rows start in: subfolders first, then files, by name,
/// depth first, and the selection the same way whatever order it came in.
#[tokio::test]
async fn the_plan_lists_the_selection_in_tree_order() {
    let engine = TestOperations::new();
    let inner = InMemoryVolume::new("remote");
    for dir in ["/src", "/src/a", "/src/a/sub", "/src/b"] {
        inner.create_directory(Path::new(dir)).await.expect("mkdir");
    }
    for file in [
        "/src/a/zzz.txt",
        "/src/a/sub/mmm.txt",
        "/src/b/aaa.txt",
        "/src/Y.txt",
        "/src/z.txt",
    ] {
        inner.create_file(Path::new(file), b"x").await.expect("seed");
    }
    let source: Arc<dyn Volume> = Arc::new(ReversedListing { inner });
    let state = Arc::new(engine.new_operation_state(Duration::ZERO));
    let selection = ["/src/z.txt", "/src/b", "/src/Y.txt", "/src/a"].map(PathBuf::from);

    let plan = plan_sources_with_context(&source, &selection, ConflictResolution::Stop, &state, None, None)
        .await
        .unwrap_or_else(|_| panic!("plan"));

    assert_eq!(
        plan.entries.iter().map(|entry| entry.name.as_str()).collect::<Vec<_>>(),
        [
            "a/",
            "a/sub/",
            "a/sub/mmm.txt",
            "a/zzz.txt",
            "b/",
            "b/aaa.txt",
            "Y.txt",
            "z.txt"
        ]
    );
}
