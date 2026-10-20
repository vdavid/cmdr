//! A folder's DELETE against another client writing into it at the worst
//! moment (`DETAILS.md` § "A folder's DELETE"). The fake server lands the
//! other client's file right after a scripted request, which no Docker server
//! can be timed into; the real servers' half is
//! [`assert_delete_moves_aside_on_a_real_server`], run from
//! `integration_test.rs` (Apache) and `nextcloud_test.rs`.

use std::path::{Path, PathBuf};

use cmdr_fs::volume::{Volume, VolumeError};

use super::WebdavVolume;
use super::fake_dav::FakeDav;
use super::mutation::{ENOTEMPTY, still_holds};

/// Each branch a real server's answers decide, in a scratch `dir` the caller
/// cleans: the empty folder goes with no aside left, a held one comes back, one
/// whose name was taken while aside is kept beside it, and a listing tidies an
/// empty leftover but nothing else.
///
/// The race itself can't be timed on a real server, so the held and taken
/// branches start from where the race would leave them: `delete_collection`
/// skips the first count, and `put_back` starts from a folder already aside.
pub(super) async fn assert_delete_moves_aside_on_a_real_server(volume: &WebdavVolume, dir: &Path) {
    let client = volume.clone_client().await.expect("a live client");
    let remote = |path: &Path| volume.to_remote_path(path).expect("a path on this volume");
    let names = || async {
        let listed = volume.list_directory(dir, None).await.expect("the scratch listing");
        listed.into_iter().map(|entry| entry.name).collect::<Vec<_>>()
    };

    let gone = dir.join("gone");
    volume.create_directory(&gone).await.expect("mkcol gone");
    volume.delete(&gone).await.expect("an empty folder deletes");
    assert!(!volume.exists(&gone).await);
    assert_eq!(names().await, Vec::<String>::new(), "and nothing is left beside it");

    let held = dir.join("held");
    volume.create_directory(&held).await.expect("mkcol held");
    volume
        .create_file(&held.join("keep.txt"), b"keep")
        .await
        .expect("put keep.txt");
    let refused = volume
        .delete_collection(std::sync::Arc::clone(&client), &remote(&held))
        .await;
    assert!(is_not_empty(&refused), "a held folder is refused, got {refused:?}");
    assert!(volume.exists(&held.join("keep.txt")).await, "and moved back whole");
    assert_eq!(names().await, vec!["held"]);

    let taken = dir.join("taken");
    let aside = dir.join(cmdr_fs::staging::deleting_sibling_name("taken"));
    volume.create_directory(&aside).await.expect("mkcol the aside");
    volume
        .create_file(&aside.join("inside.txt"), b"in")
        .await
        .expect("put inside.txt");
    volume.create_directory(&taken).await.expect("mkcol taken");
    let why = volume
        .put_back(&client, &remote(&aside), &remote(&taken), still_holds("taken", 1))
        .await;
    assert!(
        is_not_empty(&Err(why.clone())),
        "the errno survives the note, got {why:?}"
    );
    assert!(
        volume
            .exists(&dir.join("taken (kept by Cmdr)").join("inside.txt"))
            .await
    );
    assert!(!volume.exists(&aside).await, "and the aside is gone");

    let empty = dir.join(cmdr_fs::staging::deleting_sibling_name("old"));
    let full = dir.join(cmdr_fs::staging::deleting_sibling_name("full"));
    volume.create_directory(&empty).await.expect("mkcol an empty leftover");
    volume.create_directory(&full).await.expect("mkcol a held leftover");
    volume.create_file(&full.join("f.txt"), b"f").await.expect("put f.txt");
    let listed = names().await;
    assert!(
        !volume.exists(&empty).await,
        "the empty leftover is tidied, listed {listed:?}"
    );
    assert!(volume.exists(&full.join("f.txt")).await, "the held one stays");
}

fn at(volume: &WebdavVolume, rel: &str) -> PathBuf {
    volume.root().join("tree").join(rel)
}

fn is_not_empty(result: &Result<(), VolumeError>) -> bool {
    matches!(
        result,
        Err(VolumeError::IoError {
            raw_os_error: Some(code),
            ..
        }) if *code == ENOTEMPTY
    )
}

