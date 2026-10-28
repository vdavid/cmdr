//! Approving a server card: the claim, the checks again against what's saved now, one save, and
//! the group's end by what the save wrote.
//!
//! A fake [`ServerSaver`] answers from the saved-server fixture and records every save it was
//! asked for, so each refusal can prove it never reached a core.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::future::BoxFuture;
use rusqlite::Connection;

use super::test_fixtures::{NFS_ID, S3_ACCOUNT, SFTP_ID, SMB_HOST, SMB_SHARE, saved, sftp_saved, smb_saved};
use super::*;
use crate::agent::store::proposals::{
    GroupIntent, NewGroup, NewSweep, ServerEdit, ServerProposal, create_group, create_sweep, get_group,
    recover_interrupted_groups, repropose_group,
};
use crate::agent::store::{
    ConversationEvent, MIGRATIONS, StoredContent, create_conversation, list_messages, run_migrations,
};
use crate::agent::types::{ProposalOutcomeKind, ProposalStatus};
use crate::ignore_poison::IgnorePoison;
use crate::network::saved_server_fields::SavedServerOutcome;

fn migrated_conn() -> Connection {
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
    conn.execute_batch("PRAGMA foreign_keys = ON;").expect("pragma");
    run_migrations(&conn, MIGRATIONS).expect("migrate");
    conn
}

/// A saver over the fixture that answers `answer` (or never, when it's `None`) and records
/// every save.
struct FakeSaver {
    saved: SavedServers,
    answer: Option<SaveResult>,
    saves: Arc<Mutex<Vec<ServerSave>>>,
}

impl FakeSaver {
    fn answering(answer: SaveResult) -> Self {
        Self::over(saved(), Some(answer))
    }

    fn over(saved: SavedServers, answer: Option<SaveResult>) -> Self {
        Self {
            saved,
            answer,
            saves: Arc::default(),
        }
    }

    fn saves(&self) -> Vec<ServerSave> {
        self.saves.lock_ignore_poison().clone()
    }
}

impl ServerSaver for FakeSaver {
    fn saved(&self) -> SavedServers {
        self.saved.clone()
    }

    fn save(&self, save: ServerSave) -> BoxFuture<'static, SaveResult> {
        self.saves.lock_ignore_poison().push(save);
        match self.answer.clone() {
            Some(answer) => Box::pin(async move { answer }),
            None => Box::pin(std::future::pending()),
        }
    }
}

/// A draft for a server nothing in the fixture has.
fn new_sftp() -> ServerDraft {
    ServerDraft {
        display_name: Some("Render farm".into()),
        host: Some("render.lan".into()),
        port: Some(22),
        username: Some("dana".into()),
        remote_root: Some("/".into()),
        ..ServerDraft::new(DraftProtocol::Sftp)
    }
}

/// Stage a server group in a sweep of a fresh thread; answers the group and the thread.
fn stage(conn: &Connection, draft: ServerDraft, edit: Option<ServerEdit>) -> (i64, i64) {
    let conversation_id = create_conversation(conn, "Servers", 50, None).expect("thread");
    let sweep = NewSweep {
        conversation_id: Some(conversation_id),
        ..NewSweep::default()
    };
    let set_id = create_sweep(conn, &sweep, 100).expect("sweep");
    let group = NewGroup {
        intent: GroupIntent::SaveServer { draft, edit },
        source_volume_id: String::new(),
        display_name: "a server".into(),
        rationale: None,
        selector: None,
    };
    (create_group(conn, set_id, &group, 100).expect("group"), conversation_id)
}

fn status(conn: &Connection, group_id: i64) -> ProposalStatus {
    get_group(conn, group_id).expect("read").expect("exists").status
}

async fn approve(conn: &Connection, group_id: i64, saver: &FakeSaver) -> ServerApprovalOutcome {
    approve_server_within(conn, group_id, 0, saver, 200, None, Duration::from_millis(200))
        .await
        .expect("the store answers")
}

