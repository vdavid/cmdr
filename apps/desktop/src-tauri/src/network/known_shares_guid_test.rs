//! The server GUID on a share row, and the proof it was learned with: which
//! writer may set it, and that every refresh of the row keeps both.

use super::*;

const BOX: &str = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
const OTHER_BOX: &str = "0f1e2d3c-4b5a-6978-8796-a5b4c3d2e1f0";

fn lan_row() -> Vec<KnownNetworkShare> {
    vec![mounted(
        "192.168.50.10",
        "192.168.50.10",
        "testshare",
        Some("test_user_3"),
        "smb-lan",
    )]
}

/// ❗ **A probe's answer only FILLS a missing GUID.** Whatever answers on a saved
/// address isn't proven to be the share's server, so it can't replace one a
/// signed-in session recorded, and what it fills is recorded as unproven.
#[test]
fn an_unauthenticated_guid_fills_a_missing_one_and_never_overwrites() {
    let mut rows = lan_row();

    assert_eq!(record_guid_in(&mut rows, "smb-lan", BOX, None), Some(true));
    assert_eq!(rows[0].server_guid.as_deref(), Some(BOX));
    assert_eq!(rows[0].server_guid_proof, GuidProof::Unauthenticated);

    assert_eq!(
        record_guid_in(&mut rows, "smb-lan", OTHER_BOX, None),
        Some(false),
        "a probe finding a row that holds a GUID writes nothing"
    );
    assert_eq!(rows[0].server_guid.as_deref(), Some(BOX));
}

/// ❗ **A signed-in session's GUID overwrites**: a NAS restored or replaced mints a
/// new one, and the latest authenticated answer is the truth.
#[test]
fn an_authenticated_guid_overwrites_the_stored_one() {
    let mut rows = lan_row();
    rows[0].server_guid = Some(BOX.to_string());

    assert_eq!(
        record_guid_in(&mut rows, "smb-lan", OTHER_BOX, Some("test_user_3")),
        Some(true)
    );
    assert_eq!(rows[0].server_guid.as_deref(), Some(OTHER_BOX));
    assert_eq!(rows[0].server_guid_proof, GuidProof::Authenticated);
    assert_eq!(rows[0].username.as_deref(), Some("test_user_3"), "no other field moves");
    assert_eq!(record_guid_in(&mut rows, "smb-nope", BOX, Some("test_user_3")), None);
}

/// A signed-in session answering the GUID a probe filled proves it: the row's
/// proof moves up, and a later probe can't move it back down.
#[test]
fn an_authenticated_session_upgrades_a_probed_guids_proof() {
    let mut rows = lan_row();
    record_guid_in(&mut rows, "smb-lan", BOX, None);

    assert_eq!(
        record_guid_in(&mut rows, "smb-lan", BOX, Some("test_user_3")),
        Some(true),
        "the proof changed, so the row is written"
    );
    assert_eq!(rows[0].server_guid_proof, GuidProof::Authenticated);

    assert_eq!(
        record_guid_in(&mut rows, "smb-lan", BOX, Some("test_user_3")),
        Some(false)
    );
    assert_eq!(record_guid_in(&mut rows, "smb-lan", BOX, None), Some(false));
    assert_eq!(rows[0].server_guid_proof, GuidProof::Authenticated);
}

/// ❗ **`Authenticated` only from a session signed in as the row's OWN account**,
/// decided here from the row, never handed in: every writer (a session, a live
/// session adopted, the probe) passes who signed in, so none can vouch for
/// another person's row.
#[test]
fn only_a_session_signed_in_as_the_rows_account_proves_its_guid() {
    let mut rows = lan_row();
    assert_eq!(
        record_guid_in(&mut rows, "smb-lan", BOX, Some("test_user_7")),
        Some(true),
        "another account still fills a missing GUID"
    );
    assert_eq!(rows[0].server_guid_proof, GuidProof::Unauthenticated);
    assert_eq!(
        record_guid_in(&mut rows, "smb-lan", OTHER_BOX, Some("test_user_7")),
        Some(false),
        "and never overwrites one"
    );
    assert_eq!(rows[0].server_guid.as_deref(), Some(BOX));

    assert_eq!(
        record_guid_in(&mut rows, "smb-lan", BOX, Some("Guest")),
        Some(false),
        "a guest session is no account"
    );
    assert_eq!(rows[0].server_guid_proof, GuidProof::Unauthenticated);

    assert_eq!(
        record_guid_in(&mut rows, "smb-lan", BOX, Some("TEST_USER_3")),
        Some(true),
        "the row's own account, folded"
    );
    assert_eq!(rows[0].server_guid_proof, GuidProof::Authenticated);

    let mut guest_row = vec![mounted("192.168.50.10", "192.168.50.10", "testshare", None, "smb-lan")];
    record_guid_in(&mut guest_row, "smb-lan", BOX, Some("test_user_3"));
    assert_eq!(
        guest_row[0].server_guid_proof,
        GuidProof::Unauthenticated,
        "a guest row has no account a session could match"
    );
}

/// A stored all-zero GUID is no GUID, so a probe may fill it.
#[test]
fn a_stored_zero_guid_counts_as_missing() {
    let mut rows = vec![mounted("192.168.50.10", "192.168.50.10", "testshare", None, "smb-lan")];
    rows[0].server_guid = Some("00000000-0000-0000-0000-000000000000".to_string());

    assert_eq!(record_guid_in(&mut rows, "smb-lan", BOX, None), Some(true));
    assert_eq!(rows[0].server_guid.as_deref(), Some(BOX));
}

