//! A card reached through its volume, end to end with plain threads: the route
//! table, a volume that stopped answering (scripted, on millisecond budgets), a
//! healthy share, an archive's entry, and a snapshot from a repo's history.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cmdr_about::card::{AboutFact, VolumeLimit};
use cmdr_about::fact::{AboutTarget, Fact, FactKind, UnknownWhy};
use cmdr_about::headline::HeadlineLead;
use cmdr_about::payload::SnapshotKind;
use cmdr_about::removal::RemovalConsequence;
use cmdr_about::text::ItemText;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError};
use tokio::sync::Semaphore;

use super::event::AboutUpdate;
use cmdr_about::actions::{ActionCaps, RemoveBy};

use super::route::{Way, action_caps, caps_for, hinted_target, snapshot_of, through_plan};
use super::session::{Timing, Watch};
use super::tests::{LONG, assert_well_formed, gather};
use super::through::{ArchiveRecord, Asked, DownloadFrom, MAX_STUCK_PER_VOLUME, STUCK, Through, category_segment};
use crate::test_support::{TestDir, wait_until};

/// A volume whose every call waits until the test opens the gate: a share whose
/// server went silent.
struct Silent {
    root: PathBuf,
    gate: Semaphore,
}

impl Silent {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            root: PathBuf::from("/Volumes/naspi"),
            gate: Semaphore::new(0),
        })
    }

    /// The server answers again: every waiting call returns.
    fn wake(&self) {
        self.gate.add_permits(Semaphore::MAX_PERMITS / 2);
    }
}

impl Volume for Silent {
    fn name(&self) -> &str {
        "naspi"
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(cmdr_fs::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let _ = self.gate.acquire().await;
            Err(VolumeError::NotFound(path.display().to_string()))
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let _ = self.gate.acquire().await;
            Err(VolumeError::NotFound(path.display().to_string()))
        })
    }

    fn supports_xattrs(&self) -> bool {
        true
    }

    fn read_xattr_many<'a>(
        &'a self,
        paths: &'a [PathBuf],
        _name: &'a str,
        _mode: cmdr_fs::volume::XattrReadMode,
    ) -> Pin<Box<dyn Future<Output = Vec<cmdr_fs::volume::XattrAnswer>> + Send + 'a>> {
        Box::pin(async move {
            let _ = self.gate.acquire().await;
            paths.iter().map(|_| Ok(None)).collect()
        })
    }
}

fn asked(volume: Arc<dyn Volume>, stuck_id: &str) -> Asked {
    Asked {
        volume,
        stuck_id: stuck_id.to_string(),
        runtime: tokio::runtime::Handle::current(),
    }
}

fn through(asked: Asked, path: &str, download: Option<DownloadFrom>) -> Through {
    Through {
        asked,
        path: PathBuf::from(path),
        download,
        index: None,
        image: None,
        snapshot: None,
    }
}

fn on_milliseconds() -> Timing {
    Timing {
        grace: Duration::from_millis(10),
        row_budget: LONG,
        volume_budget: Duration::from_millis(60),
    }
}

fn kinds_in_order(updates: &[AboutUpdate]) -> Vec<String> {
    updates
        .iter()
        .map(|update| match update {
            AboutUpdate::Planned { .. } => "planned".to_string(),
            AboutUpdate::Fact { fact, .. } => format!("{:?}", fact.kind()),
            AboutUpdate::VolumeStoppedAnswering => "stopped".to_string(),
            AboutUpdate::FactsSettled => "settled".to_string(),
            AboutUpdate::Unavailable { .. } => "unavailable".to_string(),
            AboutUpdate::VerdictPlanned
            | AboutUpdate::VerdictUnavailable { .. }
            | AboutUpdate::VerdictGiven { .. }
            | AboutUpdate::VerdictDelta { .. }
            | AboutUpdate::VerdictEnd { .. }
            | AboutUpdate::VerdictFromEarlier { .. } => "verdict".to_string(),
        })
        .collect()
}

// ── The route table ─────────────────────────────────────────────────────────

