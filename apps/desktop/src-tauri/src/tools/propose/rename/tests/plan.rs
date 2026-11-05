//! The tool boundary: placement on any connected volume, per-row validation that names every
//! problem row, the bounded existence check, and the evidence guardrail that refuses a WHOLE
//! plan rather than staging part of one.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::deadline::Deadline;
use cmdr_fs::volume::ConnectionState;

use super::super::plan::{
    ProposalRefusal, Rejected, RenameInput, check_plan, check_row_evidence, destination_key, is_archive_inner_path,
    plan_draft, refusal_content, refusal_reason, rows_problem, validate_destination_name,
};
use super::super::rows::{RowProblem, RowRejection, is_plain_path};
use super::super::sources::check_sources;
use super::super::store::RenameDraft;
use super::{THREAD, draft_row};
use crate::file_system::volume::{InMemoryVolume, Volume};
use crate::location::miss::MissToken;
use crate::location::resolve::{IndexSources, VolumeFact, VolumeFacts};
use crate::location::share_identity::ShareIdentity;
use crate::network::server_guid::GuidProof;
use crate::test_support::SlowVolume;
use crate::tools::propose::evidence::{EvidenceProblem, EvidenceSource, ImageFactsLedger};
use crate::tools::resources::volumes::VolumeKind;

/// One volume as the resolver knows it, with no index and no share identity.
fn volume(id: &str, kind: VolumeKind, root: &str, live: bool) -> VolumeFact {
    VolumeFact {
        id: id.to_string(),
        name: id.to_string(),
        kind,
        root: root.to_string(),
        live,
        connection_state: None,
        earlier_roots: Vec::new(),
        index: IndexSources::none(id),
        share: None,
    }
}

/// The boot disk plus a connected share and one that isn't connected. No pane shows any of them:
/// a plan doesn't need one to.
fn facts() -> VolumeFacts {
    VolumeFacts {
        volumes: vec![
            volume("root", VolumeKind::Local, "/", true),
            volume("naspi", VolumeKind::Smb, "/Volumes/naspi", true),
            volume("attic", VolumeKind::Smb, "/Volumes/attic", false),
        ],
        mount_areas: vec!["/Volumes".to_string()],
    }
}

/// A plan of `filename`-evidenced rows, each `(sourcePath, volumeId, destinationName)`.
fn plan(rows: &[(&str, &str, &str)]) -> serde_json::Value {
    let renames: Vec<serde_json::Value> = rows
        .iter()
        .map(|(source, volume_id, name)| {
            serde_json::json!({
                "sourcePath": source, "volumeId": volume_id, "destinationName": name,
                "evidence": { "source": "filename", "detail": "the old name" }
            })
        })
        .collect();
    serde_json::json!({ "renames": renames })
}

/// The pure checks' draft, panicking on any refusal or rejected row.
fn placed(params: &serde_json::Value, facts: &VolumeFacts) -> RenameDraft {
    let (draft, rejected) = plan_draft(params, facts, &ImageFactsLedger::default(), THREAD)
        .unwrap_or_else(|refusal| panic!("refused: {}", refusal_reason(&refusal)));
    assert!(
        rejected.rows.is_empty() && rejected.evidence.is_empty(),
        "rows were rejected"
    );
    draft
}

/// Every row the pure checks reject, whether rows survive them or not.
fn rejected_rows(params: &serde_json::Value) -> Vec<RowRejection> {
    match plan_draft(params, &facts(), &ImageFactsLedger::default(), THREAD) {
        Ok((_, rejected)) | Err(ProposalRefusal::Rejected(rejected)) => rejected.rows,
        Err(refusal) => panic!("expected rejected rows, got {}", refusal_reason(&refusal)),
    }
}

fn refused_miss(refusal: ProposalRefusal) -> MissToken {
    match refusal {
        ProposalRefusal::Location(miss) => miss.miss,
        _ => panic!("expected a location refusal, got {}", refusal_reason(&refusal)),
    }
}

