//! What the S3 place list owes the hub: one entry per place, found again the
//! way the volume id finds it, and a pin a reconnect can't undo.
//!
//! In-memory only: these run before `load_known_s3_places` names a file, so
//! `save()` is a no-op. Each cell uses a key id of its own, since the store is
//! process-global.

use super::*;

fn place(key: &str, bucket: Option<&str>) -> KnownS3Place {
    KnownS3Place {
        provider: S3ProviderChoice::Aws {
            region: "eu-west-1".to_string(),
        },
        access_key_id: key.to_string(),
        bucket: bucket.map(str::to_string),
        auto_reconnect: true,
        pinned: false,
        last_connected_at: "2026-10-01T10:00:00Z".to_string(),
    }
}

fn id_of(entry: &KnownS3Place) -> String {
    entry.volume_id().expect("a valid test provider")
}

#[test]
fn every_bucket_under_one_key_is_its_own_entry() {
    let photos = place("AKIAREMEMBER", Some("photos"));
    let root = place("AKIAREMEMBER", None);
    remember(photos.clone());
    remember(root.clone());
    assert!(find(&id_of(&photos)).is_some());
    assert!(find(&id_of(&root)).is_some());
    assert_ne!(id_of(&photos), id_of(&root));
}

#[test]
fn a_reconnect_cant_repin_a_place_the_user_unpinned() {
    let photos = place("AKIAPINS", Some("photos"));
    remember(KnownS3Place {
        pinned: true,
        ..photos.clone()
    });
    assert!(set_pinned(&id_of(&photos), false));
    remember(KnownS3Place {
        pinned: true,
        auto_reconnect: false,
        ..photos.clone()
    });
    let stored = find(&id_of(&photos)).expect("still saved");
    assert!(!stored.pinned, "the stored pin wins on a replace");
    assert!(!stored.auto_reconnect, "everything else is the new entry");
}

#[test]
fn forgetting_one_place_leaves_its_siblings() {
    let photos = place("AKIAFORGET", Some("photos"));
    let backups = place("AKIAFORGET", Some("backups"));
    remember(photos.clone());
    remember(backups.clone());
    assert!(forget(&id_of(&photos)));
    assert!(find(&id_of(&photos)).is_none());
    assert!(find(&id_of(&backups)).is_some());
    assert!(!forget(&id_of(&photos)), "a second forget finds nothing");
}

#[test]
fn a_bucket_reads_as_itself_and_the_root_as_its_account() {
    let photos = place("AKIALABEL", Some("photos"));
    let root = place("AKIALABEL", None);
    remember(photos.clone());
    assert_eq!(place_label(&photos), "photos");
    assert_eq!(place_label(&root), "AKIALABEL@s3.eu-west-1.amazonaws.com");
    assert_eq!(account_label(&photos), "AKIALABEL@s3.eu-west-1.amazonaws.com");

    adopt_typed_name(&photos, "Work");
    assert_eq!(
        place_label(&photos),
        "photos",
        "a bucket keeps its own name under a named account"
    );
    assert_eq!(
        place_label(&root),
        "Work",
        "the root IS the account, so it reads as the account"
    );
    assert_eq!(account_label(&photos), "Work");
}

#[test]
fn the_name_belongs_to_the_account_and_every_place_reads_it() {
    let photos = place("AKIASHARED", Some("photos"));
    let backups = place("AKIASHARED", Some("backups"));
    remember(photos.clone());
    remember(backups.clone());
    let account = account_id(&photos.params().expect("valid"));

    assert!(rename_account(&account, "  Work  "));
    assert_eq!(account_name(&photos), "Work", "trimmed");
    assert_eq!(
        account_name(&backups),
        "Work",
        "one name for the account, whichever place asks"
    );

    // A later add under the same key with a name typed renames the account: the
    // newest typed name wins.
    adopt_typed_name(&backups, "Studio");
    assert_eq!(account_name(&photos), "Studio");

    // An add with the Name field left empty leaves the account's name alone.
    adopt_typed_name(&photos, "   ");
    assert_eq!(account_name(&photos), "Studio");

    // A rename to nothing unnames it, so it reads as its stand-in again.
    assert!(rename_account(&account, ""));
    assert_eq!(account_name(&photos), "");
    assert_eq!(account_label(&photos), "AKIASHARED@s3.eu-west-1.amazonaws.com");
}

#[test]
fn renaming_an_account_nothing_saved_answers_false() {
    let root = place("AKIANOSUCH", None).params().expect("valid");
    assert!(!rename_account(&account_id(&root), "Work"));
    assert_eq!(account_name(&place("AKIANOSUCH", Some("photos"))), "");
}

