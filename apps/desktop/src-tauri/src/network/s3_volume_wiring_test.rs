//! The lifecycle a connect and a disconnect owe the rest of the app: a volume in
//! the registry, a place in the saved list, and nothing left behind by a refusal.
//!
//! App-side because that is what these assert on. The dial itself is the
//! crate's own cells (`crates/cmdr-s3/DETAILS.md`).
//!
//! ❗ Every `s3_integration_` cell here needs the Docker stack:
//! `apps/desktop/test/s3-servers/start.sh`. Everything else runs without one.

use cmdr_s3::S3ConnectError;
use cmdr_s3::volume::testing::{FIXTURE_ACCESS_KEY, FIXTURE_BUCKET, VERSITYGW, fixture_port, fixture_secret};

use super::{S3Connection, failed};
use crate::network::one_shot_credentials::SecretOffer;
use crate::network::s3_known_places::{self, S3ProviderChoice};
use crate::network::{keychain, s3_volume_wiring};

/// VersityGW as the "Other S3-compatible" choice the add form sends.
fn fixture_choice() -> S3ProviderChoice {
    S3ProviderChoice::Other {
        endpoint: format!("http://127.0.0.1:{}", fixture_port(VERSITYGW)),
        region: None,
        path_style: true,
    }
}

#[test]
fn every_connect_refusal_keeps_its_own_outcome() {
    // ❗ A wrong secret, a key without rights, and a key that can't list
    // buckets each word differently, so none may collapse into another.
    assert_eq!(failed(S3ConnectError::KeysRejected), S3Connection::KeysRejected);
    assert_eq!(failed(S3ConnectError::AccessDenied), S3Connection::AccessDenied);
    assert_eq!(
        failed(S3ConnectError::BucketListRefused),
        S3Connection::BucketListRefused
    );
    assert_eq!(failed(S3ConnectError::NoSuchBucket), S3Connection::BucketNotFound);
    assert_eq!(
        failed(S3ConnectError::WrongRegion {
            region: Some("us-west-2".to_string())
        }),
        S3Connection::RegionMismatch {
            region: Some("us-west-2".to_string())
        }
    );
    assert_eq!(failed(S3ConnectError::NeedsCredentials), S3Connection::NeedsCredentials);
    assert_eq!(
        failed(S3ConnectError::Transport("x".to_string())),
        S3Connection::Unreachable
    );
}

#[test]
fn a_secret_files_under_the_account_every_bucket_reads() {
    let service = s3_volume_wiring::credential_service(&fixture_choice(), FIXTURE_ACCESS_KEY).expect("valid");
    assert_eq!(service, format!("s3+http://127.0.0.1:{}", fixture_port(VERSITYGW)));
}

#[tokio::test]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_connecting_registers_the_place_and_remembers_it() {
    let _secrets = crate::test_support::isolate_secrets();
    let offer = SecretOffer {
        secret: fixture_secret(),
        remember: true,
    };
    let S3Connection::Connected { volume_id } = s3_volume_wiring::connect_and_register(
        "Fixture bucket",
        fixture_choice(),
        FIXTURE_ACCESS_KEY,
        Some(FIXTURE_BUCKET),
        true,
        "s3-register",
        Some(offer),
    )
    .await
    else {
        panic!("the fixture bucket with its secret offered must connect");
    };

    let manager = crate::file_system::volume::manager::get_volume_manager();
    let volume = manager.get(&volume_id).expect("a connect registers the volume it made");
    assert!(
        volume.list_directory(volume.root(), None).await.is_ok(),
        "the registered volume is the live one"
    );
    let saved = s3_known_places::find(&volume_id).expect("a connect remembers the place");
    assert_eq!(
        s3_known_places::account_name(&saved),
        "Fixture bucket",
        "the name typed with the add names the ACCOUNT"
    );
    assert_eq!(saved.bucket.as_deref(), Some(FIXTURE_BUCKET));
    let service = s3_volume_wiring::credential_service(&fixture_choice(), FIXTURE_ACCESS_KEY).expect("valid");
    assert!(
        keychain::has_credentials(&service, Some(FIXTURE_ACCESS_KEY)),
        "Remember on files the secret once the dial went through"
    );

    assert!(s3_volume_wiring::disconnect(&volume_id).await);
    assert!(manager.get(&volume_id).is_none(), "a disconnect unregisters");
    s3_known_places::forget(&volume_id);
}

#[tokio::test]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
async fn s3_integration_a_refused_secret_registers_and_remembers_nothing() {
    let _secrets = crate::test_support::isolate_secrets();
    let offer = SecretOffer {
        secret: "f".repeat(64),
        remember: true,
    };
    let outcome = s3_volume_wiring::connect_and_register(
        "",
        fixture_choice(),
        FIXTURE_ACCESS_KEY,
        Some("cmdr-test-2"),
        true,
        "s3-refused",
        Some(offer),
    )
    .await;
    assert_eq!(outcome, S3Connection::KeysRejected, "VersityGW names a wrong secret");

    let params = s3_known_places::KnownS3Place {
        provider: fixture_choice(),
        access_key_id: FIXTURE_ACCESS_KEY.to_string(),
        bucket: Some("cmdr-test-2".to_string()),
        auto_reconnect: true,
        pinned: false,
        last_connected_at: String::new(),
    };
    let volume_id = params.volume_id().expect("valid");
    let manager = crate::file_system::volume::manager::get_volume_manager();
    assert!(manager.get(&volume_id).is_none(), "nothing registered");
    assert!(s3_known_places::find(&volume_id).is_none(), "nothing saved");
    let service = s3_volume_wiring::credential_service(&fixture_choice(), FIXTURE_ACCESS_KEY).expect("valid");
    assert!(
        !keychain::has_credentials(&service, Some(FIXTURE_ACCESS_KEY)),
        "a refused secret is never filed"
    );
}