fn named(rejections: &[RowRejection]) -> Vec<(&str, RowProblem)> {
    rejections
        .iter()
        .map(|rejection| (rejection.source_path.as_str(), rejection.problem))
        .collect()
}

fn ten_seconds() -> Deadline {
    Deadline::new(Duration::from_secs(10))
}

/// David's case (#213): chat about `~/Downloads`, move the panes elsewhere, then ask for the
/// invoices in a third folder. The plan stands on its own folder, whatever the panes show.
#[test]
fn a_plan_targets_any_folder_on_a_connected_volume_whatever_the_panes_show() {
    let draft = placed(
        &plan(&[
            ("/Users/david/Downloads/invoices/a.pdf", "root", "2026-01 Invoice.pdf"),
            ("/Users/david/Downloads/invoices/b.pdf", "root", "2026-02 Invoice.pdf"),
        ]),
        &facts(),
    );
    assert_eq!(draft.volume_id, "root");
    assert_eq!(draft.parent, "/Users/david/Downloads/invoices");
    assert_eq!(draft.rows.len(), 2);

    let remote = placed(
        &plan(&[("/Volumes/naspi/scans/c.pdf", "naspi", "c renamed.pdf")]),
        &facts(),
    );
    assert_eq!(remote.volume_id, "naspi");
    assert_eq!(remote.parent, "/Volumes/naspi/scans");
}

/// A share remounted at a new path (macOS's `naspi-1`) still holds the files the model named
/// at the old one: the stored rows say where they are NOW, because preflight and apply read
/// the stored rows.
#[test]
fn a_rebased_folder_rewrites_every_row_onto_the_volumes_current_root() {
    let mut facts = facts();
    let naspi = facts
        .volumes
        .iter_mut()
        .find(|volume| volume.id == "naspi")
        .expect("naspi");
    naspi.root = "/Volumes/naspi-1".into();
    naspi.earlier_roots = vec!["/Volumes/naspi".into()];

    let draft = placed(
        &plan(&[
            ("/Volumes/naspi/scans/a.pdf", "naspi", "x.pdf"),
            ("/Volumes/naspi/scans/b.pdf", "naspi", "y.pdf"),
        ]),
        &facts,
    );

    assert_eq!(draft.volume_id, "naspi");
    assert_eq!(draft.parent, "/Volumes/naspi-1/scans");
    let sources: Vec<&str> = draft.rows.iter().map(|row| row.source_path.as_str()).collect();
    assert_eq!(
        sources,
        ["/Volumes/naspi-1/scans/a.pdf", "/Volumes/naspi-1/scans/b.pdf"]
    );
}

const BOX: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";

/// An SMB share on `/Volumes/<mount>`, its GUID recorded by a signed-in session, opened as david.
fn share(id: &str, mount: &str, live: bool) -> VolumeFact {
    VolumeFact {
        connection_state: Some(if live {
            ConnectionState::Direct
        } else {
            ConnectionState::Saved
        }),
        share: Some(ShareIdentity::new(
            Some((BOX, GuidProof::Authenticated)),
            "naspi",
            Some("david"),
        )),
        ..volume(id, VolumeKind::Smb, &format!("/Volumes/{mount}"), live)
    }
}

/// The NAS over Tailscale isn't connected, the same NAS over the LAN is: the plan lands on the
/// live address, as `propose_suggestions` re-addresses, with every row moved onto it.
#[test]
fn a_folder_on_a_dead_address_of_a_live_share_moves_onto_the_live_one() {
    let mut facts = facts();
    facts.volumes.push(share("ts", "naspi-ts", false));
    facts.volumes.push(share("lan", "naspi-lan", true));

    let draft = placed(&plan(&[("/Volumes/naspi-ts/scans/a.pdf", "ts", "x.pdf")]), &facts);

    assert_eq!(draft.volume_id, "lan", "stored on the address that can run it");
    assert_eq!(draft.parent, "/Volumes/naspi-lan/scans");
    assert_eq!(draft.rows[0].source_path, "/Volumes/naspi-lan/scans/a.pdf");
}

