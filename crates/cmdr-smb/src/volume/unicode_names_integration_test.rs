//! Names whose Unicode form differs from their neighbors', against a real server.
//!
//! An SMB name is an opaque byte string: the server folds neither Unicode form
//! nor case, so a lookup succeeds only on the exact bytes it stored. One
//! directory can hold its own name composed (NFC) and its children's names
//! decomposed (NFD), which is what a Google Takeout export left on David's NAS
//! (ERR-VETBX). Samba in the fixture container stores names as the bytes it's
//! sent, so these cells seed both forms through a raw smb2 session, which puts
//! them on disk exactly as spelled.
//!
//! Every path an operation below is handed came out of a LISTING, the way a pane
//! hands one over: that's the path a user can see and act on, and it must reach
//! the wire unchanged. The watcher cell pins the other direction, that an outside
//! change reports the directory under the same spelling the pane keyed it on.
//!
//! Every test here is `#[ignore]`d so default runs skip it. Start the containers
//! with `apps/desktop/test/smb-servers/start.sh`, then run
//! `cargo nextest run -E 'package(cmdr-smb)' --run-ignored only`.

use super::test_support::*;
use super::*;
use cmdr_fs::volume::DirectoryChange;
use cmdr_fs::volume::InMemoryVolume;
use cmdr_fs::volume::host::listings::{ListingHost, RecordingListings};
use cmdr_fs::volume::{StreamLength, StreamWriteProgress, WriteMode};

/// `fotók`, composed: the album directory, spelled the way the NAS stores it.
const NFC_DIR: &str = "fot\u{f3}k";
/// `retusált.jpg`, decomposed (`a` + U+0301): a photo inside that album.
const NFD_FILE: &str = "retusa\u{301}lt.jpg";
/// `vázlatok`, decomposed: a subdirectory whose own name is NFD.
const NFD_SUBDIR: &str = "va\u{301}zlatok";
/// What the NFD file holds, so a read that reached the wrong file (or none) shows.
const PAYLOAD: &[u8] = b"the bytes of a photo nobody could open";

/// Creates `{parent}/{name}` on the share with the exact bytes of `name`,
/// bypassing every path conversion Cmdr does.
async fn seed_dir_raw(vol: &SmbVolume, share_path: &str) {
    let (tree, mut conn) = vol.clone_session().await.expect("session");
    tree.create_directory(&mut conn, share_path)
        .await
        .unwrap_or_else(|e| panic!("seeding directory {share_path:?}: {e}"));
}

/// Writes a file at `share_path` with the exact bytes given, bypassing Cmdr.
async fn seed_file_raw(vol: &SmbVolume, share_path: &str, data: &[u8]) {
    let (tree, mut conn) = vol.clone_session().await.expect("session");
    tree.write_file(&mut conn, share_path, data)
        .await
        .unwrap_or_else(|e| panic!("seeding file {share_path:?}: {e}"));
}

/// The entry named exactly `name` (byte-for-byte) in `dir`'s listing.
async fn listed(vol: &SmbVolume, dir: &Path, name: &str) -> FileEntry {
    let entries = vol
        .list_directory_impl(dir)
        .await
        .expect("listing the seeded directory");
    entries.into_iter().find(|e| e.name == name).unwrap_or_else(|| {
        panic!(
            "{name:?} ({:x?}) must be listed in {} with the server's own bytes",
            name.as_bytes(),
            dir.display()
        )
    })
}

/// A seeded album: a unique top directory holding the NFC `fotók`, which holds
/// the NFD `retusált.jpg` and the NFD `vázlatok/`. Answers the top directory's
/// share-relative name and the album's path AS ITS LISTING SPELLS IT, which is
/// the path a pane would have entered.
async fn seed_album(vol: &SmbVolume) -> (String, PathBuf) {
    let top = test_dir_name();
    ensure_clean(vol, &top).await;
    vol.create_directory(Path::new(&top)).await.unwrap();
    seed_dir_raw(vol, &format!("{top}/{NFC_DIR}")).await;
    seed_file_raw(vol, &format!("{top}/{NFC_DIR}/{NFD_FILE}"), PAYLOAD).await;
    seed_dir_raw(vol, &format!("{top}/{NFC_DIR}/{NFD_SUBDIR}")).await;

    let album = listed(vol, Path::new(&share_path(&top)), NFC_DIR).await;
    (top, PathBuf::from(album.path))
}

