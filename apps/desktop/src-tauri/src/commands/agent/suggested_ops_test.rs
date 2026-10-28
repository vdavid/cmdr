//! Tests for `suggested_ops.rs`: the view mappings, the destination check, and the wire shapes.

use super::*;
use crate::agent::store::proposals::ProposalGroup;

/// Every refusal sends the user somewhere different, so the mapping is the thing worth
/// pinning: "somebody already answered" closes the group, "the list changed" sends them back
/// to re-read it, and an unmounted drive is neither. Collapsing any two would make the
/// dialog say the wrong thing in one of the cases.
#[test]
fn each_refusal_keeps_the_recovery_it_implies() {
    assert!(
        matches!(
            refusal_view(ApprovalRefusal::NotAccepted(AcceptanceOutcome::RevisionChanged {
                shown: 0,
                found: 1
            })),
            ApprovalResultView::ListChanged
        ),
        "an amended group is re-readable, never already answered"
    );
    assert!(matches!(
        refusal_view(ApprovalRefusal::NotAccepted(AcceptanceOutcome::NotPending {
            found: ProposalStatus::Approved
        })),
        ApprovalResultView::AlreadyAnswered
    ));
    assert!(matches!(
        refusal_view(ApprovalRefusal::Claim(ClaimRefusal::StaleStatus {
            found: ProposalStatus::Rejected
        })),
        ApprovalResultView::AlreadyAnswered
    ));
    assert!(
        matches!(
            refusal_view(ApprovalRefusal::Claim(ClaimRefusal::BindingMismatch {
                accepted: None,
                live: crate::agent::store::proposals::OpBinding {
                    op_count: 3,
                    digest: "abc".into()
                }
            })),
            ApprovalResultView::ListChanged
        ),
        "a changed op set is re-readable, never an already-answered group"
    );
    assert!(matches!(
        refusal_view(ApprovalRefusal::NotAccepted(AcceptanceOutcome::Unknown)),
        ApprovalResultView::Unknown
    ));
    assert!(matches!(
        refusal_view(ApprovalRefusal::Claim(ClaimRefusal::Unknown)),
        ApprovalResultView::Unknown
    ));
}

/// Every refusal to START reaches the dialog as a reason it can word, â never a sentence
/// it can only log (#184). An unconnected phone keeps the transfer dialogs' own variant, so
/// the suggestion reads exactly as a clicked copy off that phone would.
#[test]
fn a_refusal_to_start_carries_a_reason_the_dialog_can_word() {
    assert!(matches!(
        refusal_view(ApprovalRefusal::NotAccepted(AcceptanceOutcome::NothingSelected)),
        ApprovalResultView::NothingSelected
    ));
    assert!(matches!(
        refusal_view(ApprovalRefusal::SourceUnavailable(
            WriteOperationError::SourceNotConnected {
                path: "/DCIM/one.jpg".into()
            }
        )),
        ApprovalResultView::Refused {
            error: WriteOperationError::SourceNotConnected { .. }
        }
    ));
    assert!(matches!(
        refusal_view(ApprovalRefusal::Rename(RenameStartError::Engine {
            error: WriteOperationError::SourceNoLongerConnected { path: "/a.png".into() }
        })),
        ApprovalResultView::Refused {
            error: WriteOperationError::SourceNoLongerConnected { .. }
        }
    ));
    assert!(matches!(
        refusal_view(ApprovalRefusal::Rename(RenameStartError::NothingToRename)),
        ApprovalResultView::NothingToRun
    ));
    assert!(matches!(
        refusal_view(ApprovalRefusal::TargetMissing {
            verb: ProposalVerb::Move
        }),
        ApprovalResultView::CouldNotStart
    ));
}

