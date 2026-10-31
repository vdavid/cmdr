//! Finder tags end to end through the app's own route, on a share with NO mount under it:
//! a real `SmbVolume` in the registry, a real listing, and `tags_io` between them.
//!
//! Against `smb-consumer-fruit` by default, like every `smb_integration_*` cell. To point it
//! at a real NAS, set `CMDR_TAGS_NAS_HOST`, `_SHARE`, `_USER`, and `_PASSWORD`; it works in a
//! scratch folder it removes. The check a person's Finder makes on what it wrote goes through
//! a fresh mount, by hand: `docs/notes/smb-finder-tags-2026-10-31.md`.

use super::tags_io::{clear_tags, toggle_color};
use cmdr_fs::entry::TagRef;
use cmdr_fs::volume::Volume;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_listing::testing::TestListing;
use cmdr_smb::volume::{MountAnchor, SmbConnectionParams, SmbVolume, connect_smb_volume};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Where no mount is, so nothing can quietly answer through one.
const NO_MOUNT: &str = "/Volumes/cmdr-tags-no-such-mount";
const TAGS: &str = "com.apple.metadata:_kMDItemUserTags";
const EMPTY_PLIST: &str = "62706c6973743030a0080000000000000101000000000000000100000000000000000000000000000009";

async fn connect() -> (String, Arc<SmbVolume>) {
    let var = |name: &str| std::env::var(format!("CMDR_TAGS_NAS_{name}")).ok();
    let port = if var("HOST").is_some() {
        445
    } else {
        smb2::testing::fruit_port()
    };
    let host = var("HOST").unwrap_or_else(|| "127.0.0.1".to_string());
    let share = var("SHARE").unwrap_or_else(|| "public".to_string());
    let (user, password) = (var("USER"), var("PASSWORD"));
    let params = SmbConnectionParams::new(&host, &share, port, user.as_deref(), password.as_deref());
    let id = cmdr_fs::volume::smb_volume_id(&host, port, &share);
    let anchor = MountAnchor::at_share_root(NO_MOUNT);
    let volume = connect_smb_volume(&share, anchor, &id, params, VolumeHost::detached())
        .await
        .expect("connect");
    (id, Arc::new(volume))
}

