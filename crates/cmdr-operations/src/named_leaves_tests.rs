//! A transfer that names its items one by one (`TransferLeaves::per_source`,
//! `transfer/DETAILS.md` § Named destinations) through every start that takes
//! one: what lands where, and every refusal, checked once before the operation
//! is admitted. The engines' own named landings, conflicts, and interruptions
//! live beside them (`transfer/copy/named_copy_tests.rs`,
//! `transfer/move_op/named_leaves_move_tests.rs`); undo in
//! `named_leaf_undo_tests.rs` and the move journal suite.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::event_sinks::{CollectorEventSink, OperationEventSink};
use super::manager::{OperationDescriptor, OperationSummaryText};
use super::types::{
    LifecycleStatus, NamedLeavesRefusal, SourcesGoneAnswerOutcome, VolumeCopyConfig, WriteOperationConfig,
    WriteOperationError, WriteOperationStartResult, WriteOperationType,
};
use crate::testing::TestOperations;
use crate::types::TransferLeaves;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::{TestDir, wait_until_async};
use cmdr_fs::volume::{InMemoryVolume, LaneKey, RoutedKind, Volume};
use cmdr_operation_log::types::{Initiator, OperationId};

const WAIT: Duration = Duration::from_secs(10);

/// The starts a per-source map can arrive through and run on the native engines.
#[derive(Debug, Clone, Copy)]
enum Start {
    /// `start_volume_copy` on the local `root` volume: the both-local copy facade.
    RoutedCopy,
    /// `start_volume_copy` between two local volumes over `/`.
    FacadeCopy,
    /// `start_volume_move` between two local volumes over `/`: the both-local move facade.
    FacadeMove,
    /// `move_files_start`, the same-`root` move command.
    LocalMove,
}

const STARTS: [Start; 4] = [
    Start::RoutedCopy,
    Start::FacadeCopy,
    Start::FacadeMove,
    Start::LocalMove,
];

impl Start {
    fn moves(self) -> bool {
        matches!(self, Start::FacadeMove | Start::LocalMove)
    }
}

/// An engine with the local `root` volume and a second local volume over `/`,
/// so a transfer between them takes the both-local facade.
fn engine() -> TestOperations {
    let engine = TestOperations::new();
    engine.register_local_root();
    engine.volumes().register(
        "root2",
        Arc::new(cmdr_local::LocalPosixVolume::local_folder(
            "Second root",
            "/",
            crate::host::testing::pane_order_host(),
        )),
    );
    engine
}

async fn start(
    engine: &TestOperations,
    via: Start,
    events: Arc<dyn OperationEventSink>,
    sources: Vec<PathBuf>,
    destination: &Path,
    leaves: TransferLeaves,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    let config = VolumeCopyConfig {
        leaves: leaves.clone(),
        ..VolumeCopyConfig::default()
    };
    let dest = destination.display().to_string();
    let initiator = Initiator::User;
    let (root, root2) = ("root".to_string(), "root2".to_string());
    match via {
        Start::RoutedCopy => {
            engine
                .start_volume_copy(events, root.clone(), sources, root, dest, config, initiator, None)
                .await
        }
        Start::FacadeCopy => {
            engine
                .start_volume_copy(events, root, sources, root2, dest, config, initiator, None)
                .await
        }
        Start::FacadeMove => {
            engine
                .start_volume_move(events, root, sources, root2, dest, config, initiator, None)
                .await
        }
        Start::LocalMove => {
            let config = WriteOperationConfig {
                leaves,
                ..WriteOperationConfig::default()
            };
            engine
                .move_files_start(
                    events,
                    sources,
                    destination.to_path_buf(),
                    config,
                    vec![],
                    None,
                    initiator,
                    None,
                    None,
                )
                .await
        }
    }
}