fn group(id: i64, set_id: i64, verb: ProposalVerb) -> GroupSummary {
    GroupSummary {
        group: ProposalGroup {
            id,
            set_id,
            seq: 0,
            verb,
            status: ProposalStatus::Pending,
            source_volume_id: "root".into(),
            destination: Some("/Users/someone/Documents/Invoices".into()),
            destination_volume_id: None,
            reversible: Reversibility::RestoreMove,
            display_name: "five invoices".into(),
            rationale: Some("They all look like invoices.".into()),
            selector: None,
            created_at: 100,
            decided_at: None,
            revision: 0,
            source_folder: Some("/Users/someone/Downloads".into()),
        },
        live_op_count: 5,
        total_op_count: 7,
        server: None,
    }
}

/// Read-time facts that say nothing beyond the destination state.
fn facts(destination_state: DestinationState) -> GroupReadFacts {
    GroupReadFacts {
        destination_state,
        source_volume_name: None,
        destination_volume_name: None,
        runs_as: None,
    }
}

/// Two registered volumes, named as their backends name them: an SMB share (no Trash) and the
/// boot disk (which `LocalPosixVolume` reports as keeping one).
fn registered(id: &str) -> Option<std::sync::Arc<dyn Volume>> {
    use cmdr_fs::volume::InMemoryVolume;
    match id {
        "smb-naspi" => Some(std::sync::Arc::new(InMemoryVolume::new("naspi on 192.168.1.5"))),
        "root" => Some(std::sync::Arc::new(TrashKeeping(InMemoryVolume::new("Macintosh HD")))),
        _ => None,
    }
}

fn saved_name(id: &str) -> Option<String> {
    (id == "smb-saved").then(|| "archive on 192.168.1.5".to_string())
}

fn volume_facts() -> VolumeFacts<'static> {
    VolumeFacts {
        registered: &registered,
        saved_name: &saved_name,
    }
}

/// An in-memory volume that keeps a Trash, as a local disk does.
struct TrashKeeping(cmdr_fs::volume::InMemoryVolume);

impl Volume for TrashKeeping {
    fn name(&self) -> &str {
        self.0.name()
    }
    fn root(&self) -> &Path {
        self.0.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(cmdr_fs::volume::ListingProgress) + Sync)>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Vec<cmdr_fs::entry::FileEntry>, VolumeError>> + Send + 'a>> {
        self.0.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<cmdr_fs::entry::FileEntry, VolumeError>> + Send + 'a>> {
        self.0.get_metadata(path)
    }
    fn has_trash(&self) -> bool {
        true
    }
}

/// A trash group on a volume with no Trash says it runs as a permanent delete, before anyone
/// approves it: trashing there would refuse every item (#747). The undo promise stays the verb's own.
#[test]
fn a_trash_group_on_a_volume_without_a_trash_says_it_runs_as_a_permanent_delete() {
    let mut trash = group(1, 1, ProposalVerb::Trash).group;
    trash.destination = None;

    let on_root = read_facts(&trash, volume_facts(), DestinationState::NotApplicable);
    trash.source_volume_id = "smb-naspi".into();
    let on_share = read_facts(&trash, volume_facts(), DestinationState::NotApplicable);
    trash.source_volume_id = "smb-saved".into();
    let on_unconnected = read_facts(&trash, volume_facts(), DestinationState::NotApplicable);

    assert_eq!(on_root.runs_as, None);
    assert_eq!(on_share.runs_as, Some(RunsAs::PermanentDelete));
    assert_eq!(
        on_unconnected.runs_as, None,
        "approval refuses an unconnected source with its own typed reason"
    );
}