/// Every refusal that's about rows names them: the stray row is the one named, ❌ never the
/// rows that agree, and one answer carries every problem so the model fixes them in one retry.
#[test]
fn a_refusal_names_every_problem_row_and_only_those() {
    let rejections = rejected_rows(&plan(&[
        ("/shots/a.png", "root", "one.png"),
        ("/shots/b.png", "root", "two.png"),
        ("/shots/c.png", "root", "three.png"),
        ("/elsewhere/d.png", "root", "four.png"),
        ("/shots/e.png", "naspi", "five.png"),
        ("/shots/a.png", "root", "six.png"),
        ("/shots/f.png", "root", "ONE.png"),
        ("/shots/../g.png", "root", "seven.png"),
        ("/shots/h.png", "root", "sub/eight.png"),
        ("/shots/trip.zip/i.png", "root", "nine.png"),
    ]));

    assert_eq!(
        named(&rejections),
        [
            ("/elsewhere/d.png", RowProblem::OtherFolder),
            ("/shots/e.png", RowProblem::OtherVolume),
            ("/shots/a.png", RowProblem::RepeatedSource),
            ("/shots/f.png", RowProblem::RepeatedName),
            ("/shots/../g.png", RowProblem::NotAPlainPath),
            ("/shots/h.png", RowProblem::InvalidName),
            ("/shots/trip.zip/i.png", RowProblem::InsideArchive),
        ]
    );

    let content = refusal_content(&ProposalRefusal::Rejected(Rejected {
        rows: rejections,
        evidence: Vec::new(),
    }));
    assert_eq!(content["readyForReview"], false);
    assert!(
        content.get("evidenceRejected").is_none(),
        "no evidence problem to report"
    );
    let folder = content["rowsRejected"]
        .as_array()
        .expect("grouped by problem")
        .iter()
        .find(|group| group["problem"] == "otherFolder")
        .expect("the stray folder is named");
    assert_eq!(folder["sourcePaths"], serde_json::json!(["/elsewhere/d.png"]));
    assert!(folder["detail"].is_string(), "the model reads why");
}

/// On a case-insensitive disk `A.png` and `a.png` are one file, and so are a name's two Unicode
/// forms: two rows for it would reach the review as two renames of one file, one bound to fail
/// at apply. The later row is refused, by the same fold every proposal name uses.
#[test]
fn case_or_unicode_variants_of_one_source_are_one_row() {
    let rejections = rejected_rows(&plan(&[
        ("/shots/A.png", "root", "one.png"),
        ("/shots/a.png", "root", "two.png"),
        ("/shots/Caf\u{e9}.png", "root", "three.png"),
        ("/shots/Cafe\u{301}.png", "root", "four.png"),
    ]));

    assert_eq!(
        named(&rejections),
        [
            ("/shots/a.png", RowProblem::RepeatedSource),
            ("/shots/Cafe\u{301}.png", RowProblem::RepeatedSource),
        ]
    );
}

#[test]
fn a_plain_path_is_a_full_path_with_no_empty_or_dot_segments() {
    for plain in ["/a.png", "/shots/a.png", "sftp://host/shots/a.png"] {
        assert!(is_plain_path(plain), "{plain}");
    }
    for not_plain in [
        "a.png",
        "shots/a.png",
        "~/a.png",
        "/",
        "/shots/",
        "/shots//a.png",
        "/shots/./a.png",
        "/shots/../a.png",
        "sftp://",
    ] {
        assert!(!is_plain_path(not_plain), "{not_plain}");
    }
}

