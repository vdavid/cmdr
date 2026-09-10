//! Real copies, a move, a delete, and a mkdir between local disk and a phone
//! over ADB, driven through the app's own write operations against
//! `cmdr_adb::testing::FakeAdbServer`.
//!
//! ❗ **The point is the entry point**, as in `sftp_transfer_integration_test.rs`
//! beside it: the crate's suite exercises every method the engine calls, and is
//! still no evidence that a copy works, because `supports_export`, the
//! free-space pre-flight, and the engine's own staging all live on this side.
//! The fake needs no Docker, so these cells run in the unit lane.
//!
//! The tree, cancel, conflict, and awkward-name scenarios are the shared ones in
//! `network_transfer_test_support.rs`, with the phone as the remote end.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_adb::AdbDeviceState;
use cmdr_adb::testing::{FakeAdbServer, FakeNode, FakeTree, split_argv};
use cmdr_fs::staging::STAGING_TEMP_MARKER;
use cmdr_fs::volume::host::listings::RecordingListings;
use cmdr_fs::volume::{DirectoryChange, Volume};

use super::event_sinks::{CollectorEventSink, OperationEventSink};
use super::network_transfer_test_support::{
    a_cancelled_upload_leaves_nothing_behind, a_directory_tree_lands_intact_off_the_server,
    a_directory_tree_lands_intact_on_the_server, a_pre_existing_destination_still_probes_each_name,
    an_overwrite_answer_replaces_the_destination_bytes, assert_no_staging_litter, awkward_names_survive_a_round_trip,
    read_all, self_describing_bytes, sha256, start_copy_by_id,
};
use super::state::WriteOperationState;
use super::types::{VolumeCopyConfig, WriteOperationConfig};
use crate::adb::device_provider::apply_device_list;
use crate::adb::test_support::{a_listed_phone, dial, phone, retire_phone};
use crate::file_system::volume::LocalPosixVolume;
use crate::file_system::volume::manager::get_volume_manager;
use crate::ignore_poison::IgnorePoison;
use crate::operation_log::types::Initiator;
use crate::test_support::TestDir;

/// Big enough to cross the sync service's 64 KiB `DATA` frames a few times, so
/// reassembly is exercised, and small enough to stay quick on a loaded machine.
const PAYLOAD_BYTES: usize = 200_000;

/// A backstop for one operation to settle against the loopback fake, far above
/// the tens of milliseconds it takes.
const SETTLE_BUDGET: Duration = Duration::from_secs(6);

// ── Fixtures ─────────────────────────────────────────────────────────

/// A phone the app has dialed the way a pane does, and a scratch folder of its
/// own on the shared storage. Retires the phone on drop.
struct DialedPhone {
    fake: FakeAdbServer,
    serial: &'static str,
    volume_id: String,
    volume: Arc<dyn Volume>,
    /// `adb://<serial>/sdcard/<what>`, the spelling a pane holds.
    dir: PathBuf,
    /// The same folder as the device names it.
    device_dir: String,
}

impl Drop for DialedPhone {
    fn drop(&mut self) {
        retire_phone(self.serial);
        apply_device_list(Vec::new());
    }
}

async fn dialed_phone(serial: &'static str, what: &str) -> DialedPhone {
    let fake = a_listed_phone(serial, FakeTree::new()).await;
    let (volume_id, volume) = dial(&fake, serial, &format!("adb-transfer-{what}")).await;
    let device_dir = format!("/sdcard/{what}");
    let dir = PathBuf::from(format!("{}{device_dir}", cmdr_fs::volume::adb_app_root(serial)));
    volume
        .create_directory(&dir)
        .await
        .expect("making the scratch folder on the phone");
    DialedPhone {
        fake,
        serial,
        volume_id,
        volume,
        dir,
        device_dir,
    }
}

/// A local folder registered under an id of its own, so a copy starts from two
/// ids the way the transfer dialog's does. Unregisters on drop.
struct RegisteredLocal {
    dir: TestDir,
    volume_id: String,
}

impl Drop for RegisteredLocal {
    fn drop(&mut self) {
        get_volume_manager().unregister(&self.volume_id);
    }
}

fn registered_local(what: &str) -> RegisteredLocal {
    let dir = TestDir::new(what);
    let volume_id = format!("adb-transfer-local-{}", uuid::Uuid::new_v4());
    get_volume_manager().register(&volume_id, Arc::new(LocalPosixVolume::new("Local", &*dir)));
    RegisteredLocal { dir, volume_id }
}