/// The header names the destination's volume as its backend does, falls back to a saved
/// place's name while nobody has connected it (the QA case), uses the source volume for a
/// rename (whose parent lives there), and names nothing for trash.
#[test]
fn the_destination_volume_is_named_even_before_anyone_connects_it() {
    let mut moving = group(1, 1, ProposalVerb::Move).group;
    moving.destination_volume_id = Some("smb-naspi".into());
    let mut onto_saved = group(2, 1, ProposalVerb::Move).group;
    onto_saved.destination_volume_id = Some("smb-saved".into());
    let mut renaming = group(3, 1, ProposalVerb::Rename).group;
    renaming.destination_volume_id = None;
    let mut trashing = group(4, 1, ProposalVerb::Trash).group;
    trashing.destination = None;

    let name = |group: &ProposalGroup| {
        read_facts(group, volume_facts(), DestinationState::NotApplicable).destination_volume_name
    };
    assert_eq!(name(&moving).as_deref(), Some("naspi on 192.168.1.5"));
    assert_eq!(name(&onto_saved).as_deref(), Some("archive on 192.168.1.5"));
    assert_eq!(name(&renaming).as_deref(), Some("Macintosh HD"));
    assert_eq!(name(&trashing), None);
    assert_eq!(destination_volume_id_of(&moving), Some("smb-naspi"));
    assert_eq!(destination_volume_id_of(&renaming), Some("root"));
    assert_eq!(destination_volume_id_of(&trashing), None);
}

/// A group the agent named on a share that isn't connected is stored on the live, signed-in
/// sibling it was re-addressed onto, so the header names the volume the approval will WRITE
/// to, â never the one the agent asked for.
#[test]
fn a_re_addressed_destination_is_named_as_the_volume_written_to() {
    use crate::location::resolve::{IndexSources, VolumeFact};
    use crate::location::share_identity::ShareIdentity;
    use crate::mcp::resources::volumes::VolumeKind;
    use crate::network::server_guid::GuidProof;

    let nas = |id: &str, root: &str, live: bool| VolumeFact {
        id: id.to_string(),
        name: id.to_string(),
        kind: VolumeKind::Smb,
        root: root.to_string(),
        live,
        connection_state: None,
        earlier_roots: Vec::new(),
        index: IndexSources::none(id),
        share: Some(ShareIdentity::new(
            Some(("6ba7b810-9dad-11d1-80b4-00c04fd430c8", GuidProof::Authenticated)),
            "naspi",
            Some("david"),
        )),
    };
    let facts = crate::location::resolve::VolumeFacts {
        volumes: vec![
            VolumeFact {
                kind: VolumeKind::Local,
                share: None,
                ..nas("root", "/", true)
            },
            nas("smb-saved", "/Volumes/naspi", false),
            nas("smb-naspi", "/Volumes/naspi-1", true),
        ],
        mount_areas: vec!["/Volumes".to_string()],
    };
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
    crate::agent::store::run_migrations(&conn, crate::agent::store::MIGRATIONS).expect("migrate");
    let group_id = crate::agent::tools::suggestions::stage_for_test(
        &conn,
        &serde_json::json!({ "groups": [{
            "verb": "move",
            "sourceVolumeId": "root",
            "displayName": "One invoice",
            "destination": { "volumeId": "smb-saved", "path": "/Volumes/naspi/papers" },
            "paths": ["/Users/someone/Downloads/Invoice-X.pdf"],
        }] }),
        &facts,
    );
    let group = crate::agent::store::proposals::get_group(&conn, group_id)
        .expect("read")
        .expect("exists");

    let read = read_facts(&group, volume_facts(), DestinationState::NotApplicable);
    assert_eq!(read.destination_volume_name.as_deref(), Some("naspi on 192.168.1.5"));
    assert_eq!(destination_volume_id_of(&group), Some("smb-naspi"));
    assert_eq!(group.destination.as_deref(), Some("/Volumes/naspi-1/papers"));
}

fn sweep(id: i64) -> ProposalSweep {
    ProposalSweep {
        id,
        conversation_id: Some(3),
        created_at: 100,
        created_by_model: Some("some-model".into()),
        rationale: Some("Ten new files in Downloads.".into()),
    }
}