fn decisions(conn: &Connection, conversation_id: i64) -> Vec<ProposalOutcomeKind> {
    list_messages(conn, conversation_id, 100, 0)
        .expect("messages")
        .into_iter()
        .filter_map(|message| match message.content {
            StoredContent::Event(ConversationEvent::ProposalDecided { decision }) => Some(decision.outcome),
            _ => None,
        })
        .collect()
}

fn sftp_edit(previous: ServerDraft) -> ServerEdit {
    ServerEdit {
        editing: SFTP_ID.into(),
        previous,
    }
}

fn renamed_sftp() -> ServerDraft {
    ServerDraft {
        display_name: Some("Builds".into()),
        ..sftp_saved()
    }
}

// ── A save, and what it ends as ──────────────────────────────────────────────

#[tokio::test]
async fn an_approved_add_saves_through_its_core_and_completes_with_its_line_and_metric() {
    let conn = migrated_conn();
    let (group_id, thread) = stage(&conn, new_sftp(), None);
    let saver = FakeSaver::answering(SaveResult::Saved);
    super::super::analytics::take_captured();

    let outcome = approve(&conn, group_id, &saver).await;

    let id = cmdr_fs::volume::sftp_volume_id("render.lan", 22, "dana");
    assert_eq!(outcome, ServerApprovalOutcome::Saved { volume_id: Some(id) });
    assert_eq!(
        saver.saves(),
        [ServerSave::Add(ServerTarget::Sftp {
            display_name: "Render farm".into(),
            host: "render.lan".into(),
            port: 22,
            username: "dana".into(),
            remote_root: "/".into(),
            start_folder: None,
            // The add form's defaults, ❌ never anything the draft could carry, except the SSH
            // agent: off, so the first connect offers no key to a host the agent picked.
            key_file: None,
            use_agent: false,
            auto_reconnect: true,
        })]
    );
    assert_eq!(status(&conn, group_id), ProposalStatus::Completed);
    assert_eq!(decisions(&conn, thread), [ProposalOutcomeKind::Saved { note: None }]);
    assert_eq!(
        super::super::analytics::take_captured(),
        ["suggestion_group_approved"],
        "counted once, on the save"
    );
}

#[tokio::test]
async fn a_refused_save_gives_the_card_back_pending_and_counts_nothing() {
    let conn = migrated_conn();
    let (group_id, thread) = stage(&conn, new_sftp(), None);
    let saver = FakeSaver::answering(SaveResult::Refused(SaveRefusal::Server(
        SavedServerOutcome::StartFolderOutsideRoot,
    )));
    super::super::analytics::take_captured();

    let outcome = approve(&conn, group_id, &saver).await;

    assert_eq!(
        outcome,
        ServerApprovalOutcome::Refused(SaveRefusal::Server(SavedServerOutcome::StartFolderOutsideRoot))
    );
    assert_eq!(status(&conn, group_id), ProposalStatus::Pending, "nothing was written");
    assert!(decisions(&conn, thread).is_empty(), "no line: nothing was decided");
    assert!(super::super::analytics::take_captured().is_empty());
}

/// ❗ **Anything that may have written never goes back to approvable**: a second click would
/// write twice. A save that went through is `completed`.
#[tokio::test]
async fn a_saved_card_is_never_approvable_again() {
    let conn = migrated_conn();
    let (group_id, _) = stage(&conn, new_sftp(), None);
    let saver = FakeSaver::answering(SaveResult::Saved);

    assert!(matches!(
        approve(&conn, group_id, &saver).await,
        ServerApprovalOutcome::Saved { .. }
    ));
    assert_eq!(
        approve(&conn, group_id, &saver).await,
        ServerApprovalOutcome::AlreadyAnswered
    );
    assert_eq!(saver.saves().len(), 1, "the second click reached no core");
}

