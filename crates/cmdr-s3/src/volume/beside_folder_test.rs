//! A file and a folder of one name (`photos` beside `photos/…`): the pane
//! lists both, the file as `photos (file)`, and every operation on each row
//! reaches that row's object and never the other's.
//!
//! On Garage only: VersityGW stores keys as POSIX files, so it refuses the
//! second of the two (the one VersityGW cell pins that), and the case never
//! arises there.

use std::future::Future;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use cmdr_fs::volume::{RenameWork, ServerCopyProgress, ShareLinkExpiry, Volume, VolumeError, WriteMode};

use super::S3Volume;
use super::testing::*;

const FIXTURE: &str = "s3-servers/start.sh (s3-fixture)";
const FILE_BYTES: &[u8] = b"the file";
const INSIDE_BYTES: &[u8] = b"inside the folder";

/// `photos` (a file) and `photos/inside.txt` under a fresh prefix, listed
/// once the way a pane would before acting on a row.
struct Seeded {
    volume: S3Volume,
    prefix: String,
}

impl Seeded {
    async fn new(service: FixtureService, label: &str) -> Self {
        let prefix = scratch_prefix(label);
        let file = format!("{prefix}photos");
        let inside = format!("{prefix}photos/inside.txt");
        seed(
            service,
            FIXTURE_BUCKET,
            &[object(&file, FILE_BYTES), object(&inside, INSIDE_BYTES)],
        )
        .await;
        let volume = connect_fixture(service, Some(FIXTURE_BUCKET)).await;
        let seeded = Self { volume, prefix };
        seeded.listed().await;
        seeded
    }

    fn at(&self, name: &str) -> PathBuf {
        self.volume.root().join(format!("{}{name}", self.prefix))
    }

    fn file_row(&self) -> PathBuf {
        self.at("photos (file)")
    }

    fn folder_row(&self) -> PathBuf {
        self.at("photos")
    }

    /// The prefix's rows: name, whether a folder, and path, sorted by name.
    async fn listed(&self) -> Vec<(String, bool, String)> {
        let folder = self.volume.root().join(self.prefix.trim_end_matches('/'));
        let mut rows: Vec<(String, bool, String)> = self
            .volume
            .list_directory(&folder, None)
            .await
            .expect(FIXTURE)
            .into_iter()
            .map(|entry| (entry.name, entry.is_directory, entry.path))
            .collect();
        rows.sort();
        rows
    }

    async fn names(&self) -> Vec<(String, bool)> {
        self.listed()
            .await
            .into_iter()
            .map(|(name, is_folder, _)| (name, is_folder))
            .collect()
    }
}

fn row(name: &str, is_folder: bool) -> (String, bool) {
    (name.to_string(), is_folder)
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// A server-side copy that never stops or pauses.
struct Steady;

impl ServerCopyProgress for Steady {
    fn advanced(&self, _done: u64, _total: u64) -> ControlFlow<()> {
        ControlFlow::Continue(())
    }

    fn checkpoint(&self) -> Pin<Box<dyn Future<Output = ControlFlow<()>> + Send + '_>> {
        Box::pin(async { ControlFlow::Continue(()) })
    }
}

async fn both_rows_list_and_stat_as_what_they_are(service: FixtureService) {
    let s = Seeded::new(service, "beside-list").await;

    let rows = s.listed().await;
    assert_eq!(
        rows,
        [
            ("photos".to_string(), true, path_text(&s.folder_row())),
            ("photos (file)".to_string(), false, path_text(&s.file_row())),
        ],
        "{}",
        service.key
    );

    let file = s.volume.get_metadata(&s.file_row()).await.expect(FIXTURE);
    assert!(!file.is_directory, "{}", service.key);
    assert_eq!(file.name, "photos (file)", "{}", service.key);
    assert_eq!(file.path, path_text(&s.file_row()), "{}", service.key);
    assert_eq!(file.size, Some(FILE_BYTES.len() as u64), "{}", service.key);

    let folder = s.volume.get_metadata(&s.folder_row()).await.expect(FIXTURE);
    assert!(
        folder.is_directory,
        "{}: the folder row stats as the folder",
        service.key
    );
    assert_eq!(folder.path, path_text(&s.folder_row()), "{}", service.key);
}