/// Every field here is something the user weighs before approving. A mapping that dropped
/// one would take a disclosure off the screen while the dialog still looked complete,
/// which is the failure this whole surface exists to prevent.
#[test]
fn a_group_view_carries_every_fact_the_user_judges_it_by() {
    let view = to_group_view(
        group(7, 2, ProposalVerb::Move),
        GroupReadFacts {
            destination_state: DestinationState::WillBeCreated,
            source_volume_name: Some("Macintosh HD".into()),
            destination_volume_name: Some("Macintosh HD".into()),
            runs_as: Some(RunsAs::PermanentDelete),
        },
        None,
    );

    assert_eq!(view.group_id, 7);
    assert_eq!(view.sweep_id, 2);
    assert_eq!(view.verb, ProposalVerb::Move);
    assert_eq!(view.status, ProposalStatus::Pending);
    assert_eq!(view.display_name, "five invoices");
    assert_eq!(view.rationale.as_deref(), Some("They all look like invoices."));
    assert_eq!(view.source_volume_id, "root");
    assert_eq!(view.destination.as_deref(), Some("/Users/someone/Documents/Invoices"));
    assert_eq!(view.destination_volume_name.as_deref(), Some("Macintosh HD"));
    assert_eq!(view.source_volume_name.as_deref(), Some("Macintosh HD"));
    assert_eq!(
        view.destination_volume_id.as_deref(),
        Some("root"),
        "a move with no destination volume id is on its source volume"
    );
    assert_eq!(view.source_folder.as_deref(), Some("/Users/someone/Downloads"));
    assert_eq!(
        view.reversible,
        Reversibility::RestoreMove,
        "the verb's own promise, from the store"
    );
    assert_eq!(view.runs_as, Some(RunsAs::PermanentDelete));
    assert_eq!(view.destination_state, DestinationState::WillBeCreated);
    assert_eq!(view.live_op_count, 5);
    assert_eq!(view.total_op_count, 7, "the deselected rows are part of the record");
    assert!(!view.from_selector);
}

/// A group a pattern produced says so, because "why is this file here?" has a different
/// answer for a list the user never watched being built.
#[test]
fn a_selector_built_group_reports_its_provenance() {
    let mut summary = group(1, 1, ProposalVerb::Trash);
    summary.group.selector = Some("{\"root\":\"~/Downloads\"}".into());

    assert!(to_group_view(summary, facts(DestinationState::NotApplicable), None).from_selector);
}

/// An index that held nothing must stay absent all the way to the row. A zero would render
/// as an empty file and a 1970 date, both of which are claims nothing can back.
#[test]
fn an_op_with_no_snapshot_reports_absence_rather_than_zero() {
    let bare = to_op_view(ProposalOp {
        id: 4,
        group_id: 1,
        seq: 0,
        source_path: "/Users/someone/Downloads/one.dmg".into(),
        destination: None,
        status: OpStatus::Pending,
        snapshot_size: None,
        snapshot_mtime: None,
        snapshot_inode: None,
    });

    assert_eq!(bare.snapshot_size, None);
    assert_eq!(bare.snapshot_modified, None);
    let wire = serde_json::to_value(&bare).expect("serializes");
    assert!(wire["snapshotSize"].is_null());
    assert!(wire["snapshotModified"].is_null());
}

/// The marker answers for the verbs that create a folder, and stays quiet for the ones
/// where it would mean something else: a rename binds the parent its sources already sit
/// in, and a compress binds an archive path whose overwrite risk `reversible` already
/// carries.
#[test]
fn only_the_verbs_that_can_create_a_folder_answer_the_destination_question() {
    for verb in [ProposalVerb::Move, ProposalVerb::Copy, ProposalVerb::Extract] {
        assert!(destination_check_applies(verb), "{verb:?} binds a destination folder");
    }
    for verb in [
        ProposalVerb::Rename,
        ProposalVerb::Compress,
        ProposalVerb::Trash,
        ProposalVerb::Delete,
    ] {
        assert!(
            !destination_check_applies(verb),
            "{verb:?} must not claim to create one"
        );
    }
}