/// ❗ The other half of the same rule: a save that never answered (or whose task died) may
/// have written, so it stays `approved`, the next launch calls it `interrupted`, and nothing
/// approves it again.
#[tokio::test]
async fn a_save_that_never_answers_stays_approved_and_the_next_launch_calls_it_interrupted() {
    let conn = migrated_conn();
    let (group_id, _) = stage(&conn, new_sftp(), None);
    let saver = FakeSaver::over(saved(), None);

    assert_eq!(approve(&conn, group_id, &saver).await, ServerApprovalOutcome::NoAnswer);
    assert_eq!(status(&conn, group_id), ProposalStatus::Approved, "it may have written");

    recover_interrupted_groups(&conn).expect("recovery");
    assert_eq!(status(&conn, group_id), ProposalStatus::Interrupted);
    assert_eq!(
        approve(&conn, group_id, &saver).await,
        ServerApprovalOutcome::AlreadyAnswered
    );
    assert_eq!(saver.saves().len(), 1);
}

#[tokio::test]
async fn a_card_amended_since_it_was_read_answers_list_changed_and_saves_nothing() {
    let conn = migrated_conn();
    let (group_id, _) = stage(&conn, new_sftp(), None);
    let amended = NewGroup {
        intent: GroupIntent::SaveServer {
            draft: ServerDraft {
                host: Some("elsewhere.lan".into()),
                ..new_sftp()
            },
            edit: None,
        },
        source_volume_id: String::new(),
        display_name: "a server".into(),
        rationale: None,
        selector: None,
    };
    repropose_group(&conn, group_id, &amended, 150).expect("amend");
    let saver = FakeSaver::answering(SaveResult::Saved);

    assert_eq!(
        approve(&conn, group_id, &saver).await,
        ServerApprovalOutcome::ListChanged
    );
    assert!(saver.saves().is_empty());
    assert_eq!(status(&conn, group_id), ProposalStatus::Pending);
}

// ── The checks again, against what's saved now ───────────────────────────────

/// A server saved between the proposal and the click (by the user, or another approved card)
/// makes the add one of a saved server, and the check refuses it before any core runs.
#[tokio::test]
async fn an_add_saved_since_it_was_proposed_refuses_already_saved_without_saving() {
    let conn = migrated_conn();
    let (group_id, _) = stage(
        &conn,
        ServerDraft {
            display_name: None,
            ..sftp_saved()
        },
        None,
    );
    let saver = FakeSaver::answering(SaveResult::Saved);

    assert_eq!(
        approve(&conn, group_id, &saver).await,
        ServerApprovalOutcome::AlreadySaved {
            server_id: cmdr_fs::volume::sftp_volume_id("build.lan", 22, "dana")
        }
    );
    assert!(saver.saves().is_empty());
    assert_eq!(status(&conn, group_id), ProposalStatus::Pending, "given back");
}

#[tokio::test]
async fn an_smb_add_of_a_host_saved_since_refuses_already_saved() {
    let conn = migrated_conn();
    let draft = ServerDraft {
        host: Some("NASPOLYA.local".into()),
        port: Some(445),
        ..ServerDraft::new(DraftProtocol::Smb)
    };
    let (group_id, _) = stage(&conn, draft, None);
    let saver = FakeSaver::answering(SaveResult::Saved);

    assert_eq!(
        approve(&conn, group_id, &saver).await,
        ServerApprovalOutcome::AlreadySaved {
            server_id: SMB_SHARE.into()
        }
    );
    assert!(saver.saves().is_empty());
}

#[tokio::test]
async fn a_name_another_server_took_since_refuses_name_taken_without_saving() {
    let conn = migrated_conn();
    let (group_id, _) = stage(
        &conn,
        ServerDraft {
            display_name: Some("NASPOLYA".into()),
            ..new_sftp()
        },
        None,
    );
    let saver = FakeSaver::answering(SaveResult::Saved);

    assert_eq!(
        approve(&conn, group_id, &saver).await,
        ServerApprovalOutcome::NameTaken {
            name: "Naspolya".into()
        }
    );
    assert!(saver.saves().is_empty());
    assert_eq!(status(&conn, group_id), ProposalStatus::Pending);
}

