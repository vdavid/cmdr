//! `propose_server`: what a server suggestion must survive before a card exists, and the one
//! write it makes.
//!
//! The checks run over a fixture [`SavedServers`]: an SFTP build box, a WebDAV account, an NFS
//! export, an S3 account with one bucket, and an SMB host (Naspolya) with two saved shares.

use cmdr_fs::validation::PlainLeafRefusal;
use rusqlite::Connection;
use serde_json::{Value, json};

use super::migrated_conn;
use crate::agent::store::proposals::{ServerDraft, get_group, read_server_proposal};
use crate::agent::suggested_ops::server::test_fixtures::{
    NFS_ID, S3_ACCOUNT, S3_BUCKET_ID, SFTP_ID, SMB_SHARE, WEBDAV_ID, saved, sftp_saved, smb_saved,
};
use crate::agent::types::{DraftProtocol, ProposalVerb, TurnOrigin};
use crate::tools::suggestions::server::{ServerRefusal, apply_server, problem_content};
use crate::tools::suggestions::server_plan::{FieldRule, PlannedServer, ServerProblem, plan_server};

fn plan(call: Value) -> Result<PlannedServer, ServerProblem> {
    plan_server(&call, &saved())
}

fn refused(call: Value) -> ServerProblem {
    plan(call).expect_err("refused")
}

fn token(problem: &ServerProblem) -> String {
    problem_content(problem)["refusal"]
        .as_str()
        .expect("a token")
        .to_string()
}

// ── Nothing that could carry a secret ────────────────────────────────────────

#[test]
fn every_secret_shaped_key_refuses_as_secret_refused_naming_it() {
    for key in [
        "password",
        "Password",
        "secret",
        "secretAccessKey",
        "keyFile",
        "privateKey",
        "passphrase",
    ] {
        let mut call = json!({ "protocol": "sftp", "host": "new.lan", "username": "dana" });
        call[key] = json!("hunter2");
        assert_eq!(
            refused(call),
            ServerProblem::SecretRefused { field: key.to_string() },
            "{key}"
        );
    }
}

#[test]
fn a_password_inside_a_url_refuses_as_a_secret() {
    let problem = refused(json!({ "protocol": "webdav", "url": "https://ada:hunter2@dav.lan/", "username": "ada" }));
    assert_eq!(token(&problem), "secretRefused");
}

#[test]
fn a_switch_that_unlocks_a_secret_is_no_field_at_all() {
    for key in ["useAgent", "autoReconnect", "pinned"] {
        let mut call = json!({ "protocol": "sftp", "host": "new.lan", "username": "dana" });
        call[key] = json!(true);
        assert_eq!(
            refused(call),
            ServerProblem::UnboundField { field: key.to_string() },
            "{key}"
        );
    }
}

// ── The field map ────────────────────────────────────────────────────────────

#[test]
fn a_field_outside_a_protocols_add_refuses_as_unbound() {
    let cases = [
        json!({ "protocol": "smb", "host": "nas2.lan", "path": "/x" }),
        json!({ "protocol": "sftp", "host": "new.lan", "username": "dana", "share": "media" }),
        json!({ "protocol": "webdav", "url": "https://d.lan/", "username": "ada", "host": "d.lan" }),
        json!({ "protocol": "nfs", "host": "f.lan", "export": "/x", "username": "root" }),
        json!({ "protocol": "s3", "account": S3_ACCOUNT, "bucket": "b", "host": "s3.lan" }),
    ];
    for call in cases {
        assert_eq!(token(&refused(call.clone())), "unboundField", "{call}");
    }
}

#[test]
fn a_field_outside_a_protocols_edit_refuses_as_unbound() {
    let cases = [
        json!({ "protocol": "smb", "editing": SMB_SHARE, "share": "other" }),
        json!({ "protocol": "sftp", "editing": SFTP_ID, "uid": 0 }),
        json!({ "protocol": "webdav", "editing": WEBDAV_ID, "export": "/x" }),
        json!({ "protocol": "nfs", "editing": NFS_ID, "path": "/x" }),
        json!({ "protocol": "s3", "editing": S3_BUCKET_ID, "startFolder": "/x" }),
    ];
    for call in cases {
        assert_eq!(token(&refused(call.clone())), "unboundField", "{call}");
    }
}