async fn the_file_row_reads_shares_and_copies_its_own_bytes(service: FixtureService) {
    let s = Seeded::new(service, "beside-read").await;

    assert_eq!(read_back(&s.volume, &s.file_row()).await, FILE_BYTES, "{}", service.key);
    assert_eq!(
        read_back(&s.volume, &s.at("photos/inside.txt")).await,
        INSIDE_BYTES,
        "{}",
        service.key
    );

    let link = s
        .volume
        .share_link(&s.file_row(), ShareLinkExpiry::OneHour)
        .await
        .expect(FIXTURE)
        .into_url();
    let fetched = cmdr_http::client_builder()
        .build()
        .expect("a plain client builds")
        .get(&link)
        .send()
        .await
        .expect(FIXTURE);
    assert_eq!(fetched.status(), 200, "{}", service.key);
    assert_eq!(
        fetched.bytes().await.expect(FIXTURE).as_ref(),
        FILE_BYTES,
        "{}",
        service.key
    );
    let folder_link = s.volume.share_link(&s.folder_row(), ShareLinkExpiry::OneHour).await;
    assert!(
        matches!(folder_link, Err(VolumeError::IsADirectory(_))),
        "{}: the folder row has no link, {folder_link:?}",
        service.key
    );
    let folder_read = s.volume.open_read_stream(&s.folder_row()).await.map(|_| ());
    assert!(
        matches!(folder_read, Err(VolumeError::IsADirectory(_))),
        "{}: the folder row doesn't read as the file, {folder_read:?}",
        service.key
    );

    let copied = s
        .volume
        .copy_on_server(
            &s.volume,
            &s.file_row(),
            &s.at("copy.txt"),
            WriteMode::CreateNew,
            &Steady,
        )
        .await
        .expect(FIXTURE);
    assert_eq!(copied, FILE_BYTES.len() as u64, "{}", service.key);
    assert_eq!(
        read_back(&s.volume, &s.at("copy.txt")).await,
        FILE_BYTES,
        "{}",
        service.key
    );
}

async fn deleting_the_file_row_keeps_the_folder(service: FixtureService) {
    let s = Seeded::new(service, "beside-delete-file").await;

    s.volume.delete(&s.file_row()).await.expect(FIXTURE);

    assert_eq!(s.names().await, [row("photos", true)], "{}", service.key);
    assert_eq!(
        read_back(&s.volume, &s.at("photos/inside.txt")).await,
        INSIDE_BYTES,
        "{}",
        service.key
    );
}

async fn deleting_the_folder_row_never_touches_the_file(service: FixtureService) {
    let s = Seeded::new(service, "beside-delete-folder").await;

    // The engine's order for a folder: its children, then the folder itself.
    s.volume.delete(&s.at("photos/inside.txt")).await.expect(FIXTURE);
    let folder = s.volume.delete(&s.folder_row()).await;
    assert!(
        matches!(folder, Ok(()) | Err(VolumeError::NotFound(_))),
        "{}: {folder:?}",
        service.key
    );

    // ❗ Pre-fix, the folder's delete found no folder left and deleted the
    // file of its name instead.
    assert_eq!(read_back(&s.volume, &s.file_row()).await, FILE_BYTES, "{}", service.key);
    assert_eq!(s.names().await, [row("photos", false)], "{}", service.key);
    assert_eq!(
        read_back(&s.volume, &s.folder_row()).await,
        FILE_BYTES,
        "{}",
        service.key
    );
}