#[tokio::test]
async fn an_edit_renaming_onto_a_taken_name_refuses_but_keeping_its_own_name_saves() {
    let conn = migrated_conn();
    let taken = ServerDraft {
        display_name: Some("photos".into()),
        ..sftp_saved()
    };
    let (group_id, _) = stage(&conn, taken, Some(sftp_edit(sftp_saved())));
    let saver = FakeSaver::answering(SaveResult::Saved);
    assert_eq!(
        approve(&conn, group_id, &saver).await,
        ServerApprovalOutcome::NameTaken { name: "Photos".into() }
    );

    let (group_id, _) = stage(
        &conn,
        ServerDraft {
            remote_root: Some("/srv/builds".into()),
            ..sftp_saved()
        },
        Some(sftp_edit(sftp_saved())),
    );
    assert!(matches!(
        approve(&conn, group_id, &saver).await,
        ServerApprovalOutcome::Saved { .. }
    ));
}

/// ❗ **The card's diff is checked against the saved entry at the click**, the address
/// included, so the save never overwrites a change the card didn't show.
#[tokio::test]
async fn an_edit_whose_server_changed_since_answers_server_changed_without_saving() {
    for (what, now) in [
        (
            "an editable field",
            ServerDraft {
                remote_root: Some("/data".into()),
                ..sftp_saved()
            },
        ),
        (
            "the address",
            ServerDraft {
                host: Some("10.0.0.66".into()),
                ..sftp_saved()
            },
        ),
        (
            "the port",
            ServerDraft {
                port: Some(2222),
                ..sftp_saved()
            },
        ),
    ] {
        let conn = migrated_conn();
        let (group_id, _) = stage(&conn, renamed_sftp(), Some(sftp_edit(sftp_saved())));
        let mut fixture = saved();
        for place in fixture.rows.iter_mut().flat_map(|row| row.places.iter_mut()) {
            if place.volume_id == SFTP_ID {
                place.draft = now.clone();
            }
        }
        let saver = FakeSaver::over(fixture, Some(SaveResult::Saved));

        assert_eq!(
            approve(&conn, group_id, &saver).await,
            ServerApprovalOutcome::ServerChanged,
            "{what}"
        );
        assert!(saver.saves().is_empty(), "{what}");
        assert_eq!(status(&conn, group_id), ProposalStatus::Pending, "{what}");
    }
}

/// A host's share count is the card's description, ❌ not the host: a share saved since
/// doesn't make the edit stale, and the card reads the count live anyway.
#[tokio::test]
async fn an_smb_edit_ignores_a_changed_share_count_and_saves_the_host_in_place() {
    let conn = migrated_conn();
    let draft = ServerDraft {
        display_name: Some("NAS".into()),
        username: Some("dori".into()),
        ..smb_saved()
    };
    let previous = ServerDraft {
        shares_affected: Some(1),
        ..smb_saved()
    };
    let (group_id, _) = stage(
        &conn,
        draft,
        Some(ServerEdit {
            editing: SMB_SHARE.into(),
            previous,
        }),
    );
    let saver = FakeSaver::answering(SaveResult::Saved);

    assert_eq!(
        approve(&conn, group_id, &saver).await,
        ServerApprovalOutcome::Saved {
            volume_id: Some(SMB_SHARE.into())
        }
    );
    assert_eq!(
        saver.saves(),
        [ServerSave::SmbHostEdit {
            host_id: SMB_HOST.into(),
            name: "NAS".into(),
            username: Some("dori".into()),
        }]
    );
}

#[tokio::test]
async fn an_edit_saves_only_its_editable_fields_by_the_place_id() {
    let conn = migrated_conn();
    let nfs_previous = saved().place(NFS_ID).expect("fixture").1.draft.clone();
    let draft = ServerDraft {
        uid: Some(501),
        start_folder: Some("/photos".into()),
        ..nfs_previous.clone()
    };
    let (group_id, _) = stage(
        &conn,
        draft,
        Some(ServerEdit {
            editing: NFS_ID.into(),
            previous: nfs_previous,
        }),
    );
    let saver = FakeSaver::answering(SaveResult::Saved);

    approve(&conn, group_id, &saver).await;

    assert_eq!(
        saver.saves(),
        [ServerSave::Edit {
            id: NFS_ID.into(),
            edit: PlaceEdit {
                display_name: None,
                remote_root: None,
                start_folder: Some("/photos".into()),
                uid: Some(501),
                gid: Some(1000),
            },
        }]
    );
}

