//! A live search scoped to an accented folder on a real SMB share, through
//! Cmdr's own smb2 volume, against the `smb-consumer-unicode` fixture's Samba.
//!
//! The regression: searching `/Volumes/testshare/documents/Testföretag AB` answered
//! "No files match" over a folder full of matches. `realpath` through the macOS
//! smbfs mount handed the scope back decomposed (NFD: `a` + U+0308), the NAS
//! stores it composed (NFC), and the smb2 walk asked for a name the server
//! didn't have, then said nothing about it. Two halves, one cell each:
//!
//! - A scope in the share's own spelling reaches the walk unchanged (a backend
//!   volume is `PathNamespace::Backend`, so nothing canonicalizes it) and finds
//!   the file inside.
//! - The NFD spelling smbfs used to hand over is a folder the walk couldn't
//!   open, NAMED in `failed_roots`, and the run still ends `Completed` rather
//!   than reading as a drive that went away.
//!
//! What the fixture's Samba does with the two forms (verified on the
//! `smb-consumer-unicode` image, Alpine 3.24 Samba, by these cells' own
//! preconditions, 2026-11-04): it stores a name as the bytes it's sent and
//! matches a lookup on those bytes only. Its `case sensitive = auto` fold is
//! case-only, never Unicode-form, so an NFD lookup of an NFC folder misses
//! (`exists` is false, the listing is `NotFound`), exactly as the QNAP did.
//!
//! We can't stand up an OS smbfs mount in CI, so `realpath` itself is the unit
//! tests' job (`search/query/tests.rs`, with a symlink stand-in for the rename).
//! This pins everything downstream of the scope path against a real server.
//!
//! `#[ignore]`d and named `smb_integration_` so the fixture lane
//! (`desktop-rust-integration-tests`) runs them. By hand: start the containers
//! with `apps/desktop/test/smb-servers/start.sh`, export
//! `SMB_CONSUMER_UNICODE_PORT=11484`, then
//! `cargo nextest run -E 'test(smb_integration_a_live_search)' --run-ignored only`.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::Volume;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_index::Index;
use cmdr_index::testing::host::test_lock;
use cmdr_smb::volume::testing::{ensure_clean, test_dir_name};
use cmdr_smb::volume::{MountAnchor, SmbConnectionParams, SmbVolume, connect_smb_volume};

use super::live_drive::{MOUNT_PREFIX, pattern_query_for, settled};
use super::*;
use crate::file_system::index_provider::AppVolumeProvider;
use crate::file_system::volume::manager::get_volume_manager;
use crate::search::live::{self, WalkEnding};

/// The folder from the report, composed (NFC): how the share stores it.
const NFC_FOLDER: &str = "Testf\u{f6}retag AB";
/// The same folder decomposed (`a` + U+0308): how macOS smbfs spelled it back.
const NFD_FOLDER: &str = "Testfo\u{308}retag AB";
/// The file inside, which the search has to find.
const FILE: &str = "faktura-2026.pdf";

/// A backstop for one walk of a two-entry folder on loopback: far above the
/// fraction of a second it takes, under nextest's slow-test cutoff.
const WALK_BUDGET: Duration = Duration::from_secs(20);

/// The unicode fixture's `public` share, registered the way a connected share
/// is, with a scratch folder of its own holding the NFC folder and its file.
/// Unregistered on drop; the scratch folder goes in [`Share::clean_up`].
struct Share {
    volume: Arc<SmbVolume>,
    volume_id: String,
    /// Where the share is "mounted". Nothing is: smb2 only strips the prefix.
    /// Under an external-mount prefix, because that's what routes a scope to a
    /// per-mount index.
    mount_root: String,
    /// The scratch folder's share-relative name.
    top: String,
}

impl Share {
    async fn connect() -> Self {
        let port = smb2::testing::unicode_port();
        let volume_id = cmdr_fs::volume::smb_volume_id("127.0.0.1", port, "public");
        let mount_root = format!("{MOUNT_PREFIX}/cmdr-search-{}", test_dir_name());
        let volume = connect_smb_volume(
            "public",
            MountAnchor::at_share_root(&mount_root),
            &volume_id,
            SmbConnectionParams::new("127.0.0.1", "public", port, None, None),
            VolumeHost::detached(),
        )
        .await
        .unwrap_or_else(|e| {
            panic!("Failed to connect to the unicode SMB container at 127.0.0.1:{port}. Is it running? ({e:?})")
        });
        let volume = Arc::new(volume);
        get_volume_manager().register(&volume_id, Arc::clone(&volume) as Arc<dyn Volume>);

        let top = test_dir_name();
        ensure_clean(&volume, &top).await;
        let share = Self {
            volume,
            volume_id,
            mount_root,
            top,
        };
        share
            .volume
            .create_directory(&share.path(&share.top))
            .await
            .expect("the scratch folder");
        share
            .volume
            .create_directory(&share.folder(NFC_FOLDER))
            .await
            .expect("the NFC folder");
        share
            .volume
            .create_file(&share.folder(NFC_FOLDER).join(FILE), b"a receipt")
            .await
            .expect("the file inside it");
        share
    }