/// Removes everything a cell seeded, through the raw session so a regression in
/// Cmdr's own path handling can't leave NFD debris behind on the fixture share.
async fn remove_album(vol: &SmbVolume, top: &str) {
    let (tree, mut conn) = vol.clone_session().await.expect("session");
    let album = format!("{top}/{NFC_DIR}");
    if let Ok(entries) = tree.list_directory(&mut conn, &album).await {
        for entry in entries.iter().filter(|e| e.name != "." && e.name != "..") {
            let child = format!("{album}/{}", entry.name);
            let _ = match entry.is_directory {
                true => tree.delete_directory(&mut conn, &child).await,
                false => tree.delete_file(&mut conn, &child).await,
            };
        }
    }
    let _ = tree.delete_directory(&mut conn, &album).await;
    ensure_clean(vol, top).await;
}

/// Reads a whole file through `open_read_stream`.
async fn read_all(vol: &SmbVolume, path: &Path) -> Result<Vec<u8>, VolumeError> {
    let mut stream = vol.open_read_stream(path).await?;
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        bytes.extend_from_slice(&chunk?);
    }
    Ok(bytes)
}

// ── Server-derived paths reach the wire as the server spelled them ─────────────

/// The listing half already worked before paths were made byte-faithful, and it
/// has to keep handing back the server's bytes: every other cell here builds on
/// what it returns.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn a_listing_hands_back_each_name_in_the_servers_own_form() {
    let vol = make_docker_volume().await;
    let (top, album) = seed_album(&vol).await;

    let file = listed(&vol, &album, NFD_FILE).await;
    assert_eq!(file.size, Some(PAYLOAD.len() as u64));
    assert_eq!(file.path, album.join(NFD_FILE).to_string_lossy());
    listed(&vol, &album, NFD_SUBDIR).await;

    remove_album(&vol, &top).await;
}

/// ERR-VETBX itself: the file is listed, sized, and then its read is refused
/// with `STATUS_OBJECT_NAME_NOT_FOUND` because its name went out composed.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn a_decomposed_file_in_a_composed_directory_reads() {
    let vol = make_docker_volume().await;
    let (top, album) = seed_album(&vol).await;
    let file = PathBuf::from(listed(&vol, &album, NFD_FILE).await.path);

    let metadata = vol.get_metadata(&file).await;
    let plain = read_all(&vol, &file).await;
    // The hinted read is the copy pipeline's: one compound frame for a small file.
    let hinted = async {
        let mut stream = vol
            .open_read_stream_with_hint(&file, Some(PAYLOAD.len() as u64))
            .await?;
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            bytes.extend_from_slice(&chunk?);
        }
        Ok::<_, VolumeError>(bytes)
    }
    .await;
    remove_album(&vol, &top).await;

    assert_eq!(
        metadata.expect("metadata of a listed file").size,
        Some(PAYLOAD.len() as u64)
    );
    assert_eq!(plain.expect("a listed file must read"), PAYLOAD);
    assert_eq!(
        hinted.expect("a listed file must read through the hinted path"),
        PAYLOAD
    );
}