#[tokio::test]
async fn an_edit_of_a_place_forgotten_since_answers_unreachable_without_saving() {
    let conn = migrated_conn();
    let (group_id, _) = stage(&conn, renamed_sftp(), Some(sftp_edit(sftp_saved())));
    let mut fixture = saved();
    for row in &mut fixture.rows {
        row.places.retain(|place| place.volume_id != SFTP_ID);
    }
    let saver = FakeSaver::over(fixture, Some(SaveResult::Saved));

    assert_eq!(
        approve(&conn, group_id, &saver).await,
        ServerApprovalOutcome::Refused(SaveRefusal::Server(SavedServerOutcome::Unreachable))
    );
    assert!(saver.saves().is_empty());
    assert_eq!(status(&conn, group_id), ProposalStatus::Pending);
}

#[tokio::test]
async fn an_smb_add_saves_contact_free_through_its_address_with_the_share() {
    let conn = migrated_conn();
    let draft = ServerDraft {
        display_name: Some("Attic".into()),
        host: Some("attic.lan".into()),
        port: Some(4450),
        username: Some("ada".into()),
        share: Some("Backups".into()),
        ..ServerDraft::new(DraftProtocol::Smb)
    };
    let (group_id, _) = stage(&conn, draft, None);
    let saver = FakeSaver::answering(SaveResult::Saved);

    approve(&conn, group_id, &saver).await;

    assert_eq!(
        saver.saves(),
        [ServerSave::SmbAdd {
            address: "smb://attic.lan:4450/Backups".into(),
            name: "Attic".into(),
            username: Some("ada".into()),
        }]
    );
}

#[tokio::test]
async fn an_s3_bucket_add_saves_under_its_saved_account() {
    let conn = migrated_conn();
    let draft = ServerDraft {
        account_id: Some(S3_ACCOUNT.into()),
        bucket: Some("receipts".into()),
        ..ServerDraft::new(DraftProtocol::S3)
    };
    let (group_id, _) = stage(&conn, draft, None);
    let saver = FakeSaver::answering(SaveResult::Saved);

    let ServerApprovalOutcome::Saved { volume_id, .. } = approve(&conn, group_id, &saver).await else {
        panic!("saved");
    };

    let saves = saver.saves();
    let [
        ServerSave::Add(ServerTarget::S3 {
            access_key_id, bucket, ..
        }),
    ] = saves.as_slice()
    else {
        panic!("one S3 add");
    };
    assert_eq!(access_key_id, "AKIAEXAMPLE");
    assert_eq!(bucket.as_deref(), Some("receipts"));
    assert!(volume_id.is_some());
}

// ── Through the real stores ──────────────────────────────────────────────────

/// The real stores and the hub's own cores, minus SMB, whose store is a file under the app's
/// data dir (its contact-free save is pinned in `network/manual_servers_contact_test.rs`).
struct CoreSaver;

impl ServerSaver for CoreSaver {
    fn saved(&self) -> SavedServers {
        let listing = crate::saved_servers::listing(Vec::new(), Vec::new(), &[]);
        SavedServers::from_listing(listing, Vec::new(), crate::saved_servers::saved_by_id)
    }

    fn save(&self, save: ServerSave) -> BoxFuture<'static, SaveResult> {
        Box::pin(async move {
            match save {
                ServerSave::Add(target) => SaveResult::of_outcome(crate::saved_servers::save_target(target).await),
                ServerSave::Edit { id, edit } => {
                    SaveResult::of_outcome(crate::saved_servers::save_place_edit(&id, edit).await)
                }
                smb @ (ServerSave::SmbAdd { .. } | ServerSave::SmbHostEdit { .. }) => {
                    panic!("no SMB store here: {smb:?}")
                }
            }
        })
    }
}

