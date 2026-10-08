//! Names a server already holds under another Unicode spelling, and renames,
//! written once for every network backend.
//!
//! A server that stores a name's bytes as sent and matches them exactly treats
//! `café` composed (NFC) and `café` decomposed (NFD, what macOS hands out for
//! many local files) as two entries. A copy that asks "is `café` taken?" in the
//! source's spelling hears "no" and writes a second, identical-looking entry
//! beside the user's. These scenarios pin the guard on a live server: such a
//! name is a conflict like any other, an Overwrite or a merge lands on the entry
//! that is there (so the server ends with ONE), a genuinely new name goes out
//! composed (every network backend answers `Volume::composes_new_names` with
//! `true`), and an entry the server already holds decomposed is read,
//! downloaded, overwritten, and deleted under its own bytes.
//!
//! A backend whose own lookups fold Unicode forms
//! (`matches_names_in_any_unicode_form`) can't hold two spellings in the first
//! place, and its cell shouldn't call these. Unit twins:
//! `transfer/volume/look_alike_tests.rs`, `rename/bulk/look_alike_tests.rs`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::Volume;

use super::super::event_sinks::CollectorEventSink;
use super::super::types::ConflictResolution;
use super::super::{BulkRenameRow, MutationError, SourceFingerprint, rename_managed, start_bulk_rename};
use super::network_safety_test_support::{Registered, delete_on};
use super::network_semantics_test_support::{Transfer, local_volume, names_in, seed, transfer, try_read};
use super::network_transfer_test_support::{budget, clean_deep};
use crate::ignore_poison::IgnorePoison;
use crate::operation_log::types::Initiator;

pub(super) const CAFE_NFC: &str = "caf\u{e9}.txt";
pub(super) const CAFE_NFD: &str = "cafe\u{301}.txt";
const FOTOK_NFC: &str = "fot\u{f3}k";
const FOTOK_NFD: &str = "foto\u{301}k";
pub(super) const RESUME_NFC: &str = "r\u{e9}sum\u{e9}.txt";
pub(super) const RESUME_NFD: &str = "re\u{301}sume\u{301}.txt";

/// Asserts `remote` spells a NEW name composed, the premise of every "lands
/// composed" assertion here. A network backend that stops answering `true`
/// fails loudly on this line instead of on a listing that merely differs.
pub(super) fn assert_composes_new_names(remote: &dyn Volume) {
    assert!(
        remote.composes_new_names(),
        "every network backend sends a name it creates composed (NFC)"
    );
}