#[test]
fn each_way_asks_only_what_it_can_answer() {
    let name = || ItemText::new("Naspolya");
    let local = caps_for(Way::Local, name(), true, true);
    assert!(local.content && local.system && local.git && local.operation_log);

    let share = caps_for(Way::OsMountedShare, name(), true, true);
    assert!(share.download_record && share.index);
    assert!(
        !(share.content || share.system || share.git || share.operation_log),
        "no Spotlight, signature, byte read, or repo walk over a share"
    );
    assert!(!caps_for(Way::OsMountedShare, name(), false, false).download_record);

    let server = caps_for(Way::Server, name(), true, true);
    assert!(server.index && !server.download_record && !server.content);

    for way in [Way::Device, Way::GitHistory, Way::Snapshot, Way::Offline] {
        let caps = caps_for(way, name(), true, true);
        assert!(
            !(caps.content || caps.download_record || caps.system || caps.index || caps.operation_log || caps.git),
            "{way:?} answers identity only"
        );
    }
    let archive = caps_for(Way::Archive, name(), true, true);
    assert!(archive.download_record && !archive.index && !archive.system);
}

#[test]
fn the_pane_s_listing_names_the_target_before_the_volume_answers() {
    assert_eq!(hinted_target("report.pdf", false), AboutTarget::File);
    assert_eq!(hinted_target("photos", true), AboutTarget::Folder);
    // Package suffixes make packages only where the OS says so (macOS).
    if cmdr_fs::file_extension::is_listed_package("Slack.app") {
        assert_eq!(hinted_target("Slack.app", true), AboutTarget::AppBundle);
        assert_eq!(hinted_target("Notes.rtfd", true), AboutTarget::Package);
    } else {
        assert_eq!(hinted_target("Slack.app", true), AboutTarget::Folder);
    }
    // A file named like a bundle is still a file.
    assert_eq!(hinted_target("Slack.app", false), AboutTarget::File);
}

// ── A volume that stopped answering ─────────────────────────────────────────

/// The failure the card exists to avoid: a dead NAS. The plan goes out before
/// the share is asked anything, the card says once that it stopped answering,
/// every row lands quietly, and the whole stream ends inside the volume budget
/// plus a margin, with the share's calls still out.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_share_that_stopped_answering_gets_its_card_at_once_and_says_so_once() {
    let silent = Silent::new();
    let stuck_id = "smb-test-silent-share";
    let via = asked(Arc::clone(&silent) as Arc<dyn Volume>, stuck_id);
    let caps = caps_for(Way::OsMountedShare, ItemText::new("Naspolya"), false, true);
    let gathered = through_plan(
        Way::OsMountedShare,
        "report.pdf",
        false,
        &caps,
        through(via.clone(), "/Volumes/naspi/report.pdf", Some(DownloadFrom::Volume)),
    );
    assert_eq!(gathered.reach, Watch::TheVolume);

    let started = Instant::now();
    let recorder = gather(gathered, on_milliseconds()).await;
    let took = started.elapsed();
    assert!(took < Duration::from_millis(600), "the card settled in {took:?}");

    let updates = recorder.updates();
    assert_well_formed(&updates);
    let order = kinds_in_order(&updates);
    assert_eq!(order[0], "planned");
    assert_eq!(order[1], "stopped", "the note changes before any row lands: {order:?}");
    let AboutUpdate::Planned { plan, .. } = &updates[0] else {
        unreachable!("checked above")
    };
    assert_eq!(
        plan.volume_note.as_ref().map(|note| note.limit),
        Some(VolumeLimit::SomeFacts)
    );
    for fact in recorder.facts() {
        let why = match &fact {
            AboutFact::Identity(Fact::Unknown(unknown)) | AboutFact::Download(Fact::Unknown(unknown)) => unknown.why(),
            other => panic!("only identity and the download record are planned: {other:?}"),
        };
        assert_eq!(why, UnknownWhy::VolumeStoppedAnswering);
    }
    let removal = updates.iter().rev().find_map(|update| match update {
        AboutUpdate::Fact { derived, .. } => Some(derived.removal.clone()),
        _ => None,
    });
    assert_eq!(
        removal.map(|removal| removal.shown),
        Some(vec![RemovalConsequence::CantTell]),
        "a silent share is no evidence the file is the person's own"
    );

    // Both calls are still out on the share. The next card asks it nothing: the
    // rows land at once and the note says it stopped.
    wait_until(LONG, "both calls are out", || STUCK.is_full(stuck_id));
    assert_eq!(MAX_STUCK_PER_VOLUME, 2);
    let again = through_plan(
        Way::OsMountedShare,
        "report.pdf",
        false,
        &caps,
        through(via, "/Volumes/naspi/report.pdf", Some(DownloadFrom::Volume)),
    );
    assert!(again.jobs.is_empty(), "no call to a share that's holding two already");
    assert_eq!(
        again.plan.volume_note.as_ref().map(|note| note.limit),
        Some(VolumeLimit::Unreachable)
    );
    let started = Instant::now();
    let recorder = gather(again, on_milliseconds()).await;
    assert!(started.elapsed() < Duration::from_millis(50), "settled with no wait");
    assert_well_formed(&recorder.updates());

    silent.wake();
    wait_until(LONG, "the share's calls return", || !STUCK.is_full(stuck_id));
}