#[test]
fn an_add_without_what_it_needs_names_the_missing_field() {
    let cases = [
        (json!({ "protocol": "smb" }), "host"),
        (json!({ "protocol": "sftp", "host": "new.lan" }), "username"),
        (json!({ "protocol": "webdav", "username": "ada" }), "url"),
        (json!({ "protocol": "nfs", "host": "f.lan" }), "export"),
        (json!({ "protocol": "s3", "account": S3_ACCOUNT }), "bucket"),
    ];
    for (call, field) in cases {
        assert_eq!(refused(call), ServerProblem::MissingField { field });
    }
}

#[test]
fn a_new_s3_account_or_another_protocol_is_not_proposable() {
    assert_eq!(
        refused(json!({ "protocol": "s3", "bucket": "b" })),
        ServerProblem::ProtocolNotProposable
    );
    assert_eq!(
        refused(json!({ "protocol": "ftp", "host": "old.lan" })),
        ServerProblem::ProtocolNotProposable
    );
}

#[test]
fn a_shape_this_tool_cant_read_is_malformed() {
    assert_eq!(refused(json!(["sftp"])), ServerProblem::Malformed);
    assert_eq!(
        refused(json!({ "protocol": "sftp", "host": "new.lan", "username": "dana", "port": "twenty-two" })),
        ServerProblem::Malformed
    );
}

// ── Every value one plain thing ──────────────────────────────────────────────

#[test]
fn a_name_share_or_folder_that_reads_as_something_else_refuses() {
    let bidi = "invoice\u{202E}fdp.exe";
    let cases = [
        (
            json!({ "protocol": "smb", "host": "nas2.lan", "name": bidi }),
            "name",
            FieldRule::Plain(PlainLeafRefusal::Invisible('\u{202E}')),
        ),
        (
            json!({ "protocol": "smb", "host": "nas2.lan", "share": "me\u{200D}dia" }),
            "share",
            FieldRule::Plain(PlainLeafRefusal::Invisible('\u{200D}')),
        ),
        (
            json!({ "protocol": "sftp", "host": "new.lan", "username": "dana", "path": "/srv/\u{7}bell" }),
            "path",
            FieldRule::Plain(PlainLeafRefusal::Control('\u{7}')),
        ),
        (
            json!({ "protocol": "nfs", "host": "f.lan", "export": "/volume1/../etc" }),
            "export",
            FieldRule::Plain(PlainLeafRefusal::DotName),
        ),
        (
            json!({ "protocol": "smb", "host": "nas2.lan", "name": " Naspolya" }),
            "name",
            FieldRule::Plain(PlainLeafRefusal::EdgeWhitespace),
        ),
        (
            json!({ "protocol": "sftp", "host": "new.lan", "username": "dana", "path": "srv" }),
            "path",
            FieldRule::NotAbsolute,
        ),
    ];
    for (call, field, rule) in cases {
        assert_eq!(refused(call.clone()), ServerProblem::BadField { field, rule }, "{call}");
    }
}

#[test]
fn a_host_that_only_looks_like_a_host_refuses() {
    // A Cyrillic `а` spoofing the saved `naspolya.local`, a path smuggled into the host, a
    // newline, and a port the host field doesn't carry.
    for host in ["n\u{0430}spolya.local", "nas.lan/evil", "nas.lan\nx", "nas.lan:445", ""] {
        assert_eq!(
            refused(json!({ "protocol": "smb", "host": host })),
            ServerProblem::BadField {
                field: "host",
                rule: FieldRule::NotAnAddress
            },
            "{host:?}"
        );
    }
    assert_eq!(
        refused(json!({ "protocol": "smb", "host": "nas2.lan", "port": 0 })),
        ServerProblem::BadField {
            field: "port",
            rule: FieldRule::NotAnAddress
        }
    );
}

#[test]
fn a_url_that_isnt_plain_http_refuses() {
    for url in [
        "ftp://d.lan/",
        "https://d\u{0430}v.lan/",
        "https://ada@d.lan/",
        "not a url",
    ] {
        assert_eq!(
            token(&refused(json!({ "protocol": "webdav", "url": url, "username": "ada" }))),
            "badField",
            "{url}"
        );
    }
}

#[test]
fn a_start_folder_outside_the_remote_root_refuses() {
    assert_eq!(
        refused(
            json!({ "protocol": "sftp", "host": "new.lan", "username": "dana", "path": "/srv", "startFolder": "/etc" })
        ),
        ServerProblem::BadField {
            field: "startFolder",
            rule: FieldRule::OutsideRoot
        }
    );
}

// ── Adds ─────────────────────────────────────────────────────────────────────