async fn settled(events: &CollectorEventSink) {
    wait_until_async(WAIT, "the operation to settle", || {
        !events.settled.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(
        events.errors.lock_ignore_poison().is_empty(),
        "the operation must succeed: {:?}",
        events.errors.lock_ignore_poison()
    );
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read the folder")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn write(path: &Path, bytes: &[u8]) -> PathBuf {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("parents");
    std::fs::write(path, bytes).expect("write a fixture file");
    path.to_path_buf()
}

fn names(pairs: &[(&Path, &str)]) -> TransferLeaves {
    TransferLeaves::per_source(pairs.iter().map(|(p, n)| (p.to_path_buf(), (*n).to_string())))
}

fn assert_refused(result: Result<WriteOperationStartResult, WriteOperationError>, want: NamedLeavesRefusal, ctx: &str) {
    assert!(
        matches!(&result, Err(WriteOperationError::NamedLeavesRefused { reason, .. }) if *reason == want),
        "{ctx}: want {want:?}, got {result:?}"
    );
}

// ============================================================================
// Landing
// ============================================================================

/// Three sources, two renamed and one keeping its name, land as one operation
/// on every native start, a folder's children and empty subfolders under the
/// new name too.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn three_sources_two_renamed_one_kept_land_named_on_every_native_start() {
    let engine = engine();
    for via in STARTS {
        let tmp = TestDir::new("named_leaves_land");
        let invoice = write(&tmp.join("downloads/Invoice-X.pdf"), b"invoice");
        let receipt = write(&tmp.join("downloads/receipt.pdf"), b"receipt");
        let folder = tmp.join("downloads/scans");
        write(&folder.join("page.png"), b"page");
        std::fs::create_dir_all(folder.join("empty")).expect("an empty subfolder");
        let dest = tmp.join("invoices");
        std::fs::create_dir_all(&dest).expect("dest");

        let leaves = names(&[
            (&invoice, "2026-09-27 OpenAI invoice Testco.pdf"),
            (&folder, "2026-09 scans"),
        ]);
        let events = Arc::new(CollectorEventSink::new());
        let sources = vec![invoice.clone(), receipt.clone(), folder.clone()];
        start(&engine, via, events.clone(), sources, &dest, leaves)
            .await
            .unwrap_or_else(|e| panic!("{via:?}: the named transfer starts: {e:?}"));
        settled(&events).await;

        assert_eq!(
            names_in(&dest),
            vec!["2026-09 scans", "2026-09-27 OpenAI invoice Testco.pdf", "receipt.pdf"],
            "{via:?}"
        );
        assert_eq!(
            std::fs::read(dest.join("2026-09-27 OpenAI invoice Testco.pdf")).expect("named"),
            b"invoice"
        );
        assert_eq!(
            std::fs::read(dest.join("2026-09 scans/page.png")).expect("child"),
            b"page"
        );
        assert!(
            dest.join("2026-09 scans/empty").is_dir(),
            "{via:?}: the empty subfolder lands named too"
        );
        assert_eq!(events.complete.lock_ignore_poison().len(), 1, "{via:?}: one operation");
        assert_eq!(
            invoice.exists(),
            !via.moves(),
            "{via:?}: a move takes the original, a copy keeps it"
        );
        assert_eq!(receipt.exists(), !via.moves(), "{via:?}");
    }
}

/// A source renamed to another source's ORIGINAL name, while that one is
/// renamed onward, lands each where it was asked: the subtree remap applies
/// once per path, never twice.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_name_taken_from_another_source_that_moves_on_lands_each_where_asked() {
    let engine = engine();
    for via in STARTS {
        let tmp = TestDir::new("named_leaves_shift");
        let a = tmp.join("one/a");
        write(&a.join("from-a.txt"), b"a");
        let b = tmp.join("two/b");
        write(&b.join("from-b.txt"), b"b");
        let dest = tmp.join("dest");
        std::fs::create_dir_all(&dest).expect("dest");

        let events = Arc::new(CollectorEventSink::new());
        let leaves = names(&[(&a, "b"), (&b, "c")]);
        start(&engine, via, events.clone(), vec![a, b], &dest, leaves)
            .await
            .unwrap_or_else(|e| panic!("{via:?}: {e:?}"));
        settled(&events).await;

        assert_eq!(names_in(&dest.join("b")), vec!["from-a.txt"], "{via:?}");
        assert_eq!(names_in(&dest.join("c")), vec!["from-b.txt"], "{via:?}");
    }
}