/// ❗ **An approved card is saved exactly as the servers list then shows it, and contacts
/// nothing**: per protocol, an add read back through the hub's own listing, with no dial filed
/// and no volume registered. The addresses are documentation blocks (RFC 5737), so a dial that
/// slipped through would show as a filed attempt, never as a quiet success.
#[tokio::test]
async fn each_protocol_saves_through_the_real_stores_lists_back_and_dials_nothing() {
    use crate::network::s3_known_places::S3ProviderChoice;

    let _secrets = crate::test_support::isolate_secrets();
    let conn = migrated_conn();
    let dials = crate::network::connect_wiring::dials_filed();
    let manager = crate::file_system::volume::manager::get_volume_manager();
    // An S3 bucket is proposed under an account the user saved: save one the way the hub does.
    crate::saved_servers::save_target(ServerTarget::S3 {
        display_name: "Archive".into(),
        provider: S3ProviderChoice::Wasabi {
            region: "eu-central-1".into(),
        },
        access_key_id: "AKIAROUNDTRIP".into(),
        bucket: Some("archive".into()),
        auto_reconnect: true,
    })
    .await;
    let account = CoreSaver
        .saved()
        .rows
        .iter()
        .find(|row| row.protocol == DraftProtocol::S3)
        .map(|row| row.id.clone())
        .expect("the account is listed");

    let drafts = [
        ServerDraft {
            host: Some("192.0.2.91".into()),
            port: Some(22),
            username: Some("ada".into()),
            remote_root: Some("/srv".into()),
            ..ServerDraft::new(DraftProtocol::Sftp)
        },
        ServerDraft {
            url: Some("https://192.0.2.92:5006/dav".into()),
            username: Some("ada".into()),
            remote_root: Some("/".into()),
            ..ServerDraft::new(DraftProtocol::Webdav)
        },
        ServerDraft {
            host: Some("192.0.2.93".into()),
            export: Some("/export/media".into()),
            uid: Some(501),
            gid: Some(20),
            ..ServerDraft::new(DraftProtocol::Nfs)
        },
        ServerDraft {
            account_id: Some(account),
            bucket: Some("receipts".into()),
            ..ServerDraft::new(DraftProtocol::S3)
        },
    ];
    for draft in drafts {
        let protocol = draft.protocol;
        let (group_id, _) = stage(&conn, draft, None);

        let outcome = approve_server(&conn, group_id, 0, &CoreSaver, 200, None)
            .await
            .expect("the store answers");

        let ServerApprovalOutcome::Saved { volume_id: Some(id) } = outcome else {
            panic!("{protocol:?} saved with an id, got {outcome:?}");
        };
        assert!(
            CoreSaver.saved().place(&id).is_some(),
            "{protocol:?} lists back as {id}"
        );
        assert!(manager.get(&id).is_none(), "{protocol:?} registered nothing");
        assert_eq!(status(&conn, group_id), ProposalStatus::Completed);
    }

    // An edit through the same stores: only the name moves, every switch stays as stored.
    let sftp_id = cmdr_fs::volume::sftp_volume_id("192.0.2.91", 22, "ada");
    let previous = CoreSaver.saved().place(&sftp_id).expect("saved").1.draft.clone();
    let (group_id, _) = stage(
        &conn,
        ServerDraft {
            display_name: Some("Build box".into()),
            ..previous.clone()
        },
        Some(ServerEdit {
            editing: sftp_id.clone(),
            previous,
        }),
    );
    let before = crate::network::sftp_known_servers::find("192.0.2.91", 22, "ada").expect("saved");
    assert!(matches!(
        approve_server(&conn, group_id, 0, &CoreSaver, 200, None).await,
        Ok(ServerApprovalOutcome::Saved { .. })
    ));
    let after = crate::network::sftp_known_servers::find("192.0.2.91", 22, "ada").expect("saved");
    assert_eq!(after.display_name, "Build box");
    assert_eq!(
        (after.key_file, after.use_agent, after.auto_reconnect, after.pinned),
        (before.key_file, before.use_agent, before.auto_reconnect, before.pinned)
    );

    assert_eq!(
        crate::network::connect_wiring::dials_filed(),
        dials,
        "no dial was filed"
    );
}