/// A picture on a share that stopped answering still says what it shows: the row
/// reads the image index's database, ❌ never the share, so it lands known while
/// the share's own rows say it stopped.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_silent_shares_picture_says_what_it_shows_from_the_index_alone() {
    let data = TestDir::new("about-through-image");
    let path = "/Volumes/naspi/Photos/sign.jpg";
    cmdr_index::media_index::testing::seed_ocr_row(&data, "smb-test-image-share", path, "OPEN 9-17");
    // nextest runs each test in its own process, so the switch is this test's alone.
    cmdr_index::media_index::gate::set_enabled(true);

    let silent = Silent::new();
    let via = asked(Arc::clone(&silent) as Arc<dyn Volume>, "smb-test-image-share-calls");
    let image = super::image::ImagePlace::new(
        data.as_ref().to_path_buf(),
        "smb-test-image-share",
        "smb-test-image-share",
        path,
    );
    let caps = cmdr_about::card::VolumeCaps {
        image_index: true,
        ..caps_for(Way::OsMountedShare, ItemText::new("Naspolya"), false, true)
    };
    let gathered = through_plan(
        Way::OsMountedShare,
        "sign.jpg",
        false,
        &caps,
        Through {
            image: Some(image),
            ..through(via, path, None)
        },
    );
    assert!(gathered.plan.has(FactKind::ImageContents));

    let recorder = gather(gathered, on_milliseconds()).await;
    assert_well_formed(&recorder.updates());
    let shows = recorder.facts().into_iter().find_map(|fact| match fact {
        AboutFact::ImageContents(fact) => Some(fact),
        _ => None,
    });
    let Some(Fact::Known(contents)) = shows else {
        panic!("the index answered while the share didn't: {shows:?}");
    };
    assert_eq!(contents.text_lines, [ItemText::new("OPEN 9-17")]);
    silent.wake();
}

// ── A healthy share ─────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_healthy_share_answers_identity_and_its_download_record_through_its_own_calls() {
    let mut report = FileEntry::new("report.pdf".into(), "/report.pdf".into(), false, false);
    report.size = Some(2_048);
    report.modified_at = Some(1_759_400_000);
    let share = Arc::new(InMemoryVolume::with_entries("naspi", vec![report]));
    // Each OS's own names; the platform reads its pair.
    for (name, value) in [
        ("com.apple.quarantine", b"0081;6af6442f;Chrome;".as_slice()),
        ("user.xdg.origin.url", b"https://example.com/report.pdf".as_slice()),
    ] {
        share
            .write_xattr(Path::new("/report.pdf"), name, value)
            .await
            .expect("the double stores xattrs");
    }
    let caps = caps_for(Way::OsMountedShare, ItemText::new("Naspolya"), false, true);
    let gathered = through_plan(
        Way::OsMountedShare,
        "report.pdf",
        false,
        &caps,
        through(
            asked(share as Arc<dyn Volume>, "smb-test-healthy"),
            "/report.pdf",
            Some(DownloadFrom::Volume),
        ),
    );
    let recorder = gather(gathered, on_milliseconds()).await;
    let updates = recorder.updates();
    assert_well_formed(&updates);
    assert!(
        !updates
            .iter()
            .any(|update| matches!(update, AboutUpdate::VolumeStoppedAnswering))
    );
    let facts = recorder.facts();
    assert!(facts.iter().any(|fact| matches!(
        fact,
        AboutFact::Identity(Fact::Known(identity)) if identity.logical_size == Some(2_048)
    )));
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    assert!(
        facts
            .iter()
            .any(|fact| matches!(fact, AboutFact::Download(Fact::Known(_)))),
        "the share's xattrs carried a download record: {facts:?}"
    );
}

/// A file that went between the listing and the card says so on its row.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_item_the_share_no_longer_holds_says_it_is_gone() {
    let share = Arc::new(InMemoryVolume::new("naspi"));
    let caps = caps_for(Way::Server, ItemText::new("Naspolya"), false, false);
    let gathered = through_plan(
        Way::Server,
        "gone.txt",
        false,
        &caps,
        through(asked(share as Arc<dyn Volume>, "smb-test-gone"), "/gone.txt", None),
    );
    let recorder = gather(gathered, on_milliseconds()).await;
    assert_well_formed(&recorder.updates());
    assert!(recorder.facts().iter().any(|fact| matches!(
        fact,
        AboutFact::Identity(Fact::Unknown(unknown)) if unknown.why() == UnknownWhy::NotFound
    )));
}