/// Every `mv` the phone's shell was asked to run, as `(from, to)` device paths.
fn moves_on_the_device(fake: &FakeAdbServer) -> Vec<(String, String)> {
    fake.requests()
        .iter()
        .filter_map(|request| request.strip_prefix("shell,v2,raw:"))
        .map(split_argv)
        .filter(|argv| argv.first().is_some_and(|verb| verb == "mv"))
        .filter_map(|argv| {
            let operands: Vec<&String> = argv[1..].iter().filter(|a| !a.starts_with('-')).collect();
            match operands[..] {
                [from, to] => Some((from.clone(), to.clone())),
                _ => None,
            }
        })
        .collect()
}

// ── Local ↔ phone, file and tree ─────────────────────────────────────

/// A file copied onto a phone lands whole, and ❗ the phone's writer never
/// streams at the name it lands on: `SEND` truncates on open, so it sends to
/// `<name>.cmdr-tmp-<pid>-<n>` and one `mv` gives the bytes their name.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_file_copied_onto_a_phone_lands_whole_through_a_staging_name() {
    let phone = dialed_phone("R58M-Copy-Onto", "copy-onto").await;
    let local = registered_local("adb_copy_onto_phone");
    let content = self_describing_bytes(PAYLOAD_BYTES, "uploaded.bin");
    std::fs::write(local.dir.join("uploaded.bin"), &content).expect("seeding the local file");

    let running = start_copy_by_id(
        "copy-onto-phone",
        local.volume_id.clone(),
        vec![PathBuf::from("uploaded.bin")],
        phone.volume_id.clone(),
        phone.dir.to_string_lossy().into_owned(),
        VolumeCopyConfig::default(),
    )
    .await;
    running.settle().await;
    running.assert_no_errors();

    let landed = read_all(phone.volume.as_ref(), &phone.dir.join("uploaded.bin")).await;
    assert_eq!(
        sha256(&landed),
        sha256(&content),
        "the bytes on the phone must checksum to what local disk holds"
    );

    let own_staging_prefix = format!("{STAGING_TEMP_MARKER}{}-", std::process::id());
    let moves = moves_on_the_device(&phone.fake);
    assert!(
        moves.iter().any(|(from, to)| {
            from.strip_prefix(to.as_str())
                .is_some_and(|suffix| suffix.starts_with(&own_staging_prefix))
                && to.starts_with(&phone.device_dir)
        }),
        "the phone's writer lands every upload by renaming its own staging sibling; moves: {moves:?}"
    );
    assert!(
        moves
            .iter()
            .any(|(from, to)| *to == format!("{}/uploaded.bin", phone.device_dir) && from.contains(STAGING_TEMP_MARKER)),
        "the user's filename is only ever reached by a rename from a staging name; moves: {moves:?}"
    );
    assert_no_staging_litter(phone.volume.as_ref(), &phone.dir, "a copy onto a phone").await;
}