/// A volume id Cmdr doesn't know, a path off the named volume, and a share that isn't
/// connected each refuse in the read tools' miss shape, so the model gets the same next moves
/// (`list_volumes`, `nearby`) it would from `list_dir`.
#[test]
fn a_folder_off_every_connected_volume_refuses_in_the_miss_shape() {
    let refuse = |source: &str, volume_id: &str| {
        refused_miss(
            plan_draft(
                &plan(&[(source, volume_id, "b.png")]),
                &facts(),
                &ImageFactsLedger::default(),
                THREAD,
            )
            .map(|_| ())
            .expect_err("refused"),
        )
    };

    assert_eq!(refuse("/shots/a.png", "nas-2"), MissToken::UnknownVolume);
    assert_eq!(refuse("/shots/a.png", "naspi"), MissToken::PathOutsideVolume);
    assert_eq!(
        refuse("/Volumes/attic/old/a.png", "attic"),
        MissToken::VolumeNotConnected
    );

    let content = refusal_content(&ProposalRefusal::Location(Box::new(
        crate::location::miss::LocationMissResult::from(crate::location::resolve::LocationMiss::UnknownVolume {
            requested: crate::location::resolve::LocationInput {
                volume_id: Some("nas-2".into()),
                path: "/shots".into(),
            },
        }),
    )));
    assert_eq!(content["readyForReview"], false);
    assert_eq!(content["miss"], "unknownVolume");
    assert!(content["detail"].is_string());
}

// ── The bounded existence check ───────────────────────────────────────────────

/// A share (any volume but the boot disk takes this same path) with one invented name among
/// real files: the invented row stays, frozen with no snapshot, and preflight later shows it as
/// "(doesn't exist)". One made-up name no longer costs the plan.
#[tokio::test(flavor = "multi_thread")]
async fn a_missing_file_on_a_remote_volume_stays_in_the_plan() {
    let share = InMemoryVolume::new("naspi");
    share
        .create_file(Path::new("/scans/real.pdf"), b"seven!!")
        .await
        .expect("seed file");
    let mut rows = vec![
        draft_row("/scans/real.pdf", "a.pdf", EvidenceSource::Filename, "the old name"),
        draft_row("/scans/invented.pdf", "b.pdf", EvidenceSource::Filename, "the old name"),
    ];

    let rejections = check_sources("naspi", Arc::new(share), &mut rows, &ten_seconds()).await;

    assert!(
        rejections.is_empty(),
        "a missing file doesn't refuse the plan: {rejections:?}"
    );
    assert_eq!(rows[0].snapshot.as_ref().and_then(|snapshot| snapshot.size), Some(7));
    assert!(
        rows[1].snapshot.is_none(),
        "a missing file freezes as no snapshot, never a zero"
    );
}

/// A plan where no row is a file names the wrong folder or invented every name, and a folder
/// is never a rename row. A folder among missing files reports BOTH in one answer, so the
/// model doesn't learn about the missing ones one retry later.
#[tokio::test(flavor = "multi_thread")]
async fn every_source_missing_or_a_folder_refuses_by_row_in_one_answer() {
    let share = InMemoryVolume::new("naspi");
    share
        .create_directory_all(Path::new("/scans/2026"))
        .await
        .expect("seed folder");
    share
        .create_file(Path::new("/scans/real.pdf"), b"x")
        .await
        .expect("seed file");
    let share: Arc<dyn Volume> = Arc::new(share);

    let mut gone = vec![
        draft_row("/scans/x.pdf", "a.pdf", EvidenceSource::Filename, "the old name"),
        draft_row("/scans/2026", "b", EvidenceSource::Filename, "the old name"),
    ];
    let rejections = check_sources("naspi", share.clone(), &mut gone, &ten_seconds()).await;
    assert_eq!(
        named(&rejections),
        [
            ("/scans/2026", RowProblem::Folder),
            ("/scans/x.pdf", RowProblem::Missing)
        ]
    );

    let mut with_a_file = vec![
        draft_row("/scans/real.pdf", "a.pdf", EvidenceSource::Filename, "the old name"),
        draft_row("/scans/2026", "b", EvidenceSource::Filename, "the old name"),
        draft_row("/scans/x.pdf", "c.pdf", EvidenceSource::Filename, "the old name"),
    ];
    let rejections = check_sources("naspi", share, &mut with_a_file, &ten_seconds()).await;
    assert_eq!(
        named(&rejections),
        [("/scans/2026", RowProblem::Folder)],
        "a real file among them: the missing one stays a review row"
    );
}