/// A later mount, or a reconnect through the share list, files a row with no
/// GUID; the share's recorded one stays, with its proof.
#[test]
fn a_remount_keeps_the_recorded_guid_and_its_proof() {
    let mut rows = lan_row();
    record_guid_in(&mut rows, "smb-lan", BOX, Some("test_user_3"));

    upsert_share_row(
        &mut rows,
        mounted(
            "192.168.50.10",
            "192.168.50.10",
            "testshare",
            Some("test_user_3"),
            "smb-lan",
        ),
        &[],
    );

    assert_eq!(rows[0].server_guid.as_deref(), Some(BOX));
    assert_eq!(rows[0].server_guid_proof, GuidProof::Authenticated);

    let mut reconnect = mounted(
        "192.168.50.10",
        "192.168.50.10",
        "testshare",
        Some("test_user_3"),
        "smb-lan",
    );
    keep_recorded_guid(&mut reconnect, &mut rows[0]);
    assert_eq!(reconnect.server_guid.as_deref(), Some(BOX));
    assert_eq!(reconnect.server_guid_proof, GuidProof::Authenticated);
}

/// ❗ **Proof earned as one account never vouches for another.** A save that
/// changes the share's account (a mount as someone else, a sheet's new account)
/// keeps the GUID as a hint but drops its proof, so `location::same_share` can't
/// redirect a read to a session that would answer as another person.
#[test]
fn a_save_under_another_account_demotes_the_guids_proof() {
    let mut rows = lan_row();
    record_guid_in(&mut rows, "smb-lan", BOX, Some("test_user_3"));

    upsert_share_row(
        &mut rows,
        mounted(
            "192.168.50.10",
            "192.168.50.10",
            "testshare",
            Some("test_user_7"),
            "smb-lan",
        ),
        &[],
    );
    assert_eq!(rows[0].server_guid.as_deref(), Some(BOX), "the GUID stays, as a hint");
    assert_eq!(rows[0].server_guid_proof, GuidProof::Unauthenticated);

    let mut rows = lan_row();
    record_guid_in(&mut rows, "smb-lan", BOX, Some("test_user_3"));
    upsert_share_row(
        &mut rows,
        mounted("192.168.50.10", "192.168.50.10", "testshare", None, "smb-lan"),
        &[],
    );
    assert_eq!(
        rows[0].server_guid_proof,
        GuidProof::Unauthenticated,
        "a guest mount is another account too"
    );
}

/// The account compares the way `location::same_share` reads it: folded, and an
/// add naming no account keeps the saved one, so neither moves the proof.
#[test]
fn a_save_under_the_same_account_keeps_the_proof() {
    let mut rows = lan_row();
    record_guid_in(&mut rows, "smb-lan", BOX, Some("test_user_3"));

    upsert_share_row(
        &mut rows,
        mounted(
            "192.168.50.10",
            "192.168.50.10",
            "testshare",
            Some("TEST_USER_3"),
            "smb-lan",
        ),
        &[],
    );
    assert_eq!(rows[0].server_guid_proof, GuidProof::Authenticated);

    upsert_share_row(&mut rows, added("192.168.50.10", "testshare", None), &[]);
    assert_eq!(rows[0].username.as_deref(), Some("TEST_USER_3"));
    assert_eq!(rows[0].server_guid_proof, GuidProof::Authenticated);
}

/// The share list's own writer (`update_known_share`) holds the same rule.
#[test]
fn a_share_list_reconnect_under_another_account_demotes_the_proof() {
    let mut existing = lan_row().remove(0);
    existing.server_guid = Some(BOX.to_string());
    existing.server_guid_proof = GuidProof::Authenticated;

    let mut reconnect = mounted(
        "192.168.50.10",
        "192.168.50.10",
        "testshare",
        Some("test_user_7"),
        "smb-lan",
    );
    keep_recorded_guid(&mut reconnect, &mut existing);
    assert_eq!(reconnect.server_guid.as_deref(), Some(BOX));
    assert_eq!(reconnect.server_guid_proof, GuidProof::Unauthenticated);
}

/// `known-shares.json` from before Cmdr kept the proof: its GUID reads as one no
/// sign-in vouched for, and the proof is written beside it from then on.
#[test]
fn a_row_saved_before_the_proof_reads_as_unauthenticated() {
    let json = format!(
        r#"{{"serverName":"testnas","shareName":"testshare","protocol":"smb","lastConnectedAt":"2026-01-01T00:00:00Z","lastConnectionMode":"credentials","lastKnownAuthOptions":"credentials_only","username":"test_user_3","serverGuid":"{BOX}"}}"#
    );

    let mut row: KnownNetworkShare = serde_json::from_str(&json).expect("an older row reads");

    assert_eq!(row.server_guid.as_deref(), Some(BOX));
    assert_eq!(row.server_guid_proof, GuidProof::Unauthenticated);

    row.server_guid_proof = GuidProof::Authenticated;
    let written = serde_json::to_value(&row).expect("serializes");
    assert_eq!(written["serverGuidProof"], "authenticated");
}