/// A copy scans the source, streams it out, and writes it under the SAME name
/// elsewhere: all three halves have to carry the name as the server spelled it.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn a_decomposed_file_copies_off_the_share_and_within_it() {
    let vol = make_docker_volume().await;
    let (top, album) = seed_album(&vol).await;
    let file = PathBuf::from(listed(&vol, &album, NFD_FILE).await.path);
    let subdir = PathBuf::from(listed(&vol, &album, NFD_SUBDIR).await.path);

    let scan = vol.scan_for_copy(&file).await;

    // Off the share, the way a copy to a local disk streams it.
    let elsewhere = InMemoryVolume::new("Elsewhere");
    let no_progress = &|_: StreamWriteProgress| std::ops::ControlFlow::Continue(());
    let exported = async {
        let stream = vol.open_read_stream(&file).await?;
        let dest = PathBuf::from("/").join(NFD_FILE);
        elsewhere
            .write_from_stream(
                &dest,
                WriteMode::CreateOrReplace,
                StreamLength::Known(PAYLOAD.len() as u64),
                stream,
                no_progress,
            )
            .await?;
        read_all_from(&elsewhere, &dest).await
    }
    .await;

    // Within the share, into the NFD subdirectory under the source's own name.
    let copied_within = async {
        let stream = vol.open_read_stream(&file).await?;
        let dest = subdir.join(NFD_FILE);
        vol.write_from_stream(
            &dest,
            WriteMode::CreateOrReplace,
            StreamLength::Known(PAYLOAD.len() as u64),
            stream,
            no_progress,
        )
        .await?;
        read_all(&vol, &dest).await
    }
    .await;
    let landed_as = vol
        .list_directory_impl(&subdir)
        .await
        .map(|entries| entries.into_iter().map(|e| e.name).collect::<Vec<_>>());
    remove_album(&vol, &top).await;

    let scan = scan.expect("a listed file must scan");
    assert_eq!((scan.file_count, scan.total_bytes), (1, PAYLOAD.len() as u64));
    assert_eq!(exported.expect("a listed file must copy off the share"), PAYLOAD);
    assert_eq!(
        copied_within.expect("a listed file must copy within the share"),
        PAYLOAD
    );
    assert_eq!(
        landed_as.expect("the destination must list"),
        vec![NFD_FILE.to_string()],
        "the copy lands under the source's own bytes, not a recomposed twin"
    );
}

/// Reads a whole file off any volume.
async fn read_all_from(volume: &dyn Volume, path: &Path) -> Result<Vec<u8>, VolumeError> {
    let mut stream = volume.open_read_stream(path).await?;
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        bytes.extend_from_slice(&chunk?);
    }
    Ok(bytes)
}

/// A rename addresses the source by its listed path; getting it wrong is a
/// refusal at best, and on a share holding both spellings, the wrong file.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn a_decomposed_file_renames() {
    let vol = make_docker_volume().await;
    let (top, album) = seed_album(&vol).await;
    let file = PathBuf::from(listed(&vol, &album, NFD_FILE).await.path);

    let renamed = vol.rename(&file, &album.join("renamed.jpg"), false).await;
    let names = vol
        .list_directory_impl(&album)
        .await
        .map(|entries| entries.into_iter().map(|e| e.name).collect::<Vec<_>>());
    remove_album(&vol, &top).await;

    renamed.expect("a listed file must rename");
    let mut names = names.expect("the album must list");
    names.sort();
    assert_eq!(names, vec!["renamed.jpg".to_string(), NFD_SUBDIR.to_string()]);
}

/// A delete addresses the file by its listed path, and must remove THAT file.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn a_decomposed_file_deletes() {
    let vol = make_docker_volume().await;
    let (top, album) = seed_album(&vol).await;
    let file = PathBuf::from(listed(&vol, &album, NFD_FILE).await.path);

    let deleted = vol.delete(&file).await;
    let names = vol
        .list_directory_impl(&album)
        .await
        .map(|entries| entries.into_iter().map(|e| e.name).collect::<Vec<_>>());
    remove_album(&vol, &top).await;

    deleted.expect("a listed file must delete");
    assert_eq!(names.expect("the album must list"), vec![NFD_SUBDIR.to_string()]);
}

/// A directory whose own name is NFD opens: entering it is a listing of its
/// listed path, and everything under it inherits that spelling.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn a_decomposed_directory_in_a_composed_one_opens() {
    let vol = make_docker_volume().await;
    let (top, album) = seed_album(&vol).await;
    let subdir = PathBuf::from(listed(&vol, &album, NFD_SUBDIR).await.path);

    let listing = vol.list_directory_impl(&subdir).await;
    let created = vol.create_file(&subdir.join("new.txt"), b"new").await;
    remove_album(&vol, &top).await;

    assert!(listing.expect("a listed directory must open").is_empty());
    created.expect("a file must be creatable inside a listed directory");
}