fn tag(name: &str, color: u8) -> TagRef {
    TagRef {
        name: name.to_string(),
        color,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_finder_tags_work_with_no_mount() {
    let (id, volume) = connect().await;
    assert!(!Path::new(NO_MOUNT).exists(), "the test needs a path no mount serves");
    volume.note_root_mount_gone();
    assert!(!volume.paths_are_os_visible());
    let manager = crate::file_system::volume::manager::get_volume_manager();
    manager.register(&id, volume.clone());

    let folder = PathBuf::from(NO_MOUNT).join(format!("_cmdr-tags-test-{}", std::process::id()));
    volume.create_directory(&folder).await.unwrap();
    let path_of = |name: &str| folder.join(name).to_string_lossy().into_owned();
    for name in ["red.txt", "plain.txt"] {
        volume.create_file(&folder.join(name), b"x").await.unwrap();
    }

    let entries = volume.list_directory(&folder, None).await.unwrap();
    let listings = crate::listing_host::listings();
    let listing = TestListing::new()
        .volume(&id)
        .path(folder.to_str().unwrap())
        .entries(entries)
        .insert(listings, "smb-tags-no-mount");
    let row_tags = |name: &str| -> Vec<TagRef> {
        let entries = listing.entries();
        entries
            .iter()
            .find(|entry| entry.name == name)
            .expect("a row")
            .tags
            .clone()
    };

    // Toggling a color lands, and the sweep's read path shows it in the listing.
    let outcome = toggle_color(Some(&id), vec![path_of("red.txt")], 6).await.unwrap();
    assert_eq!(outcome.updates, vec![(path_of("red.txt"), vec![tag("Red", 6)])]);
    let answer =
        super::tags_io::enrich_listing(listing.id().to_string(), vec![path_of("red.txt"), path_of("plain.txt")]).await;
    assert!(!answer.timed_out);
    assert_eq!(row_tags("red.txt"), vec![tag("Red", 6)]);
    assert_eq!(row_tags("plain.txt"), Vec::new());

    // Toggling it off, and clearing, leave macOS's empty array where a tag was, and no
    // attribute where none was.
    toggle_color(Some(&id), vec![path_of("red.txt")], 6).await.unwrap();
    clear_tags(Some(&id), vec![path_of("plain.txt")]).await.unwrap();
    let paths = [folder.join("red.txt"), folder.join("plain.txt")];
    let raw = volume
        .read_xattr_many(&paths, TAGS, cmdr_fs::volume::XattrReadMode::Verified)
        .await;
    let as_hex = |bytes: &[u8]| bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    assert_eq!(
        raw[0].as_ref().unwrap().as_deref().map(as_hex).as_deref(),
        Some(EMPTY_PLIST)
    );
    assert_eq!(raw[1].as_ref().unwrap(), &None);

    drop(listing);
    manager.unregister(&id);
    for name in ["red.txt", "plain.txt"] {
        volume.delete(&folder.join(name)).await.unwrap();
    }
    volume.delete(&folder).await.unwrap();
}

/// The by-hand half on a real NAS, for `crates/cmdr-smb/examples/tag_sweep_bench.rs`'s seeded
/// folder (`CMDR_TAGS_NAS_SCRATCH`): sweeps it into a listing with no mount, tags one file blue
/// and takes the red off another, and leaves both for a fresh mount to read.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires a NAS and a seeded scratch folder"]
async fn nas_a_seeded_folder_sweeps_and_toggles_with_no_mount() {
    let (id, volume) = connect().await;
    let scratch = std::env::var("CMDR_TAGS_NAS_SCRATCH").expect("CMDR_TAGS_NAS_SCRATCH");
    assert!(!Path::new("/Volumes").join(&scratch).exists());
    volume.note_root_mount_gone();
    let manager = crate::file_system::volume::manager::get_volume_manager();
    manager.register(&id, volume.clone());

    let folder = PathBuf::from(NO_MOUNT).join(&scratch);
    let entries = volume.list_directory(&folder, None).await.unwrap();
    let paths: Vec<String> = entries.iter().map(|entry| entry.path.clone()).collect();
    let listing = TestListing::new()
        .volume(&id)
        .path(folder.to_str().unwrap())
        .entries(entries)
        .insert(crate::listing_host::listings(), "nas-tags-no-mount");

    let answer = super::tags_io::enrich_listing(listing.id().to_string(), paths).await;
    assert!(!answer.timed_out);
    let shown = listing.entries();
    let red = shown.iter().filter(|entry| entry.tags == vec![tag("Red", 6)]).count();
    assert!(red > 50, "the seed's tagged rows show, got {red} of {}", shown.len());

    let path_of = |name: &str| folder.join(name).to_string_lossy().into_owned();
    toggle_color(Some(&id), vec![path_of("file-00101.txt")], 4)
        .await
        .unwrap();
    toggle_color(Some(&id), vec![path_of("file-00120.txt")], 6)
        .await
        .unwrap();
    let blue = listing_row(&volume, &folder, "file-00101.txt").await;
    assert_eq!(blue, vec![tag("Blue", 4)]);
    manager.unregister(&id);
}

async fn listing_row(volume: &SmbVolume, folder: &Path, name: &str) -> Vec<TagRef> {
    let paths = [folder.join(name)];
    let mut answers = volume
        .read_xattr_many(&paths, TAGS, cmdr_fs::volume::XattrReadMode::Verified)
        .await;
    let answer = answers.pop().expect("one answer per path");
    let bytes = answer.expect("the read answers").unwrap_or_default();
    super::tags::parse_tags_plist(&bytes)
}