/// A folder of a restic backup's snapshot view isn't on disk, so a plan there finds nothing. The
/// refusal says why, so the model tells the user backups are read-only instead of hunting for
/// "the right names" it already has.
#[tokio::test(flavor = "multi_thread")]
async fn nothing_found_under_a_snapshot_view_says_it_is_a_backup() {
    let disk: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Macintosh HD"));
    let mut rows = vec![draft_row(
        "/backups/repo/Browse snapshots/2026-11-01/a.pdf",
        "x.pdf",
        EvidenceSource::Filename,
        "the old name",
    )];

    let rejections = check_sources("root", disk, &mut rows, &ten_seconds()).await;

    assert_eq!(
        named(&rejections),
        [(
            "/backups/repo/Browse snapshots/2026-11-01/a.pdf",
            RowProblem::MissingUnderSnapshots
        )]
    );
}

/// A share whose every stat takes far longer than any deadline here: a dead mount, as far as a
/// check can tell. Finite (not `WedgedVolume`) so its detached stats end and the test's runtime
/// can shut down.
fn dead_share() -> Arc<dyn Volume> {
    Arc::new(SlowVolume::new(InMemoryVolume::new("attic"), Duration::from_secs(3)))
}

/// The dead-mount guarantee, kept as a bounded wait: a share that never answers costs the turn
/// the deadline, then refuses every row it couldn't check as `unreachable`, ❌ never hangs and
/// ❌ never stages the rows as missing.
#[tokio::test(flavor = "multi_thread")]
async fn a_dead_mount_costs_the_deadline_and_refuses_its_rows_as_unreachable() {
    let dead: Arc<dyn Volume> = dead_share();
    let mut rows = vec![
        draft_row("/old/a.pdf", "x.pdf", EvidenceSource::Filename, "the old name"),
        draft_row("/old/b.pdf", "y.pdf", EvidenceSource::Filename, "the old name"),
    ];
    let started = std::time::Instant::now();

    let rejections = check_sources("attic", dead, &mut rows, &Deadline::new(Duration::from_millis(300))).await;

    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the check waited {:?}, past its deadline",
        started.elapsed()
    );
    assert_eq!(rejections.len(), 2);
    assert!(
        rejections.iter().all(|r| r.problem == RowProblem::Unreachable),
        "{rejections:?}"
    );
}

/// A dead share's stragglers hold their stat permits until the kernel lets go, which may be
/// never. They must hold only THAT share's: a plan on the boot disk right after still gets its
/// stats, rather than waiting out the deadline and refusing every row as `unreachable`.
#[tokio::test(flavor = "multi_thread")]
async fn a_dead_share_never_starves_a_plan_on_another_volume() {
    let dead: Arc<dyn Volume> = dead_share();
    let mut stuck: Vec<_> = (0..20)
        .map(|n| {
            draft_row(
                &format!("/old/{n}.pdf"),
                &format!("{n}-x.pdf"),
                EvidenceSource::Filename,
                "the old name",
            )
        })
        .collect();
    let _ = check_sources("attic", dead, &mut stuck, &Deadline::new(Duration::from_millis(200))).await;

    let disk = InMemoryVolume::new("Macintosh HD");
    disk.create_file(Path::new("/Users/david/a.pdf"), b"here")
        .await
        .expect("seed file");
    let mut rows = vec![draft_row(
        "/Users/david/a.pdf",
        "b.pdf",
        EvidenceSource::Filename,
        "the old name",
    )];
    let rejections = check_sources(
        "root",
        Arc::new(disk),
        &mut rows,
        &Deadline::new(Duration::from_secs(2)),
    )
    .await;

    assert!(rejections.is_empty(), "{rejections:?}");
    assert!(rows[0].snapshot.is_some(), "the boot disk's stat answered");
}