#[test]
fn a_valid_add_per_protocol_drafts_the_add_forms_defaults() {
    let sftp =
        plan(json!({ "protocol": "sftp", "host": "new.lan", "username": "dana", "name": "New box" })).expect("sftp");
    assert_eq!(
        sftp.draft,
        ServerDraft {
            display_name: Some("New box".into()),
            host: Some("new.lan".into()),
            port: Some(22),
            username: Some("dana".into()),
            remote_root: Some("/".into()),
            ..ServerDraft::new(DraftProtocol::Sftp)
        }
    );
    assert_eq!(sftp.edit, None);
    assert_eq!(sftp.display_name, "New box");

    let smb = plan(json!({ "protocol": "smb", "host": "nas2.lan", "share": "media" })).expect("smb");
    assert_eq!(smb.draft.port, Some(445));
    assert_eq!(smb.draft.share.as_deref(), Some("media"));
    assert_eq!(smb.display_name, "nas2.lan", "an unnamed card leads with its address");

    let webdav = plan(json!({ "protocol": "webdav", "url": "https://d.lan/dav/", "username": "ada" })).expect("webdav");
    assert_eq!(webdav.draft.remote_root.as_deref(), Some("/"));

    let nfs =
        plan(json!({ "protocol": "nfs", "host": "f.lan", "export": "/Media", "uid": 501, "gid": 20 })).expect("nfs");
    assert_eq!(
        (nfs.draft.port, nfs.draft.uid),
        (None, Some(501)),
        "no port asks the portmapper"
    );

    let s3 = plan(json!({ "protocol": "s3", "account": S3_ACCOUNT, "bucket": "receipts" })).expect("s3");
    assert_eq!(
        (s3.draft.account_id.as_deref(), s3.draft.bucket.as_deref()),
        (Some(S3_ACCOUNT), Some("receipts"))
    );
}

#[test]
fn an_add_of_a_saved_address_refuses_with_the_id_to_edit_instead() {
    // Saved spellings differing only by the store's own normalization still meet the saved one.
    let cases = [
        json!({ "protocol": "sftp", "host": "BUILD.lan", "username": "dana" }),
        json!({ "protocol": "webdav", "url": "https://dav.example.com/other/", "username": "ada" }),
        json!({ "protocol": "nfs", "host": "files.lan", "export": "/Public/", "uid": 0, "gid": 0 }),
        json!({ "protocol": "s3", "account": S3_ACCOUNT, "bucket": "invoices" }),
    ];
    for call in cases {
        let problem = refused(call.clone());
        let ServerProblem::AlreadySaved { server_id } = &problem else {
            panic!("{call} answered {problem:?}");
        };
        assert!(
            saved().place(server_id).is_some(),
            "{server_id} must be an id an edit can name"
        );
    }
    assert_eq!(
        refused(json!({ "protocol": "smb", "host": "Naspolya.LOCAL", "share": "photos" })),
        ServerProblem::AlreadySaved {
            server_id: SMB_SHARE.to_string()
        },
        "an SMB host is its address and port, in any case"
    );
    assert!(
        plan(json!({ "protocol": "smb", "host": "naspolya.local", "port": 4450 })).is_ok(),
        "another port on the same machine is another server"
    );
}

#[test]
fn an_s3_bucket_under_an_unknown_account_refuses() {
    assert_eq!(
        refused(json!({ "protocol": "s3", "account": "s3-nobody", "bucket": "b" })),
        ServerProblem::UnknownServer {
            id: "s3-nobody".to_string()
        }
    );
}

#[test]
fn a_name_another_saved_server_wears_refuses_folded() {
    for name in ["naspolya", "NASPOLYA", "Build Box", "photos"] {
        assert_eq!(
            token(&refused(
                json!({ "protocol": "smb", "host": "10.0.0.66", "name": name })
            )),
            "nameTaken",
            "{name}"
        );
    }
    // NFC and NFD spellings of one name are one name.
    let mut saved = saved();
    saved.rows[0].name = "Kr\u{00E4}mer".into();
    let call = json!({ "protocol": "smb", "host": "10.0.0.66", "name": "Kra\u{0308}mer" });
    assert_eq!(token(&plan_server(&call, &saved).expect_err("taken")), "nameTaken");
}

#[test]
fn a_name_may_be_its_own_rows_current_one() {
    assert!(plan(json!({ "protocol": "sftp", "editing": SFTP_ID, "name": "build box" })).is_ok());
    // An S3 name is its account's, so a bucket added under it may repeat it.
    assert!(plan(json!({ "protocol": "s3", "account": S3_ACCOUNT, "bucket": "b", "name": "akiaexample@s3" })).is_ok());
}