async fn renaming_each_row_moves_its_own_object(service: FixtureService) {
    let s = Seeded::new(service, "beside-rename").await;

    assert_eq!(
        s.volume.rename_work(&s.file_row()).await.expect(FIXTURE),
        RenameWork::OneCall,
        "{}: the file row renames in one call",
        service.key
    );
    assert_eq!(
        s.volume.rename_work(&s.folder_row()).await.expect(FIXTURE),
        RenameWork::CopyThenDelete,
        "{}: the folder row is a folder",
        service.key
    );
    let folder_rename = s.volume.rename(&s.folder_row(), &s.at("elsewhere"), false).await;
    assert!(
        matches!(folder_rename, Err(VolumeError::NotSupported)),
        "{}: renaming the folder row never moves the file, {folder_rename:?}",
        service.key
    );

    s.volume
        .rename(&s.file_row(), &s.at("renamed.txt"), false)
        .await
        .expect(FIXTURE);

    assert_eq!(
        s.names().await,
        [row("photos", true), row("renamed.txt", false)],
        "{}",
        service.key
    );
    assert_eq!(
        read_back(&s.volume, &s.at("renamed.txt")).await,
        FILE_BYTES,
        "{}",
        service.key
    );
    assert_eq!(
        read_back(&s.volume, &s.at("photos/inside.txt")).await,
        INSIDE_BYTES,
        "{}",
        service.key
    );
}

async fn overwriting_the_file_row_replaces_the_file(service: FixtureService) {
    let s = Seeded::new(service, "beside-overwrite").await;
    let replacement = b"a newer file".to_vec();

    let written = s
        .volume
        .write_from_stream(
            &s.file_row(),
            WriteMode::CreateOrReplace,
            cmdr_fs::volume::StreamLength::Known(replacement.len() as u64),
            Box::new(BytesSource::new(replacement.clone())),
            &|_| ControlFlow::Continue(()),
        )
        .await
        .expect(FIXTURE);

    assert_eq!(written, replacement.len() as u64, "{}", service.key);
    assert_eq!(
        read_back(&s.volume, &s.file_row()).await,
        replacement,
        "{}",
        service.key
    );
    let file = s.volume.get_metadata(&s.file_row()).await.expect(FIXTURE);
    assert_eq!(file.path, path_text(&s.file_row()), "{}", service.key);
    assert_eq!(
        s.names().await,
        [row("photos", true), row("photos (file)", false)],
        "{}",
        service.key
    );
}

async fn versitygw_cant_hold_a_file_and_a_folder_of_one_name() {
    let volume = connect_fixture(VERSITYGW, Some(FIXTURE_BUCKET)).await;
    let prefix = scratch_prefix("beside-versitygw");
    let write = |key: String, bytes: &'static [u8]| {
        let volume = &volume;
        async move {
            volume
                .write_from_stream(
                    &volume.root().join(key),
                    WriteMode::CreateNew,
                    cmdr_fs::volume::StreamLength::Known(bytes.len() as u64),
                    Box::new(BytesSource::new(bytes.to_vec())),
                    &|_| ControlFlow::Continue(()),
                )
                .await
        }
    };
    write(format!("{prefix}photos"), FILE_BYTES).await.expect(FIXTURE);

    let inside = write(format!("{prefix}photos/inside.txt"), INSIDE_BYTES).await;

    // Its POSIX backend stores a key as a file: `photos` can't be a folder too.
    assert!(inside.is_err(), "{inside:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn versitygw_cant_hold_a_file_and_a_folder_of_one_name_on_versitygw() {
    versitygw_cant_hold_a_file_and_a_folder_of_one_name().await;
}

macro_rules! on_garage {
    ($($cell:ident => $garage:ident;)*) => {$(
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        #[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
        async fn $garage() {
            $cell(GARAGE).await;
        }
    )*};
}

on_garage! {
    both_rows_list_and_stat_as_what_they_are => both_rows_list_and_stat_as_what_they_are_on_garage;
    the_file_row_reads_shares_and_copies_its_own_bytes => the_file_row_reads_shares_and_copies_its_own_bytes_on_garage;
    deleting_the_file_row_keeps_the_folder => deleting_the_file_row_keeps_the_folder_on_garage;
    deleting_the_folder_row_never_touches_the_file => deleting_the_folder_row_never_touches_the_file_on_garage;
    renaming_each_row_moves_its_own_object => renaming_each_row_moves_its_own_object_on_garage;
    overwriting_the_file_row_replaces_the_file => overwriting_the_file_row_replaces_the_file_on_garage;
}