/// ❗ The bug this design closes: a file landing between the emptiness check
/// and the DELETE went with the folder, and the delete reported success.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_landing_after_the_check_is_kept_and_the_delete_refused() {
    let server = FakeDav::start().await;
    let (volume, _) = server.connected().await;
    server.put_node("box", true);
    server.after("PROPFIND", "box", &[("box/late.txt", false)]);

    let deleted = volume.delete(&at(&volume, "box")).await;

    assert!(
        server.nodes().contains(&"box/late.txt".to_string()),
        "the other client's file is still where it put it, got {:?}",
        server.nodes()
    );
    assert!(
        is_not_empty(&deleted),
        "and the delete says why it stopped, got {deleted:?}"
    );
    assert_eq!(
        server.nodes(),
        vec!["box/", "box/late.txt"],
        "with nothing else left behind"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_empty_folder_is_deleted_and_leaves_nothing_behind() {
    let server = FakeDav::start().await;
    let (volume, _) = server.connected().await;
    server.put_node("box", true);
    server.put_node("neighbor.txt", false);

    volume
        .delete(&at(&volume, "box"))
        .await
        .expect("an empty folder deletes");

    assert_eq!(server.nodes(), vec!["neighbor.txt"]);
}

/// The folder's name was taken again while it was aside, so it can't go back:
/// it stays beside under a name a person can read, and nothing is lost.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_whose_name_was_taken_meanwhile_is_kept_beside_it() {
    let server = FakeDav::start().await;
    let (volume, _) = server.connected().await;
    server.put_node("box", true);
    server.after("PROPFIND", "box", &[("box/late.txt", false)]);
    server.after("MOVE", "box", &[("box", true)]);

    let deleted = volume.delete(&at(&volume, "box")).await;

    assert!(is_not_empty(&deleted), "got {deleted:?}");
    assert_eq!(
        server.nodes(),
        vec!["box/", "box (kept by Cmdr)/", "box (kept by Cmdr)/late.txt"]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_kept_name_already_taken_gets_a_number() {
    let server = FakeDav::start().await;
    let (volume, _) = server.connected().await;
    server.put_node("box", true);
    server.put_node("box (kept by Cmdr)", true);
    server.after("PROPFIND", "box", &[("box/late.txt", false)]);
    server.after("MOVE", "box", &[("box", true)]);

    let deleted = volume.delete(&at(&volume, "box")).await;

    assert!(is_not_empty(&deleted), "got {deleted:?}");
    assert_eq!(
        server.nodes(),
        vec![
            "box/",
            "box (kept by Cmdr 2)/",
            "box (kept by Cmdr 2)/late.txt",
            "box (kept by Cmdr)/"
        ]
    );
}

/// A server that won't rename the folder still deletes it, the old way.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_that_refuses_the_move_falls_back_to_a_plain_delete() {
    for status in ["405 Method Not Allowed", "403 Forbidden", "501 Not Implemented"] {
        let server = FakeDav::start().await;
        let (volume, _) = server.connected().await;
        server.put_node("box", true);
        server.put_node("note.txt", false);
        server.refuse_moves(status);

        volume
            .delete(&at(&volume, "box"))
            .await
            .unwrap_or_else(|e| panic!("a {status} on the MOVE falls back, got {e:?}"));
        volume
            .delete(&at(&volume, "note.txt"))
            .await
            .expect("a file never moves");

        assert_eq!(server.nodes(), Vec::<String>::new(), "after a {status}");
    }
}

/// A crash between the steps leaves an EMPTY aside at worst; the next listing
/// of its parent tidies it. One with something in it, or a name that only looks
/// like ours, stays.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_listing_tidies_an_empty_leftover_aside_and_nothing_else() {
    let server = FakeDav::start().await;
    let (volume, _) = server.connected().await;
    let empty = format!("old.cmdr-deleting-{}", uuid::Uuid::new_v4());
    let held = format!("full.cmdr-deleting-{}", uuid::Uuid::new_v4());
    server.put_node(&empty, true);
    server.put_node(&held, true);
    server.put_node(&format!("{held}/keep.txt"), false);
    server.put_node("mine.cmdr-deleting-notes", true);

    let listed: Vec<String> = volume
        .list_directory(&at(&volume, ""), None)
        .await
        .expect("the listing")
        .into_iter()
        .map(|entry| entry.name)
        .collect();

    assert!(
        !listed.contains(&empty),
        "the empty leftover isn't listed, got {listed:?}"
    );
    assert!(listed.contains(&held) && listed.contains(&"mine.cmdr-deleting-notes".to_string()));
    assert!(
        !server.nodes().contains(&format!("{empty}/")),
        "and it's gone from the server"
    );
    assert!(server.nodes().contains(&format!("{held}/keep.txt")));
}