/// The whole path a call takes, short of the app's globals: the pure checks, the placement, and
/// the stat, with every row any of them rejected in ONE refusal.
#[tokio::test(flavor = "multi_thread")]
async fn the_whole_check_stages_a_good_plan_and_refuses_a_bad_one_in_one_answer() {
    let share = InMemoryVolume::new("naspi");
    share
        .create_file(Path::new("/Volumes/naspi/scans/real.pdf"), b"x")
        .await
        .expect("seed file");
    share
        .create_directory_all(Path::new("/Volumes/naspi/scans/2026"))
        .await
        .expect("seed folder");
    let share: Arc<dyn Volume> = Arc::new(share);
    let volume_for = |id: &str| (id == "naspi").then(|| share.clone());

    let draft = check_plan(
        &plan(&[
            ("/Volumes/naspi/scans/real.pdf", "naspi", "a.pdf"),
            ("/Volumes/naspi/scans/invented.pdf", "naspi", "b.pdf"),
        ]),
        &facts(),
        &ImageFactsLedger::default(),
        THREAD,
        volume_for,
        &ten_seconds(),
    )
    .await
    .unwrap_or_else(|refusal| panic!("staged: {}", refusal_reason(&refusal)));
    assert_eq!(draft.volume_id, "naspi");
    assert_eq!(draft.rows.len(), 2, "the invented row stays, for the review to mark");

    let refusal = check_plan(
        &plan(&[
            ("/Volumes/naspi/scans/real.pdf", "naspi", "a.pdf"),
            ("/Volumes/naspi/scans/2026", "naspi", "b"),
            ("/Volumes/naspi/elsewhere/c.pdf", "naspi", "c.pdf"),
        ]),
        &facts(),
        &ImageFactsLedger::default(),
        THREAD,
        volume_for,
        &ten_seconds(),
    )
    .await
    .map(|_| ())
    .expect_err("refused");
    let ProposalRefusal::Rejected(rejected) = refusal else {
        panic!("expected rejected rows, got {}", refusal_reason(&refusal));
    };
    assert_eq!(
        named(&rejected.rows),
        [
            ("/Volumes/naspi/elsewhere/c.pdf", RowProblem::OtherFolder),
            ("/Volumes/naspi/scans/2026", RowProblem::Folder),
        ],
        "the row's own problem and the stat's, in one answer"
    );
}

/// The log line for a row refusal names problems and paths, never a proposed name.
#[test]
fn a_row_refusal_log_line_names_problems_and_paths_but_never_names() {
    let reason = refusal_reason(&ProposalRefusal::Rejected(Rejected {
        rows: vec![
            RowRejection::new("/scans/a.pdf", RowProblem::Unreachable),
            RowRejection::new("/scans/b.pdf", RowProblem::Unreachable),
        ],
        evidence: Vec::new(),
    }));
    assert!(reason.contains("Unreachable"), "{reason}");
    assert!(
        reason.contains("/scans/a.pdf") && reason.contains("/scans/b.pdf"),
        "{reason}"
    );
}

#[test]
fn only_a_path_inside_an_archive_is_refused_never_the_archive_file_itself() {
    // Inside: no file on disk for the executor to rename.
    assert!(is_archive_inner_path("/photos/trip.zip/DSC_0001.jpg"));
    assert!(is_archive_inner_path("/docs/report.docx/word/document.xml"));

    // The archive FILE is an ordinary file, and renaming one is an ordinary
    // rename. Asking the wider "does any component carry an archive suffix"
    // refused these, which since `.docx` became a browsable container would have
    // meant refusing a bulk rename of every Office document in a folder.
    assert!(!is_archive_inner_path("/photos/trip.zip"));
    assert!(!is_archive_inner_path("/docs/report.docx"));
    assert!(!is_archive_inner_path("/docs/sheet.xlsx"));
    assert!(!is_archive_inner_path("/apps/lib.jar"));

    // Ordinary paths are untouched either way.
    assert!(!is_archive_inner_path("/photos/DSC_0001.jpg"));
    assert!(!is_archive_inner_path("/photos"));
}

#[test]
fn destination_names_reject_paths_and_dot_entries() {
    for name in ["", ".", "..", "folder/name.png", "folder\\name.png"] {
        assert!(validate_destination_name(name).is_err(), "{name}");
    }
    assert!(validate_destination_name("2026-07-20 - Receipt.png").is_ok());
}