/// A decomposed file copied onto a server holding its composed twin, under
/// Skip: the user's file keeps its bytes, the copy reports it skipped, and no
/// second `café.txt` appears.
pub(super) async fn a_decomposed_file_onto_its_composed_twin_is_skipped(remote: Arc<dyn Volume>, dir: PathBuf) {
    seed(remote.as_ref(), &dir, &[(CAFE_NFC, b"DEST")]).await;
    let (_local_dir, local) = local_volume("look-alike-skip");
    seed(local.as_ref(), Path::new(""), &[(CAFE_NFD, b"SOURCE")]).await;

    let finished = transfer(
        "look-alike-skip",
        Transfer::Copy,
        &local,
        &[PathBuf::from(CAFE_NFD)],
        &remote,
        &dir,
        ConflictResolution::Skip,
    )
    .await;

    assert_eq!(
        names_in(remote.as_ref(), &dir).await,
        vec![CAFE_NFC.to_string()],
        "❗ the server must still hold exactly the user's own spelling"
    );
    assert_eq!(
        try_read(remote.as_ref(), &dir.join(CAFE_NFC)).await.as_deref(),
        Some(&b"DEST"[..])
    );
    assert_eq!(
        finished.state.skipped_totals().0,
        1,
        "the look-alike is reported skipped"
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// A decomposed file deep in a merged folder, under Overwrite: the source's
/// bytes replace the user's file IN PLACE, under the spelling the server already
/// had, so the folder ends with one `café.txt`.
pub(super) async fn overwriting_a_composed_twin_leaves_one_entry(remote: Arc<dyn Volume>, dir: PathBuf) {
    let album = dir.join("album");
    seed(remote.as_ref(), &album, &[(CAFE_NFC, b"DEST")]).await;
    let (_local_dir, local) = local_volume("look-alike-overwrite");
    seed(local.as_ref(), Path::new("album"), &[(CAFE_NFD, b"SOURCE")]).await;

    transfer(
        "look-alike-overwrite",
        Transfer::Copy,
        &local,
        &[PathBuf::from("album")],
        &remote,
        &dir,
        ConflictResolution::Overwrite,
    )
    .await;

    assert_eq!(
        names_in(remote.as_ref(), &album).await,
        vec![CAFE_NFC.to_string()],
        "❗ an Overwrite replaces the entry that's there, not stands a second one beside it"
    );
    assert_eq!(
        try_read(remote.as_ref(), &album.join(CAFE_NFC)).await.as_deref(),
        Some(&b"SOURCE"[..])
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// A decomposed FOLDER copied onto its composed twin merges into the folder the
/// server has: the user's file inside survives, the source's joins it, and no
/// second `fotók` appears.
pub(super) async fn a_decomposed_folder_merges_into_its_composed_twin(remote: Arc<dyn Volume>, dir: PathBuf) {
    let fotok = dir.join(FOTOK_NFC);
    seed(remote.as_ref(), &fotok, &[("keep.txt", b"DEST")]).await;
    let (_local_dir, local) = local_volume("look-alike-merge");
    seed(local.as_ref(), Path::new(FOTOK_NFD), &[("new.txt", b"SOURCE")]).await;

    transfer(
        "look-alike-merge",
        Transfer::Copy,
        &local,
        &[PathBuf::from(FOTOK_NFD)],
        &remote,
        &dir,
        ConflictResolution::Stop,
    )
    .await;

    assert_eq!(
        names_in(remote.as_ref(), &dir).await,
        vec![FOTOK_NFC.to_string()],
        "❗ the folder merges into the one the server has"
    );
    assert_eq!(
        names_in(remote.as_ref(), &fotok).await,
        vec!["keep.txt".to_string(), "new.txt".to_string()],
        "the user's file survives and the source's joins it"
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// Names the copy CREATES go out composed, at every depth: a decomposed name
/// from macOS would land in a form that web servers, PHP, and scripts on the
/// server (which match bytes) don't find.
pub(super) async fn a_new_name_lands_composed(remote: Arc<dyn Volume>, dir: PathBuf) {
    assert_composes_new_names(remote.as_ref());
    let (_local_dir, local) = local_volume("look-alike-new-name");
    seed(
        local.as_ref(),
        Path::new(""),
        &[(&format!("{FOTOK_NFD}/{CAFE_NFD}"), b"SOURCE"), (CAFE_NFD, b"TOP")],
    )
    .await;

    transfer(
        "look-alike-new-name",
        Transfer::Copy,
        &local,
        &[PathBuf::from(FOTOK_NFD), PathBuf::from(CAFE_NFD)],
        &remote,
        &dir,
        ConflictResolution::Stop,
    )
    .await;

    let mut top = vec![CAFE_NFC.to_string(), FOTOK_NFC.to_string()];
    top.sort();
    assert_eq!(
        names_in(remote.as_ref(), &dir).await,
        top,
        "❗ top-level names land composed"
    );
    assert_eq!(
        names_in(remote.as_ref(), &dir.join(FOTOK_NFC)).await,
        vec![CAFE_NFC.to_string()],
        "❗ and so do the names inside a folder the copy created"
    );
    assert_eq!(
        try_read(remote.as_ref(), &dir.join(FOTOK_NFC).join(CAFE_NFC))
            .await
            .as_deref(),
        Some(&b"SOURCE"[..])
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// An entry the server ALREADY holds decomposed is only ever addressed by its
/// own bytes: it reads, it downloads under its own spelling, an Overwrite from
/// either spelling replaces it in place (no composed twin appears beside it),
/// and a delete removes it. Only names Cmdr creates get composed.
pub(super) async fn an_existing_decomposed_entry_keeps_its_exact_bytes(remote: Arc<dyn Volume>, dir: PathBuf) {
    seed(remote.as_ref(), &dir, &[(CAFE_NFD, b"THEIRS")]).await;
    assert_eq!(
        try_read(remote.as_ref(), &dir.join(CAFE_NFD)).await.as_deref(),
        Some(&b"THEIRS"[..]),
        "it reads under its own bytes"
    );

    // Downloading it keeps the server's spelling.
    let (_down_dir, down) = local_volume("look-alike-existing-down");
    transfer(
        "look-alike-existing-down",
        Transfer::Copy,
        &remote,
        &[dir.join(CAFE_NFD)],
        &down,
        Path::new(""),
        ConflictResolution::Stop,
    )
    .await;
    assert_eq!(
        names_in(down.as_ref(), Path::new("")).await,
        vec![CAFE_NFD.to_string()],
        "❗ a download keeps the server's bytes"
    );

    // An Overwrite in EITHER spelling replaces that entry where it stands.
    for (label, spelling, bytes) in [
        ("look-alike-existing-same", CAFE_NFD, &b"SAME SPELLING"[..]),
        ("look-alike-existing-other", CAFE_NFC, &b"OTHER SPELLING"[..]),
    ] {
        let (_up_dir, up) = local_volume(label);
        seed(up.as_ref(), Path::new(""), &[(spelling, bytes)]).await;
        transfer(
            label,
            Transfer::Copy,
            &up,
            &[PathBuf::from(spelling)],
            &remote,
            &dir,
            ConflictResolution::Overwrite,
        )
        .await;
        assert_eq!(
            names_in(remote.as_ref(), &dir).await,
            vec![CAFE_NFD.to_string()],
            "❗ {label}: the overwrite lands on the stored name, never a composed twin"
        );
        assert_eq!(
            try_read(remote.as_ref(), &dir.join(CAFE_NFD)).await.as_deref(),
            Some(bytes),
            "{label}"
        );
    }

    let registered = Registered::new(&remote, "look-alike-existing-delete");
    let deleted = delete_on(
        &remote,
        &registered.id,
        "look-alike-existing-delete",
        &[dir.join(CAFE_NFD)],
        None,
    )
    .await;
    assert!(deleted.is_ok(), "the delete should succeed: {deleted:?}");
    assert!(
        names_in(remote.as_ref(), &dir).await.is_empty(),
        "❗ the delete removed the decomposed entry itself"
    );

    clean_deep(remote.as_ref(), &dir).await;
    registered.leave();
}

/// A SAME-SERVER move of a decomposed file, deep in a merged folder and at the
/// top level, onto folders holding the composed twin, under Skip: nothing lands
/// beside the user's file, and both sources stay where they were (a skipped
/// move keeps its source).
pub(super) async fn a_same_server_move_skips_a_look_alike(remote: Arc<dyn Volume>, dir: PathBuf) {
    let deep_source = format!("album/{CAFE_NFD}");
    let deep_dest = format!("album/{CAFE_NFC}");
    seed(
        remote.as_ref(),
        &dir.join("src"),
        &[(&deep_source, b"DEEP-SOURCE"), (CAFE_NFD, b"TOP-SOURCE")],
    )
    .await;
    seed(
        remote.as_ref(),
        &dir.join("dst"),
        &[(&deep_dest, b"DEEP-DEST"), (CAFE_NFC, b"TOP-DEST")],
    )
    .await;

    transfer(
        "look-alike-same-server-move",
        Transfer::MoveWithinOneVolume,
        &remote,
        &[dir.join("src/album"), dir.join("src").join(CAFE_NFD)],
        &remote,
        &dir.join("dst"),
        ConflictResolution::Skip,
    )
    .await;

    let mut top = vec!["album".to_string(), CAFE_NFC.to_string()];
    top.sort();
    assert_eq!(
        names_in(remote.as_ref(), &dir.join("dst")).await,
        top,
        "❗ nothing lands beside the top-level twin"
    );
    assert_eq!(
        names_in(remote.as_ref(), &dir.join("dst/album")).await,
        vec![CAFE_NFC.to_string()],
        "❗ nor beside the deep one"
    );
    assert_eq!(
        try_read(remote.as_ref(), &dir.join("dst").join(&deep_dest))
            .await
            .as_deref(),
        Some(&b"DEEP-DEST"[..])
    );
    assert_eq!(
        try_read(remote.as_ref(), &dir.join("src").join(&deep_source))
            .await
            .as_deref(),
        Some(&b"DEEP-SOURCE"[..])
    );
    assert_eq!(
        try_read(remote.as_ref(), &dir.join("src").join(CAFE_NFD))
            .await
            .as_deref(),
        Some(&b"TOP-SOURCE"[..])
    );

    clean_deep(remote.as_ref(), &dir).await;
}

/// A single RENAME on the server, the way the inline editor issues it: a free
/// name takes it, a taken name and a name the folder holds in the other spelling
/// are both refused as taken, and a refusal leaves both entries exactly as they
/// were.
pub(super) async fn a_rename_never_lands_on_a_taken_name(remote: Arc<dyn Volume>, dir: PathBuf) {
    seed(
        remote.as_ref(),
        &dir,
        &[("a.txt", b"A"), ("b.txt", b"B"), (CAFE_NFC, b"THEIRS")],
    )
    .await;
    let registered = Registered::new(&remote, "rename");
    let rename = |from: &str, to: &str| {
        rename_managed(
            dir.join(from),
            dir.join(to),
            false,
            registered.id.clone(),
            Initiator::User,
        )
    };

    let taken = rename("a.txt", "b.txt").await;
    assert!(
        matches!(taken, Err(MutationError::AlreadyExists { .. })),
        "a rename onto a taken name is refused as taken, got {taken:?}"
    );
    let look_alike = rename("a.txt", CAFE_NFD).await;
    assert!(
        matches!(look_alike, Err(MutationError::AlreadyExists { .. })),
        "❗ a rename onto the other spelling of a taken name is refused as taken, got {look_alike:?}"
    );
    let mut before = vec!["a.txt".to_string(), "b.txt".to_string(), CAFE_NFC.to_string()];
    before.sort();
    assert_eq!(
        names_in(remote.as_ref(), &dir).await,
        before,
        "a refused rename leaves the folder exactly as it was"
    );
    assert_eq!(
        try_read(remote.as_ref(), &dir.join("b.txt")).await.as_deref(),
        Some(&b"B"[..])
    );
    assert_eq!(
        try_read(remote.as_ref(), &dir.join(CAFE_NFC)).await.as_deref(),
        Some(&b"THEIRS"[..])
    );

    let free = rename("a.txt", "renamed.txt").await;
    assert!(free.is_ok(), "a rename to a free name goes through, got {free:?}");
    assert_eq!(
        try_read(remote.as_ref(), &dir.join("renamed.txt")).await.as_deref(),
        Some(&b"A"[..])
    );
    assert!(!remote.exists(&dir.join("a.txt")).await, "and the old name is gone");

    clean_deep(remote.as_ref(), &dir).await;
    registered.leave();
}

/// An Ask Cmdr bulk rename on the server: a row whose destination the folder
/// holds in the other spelling is skipped (no twin), and a genuinely new name
/// lands composed.
pub(super) async fn a_bulk_rename_skips_a_look_alike(remote: Arc<dyn Volume>, dir: PathBuf) {
    seed(
        remote.as_ref(),
        &dir,
        &[(CAFE_NFC, b"THEIRS"), ("a.txt", b"A"), ("b.txt", b"B")],
    )
    .await;
    let registered = Registered::new(&remote, "bulk-rename");

    let mut rows = Vec::new();
    for (source, destination) in [("a.txt", CAFE_NFD), ("b.txt", RESUME_NFD)] {
        let source = dir.join(source);
        rows.push(BulkRenameRow {
            row_id: rows.len().to_string(),
            expected_fingerprint: SourceFingerprint::capture_remote(remote.as_ref(), &source)
                .await
                .expect("fingerprint"),
            source,
            destination: dir.join(destination),
        });
    }
    let events = Arc::new(CollectorEventSink::new());
    start_bulk_rename(
        events.clone() as Arc<dyn crate::file_system::OperationEventSink>,
        registered.id.clone(),
        rows,
        Initiator::Agent,
    )
    .expect("start bulk rename");
    crate::test_support::wait_until_async(budget(Duration::from_secs(6)), "the bulk rename to settle", || {
        !events.settled.lock_ignore_poison().is_empty()
    })
    .await;

    assert_composes_new_names(remote.as_ref());
    let mut expected = vec![CAFE_NFC.to_string(), "a.txt".to_string(), RESUME_NFC.to_string()];
    expected.sort();
    assert_eq!(names_in(remote.as_ref(), &dir).await, expected);
    assert_eq!(
        try_read(remote.as_ref(), &dir.join(CAFE_NFC)).await.as_deref(),
        Some(&b"THEIRS"[..])
    );
    assert_eq!(events.complete.lock_ignore_poison()[0].files_skipped, 1);

    clean_deep(remote.as_ref(), &dir).await;
    registered.leave();
}