// ── An archive's entry ──────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_archive_entry_names_its_archive_with_the_archives_own_record() {
    let dir = TestDir::new("about-archive-entry");
    let zip = dir.as_ref().join("Photos.zip");
    std::fs::write(&zip, b"PK\x03\x04").expect("write the archive");
    let inner = format!("{}/a.jpg", zip.display());
    let mut entry = FileEntry::new("a.jpg".into(), inner.clone(), false, false);
    entry.size = Some(1_000);
    let archive = Arc::new(InMemoryVolume::with_entries("Photos.zip", vec![entry]));
    let caps = caps_for(Way::Archive, ItemText::new("Macintosh HD"), false, false);
    let gathered = through_plan(
        Way::Archive,
        "a.jpg",
        false,
        &caps,
        through(
            asked(archive as Arc<dyn Volume>, "root"),
            &inner,
            Some(DownloadFrom::Archive {
                archive: ItemText::new("Photos.zip"),
                record: ArchiveRecord::Local {
                    archive_path: zip.to_string_lossy().into_owned(),
                },
            }),
        ),
    );
    assert_eq!(gathered.plan.target, AboutTarget::ArchiveEntry);
    assert_eq!(
        gathered.plan.volume_note, None,
        "the download row names the archive instead"
    );
    let recorder = gather(gathered, on_milliseconds()).await;
    let updates = recorder.updates();
    assert_well_formed(&updates);
    let download = recorder.facts().into_iter().find_map(|fact| match fact {
        AboutFact::Download(Fact::Known(download)) => Some(download),
        _ => None,
    });
    let download = download.expect("an entry's download row always answers");
    assert_eq!(download.archive, Some(ItemText::new("Photos.zip")));
    assert_eq!(download.sources, Fact::unknown(UnknownWhy::NoRecord));
    let lead = updates.iter().rev().find_map(|update| match update {
        AboutUpdate::Fact { derived, .. } => Some(derived.headline.lead.clone()),
        _ => None,
    });
    assert_eq!(
        lead,
        Some(HeadlineLead::InsideArchive {
            archive: ItemText::new("Photos.zip")
        })
    );
}

// ── A snapshot from a repo's history ────────────────────────────────────────

fn portal_over(repo: &Path) -> Arc<dyn Volume> {
    let portal = Arc::new(cmdr_git::GitPortal::new(
        crate::volume_host::host(),
        cmdr_git::no_git_state_sink(),
    ));
    let parent: Arc<dyn Volume> = Arc::new(cmdr_local::LocalPosixVolume::local_folder(
        "Parent",
        repo,
        crate::volume_host::host(),
    ));
    Arc::new(cmdr_git::GitPortalVolume::new(portal, repo.to_path_buf(), parent))
}

/// The category words the card reads off a path are the portal's own.
#[test]
fn the_snapshot_categories_are_the_portals_own_words() {
    let repo = cmdr_git::test_fixtures::build_repo_with_a_tag_and_a_slashed_branch("about-snapshot-words");
    let portal = cmdr_git::GitPortal::new(crate::volume_host::host(), cmdr_git::no_git_state_sink());
    let rows: Vec<String> = portal.category_rows(&repo).into_iter().map(|row| row.name).collect();
    for kind in [
        SnapshotKind::Branch,
        SnapshotKind::Tag,
        SnapshotKind::Commit,
        SnapshotKind::Stash,
    ] {
        assert!(
            rows.iter().any(|row| row == category_segment(kind)),
            "{kind:?} is `{}` in {rows:?}",
            category_segment(kind)
        );
    }
    let dot_git = repo.join(".git");
    assert_eq!(
        snapshot_of(&dot_git.join("branches")),
        None,
        "the category itself names no snapshot"
    );
    assert_eq!(snapshot_of(&dot_git.join("config")), None);
    let snapshot = snapshot_of(&dot_git.join("tags/v1.0/README.md")).expect("a tag's file");
    assert_eq!(snapshot.kind, SnapshotKind::Tag);
    assert_eq!(snapshot.after, ["v1.0", "README.md"]);
    cmdr_git::test_fixtures::cleanup(&repo);
}