fn move_to(destination_volume_id: &str, destination: &str) -> ProposalGroup {
    let mut summary = group(1, 1, ProposalVerb::Move);
    summary.group.destination = Some(destination.into());
    summary.group.destination_volume_id = Some(destination_volume_id.into());
    summary.group
}

/// The QA repro (#747): four moves onto a NAS share the switcher showed as `saved` all read
/// "Cmdr couldn't check the target folder". A `saved` row is published only for a share whose
/// volume ISN'T registered, so the lookup found nothing and the check answered `Unknown`
/// without a word. Nothing connected is its own answer, and approving would refuse on it.
#[tokio::test(flavor = "multi_thread")]
async fn a_destination_on_a_saved_share_nobody_connected_says_so() {
    use crate::network::known_shares::{self, AuthOptions, ConnectionMode, KnownNetworkShare};
    let volume_id = "smb-198-51-100-62-445-naspi-suggested-ops-test";
    known_shares::remember_share(KnownNetworkShare {
        server_name: "198.51.100.62".to_string(),
        share_name: "naspi".to_string(),
        protocol: "smb".to_string(),
        last_connected_at: "2026-10-26T00:00:00Z".to_string(),
        last_connection_mode: ConnectionMode::Guest,
        last_known_auth_options: AuthOptions::GuestOrCredentials,
        username: None,
        address: Some("198.51.100.62".to_string()),
        port: Some(445),
        volume_id: Some(volume_id.to_string()),
        mount_path: Some("/Volumes/naspi-suggested-ops-test".to_string()),
        pinned: true,
        server_guid: None,
        server_guid_proof: crate::network::server_guid::GuidProof::Unauthenticated,
    });

    let state = destination_state(&move_to(volume_id, "/Volumes/naspi-suggested-ops-test/Installers")).await;

    assert_eq!(state, DestinationState::NotConnected);
}

/// A connected destination answers from its own volume: there, or created by approving.
#[tokio::test(flavor = "multi_thread")]
async fn a_connected_destination_answers_whether_the_folder_is_there() {
    use crate::file_system::volume::manager::test_support::TestVolumeRegistration;
    use cmdr_fs::volume::{InMemoryVolume, Volume};
    let volume_id = "suggested-ops-destination-test";
    let volume = InMemoryVolume::new("Share").with_root("/Volumes/share-test");
    volume
        .create_directory(Path::new("/Volumes/share-test/Invoices"))
        .await
        .expect("seed folder");
    let _registration = TestVolumeRegistration::install(volume_id, std::sync::Arc::new(volume));

    assert_eq!(
        destination_state(&move_to(volume_id, "/Volumes/share-test/Invoices")).await,
        DestinationState::Exists
    );
    assert_eq!(
        destination_state(&move_to(volume_id, "/Volumes/share-test/Receipts")).await,
        DestinationState::WillBeCreated
    );
}

/// A volume that refuses the check (a dropped session, a permission wall) doesn't know, and
/// must not read as "will be created".
#[tokio::test(flavor = "multi_thread")]
async fn a_destination_whose_volume_refuses_the_check_is_unknown() {
    use crate::file_system::volume::manager::test_support::TestVolumeRegistration;
    use cmdr_fs::volume::{InMemoryVolume, Volume};
    let volume_id = "suggested-ops-destination-refusing-test";
    let volume = InMemoryVolume::new("Share").with_root("/Volumes/refusing-test");
    volume
        .create_directory(Path::new("/Volumes/refusing-test/Invoices"))
        .await
        .expect("seed folder");
    volume.set_stat_failing(Path::new("/Volumes/refusing-test/Invoices"));
    let _registration = TestVolumeRegistration::install(volume_id, std::sync::Arc::new(volume));

    assert_eq!(
        destination_state(&move_to(volume_id, "/Volumes/refusing-test/Invoices")).await,
        DestinationState::Unknown
    );
}

