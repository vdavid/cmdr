//! New folder, new file, and rename on a byte-exact volume that holds a name in
//! another Unicode spelling, or that asks for composed new names.
//!
//! The transfer engines' twin is `transfer/volume/look_alike_tests.rs`; the rule
//! both follow is `look_alike.rs`.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::create::{create_directory_core, create_file_core};
use super::mutation_error::MutationError;
use super::rename::{check_rename_validity_impl, rename_managed};
use crate::file_system::Volume;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::volume::manager::get_volume_manager;
use crate::operation_log::types::Initiator;

const CAFE_NFC: &str = "caf\u{e9}.txt";
const CAFE_NFD: &str = "cafe\u{301}.txt";
const FOTOK_NFC: &str = "fot\u{f3}k";
const FOTOK_NFD: &str = "foto\u{301}k";

/// A registered in-memory share: byte-exact, composing new names, holding
/// `/dir`.
async fn share(label: &str) -> (String, Arc<InMemoryVolume>) {
    static N: AtomicU64 = AtomicU64::new(0);
    let id = format!("look-alike-{label}-{}", N.fetch_add(1, Ordering::Relaxed));
    let volume = Arc::new(InMemoryVolume::new(&id).with_composed_new_names());
    volume.create_directory(Path::new("/dir")).await.unwrap();
    get_volume_manager().register(&id, Arc::clone(&volume) as Arc<dyn Volume>);
    (id, volume)
}

async fn names_in(volume: &InMemoryVolume, dir: &str) -> Vec<String> {
    let mut names: Vec<String> = volume
        .list_directory(Path::new(dir), None)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn a_new_folder_and_a_new_file_go_out_composed() {
    let (id, volume) = share("new").await;

    create_directory_core(Some(id.clone()), "/dir", FOTOK_NFD)
        .await
        .unwrap();
    create_file_core(Some(id), "/dir", CAFE_NFD).await.unwrap();

    let mut expected = vec![CAFE_NFC.to_string(), FOTOK_NFC.to_string()];
    expected.sort();
    assert_eq!(names_in(&volume, "/dir").await, expected);
}

#[tokio::test]
async fn a_new_folder_or_file_is_refused_beside_its_look_alike() {
    let (id, volume) = share("taken").await;
    volume
        .create_file(Path::new(&format!("/dir/{CAFE_NFD}")), b"x")
        .await
        .unwrap();
    volume
        .create_directory(Path::new(&format!("/dir/{FOTOK_NFD}")))
        .await
        .unwrap();

    let file = create_file_core(Some(id.clone()), "/dir", CAFE_NFC).await;
    let folder = create_directory_core(Some(id), "/dir", FOTOK_NFC).await;

    assert!(matches!(file, Err(MutationError::AlreadyExists { .. })), "{file:?}");
    assert!(matches!(folder, Err(MutationError::AlreadyExists { .. })), "{folder:?}");
    let mut expected = vec![CAFE_NFD.to_string(), FOTOK_NFD.to_string()];
    expected.sort();
    assert_eq!(names_in(&volume, "/dir").await, expected, "no twin appeared");
}

#[tokio::test]
async fn a_rename_target_goes_out_composed() {
    let (id, volume) = share("rename-new").await;
    volume.create_file(Path::new("/dir/a.txt"), b"x").await.unwrap();

    rename_managed(
        "/dir/a.txt".into(),
        format!("/dir/{CAFE_NFD}").into(),
        false,
        id,
        Initiator::User,
    )
    .await
    .unwrap();

    assert_eq!(names_in(&volume, "/dir").await, vec![CAFE_NFC.to_string()]);
}

#[tokio::test]
async fn a_rename_onto_a_look_alike_is_refused_unless_forced_and_then_replaces_it() {
    let (id, volume) = share("rename-taken").await;
    volume.create_file(Path::new("/dir/a.txt"), b"NEW").await.unwrap();
    volume
        .create_file(Path::new(&format!("/dir/{CAFE_NFD}")), b"OLD")
        .await
        .unwrap();

    let refused = rename_managed(
        "/dir/a.txt".into(),
        format!("/dir/{CAFE_NFC}").into(),
        false,
        id.clone(),
        Initiator::User,
    )
    .await;
    assert!(
        matches!(refused, Err(MutationError::AlreadyExists { .. })),
        "{refused:?}"
    );
    let mut both = vec!["a.txt".to_string(), CAFE_NFD.to_string()];
    both.sort();
    assert_eq!(names_in(&volume, "/dir").await, both);

    rename_managed(
        "/dir/a.txt".into(),
        format!("/dir/{CAFE_NFC}").into(),
        true,
        id,
        Initiator::User,
    )
    .await
    .unwrap();
    assert_eq!(
        names_in(&volume, "/dir").await,
        vec![CAFE_NFD.to_string()],
        "a forced rename replaces the entry that's there, in its own spelling"
    );
}

/// Renaming a decomposed name to its own composed spelling is a respell, not a
/// clash with itself: the one way to fix a name Finder can't open.
#[tokio::test]
async fn respelling_a_name_is_not_a_clash_with_itself() {
    let (id, volume) = share("respell").await;
    volume
        .create_file(Path::new(&format!("/dir/{CAFE_NFD}")), b"x")
        .await
        .unwrap();

    let validity = check_rename_validity_impl("/dir".into(), CAFE_NFD.into(), CAFE_NFC.into(), id.clone(), None).await;
    assert!(!validity.has_conflict, "the only look-alike is the file itself");

    rename_managed(
        format!("/dir/{CAFE_NFD}").into(),
        format!("/dir/{CAFE_NFC}").into(),
        false,
        id,
        Initiator::User,
    )
    .await
    .unwrap();
    assert_eq!(names_in(&volume, "/dir").await, vec![CAFE_NFC.to_string()]);
}

/// The rename editor's live check names the look-alike, so the user sees the
/// clash before pressing Enter.
#[tokio::test]
async fn the_live_rename_check_reports_a_look_alike() {
    let (id, volume) = share("validity").await;
    volume.create_file(Path::new("/dir/a.txt"), b"x").await.unwrap();
    volume
        .create_file(Path::new(&format!("/dir/{CAFE_NFD}")), b"x")
        .await
        .unwrap();

    let validity = check_rename_validity_impl("/dir".into(), "a.txt".into(), CAFE_NFC.into(), id, None).await;

    assert!(validity.has_conflict);
    assert_eq!(validity.conflict.map(|c| c.name).as_deref(), Some(CAFE_NFD));
}

/// The mirror case: the typed name composes onto a name the folder holds
/// exactly. The rename itself refuses it, so the live check has to say so too,
/// or the editor promises a name that Enter then refuses.
#[tokio::test]
async fn the_live_rename_check_reports_the_name_a_typed_one_composes_onto() {
    let (id, volume) = share("validity-composed").await;
    volume.create_file(Path::new("/dir/a.txt"), b"x").await.unwrap();
    volume
        .create_file(Path::new(&format!("/dir/{CAFE_NFC}")), b"x")
        .await
        .unwrap();

    let validity = check_rename_validity_impl("/dir".into(), "a.txt".into(), CAFE_NFD.into(), id, None).await;

    assert!(validity.has_conflict);
    assert_eq!(validity.conflict.map(|c| c.name).as_deref(), Some(CAFE_NFC));
}