/// A file copied off a phone lands whole: the direction `supports_export`
/// gates, which no crate-side cell can reach.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_file_copied_off_a_phone_lands_whole() {
    let phone = dialed_phone("R58M-Copy-Off", "copy-off").await;
    let content = self_describing_bytes(PAYLOAD_BYTES, "downloaded.bin");
    let on_phone = phone.dir.join("downloaded.bin");
    phone
        .volume
        .create_file(&on_phone, &content)
        .await
        .expect("seeding the file on the phone");
    let local = registered_local("adb_copy_off_phone");

    let running = start_copy_by_id(
        "copy-off-phone",
        phone.volume_id.clone(),
        vec![on_phone],
        local.volume_id.clone(),
        String::new(),
        VolumeCopyConfig::default(),
    )
    .await;
    running.settle().await;
    running.assert_no_errors();

    let landed = std::fs::read(local.dir.join("downloaded.bin")).expect("the copy landed on local disk");
    assert_eq!(
        sha256(&landed),
        sha256(&content),
        "the bytes on local disk must checksum to what the phone holds"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_folder_tree_lands_intact_on_a_phone() {
    let phone = dialed_phone("R58M-Tree-Onto", "tree-onto").await;
    a_directory_tree_lands_intact_on_the_server(Arc::clone(&phone.volume), phone.dir.clone()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_folder_tree_lands_intact_off_a_phone() {
    let phone = dialed_phone("R58M-Tree-Off", "tree-off").await;
    a_directory_tree_lands_intact_off_the_server(Arc::clone(&phone.volume), phone.dir.clone()).await;
}

/// ❗ The fake creates a file at `SEND` open, as a device does, so a cancel
/// mid-upload really leaves a torn staging file for the writer to take away.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_copy_onto_a_phone_cancelled_mid_file_leaves_no_torn_file_and_no_staging() {
    let phone = dialed_phone("R58M-Cancel-Onto", "cancel-onto").await;
    a_cancelled_upload_leaves_nothing_behind(Arc::clone(&phone.volume), phone.dir.clone()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_overwrite_answer_on_a_phone_replaces_the_bytes() {
    let phone = dialed_phone("R58M-Overwrite", "overwrite").await;
    an_overwrite_answer_replaces_the_destination_bytes(Arc::clone(&phone.volume), phone.dir.clone()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_folder_already_on_a_phone_still_asks_about_each_clashing_name() {
    let phone = dialed_phone("R58M-Pre-Existing", "pre-existing").await;
    a_pre_existing_destination_still_probes_each_name(Arc::clone(&phone.volume), phone.dir.clone()).await;
}

/// Every name goes through the device shell's quoting on the way to its `mv`,
/// which is where a space, an `&`, or a `#` would split or vanish.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn awkward_names_survive_a_round_trip_through_a_phone() {
    let phone = dialed_phone("R58M-Awkward-Names", "awkward-names").await;
    awkward_names_survive_a_round_trip(Arc::clone(&phone.volume), phone.dir.clone()).await;
}

// ── Mutations on the phone, and the pane patches they owe ────────────

/// A phone registered with a host that records every pane patch.
///
/// ❗ Recorded rather than read back from the app's listing cache: the cache
/// patch needs a running app (`caching::notify_directory_changed` returns early
/// without an `AppHandle`), and a phone has no watcher to fall back on, so the
/// patch the volume reports IS what keeps a pane honest.
struct RecordedPhone {
    fake: FakeAdbServer,
    volume_id: String,
    volume: Arc<dyn Volume>,
    listings: Arc<RecordingListings>,
    /// `adb://<serial>/sdcard`.
    sdcard: PathBuf,
}

impl Drop for RecordedPhone {
    fn drop(&mut self) {
        get_volume_manager().unregister(&self.volume_id);
    }
}

async fn recorded_phone(serial: &str, tree: FakeTree) -> RecordedPhone {
    let fake = FakeAdbServer::start(tree).await;
    fake.push_devices(vec![phone(serial, AdbDeviceState::Ready)]);
    let (volume, listings) = cmdr_adb::volume::testing::connect_fake(&fake, serial).await;
    let volume_id = volume.volume_id().to_string();
    let volume: Arc<dyn Volume> = volume;
    assert!(
        get_volume_manager().register_if_absent(&volume_id, Arc::clone(&volume)),
        "precondition: nothing else holds {volume_id}"
    );
    let sdcard = PathBuf::from(format!("{}/sdcard", cmdr_fs::volume::adb_app_root(serial)));
    RecordedPhone {
        fake,
        volume_id,
        volume,
        listings,
        sdcard,
    }
}

impl RecordedPhone {
    /// Whether the pane showing `dir` was told `matches` about it.
    fn was_told(&self, dir: &Path, matches: impl Fn(&DirectoryChange) -> bool) -> bool {
        self.listings
            .changes()
            .iter()
            .any(|(volume_id, parent, change)| *volume_id == self.volume_id && parent == dir && matches(change))
    }

    fn device_node(&self, device_path: &str) -> Option<FakeNode> {
        self.fake.tree().lock_ignore_poison().get(device_path).cloned()
    }

    /// Every patch the volume reported, in words, for a failure message:
    /// `DirectoryChange` carries no `Debug`.
    fn changes_seen(&self) -> Vec<String> {
        self.listings
            .changes()
            .iter()
            .map(|(_, parent, change)| {
                let parent = parent.display();
                match change {
                    DirectoryChange::Added(entry) => format!("added {parent}/{}", entry.name),
                    DirectoryChange::Removed(name) => format!("removed {parent}/{name}"),
                    DirectoryChange::Modified(entry) => format!("modified {parent}/{}", entry.name),
                    DirectoryChange::Renamed { old_name, new_entry } => {
                        format!("renamed {parent}/{old_name} to {}", new_entry.name)
                    }
                    DirectoryChange::Replaced(entries) => format!("replaced {parent} ({} entries)", entries.len()),
                    DirectoryChange::FullRefresh => format!("full refresh of {parent}"),
                }
            })
            .collect()
    }
}

/// Waits for an operation's `write-settled`, which fires after its terminal
/// event and every cleanup the driver owns.
async fn settle(events: &CollectorEventSink, what: &str) {
    crate::test_support::wait_until_async(SETTLE_BUDGET, what, || !events.settled.lock_ignore_poison().is_empty())
        .await;
    let errors: Vec<String> = events
        .errors
        .lock_ignore_poison()
        .iter()
        .map(|e| format!("{:?}", e.error))
        .collect();
    assert!(errors.is_empty(), "{what}: the operation reported {errors:?}");
}

/// ❗ The copy dialog previews a destination folder before the copy makes it
/// (`copy_volumes_with_progress` creates a missing destination). A phone answers
/// for the storage that folder will land on, so the preview still checks for
/// room, and it opens: `dest_space_if_known` tolerates only `NotSupported`, so a
/// `NotFound` about the missing folder would refuse the preview outright.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_preview_into_a_phone_folder_the_copy_will_create_checks_the_room_on_its_shared_storage() {
    let phone = recorded_phone("PREVIEWNEWDIR01", FakeTree::new()).await;
    let source = crate::file_system::volume::InMemoryVolume::new("Source");
    source
        .create_file(Path::new("/photo.jpg"), b"a photo's worth of bytes")
        .await
        .expect("the source file");
    let destination = phone.sdcard.join("New album").join("Deeper");

    let preview = crate::file_system::scan_for_volume_copy(
        &source,
        &[PathBuf::from("/photo.jpg")],
        phone.volume.as_ref(),
        &destination,
        10,
    )
    .await
    .expect("a preview into a folder the copy will create opens");
    // The Pixel's own `df -k /sdcard` figure (`cmdr_adb::testing::pixel_captures`).
    assert_eq!(
        preview.dest_space.and_then(|space| space.available_bytes()),
        Some(26_956_476 * 1024),
        "the preview judges the copy against the shared storage it lands on"
    );
}

/// mkdir on a phone makes the folder and tells the pane showing its parent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mkdir_on_a_phone_makes_the_folder_and_patches_the_pane_showing_its_parent() {
    let phone = recorded_phone("R58M-Mkdir", FakeTree::new()).await;
    let parent = phone.sdcard.to_string_lossy().into_owned();

    let (made, _) = super::create::create_directory_core(Some(phone.volume_id.clone()), &parent, "album")
        .await
        .expect("mkdir on the phone");

    assert_eq!(
        made,
        phone.sdcard.join("album"),
        "the new folder keeps the pane's spelling"
    );
    assert!(
        matches!(phone.device_node("/sdcard/album"), Some(FakeNode::Dir { .. })),
        "the phone holds the folder"
    );
    assert!(
        phone.was_told(
            &phone.sdcard,
            |change| matches!(change, DirectoryChange::Added(entry) if entry.name == "album")
        ),
        "the pane on /sdcard hears the folder arrived; changes: {:?}",
        phone.changes_seen()
    );
}

/// A move within a phone is a rename on the device, and both panes it touched
/// hear about it: the one the file left and the one it arrived in.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_on_a_phone_renames_on_the_device_and_patches_both_panes() {
    let mut tree = FakeTree::new();
    tree.add_file("/sdcard/a.txt", b"moving").add_dir("/sdcard/album");
    let phone = recorded_phone("R58M-Move", tree).await;
    let album = phone.sdcard.join("album");

    let events = Arc::new(CollectorEventSink::new());
    super::start_volume_move(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        phone.volume_id.clone(),
        vec![phone.sdcard.join("a.txt")],
        phone.volume_id.clone(),
        album.to_string_lossy().into_owned(),
        VolumeCopyConfig::default(),
        Initiator::User,
        None,
    )
    .await
    .expect("the move starts");
    settle(&events, "the move on the phone to settle").await;

    assert!(phone.device_node("/sdcard/a.txt").is_none(), "the file left its folder");
    assert!(
        matches!(phone.device_node("/sdcard/album/a.txt"), Some(FakeNode::File { .. })),
        "and arrived in the album"
    );
    let changes = phone.changes_seen();
    assert!(
        phone.was_told(
            &phone.sdcard,
            |change| matches!(change, DirectoryChange::Removed(name) if name == "a.txt")
        ),
        "the pane on /sdcard hears the file left; changes: {changes:?}"
    );
    assert!(
        phone.was_told(
            &album,
            |change| matches!(change, DirectoryChange::Added(entry) if entry.name == "a.txt")
        ),
        "the pane on the album hears it arrived; changes: {changes:?}"
    );
}

/// A delete on a phone removes the file and tells the pane showing its folder.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_on_a_phone_removes_the_file_and_patches_its_pane() {
    let mut tree = FakeTree::new();
    tree.add_file("/sdcard/old.txt", b"deleting");
    let phone = recorded_phone("R58M-Delete", tree).await;

    let events = CollectorEventSink::new();
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(0)));
    super::delete_volume_files_for_test(
        Arc::clone(&phone.volume),
        &phone.volume_id,
        &events,
        "adb-delete-on-phone",
        &state,
        &[phone.sdcard.join("old.txt")],
        &WriteOperationConfig::default(),
    )
    .await
    .expect("the delete on the phone");

    assert!(
        phone.device_node("/sdcard/old.txt").is_none(),
        "the phone no longer holds the file"
    );
    assert!(
        phone.was_told(
            &phone.sdcard,
            |change| matches!(change, DirectoryChange::Removed(name) if name == "old.txt")
        ),
        "the pane on /sdcard hears the file is gone; changes: {:?}",
        phone.changes_seen()
    );
}
