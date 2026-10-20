//! Links on a share, against a real server: a Unix symlink Samba serves, made
//! inside the guest fixture container, since no SMB client can make one there.
//!
//! The fixture's Samba (4.23.8 on `alpine:3.24`) honors `FILE_OPEN_REPARSE_POINT`,
//! so a `stat` reports the link and a delete removes the link alone. Its
//! listings never mark one, which is the gap `DETAILS.md` § "Junctions and
//! symlinks" records. A Windows junction has no fixture; its half is pinned by
//! the mapping cells in `mapping_test.rs`.
//!
//! Every test here is `#[ignore]`d so default runs skip it. Start the containers
//! with `apps/desktop/test/smb-servers/start.sh`, then run
//! `cargo nextest run -E 'package(cmdr-smb)' --run-ignored only`.

use super::test_support::*;
use super::*;
use cmdr_fs::volume::EntryKind;

/// `<dir>/target/keep.txt` written through the share, then `<dir>/dirlink ->
/// target` and `<dir>/filelink -> target/keep.txt` made on the server's disk.
async fn seed_links(vol: &SmbVolume, dir: &str) {
    vol.create_directory(Path::new(dir)).await.unwrap();
    vol.create_directory(Path::new(&format!("{dir}/target"))).await.unwrap();
    vol.create_file(Path::new(&format!("{dir}/target/keep.txt")), b"keep")
        .await
        .unwrap();
    let on_disk = format!("/shares/public/{dir}");
    run_in_the_guest_container(&["ln", "-s", "target", &format!("{on_disk}/dirlink")]);
    run_in_the_guest_container(&["ln", "-s", "target/keep.txt", &format!("{on_disk}/filelink")]);
}

/// Deleting a link removes the link and leaves what it points to (#516). Before
/// smb2 0.34 a delete opened the link's target, so deleting `filelink` deleted
/// `keep.txt`.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_deleting_a_symlink_leaves_its_target() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    seed_links(&vol, &dir).await;

    for link in ["dirlink", "filelink"] {
        let path = format!("{dir}/{link}");
        let entry = vol.get_metadata(Path::new(&path)).await.unwrap();
        assert!(entry.is_symlink, "a stat reports {link} as a link");
        assert_eq!(vol.entry_kind(Path::new(&path)).await.unwrap(), EntryKind::Symlink);

        vol.delete(Path::new(&path)).await.unwrap();
        assert!(!vol.exists(Path::new(&path)).await, "{link} is gone");
    }

    let kept = vol
        .get_metadata(Path::new(&format!("{dir}/target/keep.txt")))
        .await
        .expect("the links' target stays");
    assert_eq!(kept.size, Some(4), "the links' target stays whole");

    ensure_clean(&vol, &dir).await;
}

/// Samba lists a symlink as what it points to, with no reparse bit, so a walk
/// can't tell it from a folder by its listing. If this starts failing, Samba
/// marks links in listings now: update `DETAILS.md` § "Junctions and symlinks",
/// whose open gap this is.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_samba_lists_a_symlink_as_its_target() {
    let vol = make_docker_volume().await;
    let dir = test_dir_name();
    ensure_clean(&vol, &dir).await;
    seed_links(&vol, &dir).await;

    let entries = vol.list_directory(Path::new(&dir), None).await.unwrap();
    let listed = |name: &str| entries.iter().find(|e| e.name == name).expect("listed");
    assert!(listed("dirlink").is_directory && !listed("dirlink").is_symlink);
    assert!(!listed("filelink").is_directory && !listed("filelink").is_symlink);

    for link in ["dirlink", "filelink"] {
        vol.delete(Path::new(&format!("{dir}/{link}"))).await.unwrap();
    }
    ensure_clean(&vol, &dir).await;
}