/// A file in a branch whose name holds a slash: the portal says where the name
/// ends, the card names the branch whole, and removal says only that it's git's
/// record.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_in_a_branch_snapshot_names_the_branch_and_when_it_last_changed() {
    let repo = cmdr_git::test_fixtures::build_repo_with_a_tag_and_a_slashed_branch("about-snapshot-branch");
    let path = repo.join(".git/branches/feature/foo/README.md");
    let snapshot = snapshot_of(&path).expect("a branch's file");
    let caps = caps_for(Way::GitHistory, ItemText::new("Macintosh HD"), false, false);
    let gathered = through_plan(
        Way::GitHistory,
        "README.md",
        false,
        &caps,
        Through {
            snapshot: Some(snapshot),
            ..through(asked(portal_over(&repo), "root"), &path.to_string_lossy(), None)
        },
    );
    assert_eq!(gathered.plan.volume_note, None);
    assert!(gathered.plan.has(FactKind::GitSnapshot));
    let recorder = gather(
        gathered,
        Timing {
            volume_budget: LONG,
            ..on_milliseconds()
        },
    )
    .await;
    let updates = recorder.updates();
    assert_well_formed(&updates);
    let found = recorder.facts().into_iter().find_map(|fact| match fact {
        AboutFact::GitSnapshot(Fact::Known(snapshot)) => Some(snapshot),
        _ => None,
    });
    let found = found.expect("the portal answers for its own path");
    assert_eq!(found.kind, SnapshotKind::Branch);
    assert_eq!(found.name, ItemText::new("feature/foo"));
    assert_eq!(found.last_changed, Some(1_700_000_000));
    let removal = updates.iter().rev().find_map(|update| match update {
        AboutUpdate::Fact { derived, .. } => Some(derived.removal.shown.clone()),
        _ => None,
    });
    assert_eq!(removal, Some(vec![RemovalConsequence::InGitHistory]));
    cmdr_git::test_fixtures::cleanup(&repo);
}

// ── What the footer's actions can do, by way ────────────────────────────────

/// A volume that answers nothing but whether it takes writes and keeps a Trash.
struct Shelf {
    root: PathBuf,
    writable: bool,
    trash: bool,
}

impl Volume for Shelf {
    fn name(&self) -> &str {
        "shelf"
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(cmdr_fs::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move { Err(VolumeError::NotFound(path.display().to_string())) })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move { Err(VolumeError::NotFound(path.display().to_string())) })
    }

    fn is_writable(&self) -> bool {
        self.writable
    }

    fn has_trash(&self) -> bool {
        self.trash
    }
}

fn shelf(writable: bool, trash: bool) -> Shelf {
    Shelf {
        root: PathBuf::from("/"),
        writable,
        trash,
    }
}

#[test]
fn removal_is_offered_only_where_the_volume_takes_writes_and_named_by_its_trash() {
    let removal = |way, volume: &Shelf| action_caps(way, volume, "/a/b.txt", None).removal;
    assert_eq!(removal(Way::Local, &shelf(true, true)), Some(RemoveBy::Trash));
    assert_eq!(
        removal(Way::OsMountedShare, &shelf(true, false)),
        Some(RemoveBy::Delete)
    );
    assert_eq!(removal(Way::Device, &shelf(true, false)), Some(RemoveBy::Delete));
    assert_eq!(removal(Way::Server, &shelf(false, false)), None);
    // A view is never removed from the card, however writable what's under it is.
    for way in [Way::Archive, Way::GitHistory, Way::Snapshot, Way::Offline] {
        assert_eq!(
            action_caps(way, &shelf(true, true), "/a/b.txt", None),
            ActionCaps::none()
        );
    }
}

#[test]
fn only_a_path_the_os_sees_can_be_shown_in_its_file_manager() {
    for (way, shows) in [
        (Way::Local, true),
        (Way::OsMountedShare, true),
        (Way::Server, false),
        (Way::Device, false),
        (Way::Archive, false),
        (Way::GitHistory, false),
    ] {
        assert_eq!(
            action_caps(way, &shelf(true, true), "/a/b.txt", None).file_manager,
            shows,
            "{way:?}"
        );
    }
}

#[test]
fn the_app_folder_comes_from_the_home_it_is_given_and_only_on_a_local_disk() {
    let inside = "/fixture/home/Library/Application Support/com.example.tool/state.json";
    let local = action_caps(Way::Local, &shelf(true, true), inside, Some("/fixture/home"));
    assert_eq!(
        local.app_folder.as_deref(),
        Some("/fixture/home/Library/Application Support/com.example.tool")
    );
    let share = action_caps(Way::OsMountedShare, &shelf(true, true), inside, Some("/fixture/home"));
    assert_eq!(share.app_folder, None);
}