/// A local volume rooted at a folder takes sources relative to its root, and
/// the both-local facades re-root them before the native engines walk them.
/// The names are keyed the way the sources were given, so the facade re-keys
/// them in the same step, or every item would land under its old name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_relative_sources_land_named_through_the_both_local_facades() {
    for moves in [false, true] {
        let engine = engine();
        let tmp = TestDir::new("named_leaves_relative");
        write(&tmp.join("downloads/Invoice-X.pdf"), b"invoice");
        write(&tmp.join("downloads/keep.pdf"), b"keep");
        std::fs::create_dir_all(tmp.join("filed")).expect("dest");
        engine.volumes().register(
            "work",
            Arc::new(cmdr_local::LocalPosixVolume::local_folder(
                "Work",
                tmp.to_path_buf(),
                crate::host::testing::pane_order_host(),
            )),
        );
        let invoice = PathBuf::from("downloads/Invoice-X.pdf");
        let config = VolumeCopyConfig {
            leaves: names(&[(&invoice, "2026-09-27 invoice.pdf")]),
            ..VolumeCopyConfig::default()
        };
        let sources = vec![invoice.clone(), PathBuf::from("downloads/keep.pdf")];
        let dest = tmp.join("filed").display().to_string();
        let events = Arc::new(CollectorEventSink::new());
        let (work, root2) = ("work".to_string(), "root2".to_string());
        let started = if moves {
            let sink = events.clone();
            engine
                .start_volume_move(sink, work, sources, root2, dest, config, Initiator::User, None)
                .await
        } else {
            let sink = events.clone();
            engine
                .start_volume_copy(sink, work, sources, root2, dest, config, Initiator::User, None)
                .await
        };
        started.unwrap_or_else(|e| panic!("moves={moves}: {e:?}"));
        settled(&events).await;

        assert_eq!(
            names_in(&tmp.join("filed")),
            vec!["2026-09-27 invoice.pdf", "keep.pdf"],
            "moves={moves}"
        );
        assert_eq!(tmp.join("downloads/Invoice-X.pdf").exists(), !moves);
    }
}

/// A source removed while the operation waited in the queue is pruned after
/// admission: the rest land named, and its unused name is no error.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_source_pruned_after_admission_leaves_the_rest_landing_named() {
    let engine = engine();
    let tmp = TestDir::new("named_leaves_pruned");
    let a = write(&tmp.join("src/a.pdf"), b"a");
    let b = write(&tmp.join("src/b.pdf"), b"b");
    let c = write(&tmp.join("src/c.pdf"), b"c");
    let dest = tmp.join("dst");
    std::fs::create_dir_all(&dest).expect("dest");

    let lane = LaneKey::new("lane");
    let (release, released) = tokio::sync::oneshot::channel::<()>();
    let holder = OperationId::from("holder");
    let settle_holder = holder.clone();
    engine.manager().spawn_managed(
        OperationDescriptor {
            operation_id: holder,
            operation_type: WriteOperationType::Copy,
            lanes: vec![lane.clone()],
            volume_ids: vec![],
            summary: OperationSummaryText::default(),
            rollback_support: crate::types::RollbackSupport::Supported,
            preview_id: None,
            reverses: None,
        },
        engine.new_state(),
        Box::new(move |ops: crate::Operations| {
            Box::pin(async move {
                let _ = released.await;
                ops.manager().on_settled(&settle_holder);
            })
        }),
    );

    let events = Arc::new(CollectorEventSink::new());
    let config = WriteOperationConfig {
        leaves: names(&[(&a, "alpha.pdf"), (&b, "beta.pdf")]),
        ..WriteOperationConfig::default()
    };
    let started = engine
        .move_files_start(
            events.clone(),
            vec![a.clone(), b.clone(), c.clone()],
            dest.clone(),
            config,
            vec![],
            Some(vec![lane]),
            Initiator::User,
            None,
            None,
        )
        .await
        .expect("admitted, queued");
    let op_id = OperationId::from(started.operation_id);
    assert_eq!(engine.manager().lifecycle_status(&op_id), Some(LifecycleStatus::Queued));

    // A renamed source leaves while the move waits its turn.
    std::fs::remove_file(&b).expect("b leaves");
    let _ = release.send(());
    wait_until_async(WAIT, "the move to ask about its sources", || {
        engine.pending_sources_gone(&op_id).is_some()
    })
    .await;
    assert_eq!(
        engine.continue_without_gone_sources(&op_id),
        SourcesGoneAnswerOutcome::Continued
    );
    settled(&events).await;

    assert_eq!(names_in(&dest), vec!["alpha.pdf", "c.pdf"]);
    assert_eq!(std::fs::read(dest.join("alpha.pdf")).expect("a landed named"), b"a");
}