    /// An absolute path under the mount root.
    fn path(&self, relative: &str) -> std::path::PathBuf {
        Path::new(&self.mount_root).join(relative)
    }

    /// `folder` inside the scratch folder, as an absolute path.
    fn folder(&self, folder: &str) -> std::path::PathBuf {
        self.path(&self.top).join(folder)
    }

    async fn clean_up(&self) {
        ensure_clean(&self.volume, &self.top).await;
    }
}

impl Drop for Share {
    fn drop(&mut self) {
        get_volume_manager().unregister(&self.volume_id);
    }
}

/// The index, wired to the app's own volume registry, over a data directory the
/// search reads too.
fn install_index(data: &Path) -> (Index, impl Sized, impl Sized) {
    let (index, installed) = Index::builder()
        .data_dir(data)
        .volumes(Arc::new(AppVolumeProvider))
        .install_for_test();
    (index, installed, volumes::install_data_dir_for_test(data))
}

/// Run one live search for `pattern` over `scope`, to the end, the way MCP's
/// `search` does.
async fn search(scope: String, pattern: &'static str) -> live::LiveAnswer {
    tokio::task::spawn_blocking(move || run_live_collected(pattern_query_for(&scope, pattern), None, WALK_BUDGET))
        .await
        .expect("the search thread")
        .expect("the live search")
}

/// The share's own spelling of the folder is the scope the walk takes, and the
/// file inside comes back as a path under the mount root.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the index's process-wide seams for the whole cell; holding it across the awaits IS the point"
)]
async fn smb_integration_a_live_search_scoped_to_an_accented_folder_finds_its_file() {
    let _serialized = test_lock();
    let _one_run_at_a_time = live::test_registry_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let (index, _installed, _search_data) = install_index(data.path());
    let share = Share::connect().await;
    let scope = share.folder(NFC_FOLDER).to_string_lossy().into_owned();

    assert_eq!(
        crate::search::index_source::path_namespace(&share.volume_id),
        crate::search::index_source::PathNamespace::Backend,
        "a share's scope never goes through realpath"
    );
    let answer = search(scope.clone(), "faktura").await;
    let walked = index.volume_ids().contains(&share.volume_id);
    share.clean_up().await;

    assert_eq!(
        answer.target_volume_id, share.volume_id,
        "the scope routes to the share"
    );
    let paths: Vec<&str> = answer.entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        [format!("{scope}/{FILE}")],
        "the walk found the file, under the NFC scope"
    );
    let coverage = settled(&answer.ending);
    assert_eq!(coverage.walk, WalkEnding::Completed);
    assert!(coverage.failed_roots.is_empty(), "{:?}", coverage.failed_roots);
    assert!(
        coverage.unresolved_scopes.is_empty(),
        "{:?}",
        coverage.unresolved_scopes
    );
    assert!(walked, "the walk stood the share's index up");

    volumes::forget_volume_for_test(&share.volume_id);
    let _ = index.forget_volume(&share.volume_id);
}

/// The NFD spelling smbfs used to hand over names a folder this Samba doesn't
/// have. The walk can't open it, says so in `failed_roots`, and the run is
/// `Completed`: the server answered, it just doesn't have that name.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the index's process-wide seams for the whole cell; holding it across the awaits IS the point"
)]
async fn smb_integration_a_live_search_scoped_to_the_nfd_spelling_names_the_folder_it_could_not_open() {
    let _serialized = test_lock();
    let _one_run_at_a_time = live::test_registry_lock();
    let data = tempfile::tempdir().expect("index data dir");
    let (index, _installed, _search_data) = install_index(data.path());
    let share = Share::connect().await;
    let nfd_scope = share.folder(NFD_FOLDER).to_string_lossy().into_owned();

    // The precondition the whole cell rests on: this Samba matches names by their
    // bytes, so the NFD spelling isn't there. If an image bump ever made Samba
    // fold Unicode forms, this is where it says so.
    let nfd_exists = share.volume.exists(Path::new(&nfd_scope)).await;
    let nfc_exists = share.volume.exists(&share.folder(NFC_FOLDER)).await;
    let answer = search(nfd_scope.clone(), "faktura").await;
    share.clean_up().await;

    assert!(nfc_exists, "the NFC folder is on the share");
    assert!(!nfd_exists, "Samba doesn't fold NFD onto the stored NFC name");
    assert_eq!(
        answer.target_volume_id, share.volume_id,
        "the scope routes to the share"
    );
    assert!(answer.entries.is_empty(), "nothing was read: {:?}", answer.entries);
    let coverage = settled(&answer.ending);
    assert_eq!(
        coverage.failed_roots,
        std::slice::from_ref(&nfd_scope),
        "the folder the walk couldn't open is named, never a confident zero"
    );
    assert_eq!(
        coverage.walk,
        WalkEnding::Completed,
        "the share answered, so this isn't a drive that went away"
    );
    assert!(
        !coverage.unresolved_scopes.contains(&nfd_scope),
        "a failed root isn't also unresolved: {:?}",
        coverage.unresolved_scopes
    );

    volumes::forget_volume_for_test(&share.volume_id);
    let _ = index.forget_volume(&share.volume_id);
}