/// A sweep is how the user sees one thing the agent noticed, so its groups arrive under one
/// header however many there are.
#[test]
fn groups_fold_under_the_sweep_that_produced_them() {
    let rows = vec![
        (
            to_group_view(group(1, 10, ProposalVerb::Move), facts(DestinationState::Exists), None),
            Some(sweep(10)),
        ),
        (
            to_group_view(
                group(2, 10, ProposalVerb::Trash),
                facts(DestinationState::NotApplicable),
                None,
            ),
            Some(sweep(10)),
        ),
        (
            to_group_view(
                group(3, 11, ProposalVerb::Delete),
                facts(DestinationState::NotApplicable),
                None,
            ),
            Some(sweep(11)),
        ),
    ];

    let sweeps = group_into_sweeps(rows);

    assert_eq!(sweeps.len(), 2, "two wakes, not three groups: {sweeps:?}");
    assert_eq!(sweeps[0].sweep_id, 10);
    assert_eq!(sweeps[0].groups.len(), 2);
    assert_eq!(sweeps[0].rationale.as_deref(), Some("Ten new files in Downloads."));
    assert_eq!(sweeps[1].sweep_id, 11);
    assert_eq!(sweeps[1].groups.len(), 1);
}

/// A sweep row missing under its own groups can't happen through any path here, but
/// dropping the groups would hide work the user is being asked about. They get a
/// placeholder header instead.
#[test]
fn groups_whose_sweep_row_is_missing_are_still_shown() {
    let rows = vec![(
        to_group_view(group(1, 42, ProposalVerb::Move), facts(DestinationState::Exists), None),
        None,
    )];

    let sweeps = group_into_sweeps(rows);

    assert_eq!(sweeps.len(), 1);
    assert_eq!(sweeps[0].sweep_id, 42);
    assert_eq!(sweeps[0].groups.len(), 1);
    assert_eq!(sweeps[0].rationale, None);
}

/// "The store isn't open" (agent features off) and "the store refused" (a
/// locked database) call for different answers from the dialog, so they stay
/// separate variants rather than two spellings of one sentence.
#[test]
fn a_store_that_isnt_open_is_not_a_store_that_refused() {
    assert_eq!(
        serde_json::to_value(SuggestedOpsError::StoreNotOpen).unwrap(),
        serde_json::json!({ "type": "storeNotOpen" })
    );
    assert_eq!(
        serde_json::to_value(SuggestedOpsError::Store {
            detail: "database is locked".to_string()
        })
        .unwrap(),
        serde_json::json!({ "type": "store", "detail": "database is locked" })
    );
    assert_eq!(
        serde_json::to_value(SuggestedOpsError::ApprovalDidntFinish).unwrap(),
        serde_json::json!({ "type": "approvalDidntFinish" })
    );
}

/// A window is capped server-side, so a caller asking for a 60,000-row "page" can't load or
/// stat the whole group in one call; it pages for the rest.
#[test]
fn an_op_window_is_capped_at_the_servers_ceiling() {
    assert_eq!(op_window(200), 200);
    assert_eq!(op_window(MAX_OP_WINDOW), MAX_OP_WINDOW);
    assert_eq!(op_window(60_000), MAX_OP_WINDOW);
}

/// The source volume is named the way the destination is: the registered volume's own name, a
/// saved place's while nobody connected it, and nothing for an id nobody knows.
#[test]
fn the_source_volume_is_named_even_before_anyone_connects_it() {
    let mut group = group(1, 1, ProposalVerb::Trash).group;
    let name =
        |group: &ProposalGroup| read_facts(group, volume_facts(), DestinationState::NotApplicable).source_volume_name;

    group.source_volume_id = "smb-naspi".into();
    assert_eq!(name(&group).as_deref(), Some("naspi on 192.168.1.5"));
    group.source_volume_id = "smb-saved".into();
    assert_eq!(name(&group).as_deref(), Some("archive on 192.168.1.5"));
    group.source_volume_id = "nothing-knows-this".into();
    assert_eq!(name(&group), None);
}