// ============================================================================
// Refusals, all before anything is admitted
// ============================================================================

/// Every source and the destination exactly as they were.
struct Untouched {
    files: Vec<(PathBuf, Vec<u8>)>,
    dest: PathBuf,
    dest_names: Vec<String>,
}

impl Untouched {
    fn capture(files: &[&Path], dest: &Path) -> Self {
        Self {
            files: files
                .iter()
                .map(|f| (f.to_path_buf(), std::fs::read(f).expect("a fixture file")))
                .collect(),
            dest: dest.to_path_buf(),
            dest_names: names_in(dest),
        }
    }

    fn assert_still(&self, ctx: &str) {
        for (path, bytes) in &self.files {
            assert_eq!(
                &std::fs::read(path).expect("a source stays"),
                bytes,
                "{ctx}: {}",
                path.display()
            );
        }
        assert_eq!(names_in(&self.dest), self.dest_names, "{ctx}: nothing landed");
    }
}

/// Runs `case` through every native start and expects `want`, with nothing
/// written. `case` builds its fixture in a fresh folder and returns the
/// sources, destination, names, and files to keep an eye on.
async fn refuses_everywhere(
    want: NamedLeavesRefusal,
    case: impl Fn(&TestDir) -> (Vec<PathBuf>, PathBuf, TransferLeaves),
) {
    let engine = engine();
    for via in STARTS {
        let tmp = TestDir::new("named_leaves_refuse");
        let (sources, dest, leaves) = case(&tmp);
        let watched: Vec<&Path> = sources.iter().map(PathBuf::as_path).collect();
        let before = Untouched::capture(&watched, &dest);
        let events = Arc::new(CollectorEventSink::new());
        let result = start(&engine, via, events, sources, &dest, leaves).await;
        assert_refused(result, want, &format!("{via:?}"));
        before.assert_still(&format!("{via:?}"));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_items_named_alike_refuse() {
    refuses_everywhere(NamedLeavesRefusal::SameName, |tmp| {
        let a = write(&tmp.join("src/a.pdf"), b"a");
        let b = write(&tmp.join("src/b.pdf"), b"b");
        let dest = tmp.join("dst");
        std::fs::create_dir_all(&dest).expect("dest");
        let leaves = names(&[(&a, "same.pdf"), (&b, "same.pdf")]);
        (vec![a, b], dest, leaves)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn names_differing_only_by_case_or_unicode_form_refuse() {
    for (one, other) in [
        ("Invoice.pdf", "invoice.pdf"),
        ("R\u{e9}sum\u{e9}.pdf", "Re\u{301}sume\u{301}.pdf"),
    ] {
        refuses_everywhere(NamedLeavesRefusal::SameName, |tmp| {
            let a = write(&tmp.join("src/a.pdf"), b"a");
            let b = write(&tmp.join("src/b.pdf"), b"b");
            let dest = tmp.join("dst");
            std::fs::create_dir_all(&dest).expect("dest");
            let leaves = names(&[(&a, one), (&b, other)]);
            (vec![a, b], dest, leaves)
        })
        .await;
    }
}

/// A rename can collide with an item that keeps its name just as well.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_name_equal_to_an_item_that_keeps_its_own_refuses() {
    refuses_everywhere(NamedLeavesRefusal::SameName, |tmp| {
        let a = write(&tmp.join("one/a.pdf"), b"a");
        let b = write(&tmp.join("two/B.pdf"), b"b");
        let dest = tmp.join("dst");
        std::fs::create_dir_all(&dest).expect("dest");
        let leaves = names(&[(&a, "b.pdf")]);
        (vec![a, b], dest, leaves)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_name_for_an_item_the_transfer_does_not_carry_refuses() {
    refuses_everywhere(NamedLeavesRefusal::NotASource, |tmp| {
        let a = write(&tmp.join("src/a.pdf"), b"a");
        let dest = tmp.join("dst");
        std::fs::create_dir_all(&dest).expect("dest");
        let leaves = names(&[(&a, "alpha.pdf"), (&tmp.join("src/ghost.pdf"), "ghost.pdf")]);
        (vec![a], dest, leaves)
    })
    .await;
}

/// Names go to OTHER folders: one source already in the destination makes the
/// whole group a rename, whether it's the renamed item or one keeping its name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn names_into_a_folder_that_already_holds_a_source_refuse() {
    for renamed_is_inside in [true, false] {
        refuses_everywhere(NamedLeavesRefusal::AlreadyInDestination, |tmp| {
            let inside = write(&tmp.join("dst/inside.pdf"), b"inside");
            let outside = write(&tmp.join("src/outside.pdf"), b"outside");
            let renamed = if renamed_is_inside { &inside } else { &outside };
            let leaves = names(&[(renamed, "renamed.pdf")]);
            (vec![inside.clone(), outside.clone()], tmp.join("dst"), leaves)
        })
        .await;
    }
}

/// The chain: `a` lands as `b`, which is itself a source renamed onward. In
/// whichever order it runs, one of them overwrites the other before it's read
/// (a copy) or moved (a move).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_chain_onto_another_source_refuses_for_move_and_copy() {
    refuses_everywhere(NamedLeavesRefusal::OntoASource, |tmp| {
        let a = write(&tmp.join("src/a.txt"), b"a");
        let b = write(&tmp.join("dst/b.txt"), b"b");
        let leaves = names(&[(&a, "b.txt"), (&b, "c.txt")]);
        (vec![a, b], tmp.join("dst"), leaves)
    })
    .await;
}

/// `dev+ino` catches a landing that IS a source under another spelling: the
/// destination reached through a symlinked folder, or a hard link's other name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_landing_that_is_a_source_by_inode_refuses() {
    // Through a symlinked folder: no folded name matches, the inode does.
    refuses_everywhere(NamedLeavesRefusal::OntoASource, |tmp| {
        let a = write(&tmp.join("src/a.txt"), b"a");
        let b = write(&tmp.join("real/b.txt"), b"b");
        cmdr_fs::testing::symlink(tmp.join("real"), tmp.join("link")).expect("a symlinked folder");
        let leaves = names(&[(&a, "b.txt"), (&b, "kept-apart.txt")]);
        (vec![a, b], tmp.join("link"), leaves)
    })
    .await;
    // A hard link: `alias.txt` is the source `b.txt` under another name.
    refuses_everywhere(NamedLeavesRefusal::OntoASource, |tmp| {
        let a = write(&tmp.join("src/a.txt"), b"a");
        let b = write(&tmp.join("elsewhere/b.txt"), b"b");
        let dest = tmp.join("dst");
        std::fs::create_dir_all(&dest).expect("dest");
        std::fs::hard_link(&b, dest.join("alias.txt")).expect("a hard link");
        let leaves = names(&[(&a, "alias.txt")]);
        (vec![a, b], dest, leaves)
    })
    .await;
}

/// On a case-insensitive disk (APFS's default), `Invoice.pdf` and
/// `invoice.pdf` are one file, and the destination spelled in another case is
/// the source's folder. The checks fold whatever the disk does, so this holds
/// on a case-sensitive disk too; here it's proven where the collision is real.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn on_a_case_insensitive_disk_case_only_names_and_spellings_refuse() {
    let probe = TestDir::new("named_leaves_case_probe");
    write(&probe.join("Probe.txt"), b"p");
    if !probe.join("probe.txt").exists() {
        log::warn!("skipped: the temp folder's disk is case-sensitive, so a case-only collision isn't real here");
        return;
    }
    refuses_everywhere(NamedLeavesRefusal::SameName, |tmp| {
        let a = write(&tmp.join("src/a.pdf"), b"a");
        let b = write(&tmp.join("src/b.pdf"), b"b");
        let dest = tmp.join("dst");
        std::fs::create_dir_all(&dest).expect("dest");
        (
            vec![a.clone(), b.clone()],
            dest,
            names(&[(&a, "Invoice.pdf"), (&b, "invoice.pdf")]),
        )
    })
    .await;
    refuses_everywhere(NamedLeavesRefusal::OntoASource, |tmp| {
        let a = write(&tmp.join("src/a.pdf"), b"a");
        let b = write(&tmp.join("dst/invoice.pdf"), b"b");
        // The source's folder spelled in another case, and a name that is
        // that source in another case.
        let leaves = names(&[(&a, "INVOICE.PDF"), (&b, "kept-apart.pdf")]);
        (vec![a, b], tmp.join("DST"), leaves)
    })
    .await;
}

/// A name that renders as something else, or holds something invisible, is a
/// spoof the review couldn't show.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_name_with_a_bidi_override_or_a_zero_width_joiner_refuses() {
    let engine = engine();
    for leaf in [
        "invoice\u{202E}fdp.exe",
        "in\u{200D}voice.pdf",
        " padded.pdf",
        "a/b.pdf",
        "..",
    ] {
        for via in STARTS {
            let tmp = TestDir::new("named_leaves_spoof");
            let a = write(&tmp.join("src/a.pdf"), b"a");
            let dest = tmp.join("dst");
            std::fs::create_dir_all(&dest).expect("dest");
            let before = Untouched::capture(&[&a], &dest);
            let events = Arc::new(CollectorEventSink::new());
            let result = start(&engine, via, events, vec![a.clone()], &dest, names(&[(&a, leaf)])).await;
            assert!(
                matches!(result, Err(WriteOperationError::InvalidName { .. })),
                "{via:?} {leaf:?}: {result:?}"
            );
            before.assert_still(&format!("{via:?} {leaf:?}"));
        }
    }
}

/// An archive changeset never names each source: copy into, move into, and
/// move out of a zip refuse the per-source form typed, before anything starts.
/// The volume engines name each source (`named_leaves_volume_tests.rs`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn archive_routes_refuse_per_source_names() {
    let engine = engine();
    let zip = Arc::new(InMemoryVolume::new("Zip").with_space_info(1_000_000, 900_000));
    zip.create_file(Path::new("/archive.zip/inner.txt"), b"z")
        .await
        .expect("a zip entry");
    engine
        .volumes()
        .route("/archive.zip", RoutedKind::Archive, Arc::clone(&zip) as Arc<dyn Volume>);
    let tmp = TestDir::new("named_leaves_routes");
    let local = write(&tmp.join("src/a.txt"), b"a");
    let local_dest = tmp.join("dst");
    std::fs::create_dir_all(&local_dest).expect("dest");

    struct Case {
        what: &'static str,
        moves: bool,
        from: &'static str,
        source: PathBuf,
        to: &'static str,
        dest: String,
    }
    let cases = [
        Case {
            what: "copy into a zip",
            moves: false,
            from: "root",
            source: local.clone(),
            to: "root",
            dest: "/archive.zip/sub".into(),
        },
        Case {
            what: "move into a zip",
            moves: true,
            from: "root",
            source: local.clone(),
            to: "root",
            dest: "/archive.zip/sub".into(),
        },
        Case {
            what: "move out of a zip",
            moves: true,
            from: "root",
            source: PathBuf::from("/archive.zip/inner.txt"),
            to: "root",
            dest: local_dest.display().to_string(),
        },
    ];
    for case in cases {
        let config = VolumeCopyConfig {
            leaves: names(&[(&case.source, "renamed.txt")]),
            ..VolumeCopyConfig::default()
        };
        let events: Arc<dyn OperationEventSink> = Arc::new(CollectorEventSink::new());
        let (from, to) = (case.from.to_string(), case.to.to_string());
        let result = if case.moves {
            engine
                .start_volume_move(
                    events,
                    from,
                    vec![case.source.clone()],
                    to,
                    case.dest,
                    config,
                    Initiator::User,
                    None,
                )
                .await
        } else {
            engine
                .start_volume_copy(
                    events,
                    from,
                    vec![case.source.clone()],
                    to,
                    case.dest,
                    config,
                    Initiator::User,
                    None,
                )
                .await
        };
        assert_refused(result, NamedLeavesRefusal::RouteCannotName, case.what);
    }
    assert!(
        zip.exists(Path::new("/archive.zip/inner.txt")).await,
        "the zip entry stays"
    );
    assert!(!zip.exists(Path::new("/archive.zip/sub/renamed.txt")).await);
    assert!(local.exists(), "the local source stays");
    assert!(names_in(&local_dest).is_empty());
}

/// A per-source map never goes on the wire: one optional leaf can't say it,
/// and dropping it would land every item under its old name.
#[test]
fn a_per_source_map_refuses_to_serialize() {
    let config = WriteOperationConfig {
        leaves: names(&[(Path::new("/a.pdf"), "b.pdf")]),
        ..WriteOperationConfig::default()
    };
    assert!(serde_json::to_value(&config).is_err());
    assert_eq!(
        TransferLeaves::per_source([]),
        TransferLeaves::default(),
        "no names is no map"
    );
}