// ── Which core answers wrote nothing ─────────────────────────────────────────

/// ❗ Every refusal a core answers is one it makes before writing anything, checked per
/// variant against the core. A variant wrongly called pre-write would let a second click
/// write twice, so each is pinned here, and the classification has no `_` arm.
#[test]
fn every_core_refusal_classifies_as_written_nothing_and_only_saved_as_saved() {
    use crate::network::manual_servers::AddServerError;

    assert_eq!(SaveResult::of_outcome(SavedServerOutcome::Saved), SaveResult::Saved);
    for refusal in [
        SavedServerOutcome::StartFolderOutsideRoot,
        SavedServerOutcome::RootNotFound,
        SavedServerOutcome::StartFolderNotFound,
        SavedServerOutcome::Unreachable,
        SavedServerOutcome::AddressTaken { name: "NAS".into() },
        SavedServerOutcome::SecretNotMoved,
        SavedServerOutcome::OperationRunning,
        SavedServerOutcome::ShareMounted { name: "media".into() },
        SavedServerOutcome::AccountChanged,
    ] {
        assert_eq!(
            SaveResult::of_outcome(refusal.clone()),
            SaveResult::Refused(SaveRefusal::Server(refusal))
        );
    }
    for error in [
        AddServerError::InvalidAddress { message: "bad".into() },
        AddServerError::Unreachable {
            message: "silent".into(),
            hint: None,
        },
    ] {
        assert_eq!(
            SaveResult::of_smb_add(Err(error.clone())),
            SaveResult::Refused(SaveRefusal::AddServer(error))
        );
    }
}

// ── The card ─────────────────────────────────────────────────────────────────

fn card_of(draft: ServerDraft, edit: Option<ServerEdit>) -> ServerProposalView {
    server_card(
        &ServerProposal {
            draft,
            edit,
            note: None,
        },
        &saved(),
    )
}

fn row(field: ServerField, value: &str) -> ServerFieldView {
    ServerFieldView {
        field,
        value: Some(value.into()),
        changed: false,
        previous: None,
    }
}

#[test]
fn an_add_card_lists_what_the_click_saves_and_marks_a_host_nothing_saved_is_at() {
    let card = card_of(new_sftp(), None);

    assert_eq!(card.protocol, DraftProtocol::Sftp);
    assert_eq!(
        card.fields,
        [
            row(ServerField::Name, "Render farm"),
            row(ServerField::Address, "render.lan"),
            row(ServerField::Port, "22"),
            row(ServerField::Account, "dana"),
            row(ServerField::Folder, "/"),
        ]
    );
    assert!(card.address_is_new);
    assert_eq!(card.shares_affected, None);

    let beside_the_nas = card_of(
        ServerDraft {
            host: Some("Naspolya.local".into()),
            ..new_sftp()
        },
        None,
    );
    assert!(!beside_the_nas.address_is_new, "an SMB host is saved there");
}

#[test]
fn an_edit_card_shows_old_and_new_on_changed_rows_only() {
    let card = card_of(
        ServerDraft {
            remote_root: Some("/srv/builds".into()),
            start_folder: Some("/srv/builds/nightly".into()),
            ..sftp_saved()
        },
        Some(sftp_edit(sftp_saved())),
    );

    assert!(!card.address_is_new);
    let folder = card
        .fields
        .iter()
        .find(|row| row.field == ServerField::Folder)
        .expect("folder");
    assert_eq!(
        folder,
        &ServerFieldView {
            field: ServerField::Folder,
            value: Some("/srv/builds".into()),
            changed: true,
            previous: Some("/srv".into()),
        }
    );
    let start = card
        .fields
        .iter()
        .find(|row| row.field == ServerField::StartFolder)
        .expect("start folder");
    assert!(start.changed);
    assert_eq!(start.previous, None, "nothing was there before");
    let name = card
        .fields
        .iter()
        .find(|row| row.field == ServerField::Name)
        .expect("name");
    assert_eq!(name, &row(ServerField::Name, "Build box"), "unchanged rows read plain");
}