// ── The watcher keys a change the way the pane keyed its listing ──────────────

/// A change made from OUTSIDE Cmdr reaches the listing seam naming the directory
/// in the same spelling the pane entered it with. The listing cache compares that
/// path byte-exactly (`ListingPath`), so any other spelling is a miss: the open
/// pane never learns the directory changed.
///
/// The directory is NFC, as nearly every accented directory on a real server is,
/// and the pane's path for it is the one its parent's listing handed out.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn an_outside_change_in_an_accented_directory_names_the_path_the_pane_opened() {
    let listings = Arc::new(RecordingListings::new());
    let host = VolumeHost::builder()
        .listings(Arc::clone(&listings) as Arc<dyn ListingHost>)
        .build();
    let vol = make_docker_volume_with_host(host).await;
    let (top, pane_path) = seed_album(&vol).await;

    // The watcher's session arms on its own schedule after connect, so an event
    // written before it's armed is never sent. Keep writing fresh files until one
    // is heard, and identify ours by this cell's unique name: the watch covers
    // the whole share, which other suites (and other runs of this one) are busy on.
    //
    // 20 s, though delivery takes ~0.3 s on a quiet stack: Samba's `notifyd` in the
    // CPU-capped fixture container fans every write on the share out to every
    // recursive watch on it, and with several worktrees' lanes on one stack it falls
    // seconds behind and keeps draining after they stop. A 6 s budget failed there
    // with nothing heard at all. The cap above it is in `.config/nextest.toml`.
    let ours = format!("outside-{top}-");
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut heard = None;
    let mut n = 0;
    while heard.is_none() && std::time::Instant::now() < deadline {
        n += 1;
        let name = format!("{ours}{n}.txt");
        seed_file_raw(&vol, &format!("{top}/{NFC_DIR}/{name}"), b"outside").await;
        let until = std::time::Instant::now() + Duration::from_secs(1);
        while heard.is_none() && std::time::Instant::now() < until {
            heard = listings
                .changes()
                .into_iter()
                .find_map(|(_, parent, change)| match change {
                    DirectoryChange::Added(entry) if entry.name.starts_with(&ours) => Some(parent),
                    _ => None,
                });
            // allowed-test-sleep: the poll between two looks at what the watcher heard; the outer loop re-seeds a
            // fresh file each second because an event written before the watcher armed is never sent, which a
            // panicking `wait_until_async` can't express.
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    remove_album(&vol, &top).await;

    let heard = heard.expect("the watcher must report an outside file creation within 20 s, inside nextest's 30 s cap");
    assert_eq!(
        heard.as_os_str().as_encoded_bytes(),
        pane_path.as_os_str().as_encoded_bytes(),
        "the change must name {} byte-for-byte; it named {}",
        pane_path.display(),
        heard.display()
    );
}

// ── Paths from anywhere else resolve against a listing ───────────────────────
//
// A path that didn't come from a listing (typed, restored, carried over from the
// macOS mount, which spells everything NFD) is not the server's spelling. Every
// `Volume` call means the exact bytes it's handed, so such a path misses, and
// `find_stored_spelling` is where it becomes the server's spelling: component by
// component, against real listings, refusing when two entries fit.

/// `kép.jpg`, composed: a photo whose name the server stores NFC.
const NFC_FILE: &str = "k\u{e9}p.jpg";
/// `élő.jpg`, composed, and its decomposed look-alike twin. Two accents, so a
/// third spelling (one composed, one not) exists that is neither twin.
const TWIN_NFC: &str = "\u{e9}l\u{151}.jpg";
const TWIN_NFD: &str = "e\u{301}lo\u{30b}.jpg";
const TWIN_MIXED: &str = "e\u{301}l\u{151}.jpg";

/// `path` in the form the macOS kernel mount hands out: every name decomposed.
fn decomposed(path: &str) -> PathBuf {
    use unicode_normalization::UnicodeNormalization;
    PathBuf::from(path.nfd().collect::<String>())
}

/// The spelling `vol` finds for `foreign`, which must differ from what was given.
async fn stored_spelling(vol: &SmbVolume, foreign: &Path) -> Result<PathBuf, VolumeError> {
    let stored = vol.find_stored_spelling(foreign, None).await?;
    Ok(stored.unwrap_or_else(|| panic!("{} must resolve to another spelling", foreign.display())))
}

/// Asserts a path is exactly `want`'s bytes, with a message that shows both.
fn assert_same_bytes(got: &Path, want: &str) {
    assert_eq!(
        got.as_os_str().as_encoded_bytes(),
        want.as_bytes(),
        "resolved to {got:?}, want {want:?}"
    );
}

/// A foreign all-NFD path to a directory and file the server stores NFC: the
/// case the old blanket NFC fold existed for, which must keep working.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn a_foreign_decomposed_path_reaches_a_composed_file() {
    let vol = make_docker_volume().await;
    let top = test_dir_name();
    ensure_clean(&vol, &top).await;
    vol.create_directory(Path::new(&top)).await.unwrap();
    seed_dir_raw(&vol, &format!("{top}/{NFC_DIR}")).await;
    seed_file_raw(&vol, &format!("{top}/{NFC_DIR}/{NFC_FILE}"), PAYLOAD).await;

    let exact = share_path(&format!("{top}/{NFC_DIR}/{NFC_FILE}"));
    let stored = stored_spelling(&vol, &decomposed(&exact)).await;
    let read = match &stored {
        Ok(path) => read_all(&vol, path).await,
        Err(e) => Err(e.clone()),
    };
    remove_album(&vol, &top).await;

    assert_same_bytes(
        &stored.expect("a foreign NFD path must resolve to the NFC file"),
        &exact,
    );
    assert_eq!(read.expect("the resolved path must read"), PAYLOAD);
}

