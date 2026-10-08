//! What "Copy share link" answers without a server: links name files, and a
//! place with no live client has no credentials to sign with.

use std::path::Path;

use cmdr_fs::volume::{ShareLinkExpiry, Volume, VolumeError};

use super::test_support::{PREFIX, make_test_volume};

#[tokio::test]
async fn a_bucket_or_the_account_root_gets_no_link() {
    let account = make_test_volume(None);
    let outcome = account
        .share_link(Path::new(&format!("{PREFIX}/")), ShareLinkExpiry::SevenDays)
        .await;
    assert!(matches!(outcome, Err(VolumeError::IsADirectory(_))), "{outcome:?}");
    let outcome = account
        .share_link(Path::new(&format!("{PREFIX}/photos")), ShareLinkExpiry::OneDay)
        .await;
    assert!(matches!(outcome, Err(VolumeError::IsADirectory(_))), "{outcome:?}");
}

#[tokio::test]
async fn a_place_with_no_live_client_says_it_is_disconnected() {
    let volume = make_test_volume(Some("photos"));
    let outcome = volume
        .share_link(Path::new(&format!("{PREFIX}/photos/a.jpg")), ShareLinkExpiry::OneHour)
        .await;
    assert!(
        matches!(outcome, Err(VolumeError::DeviceDisconnected(_))),
        "{outcome:?}"
    );
}

#[test]
fn the_place_says_it_can_mint_links() {
    let volume = make_test_volume(Some("photos"));
    assert!(volume.supports_share_links());
    assert!(volume.capabilities().can_share_links);
}