#[test]
fn an_smb_edit_card_counts_the_hosts_shares_now() {
    let card = card_of(
        ServerDraft {
            display_name: Some("NAS".into()),
            shares_affected: Some(9),
            ..smb_saved()
        },
        Some(ServerEdit {
            editing: SMB_SHARE.into(),
            previous: smb_saved(),
        }),
    );

    assert_eq!(card.shares_affected, Some(2), "live, not what the proposal counted");
}

#[test]
fn an_s3_card_names_the_account_as_the_servers_list_does() {
    let card = card_of(
        ServerDraft {
            account_id: Some(S3_ACCOUNT.into()),
            bucket: Some("receipts".into()),
            ..ServerDraft::new(DraftProtocol::S3)
        },
        None,
    );

    assert_eq!(
        card.fields,
        [
            row(ServerField::Account, "AKIAEXAMPLE@s3"),
            row(ServerField::Bucket, "receipts")
        ]
    );
    assert!(!card.address_is_new, "the account is saved");
}

// ── The saved facts ──────────────────────────────────────────────────────────

#[test]
fn the_saved_facts_draft_each_place_and_its_smb_host() {
    use crate::network::manual_servers::ManualServerEntry;
    use crate::saved_servers::SavedEntry;
    use crate::saved_servers::test_fixtures::{sftp_entry, share_row};

    let manual = vec![ManualServerEntry {
        id: "192.0.2.77".into(),
        display_name: "Attic NAS".into(),
        address: "192.0.2.77".into(),
        port: 445,
        added_at: String::new(),
        username: Some("david".into()),
        awaiting_first_connect: false,
    }];
    let known = vec![share_row("192.0.2.77", "media", Some("david"), Some("smb-attic-media"))];
    let smb_hosts = crate::saved_servers::smb_hosts::smb_host_groups(manual, known, &[]);
    let sftp = sftp_entry("192.0.2.78", true);
    let sftp_id = cmdr_fs::volume::sftp_volume_id(&sftp.host, sftp.port, &sftp.username);
    let listing = vec![crate::saved_servers::SavedServer {
        id: sftp_id.clone(),
        protocol: crate::saved_servers::ServerProtocol::Sftp,
        display_name: sftp.display_name.clone(),
        name_source: crate::saved_servers::ServerNameSource::User,
        address: String::new(),
        username: None,
        pinned: true,
        last_connected_at: None,
        auto_reconnect: None,
        places: vec![crate::saved_servers::SavedPlace {
            volume_id: sftp_id.clone(),
            name: String::new(),
            pinned: true,
            connected: false,
            app_root: String::new(),
            username: None,
            auto_reconnect: None,
        }],
        awaiting_first_connect: false,
    }];
    let entry = sftp.clone();
    let saved = SavedServers::from_listing(listing, smb_hosts, move |id| {
        (id == sftp_id).then(|| SavedEntry::Sftp(entry.clone()))
    });

    let (_, place) = saved.place("smb-attic-media").expect("the share names its host");
    assert_eq!(place.draft.host.as_deref(), Some("192.0.2.77"));
    assert_eq!(place.draft.display_name.as_deref(), Some("Attic NAS"));
    assert_eq!(place.draft.shares_affected, Some(1));
    let (row, place) = saved
        .rows
        .iter()
        .find(|row| row.protocol == DraftProtocol::Sftp)
        .and_then(|row| row.places.first().map(|place| (row, place)))
        .expect("the SFTP place");
    assert_eq!(row.name, sftp.display_name);
    assert_eq!(place.draft.host.as_deref(), Some("192.0.2.78"));
    assert_eq!(place.draft.remote_root.as_deref(), Some("/srv/data"));
}
