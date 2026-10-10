//! An S3 account's places as the hub lists them: grouped under one account row,
//! each with its own id and pin, and the account's id the one its root names.

use super::super::outcomes::outcome_from_s3;
use super::super::wire::{ServerConnectOutcome, ServerNameSource, ServerProtocol};
use super::{s3_accounts, s3_place};
use crate::network::s3_known_places::{self, KnownS3Place, S3ProviderChoice};
use crate::network::s3_volume_wiring::S3Connection;
use crate::network::saved_server_fields::SavedServerOutcome;

fn place(key: &str, bucket: Option<&str>, pinned: bool) -> KnownS3Place {
    KnownS3Place {
        provider: S3ProviderChoice::Wasabi {
            region: "eu-central-1".to_string(),
        },
        access_key_id: key.to_string(),
        bucket: bucket.map(str::to_string),
        auto_reconnect: true,
        pinned,
        last_connected_at: "2026-10-01T00:00:00Z".to_string(),
    }
}

fn listed(key: &str) -> Vec<super::SavedServer> {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let app_roots = crate::server_volumes::server_places()
        .into_iter()
        .map(|place| (place.id, place.app_root))
        .collect();
    s3_accounts(manager, &app_roots)
        .into_iter()
        .filter(|account| account.username.as_deref() == Some(key))
        .collect()
}

#[test]
fn an_accounts_buckets_list_as_places_under_one_row() {
    // A key no other cell uses: the store is process-global.
    let key = "AKIAGROUPED";
    s3_known_places::remember(place(key, Some("photos"), true));
    s3_known_places::remember(place(key, Some("backups"), false));

    let accounts = listed(key);
    assert_eq!(accounts.len(), 1, "one row per account");
    let account = &accounts[0];
    assert_eq!(account.protocol, ServerProtocol::S3);
    assert_eq!(account.address, "s3.eu-central-1.wasabisys.com");
    assert!(!account.pinned, "the account row carries no pin; its places do");
    assert_eq!(account.name_source, ServerNameSource::Fallback);
    assert_eq!(
        account.display_name, "AKIAGROUPED@s3.eu-central-1.wasabisys.com",
        "an unnamed account reads as its key on its host"
    );
    let params = place(key, None, false).params().expect("valid");
    assert_eq!(account.id, s3_known_places::account_id(&params));

    let mut places: Vec<(&str, bool)> = account
        .places
        .iter()
        .map(|place| (place.name.as_str(), place.pinned))
        .collect();
    places.sort_unstable();
    assert_eq!(
        places,
        vec![("backups", false), ("photos", true)],
        "each bucket reads as its own name, as the provider spells it"
    );
    assert!(
        account
            .places
            .iter()
            .all(|p| p.app_root.ends_with("/photos") || p.app_root.ends_with("/backups")),
        "each place is rooted at its bucket"
    );
}

#[test]
fn the_name_typed_with_an_add_names_the_account_row_and_its_buckets_keep_theirs() {
    let key = "AKIANAMED";
    let photos = place(key, Some("cmdr-s3-test"), true);
    s3_known_places::remember(photos.clone());
    s3_known_places::remember(place(key, None, true));
    s3_known_places::adopt_typed_name(&photos, "Cloudflare R2 test3");

    let accounts = listed(key);
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].display_name, "Cloudflare R2 test3");
    assert_eq!(accounts[0].name_source, ServerNameSource::User);
    let mut names: Vec<&str> = accounts[0].places.iter().map(|p| p.name.as_str()).collect();
    names.sort_unstable();
    // The root IS the account opened, so it reads as the account; the hub words
    // that row as its own (`servers-hub-rows.ts`).
    assert_eq!(names, vec!["Cloudflare R2 test3", "cmdr-s3-test"]);
}

#[tokio::test]
async fn renaming_the_account_relabels_its_row_and_an_empty_name_unnames_it() {
    let key = "AKIARENAMED";
    s3_known_places::remember(place(key, Some("photos"), true));
    let id = listed(key)[0].id.clone();

    let saved = super::super::update_saved_s3_account(id.clone(), "Studio".to_string(), None).await;
    assert_eq!(saved, SavedServerOutcome::Saved);
    assert_eq!(listed(key)[0].display_name, "Studio");

    let unnamed = super::super::update_saved_s3_account(id.clone(), String::new(), None).await;
    assert_eq!(unnamed, SavedServerOutcome::Saved);
    let account = &listed(key)[0];
    assert_eq!(account.display_name, "AKIARENAMED@s3.eu-central-1.wasabisys.com");
    assert_eq!(account.name_source, ServerNameSource::Fallback);

    let nobody = s3_known_places::account_id(&place("AKIANOBODY", None, false).params().expect("valid"));
    assert_eq!(
        super::super::update_saved_s3_account(nobody, "Ghost".to_string(), None).await,
        SavedServerOutcome::Unreachable,
        "an account nothing saved has nothing to name"
    );
}

#[test]
fn a_target_and_its_saved_entry_derive_one_id() {
    // The add form sends what the person typed; the store keys what a dial
    // trims. ❗ Both must land on one id, or "Add" saves a row "Add and open"
    // can't find.
    let typed = s3_place(
        S3ProviderChoice::Aws {
            region: "eu-west-1".to_string(),
        },
        " AKIATRIM ".to_string(),
        Some(" photos ".to_string()),
        true,
    );
    let clean = place("AKIATRIM", Some("photos"), false);
    let clean = KnownS3Place {
        provider: S3ProviderChoice::Aws {
            region: "eu-west-1".to_string(),
        },
        ..clean
    };
    assert_eq!(typed.volume_id(), clean.volume_id());
}

#[test]
fn every_s3_outcome_keeps_its_own_superset_twin() {
    let cases = [
        (S3Connection::KeysRejected, "authentication_rejected"),
        (S3Connection::AccessDenied, "access_denied"),
        (S3Connection::BucketListRefused, "bucket_list_refused"),
        (S3Connection::BucketNotFound, "bucket_not_found"),
        (S3Connection::ClockSkewed, "clock_skewed"),
        (S3Connection::NotAnS3Endpoint, "not_an_s3_endpoint"),
        (S3Connection::InvalidProvider, "invalid_url"),
        (S3Connection::NeedsCredentials, "needs_credentials"),
        (S3Connection::CertificateUntrusted, "certificate_untrusted"),
    ];
    for (connection, wire) in cases {
        let outcome = outcome_from_s3(connection);
        let json = serde_json::to_value(&outcome).expect("serializes");
        assert_eq!(json["outcome"], wire, "{outcome:?}");
    }
    let region = outcome_from_s3(S3Connection::RegionMismatch {
        region: Some("us-west-2".to_string()),
    });
    assert!(matches!(region, ServerConnectOutcome::RegionMismatch { region: Some(ref r) } if r == "us-west-2"));
}