// ── Edits ────────────────────────────────────────────────────────────────────

#[test]
fn an_edit_that_names_another_address_or_account_refuses() {
    let cases = [
        (
            json!({ "protocol": "sftp", "editing": SFTP_ID, "host": "evil.lan" }),
            "host",
        ),
        (json!({ "protocol": "sftp", "editing": SFTP_ID, "port": 2222 }), "port"),
        (
            json!({ "protocol": "sftp", "editing": SFTP_ID, "username": "root" }),
            "username",
        ),
        (
            json!({ "protocol": "webdav", "editing": WEBDAV_ID, "url": "https://evil.lan/dav/" }),
            "url",
        ),
        (
            json!({ "protocol": "webdav", "editing": WEBDAV_ID, "username": "eve" }),
            "username",
        ),
        (
            json!({ "protocol": "nfs", "editing": NFS_ID, "export": "/Private" }),
            "export",
        ),
        // Who a server signs in as is identity on every protocol: an SMB username, an NFS
        // uid or gid.
        (json!({ "protocol": "nfs", "editing": NFS_ID, "uid": 0 }), "uid"),
        (json!({ "protocol": "nfs", "editing": NFS_ID, "gid": 0 }), "gid"),
        (
            json!({ "protocol": "smb", "editing": SMB_SHARE, "host": "10.0.0.66" }),
            "host",
        ),
        (
            json!({ "protocol": "smb", "editing": SMB_SHARE, "username": "dori" }),
            "username",
        ),
        (
            json!({ "protocol": "s3", "editing": S3_BUCKET_ID, "bucket": "other" }),
            "bucket",
        ),
        (
            json!({ "protocol": "webdav", "editing": SFTP_ID, "name": "x" }),
            "protocol",
        ),
    ];
    for (call, field) in cases {
        assert_eq!(refused(call.clone()), ServerProblem::AddressChange { field }, "{call}");
    }
}

#[test]
fn an_edit_may_restate_its_address_in_the_stores_own_spelling() {
    let restated = plan(json!({
        "protocol": "webdav", "editing": WEBDAV_ID, "url": "https://DAV.example.com:443/dav", "username": "ada", "name": "Pictures"
    }))
    .expect("the same place");
    assert_eq!(
        restated.draft.url.as_deref(),
        Some("https://dav.example.com/dav/"),
        "the draft keeps the saved address, never the call's spelling"
    );
    assert!(plan(json!({ "protocol": "sftp", "editing": SFTP_ID, "host": "Build.LAN", "port": 22 })).is_ok());
    assert!(plan(json!({ "protocol": "smb", "editing": SMB_SHARE, "username": "david", "name": "NAS" })).is_ok());
    assert!(plan(json!({ "protocol": "nfs", "editing": NFS_ID, "uid": 1000, "gid": 1000, "name": "Files" })).is_ok());
}

#[test]
fn an_edit_stores_what_it_changes_and_the_saved_entry_with_its_address() {
    let planned =
        plan(json!({ "protocol": "sftp", "editing": SFTP_ID, "path": "/srv/builds", "rationale": "You build there." }))
            .expect("edit");
    let edit = planned.edit.expect("an edit");
    assert_eq!(edit.editing, SFTP_ID);
    assert_eq!(edit.previous, sftp_saved(), "previous holds the address too");
    assert_eq!(planned.draft.remote_root.as_deref(), Some("/srv/builds"));
    assert_eq!(planned.draft.host.as_deref(), Some("build.lan"));
    assert_eq!(
        planned.display_name, "Build box",
        "an edit that keeps its name leads with it"
    );
}

#[test]
fn an_smb_edits_name_lands_on_the_host_and_counts_its_shares() {
    let planned = plan(json!({ "protocol": "smb", "editing": SMB_SHARE, "name": "NAS" })).expect("edit");
    assert_eq!(planned.draft.display_name.as_deref(), Some("NAS"));
    assert_eq!(
        planned.draft.username.as_deref(),
        Some("david"),
        "the saved account rides along"
    );
    assert_eq!(planned.draft.share, None, "a name is the host's, never a share's");
    assert_eq!(planned.draft.shares_affected, Some(2));
    assert_eq!(planned.edit.expect("edit").previous, smb_saved());
}

#[test]
fn an_edit_of_nothing_saved_refuses() {
    assert_eq!(
        refused(json!({ "protocol": "sftp", "editing": "sftp-gone", "name": "x" })),
        ServerProblem::UnknownServer {
            id: "sftp-gone".to_string()
        }
    );
}