#[test]
fn forgetting_the_last_place_forgets_the_accounts_name() {
    let photos = place("AKIAFORGETNAME", Some("photos"));
    let backups = place("AKIAFORGETNAME", Some("backups"));
    remember(photos.clone());
    remember(backups.clone());
    adopt_typed_name(&photos, "Work");

    assert!(forget(&id_of(&photos)));
    assert_eq!(account_name(&backups), "Work", "a sibling still holds the account");
    assert!(forget(&id_of(&backups)));
    // Adding the key again starts unnamed: Forget forgot.
    remember(photos.clone());
    assert_eq!(account_name(&photos), "");
}

#[test]
fn a_store_with_per_place_names_moves_the_newest_name_to_its_account() {
    let stored = r#"{
      "knownS3Places": [
        {
          "provider": { "kind": "aws", "region": "eu-west-1" },
          "accessKeyId": "AKIAOLD",
          "bucket": "photos",
          "displayName": "Older name",
          "lastConnectedAt": "2026-09-01T10:00:00Z"
        },
        {
          "provider": { "kind": "aws", "region": "eu-west-1" },
          "accessKeyId": "AKIAOLD",
          "bucket": "backups",
          "displayName": "Newer name",
          "lastConnectedAt": "2026-09-20T10:00:00Z"
        },
        {
          "provider": { "kind": "aws", "region": "eu-west-1" },
          "accessKeyId": "AKIAOLD",
          "bucket": "logs",
          "displayName": "",
          "lastConnectedAt": "2026-09-30T10:00:00Z"
        },
        {
          "provider": { "kind": "aws", "region": "eu-west-1" },
          "accessKeyId": "AKIAUNNAMED",
          "bucket": null,
          "displayName": "",
          "lastConnectedAt": "2026-09-30T10:00:00Z"
        }
      ]
    }"#;
    let (store, migrated) = migrate(serde_json::from_str(stored).expect("an old store parses"));
    assert!(migrated, "a store that carried per-place names is written back");
    assert_eq!(store.known_s3_places.len(), 4, "every place survives");
    assert_eq!(store.known_s3_accounts.len(), 1, "only a named account gets a record");
    assert_eq!(store.known_s3_accounts[0].access_key_id, "AKIAOLD");
    assert_eq!(
        store.known_s3_accounts[0].display_name, "Newer name",
        "the most recently connected named place wins"
    );

    let rewritten = serde_json::to_value(&store).expect("serializes");
    assert!(
        rewritten["knownS3Places"][0].get("displayName").is_none(),
        "a place carries no name of its own any more"
    );
    let (again, migrated_again) = migrate(serde_json::from_value(rewritten).expect("the new store parses"));
    assert!(!migrated_again, "a migrated store needs no second write");
    assert_eq!(again.known_s3_accounts[0].display_name, "Newer name");
}

#[test]
fn every_place_of_one_account_names_the_same_account() {
    let photos = place("AKIAACCOUNT", Some("photos")).params().expect("valid");
    let root = place("AKIAACCOUNT", None).params().expect("valid");
    assert_eq!(account_id(&photos), account_id(&root));
    assert_eq!(
        account_id(&root),
        place_id(&root),
        "the account id IS the root place's id"
    );
}

#[test]
fn a_store_from_disk_reads_its_provider_by_kind() {
    let stored = r#"{
      "knownS3Places": [
        {
          "provider": { "kind": "other", "endpoint": "http://127.0.0.1:14480", "region": null, "pathStyle": true },
          "accessKeyId": "GK1",
          "bucket": "cmdr-test",
          "displayName": "",
          "lastConnectedAt": "2026-10-01T10:00:00Z"
        }
      ]
    }"#;
    let (store, _) = migrate(serde_json::from_str(stored).expect("parses"));
    let entry = &store.known_s3_places[0];
    assert!(entry.auto_reconnect, "a missing switch reads as on");
    assert!(!entry.pinned, "a missing pin reads as off");
    let params = entry.params().expect("a usable provider");
    assert_eq!(params.port(), 14480);
}

/// The wire names match the crate's `kind_name`, which the price table and the
/// `s3_connected` counter key on.
#[test]
fn gcs_and_spaces_travel_by_their_kind_names() {
    let gcs = serde_json::to_value(S3ProviderChoice::Gcs).unwrap();
    assert_eq!(gcs, serde_json::json!({ "kind": "gcs" }));
    let spaces: S3ProviderChoice =
        serde_json::from_value(serde_json::json!({ "kind": "digitalocean", "region": " fra1 " })).unwrap();
    let provider = spaces.to_provider().unwrap();
    assert_eq!(provider, S3Provider::DigitalOcean { region: "fra1".into() });
    assert_eq!(provider.kind_name(), "digitalocean");
    assert_eq!(S3ProviderChoice::Gcs.to_provider().unwrap().kind_name(), "gcs");
}