/// Two destination names a filesystem may consider one are one, on every platform: the same
/// fold a move's or copy's names and the transfer engine use (`cmdr_fs::name_fold::fold_name`).
/// The machine-local fold this replaced was identity off macOS, so on Linux a case-only
/// duplicate staged and then collided on a case-insensitive share.
#[test]
fn destination_names_are_one_name_ignoring_case_and_unicode_form_on_every_platform() {
    assert_eq!(destination_key("Invoice.pdf"), destination_key("invoice.pdf"));
    assert_eq!(destination_key("Caf\u{e9}.pdf"), destination_key("Cafe\u{301}.pdf"));
    assert_eq!(
        destination_key("Cafe\u{301}.PDF"),
        cmdr_fs::name_fold::fold_name("caf\u{e9}.pdf"),
        "one fold: NFC, then lowercase"
    );
    assert_ne!(destination_key("a.pdf"), destination_key("b.pdf"));
}

#[test]
fn one_unbacked_content_claim_rejects_that_row_and_refuses_the_whole_plan() {
    // The incident's shape at the plan boundary: one row cites image text nobody delivered,
    // its neighbours are fine. Every offending row comes back named, and the caller refuses
    // the WHOLE plan rather than staging a partial one the user would read as complete.
    let ledger = ImageFactsLedger::default();
    ledger.record_delivered(
        THREAD,
        "call-1",
        &serde_json::json!({ "status": "ok", "facts": [
            { "path": "/shots/one.png", "state": "indexed", "text": "Invoice 4021 total 250 SEK" }
        ] }),
    );
    let mut rows = vec![
        draft_row(
            "/shots/one.png",
            "Invoice 4021.png",
            EvidenceSource::ImageText,
            "Invoice 4021",
        ),
        draft_row(
            "/shots/two.png",
            "hello-world-output.png",
            EvidenceSource::ImageText,
            "hello world output",
        ),
        draft_row(
            "/shots/three.png",
            "2026-07-20.png",
            EvidenceSource::Metadata,
            "Taken 2026-07-20",
        ),
    ];

    let rejections = check_row_evidence(&ledger, THREAD, &mut rows).expect_err("the plan is refused");

    assert_eq!(rejections.len(), 1, "only the unbacked row is rejected");
    assert_eq!(rejections[0].source_path, "/shots/two.png");
    assert_eq!(rejections[0].proposed_name, "hello-world-output.png");
    assert_eq!(rejections[0].problem, EvidenceProblem::FactsNotDelivered);

    // A rejected row is never silently dropped: the model gets the typed verdict, and
    // nothing was staged, so the user sees no plan at all.
    let content = refusal_content(&ProposalRefusal::Rejected(Rejected {
        rows: Vec::new(),
        evidence: rejections,
    }));
    assert_eq!(content["readyForReview"], false);
    assert_eq!(content["evidenceRejected"][0]["problem"], "factsNotDelivered");
    assert_eq!(content["evidenceRejected"][0]["evidenceSource"], "imageText");
    assert!(
        content["guidance"].is_string(),
        "the model gets something it can act on"
    );
}

/// The tool can't be called without evidence at all: an old-shaped plan is a param refusal,
/// not a plan with an empty justification.
#[test]
fn a_rename_row_without_evidence_does_not_parse() {
    let without = serde_json::json!({ "sourcePath": "/x/a.png", "volumeId": "root", "destinationName": "b.png" });
    assert!(serde_json::from_value::<RenameInput>(without).is_err());

    let with = serde_json::json!({
        "sourcePath": "/x/a.png", "volumeId": "root", "destinationName": "b.png",
        "evidence": { "source": "filename", "detail": "the old name" }
    });
    assert!(serde_json::from_value::<RenameInput>(with).is_ok());
}