// ── The write ────────────────────────────────────────────────────────────────

fn proposal_rows(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM proposals", [], |row| row.get(0))
        .expect("count")
}

fn stage(conn: &Connection, call: Value) -> Result<crate::tools::suggestions::propose::ProposeReport, ServerRefusal> {
    apply_server(conn, &saved(), &call, TurnOrigin::Text, None, 100)
}

#[test]
fn a_turn_the_user_didnt_start_stages_nothing() {
    let conn = migrated_conn();
    for origin in [TurnOrigin::Wake, TurnOrigin::Outcomes] {
        let call = json!({ "protocol": "sftp", "host": "new.lan", "username": "dana" });
        let refusal = apply_server(&conn, &saved(), &call, origin, None, 100).expect_err("refused");
        assert!(
            matches!(refusal, ServerRefusal::Problem(ServerProblem::RefusedInBackground)),
            "{origin:?}"
        );
    }
    assert_eq!(proposal_rows(&conn), 0);
    assert_eq!(
        problem_content(&ServerProblem::RefusedInBackground)["readyForReview"],
        false
    );
}

#[test]
fn a_refused_suggestion_writes_no_row() {
    let conn = migrated_conn();
    let refusal = stage(
        &conn,
        json!({ "protocol": "sftp", "editing": SFTP_ID, "host": "evil.lan" }),
    )
    .expect_err("refused");
    assert!(matches!(
        refusal,
        ServerRefusal::Problem(ServerProblem::AddressChange { .. })
    ));
    assert_eq!(proposal_rows(&conn), 0);
}

#[test]
fn a_valid_suggestion_stages_one_server_group_with_its_draft() {
    let conn = migrated_conn();
    let report = stage(
        &conn,
        json!({ "protocol": "sftp", "host": "new.lan", "username": "dana", "rationale": "You copy builds there." }),
    )
    .expect("staged");
    assert!(report.ready_for_review);
    let group_id = report.groups[0].group_id;
    let group = get_group(&conn, group_id).expect("read").expect("group");
    assert_eq!(group.verb, ProposalVerb::AddServer);
    assert_eq!(group.rationale.as_deref(), Some("You copy builds there."));
    let proposal = read_server_proposal(&conn, group_id)
        .expect("read")
        .expect("a server group");
    assert_eq!(proposal.draft.host.as_deref(), Some("new.lan"));
    assert_eq!(proposal.edit, None);

    let edit = stage(&conn, json!({ "protocol": "smb", "editing": SMB_SHARE, "name": "NAS" })).expect("staged");
    let group = get_group(&conn, edit.groups[0].group_id).expect("read").expect("group");
    assert_eq!(group.verb, ProposalVerb::EditServer);
    assert_eq!(group.source_volume_id, SMB_SHARE);
}

#[test]
fn rewriting_a_pending_suggestion_bumps_its_revision() {
    let conn = migrated_conn();
    let first = stage(
        &conn,
        json!({ "protocol": "sftp", "host": "new.lan", "username": "dana" }),
    )
    .expect("staged");
    let group_id = first.groups[0].group_id;
    let before = get_group(&conn, group_id).expect("read").expect("group").revision;
    let amended = stage(
        &conn,
        json!({ "protocol": "sftp", "host": "new.lan", "username": "dana", "port": 2222,
                "sweepId": first.sweep_id, "groupId": group_id }),
    )
    .expect("amended");
    assert_eq!(amended.groups[0].group_id, group_id);
    assert_eq!(
        get_group(&conn, group_id).expect("read").expect("group").revision,
        before + 1
    );
    let proposal = read_server_proposal(&conn, group_id).expect("read").expect("server");
    assert_eq!(proposal.draft.port, Some(2222));
    assert_eq!(proposal_rows(&conn), 1);

    assert_eq!(
        refused(json!({ "protocol": "sftp", "host": "new.lan", "username": "dana", "groupId": group_id })),
        ServerProblem::GroupIdWithoutSweep
    );
}

#[test]
fn every_refusal_says_nothing_was_staged_and_names_its_field() {
    let content = problem_content(&ServerProblem::AddressChange { field: "host" });
    assert_eq!(content["readyForReview"], false);
    assert_eq!(content["refusal"], "addressChange");
    assert_eq!(content["field"], "host");
    let content = problem_content(&ServerProblem::AlreadySaved {
        server_id: SFTP_ID.to_string(),
    });
    assert_eq!(content["serverId"], SFTP_ID);
}