/// A foreign all-NFD path to ERR-VETBX's shape, an NFC directory holding an NFD
/// file: no single normalization of the whole path spells it, so only a
/// per-component resolve reaches it.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn a_foreign_decomposed_path_reaches_a_mixed_form_file() {
    let vol = make_docker_volume().await;
    let (top, _) = seed_album(&vol).await;

    let exact = share_path(&format!("{top}/{NFC_DIR}/{NFD_FILE}"));
    let stored = stored_spelling(&vol, &decomposed(&exact)).await;
    let read = match &stored {
        Ok(path) => read_all(&vol, path).await,
        Err(e) => Err(e.clone()),
    };
    remove_album(&vol, &top).await;

    assert_same_bytes(&stored.expect("a foreign NFD path must resolve per component"), &exact);
    assert_eq!(read.expect("the resolved path must read"), PAYLOAD);
}

/// A path in another case AND another form: a Mac user types `FOTÓK` and the
/// kernel mount decomposes it. David's QNAP matches case exactly; the fixture's
/// Samba folds case itself (`case sensitive = auto`, so a case-only difference
/// opens as given), but it doesn't fold Unicode form, so the decomposed half
/// makes this miss and only a fold of BOTH case and form finds `fotók`.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn a_foreign_path_in_another_case_reaches_its_directory() {
    let vol = make_docker_volume().await;
    let (top, album) = seed_album(&vol).await;

    let shouted = share_path(&format!("{top}/FOTO\u{301}K"));
    let stored = stored_spelling(&vol, Path::new(&shouted)).await;
    let listing = match &stored {
        Ok(path) => vol.list_directory_impl(path).await,
        Err(e) => Err(e.clone()),
    };
    remove_album(&vol, &top).await;

    assert_same_bytes(
        &stored.expect("a differently cased path must resolve"),
        &album.to_string_lossy(),
    );
    let mut names: Vec<String> = listing
        .expect("the resolved directory must list")
        .into_iter()
        .map(|e| e.name)
        .collect();
    names.sort();
    assert_eq!(names, vec![NFD_FILE.to_string(), NFD_SUBDIR.to_string()]);
}

