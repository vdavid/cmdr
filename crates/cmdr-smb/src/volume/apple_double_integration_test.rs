//! A delete taking its AppleDouble sidecar along, against the guest fixture
//! playing a QNAP: visible `._` files on a share that claims named streams.
//! (`smb-consumer-fruit` vetoes `._` names, `fruit:veto_appledouble`'s default,
//! so it can't hold one.) What it pins and why: `DETAILS.md` § "AppleDouble
//! sidecars". Docker-gated, like every `smb_integration_*` test.

use super::test_support::*;
use super::*;
use cmdr_fs::volume::Volume;

/// A small AppleDouble file: the magic, version 2, filler, no entries.
fn apple_double_bytes() -> Vec<u8> {
    let mut bytes = vec![0x00, 0x05, 0x16, 0x07, 0x00, 0x02, 0x00, 0x00];
    bytes.extend_from_slice(&[0u8; 18]);
    bytes
}

/// The guest share, claiming streams the way a QNAP's does.
async fn qnap_like_volume() -> SmbVolume {
    let volume = make_docker_volume().await;
    volume.inner.stores_xattrs.store(true, Ordering::Relaxed);
    volume
}

async fn seeded(volume: &SmbVolume, files: &[(&str, &[u8])]) -> String {
    let base = test_dir_name();
    ensure_clean(volume, &base).await;
    volume.create_directory(Path::new(&base)).await.unwrap();
    for (name, bytes) in files {
        volume
            .create_file(Path::new(&format!("{base}/{name}")), bytes)
            .await
            .unwrap();
    }
    base
}

fn exists_at(base: &str, name: &str) -> PathBuf {
    PathBuf::from(format!("{base}/{name}"))
}

#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_delete_takes_its_apple_double_sidecar_and_keeps_a_users_own() {
    let volume = qnap_like_volume().await;
    let fork = apple_double_bytes();
    let base = seeded(
        &volume,
        &[
            ("photo.jpg", b"jpeg"),
            ("._photo.jpg", &fork),
            ("notes", b"text"),
            ("._notes", b"my own file, not AppleDouble"),
        ],
    )
    .await;
    // The pane's listing is what shows the share keeps sidecars.
    volume.list_directory(Path::new(&base), None).await.unwrap();

    volume.delete(&exists_at(&base, "photo.jpg")).await.unwrap();
    volume.delete(&exists_at(&base, "notes")).await.unwrap();

    assert!(
        !volume.exists(&exists_at(&base, "._photo.jpg")).await,
        "the sidecar went"
    );
    assert!(
        volume.exists(&exists_at(&base, "._notes")).await,
        "a `._` file that isn't AppleDouble stays"
    );
    ensure_clean(&volume, &base).await;
}

/// A folder's `._Folder` goes with it, and a large sidecar (a resource fork
/// past the probe's one-frame read) is recognized by its head.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_folder_and_a_large_fork_take_their_sidecars() {
    let volume = qnap_like_volume().await;
    let mut large = apple_double_bytes();
    large.resize(300 * 1024, 0xAB);
    let base = seeded(
        &volume,
        &[
            ("._Folder", &apple_double_bytes()),
            ("big.psd", b"psd"),
            ("._big.psd", &large),
        ],
    )
    .await;
    volume.create_directory(&exists_at(&base, "Folder")).await.unwrap();
    volume.list_directory(Path::new(&base), None).await.unwrap();

    volume.delete(&exists_at(&base, "Folder")).await.unwrap();
    volume.delete(&exists_at(&base, "big.psd")).await.unwrap();

    assert!(!volume.exists(&exists_at(&base, "._Folder")).await);
    assert!(!volume.exists(&exists_at(&base, "._big.psd")).await);
    ensure_clean(&volume, &base).await;
}

/// A walk that listed `._a` beside `a` deletes `a` first, and its own delete of
/// `._a` then finds it gone: done, ❌ never a failed delete.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_walks_later_delete_of_a_taken_sidecar_is_done() {
    let volume = qnap_like_volume().await;
    let base = seeded(&volume, &[("a.txt", b"a"), ("._a.txt", &apple_double_bytes())]).await;
    volume.list_directory(Path::new(&base), None).await.unwrap();

    volume.delete(&exists_at(&base, "a.txt")).await.unwrap();
    volume
        .delete(&exists_at(&base, "._a.txt"))
        .await
        .expect("the sidecar the first delete took reads as deleted");
    assert!(
        matches!(
            volume.delete(&exists_at(&base, "._a.txt")).await,
            Err(VolumeError::NotFound(_))
        ),
        "only once: a third delete is an ordinary missing file"
    );
    volume.delete(Path::new(&base)).await.expect("the folder is empty");
}

/// A share without named streams keeps its `._` files: they're Finder's own
/// AppleDouble there, which a move off the share can't carry.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_share_without_streams_keeps_its_sidecars() {
    let volume = make_docker_volume().await;
    assert!(!volume.stores_xattrs(), "the guest fixture has no streams");
    let base = seeded(&volume, &[("a.txt", b"a"), ("._a.txt", &apple_double_bytes())]).await;
    volume.list_directory(Path::new(&base), None).await.unwrap();

    volume.delete(&exists_at(&base, "a.txt")).await.unwrap();

    assert!(volume.exists(&exists_at(&base, "._a.txt")).await);
    ensure_clean(&volume, &base).await;
}

/// What it costs: nothing on a share that hasn't shown a sidecar, one
/// CREATE+READ+CLOSE frame more on one that has.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_delete_probes_only_on_a_share_that_shows_sidecars() {
    let volume = make_counting_volume().await;
    volume.inner.stores_xattrs.store(true, Ordering::Relaxed);
    let base = seeded(&volume, &[("first.txt", b"1"), ("second.txt", b"2")]).await;

    let delete_cost = async |name: &str| {
        let before = request_counts(&volume).await;
        volume.delete(&exists_at(&base, name)).await.unwrap();
        let after = request_counts(&volume).await;
        (after.1 - before.1, after.0 - before.0)
    };
    assert_eq!(
        delete_cost("first.txt").await,
        (1, 3),
        "no sidecar seen: the delete alone"
    );
    volume.inner.sidecars.note_name("._elsewhere");
    assert_eq!(
        delete_cost("second.txt").await,
        (2, 6),
        "a share with sidecars: the delete, then one probe frame"
    );
    ensure_clean(&volume, &base).await;
}