/// From a live transcript: the model sent a plan whose rows all lacked `volumeId`, got back one
/// fixed sentence naming all four fields, and re-sent the identical call eight times until the
/// turn's budget ran out. A refusal has to say WHICH rows and WHICH field, or a model with
/// nothing new to read has nothing new to try.
#[test]
fn a_row_that_misses_a_field_is_refused_by_row_and_by_field() {
    let params = serde_json::json!({ "renames": [
        { "sourcePath": "/x/a.png", "destinationName": "b.png", "evidence": { "source": "filename", "detail": "old" } },
        { "sourcePath": "/x/c.png", "destinationName": "d.png", "evidence": { "source": "filename", "detail": "old" } }
    ]});
    assert_eq!(
        rows_problem(&params).expect("the rows don't match the declared row shape"),
        "Every rename row needs volumeId. A row takes destinationName, evidence, sourcePath, and volumeId."
    );
}

#[test]
fn a_refusal_names_the_rows_it_means_and_keeps_the_kinds_apart() {
    let params = serde_json::json!({ "renames": [
        { "sourcePath": "/x/a.png", "volumeId": "root", "destinationName": "b.png",
          "evidence": { "source": "filename", "detail": "old" } },
        { "sourcePath": "/x/c.png", "volumeId": "root", "destinationName": "d.png",
          "evidence": { "source": "filename", "detail": "old" }, "note": "why not" },
        { "sourcePath": "/x/e.png", "volumeId": "root",
          "evidence": { "source": "filename", "detail": "old" } }
    ]});
    assert_eq!(
        rows_problem(&params).expect("row 2 carries an extra field and row 3 misses one"),
        "Rename row 2 has no note parameter, and rename row 3 needs destinationName. \
         A row takes destinationName, evidence, sourcePath, and volumeId."
    );
}

#[test]
fn a_plan_whose_rows_all_check_out_has_nothing_to_report() {
    // Whatever else is wrong with the call (a bad name, a row out of scope, unbacked
    // evidence), it isn't the row SHAPE, and this must not claim otherwise.
    let params = serde_json::json!({ "renames": [
        { "sourcePath": "/x/a.png", "volumeId": "root", "destinationName": "b.png",
          "evidence": { "source": "filename", "detail": "old" } }
    ]});
    assert_eq!(rows_problem(&params), None);
    assert_eq!(rows_problem(&serde_json::json!({ "renames": "not an array" })), None);
}

/// `userEdited` is the review dialog's word for a name the USER typed, so the model may never
/// send it: a plan that did would put "You typed this name" beside a name it invented itself.
#[test]
fn a_plan_cannot_claim_the_user_typed_a_name() {
    let mut rows = vec![draft_row(
        "/shots/one.png",
        "hand-typed.png",
        EvidenceSource::UserEdited,
        "the user typed this",
    )];

    let rejections = check_row_evidence(&ImageFactsLedger::default(), THREAD, &mut rows).expect_err("refused");

    assert_eq!(rejections.len(), 1);
    assert_eq!(rejections[0].problem, EvidenceProblem::SourceReservedForUser);
}

/// The log line a refused plan leaves names the offending PATHS and the typed verdict, and
/// never the name the model proposed. A proposed name is where a model's reading of a file's
/// CONTENTS ends up, and the log is written unconditionally and rides an error report; paths
/// are already in the log in the ordinary course, contents-derived names are not.
#[test]
fn a_refusal_log_line_names_the_path_and_the_typed_verdict_but_never_the_proposed_name() {
    let rejections = vec![crate::tools::propose::evidence::EvidenceRejection {
        source_path: "/shots/two.png".to_string(),
        proposed_name: "hello-world-output.png".to_string(),
        evidence_source: EvidenceSource::ImageText,
        problem: EvidenceProblem::FactsNotDelivered,
    }];

    let reason = refusal_reason(&ProposalRefusal::Rejected(Rejected {
        rows: Vec::new(),
        evidence: rejections,
    }));

    assert!(reason.contains("/shots/two.png"), "the path locates the row: {reason}");
    assert!(
        reason.contains("FactsNotDelivered"),
        "the typed verdict says what went wrong: {reason}"
    );
    assert!(
        !reason.contains("hello-world-output"),
        "the proposed name must not reach the log: {reason}"
    );
}