/// Two entries that differ only in Unicode form: a path matching both under
/// folding and neither exactly is refused, because on a delete or an overwrite
/// the wrong twin is data loss. A path that IS one of them exactly names that one.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn look_alike_twins_refuse_a_path_that_matches_neither_exactly() {
    let vol = make_docker_volume().await;
    let (top, album) = seed_album(&vol).await;
    seed_file_raw(&vol, &format!("{top}/{NFC_DIR}/{TWIN_NFC}"), b"composed twin").await;
    seed_file_raw(&vol, &format!("{top}/{NFC_DIR}/{TWIN_NFD}"), b"decomposed twin").await;

    // One accent composed, one not: folds onto both, is neither.
    let neither = vol.find_stored_spelling(&album.join(TWIN_MIXED), None).await;
    // The kernel mount's spelling of the directory, with the NFD twin's own bytes
    // as the leaf: the directory needs resolving, the leaf is exact.
    let exact_twin = share_path(&format!("{top}/{NFC_DIR}/{TWIN_NFD}"));
    let one = vol
        .find_stored_spelling(&decomposed(&album.to_string_lossy()).join(TWIN_NFD), None)
        .await;
    remove_album(&vol, &top).await;

    match neither {
        Err(VolumeError::AmbiguousName(_)) => {}
        other => panic!("a path matching two twins must be refused, got {other:?}"),
    }
    let one = one
        .expect("a path naming one twin exactly must resolve")
        .expect("its directory needed resolving");
    assert_same_bytes(&one, &exact_twin);
}

/// A pane that sat in an accented directory while the share was still on the
/// kernel mount carries that directory in the kernel's spelling (NFD) into the
/// direct connection. It must open, and what it lists must carry the server's
/// bytes, so the next click inside it is exact.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn a_pane_path_carried_over_from_the_kernel_mount_opens() {
    let vol = make_docker_volume().await;
    let (top, album) = seed_album(&vol).await;

    let carried = decomposed(&album.to_string_lossy());
    let as_given = vol.list_directory_impl(&carried).await;
    let stored = stored_spelling(&vol, &carried).await;
    let listed = match &stored {
        Ok(path) => vol.list_directory_impl(path).await,
        Err(e) => Err(e.clone()),
    };
    let file = listed
        .as_ref()
        .ok()
        .and_then(|entries| entries.iter().find(|e| e.name == NFD_FILE).cloned());
    let read = match &file {
        Some(entry) => read_all(&vol, Path::new(&entry.path)).await,
        None => Err(VolumeError::NotFound(NFD_FILE.to_string())),
    };
    remove_album(&vol, &top).await;

    assert!(
        matches!(as_given, Err(VolumeError::NotFound(_))),
        "the kernel's spelling is not the server's, so as given it must miss: {as_given:?}"
    );
    assert_same_bytes(
        &stored.expect("the carried path must resolve"),
        &album.to_string_lossy(),
    );
    assert_eq!(
        read.expect("a file listed from the resolved directory must read"),
        PAYLOAD
    );
}

/// A remembered correction is only a guess: when it no longer opens, it is
/// dropped and the real listing answers, and what's remembered afterwards is
/// the right spelling.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn a_stale_remembered_spelling_heals() {
    let vol = make_docker_volume().await;
    let (top, album) = seed_album(&vol).await;
    let parent = format!("{top}/{NFC_DIR}");
    // The composed spelling of the decomposed photo: what a typed name looks like.
    let foreign_leaf: String = {
        use unicode_normalization::UnicodeNormalization;
        NFD_FILE.nfc().collect()
    };

    // What a correction learned before the file was renamed would look like.
    vol.inner.spellings.remember(&parent, &foreign_leaf, "gone.jpg");
    let stored = vol.find_stored_spelling(&album.join(&foreign_leaf), None).await;
    let remembered = vol.inner.spellings.get(&parent, &foreign_leaf);
    // And the healed correction serves the next ask.
    let again = vol.find_stored_spelling(&album.join(&foreign_leaf), None).await;
    remove_album(&vol, &top).await;

    let exact = share_path(&format!("{parent}/{NFD_FILE}"));
    assert_same_bytes(
        &stored
            .expect("a stale guess must fall back to the listing")
            .expect("must resolve"),
        &exact,
    );
    assert_eq!(remembered.as_deref(), Some(NFD_FILE), "the stale guess is replaced");
    assert_same_bytes(&again.expect("asks again").expect("resolves again"), &exact);
}
