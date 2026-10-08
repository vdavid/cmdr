//! The wrapper that lets one dial use a secret nobody remembered.
//!
//! Everything here is in-memory: the point of the wrapper is that it never
//! touches the durable store, so a cell that needed one would be testing the
//! wrong thing.

use std::sync::Arc;

use cmdr_fs::volume::host::credentials::{CredentialStore, InMemoryCredentials, StoredCredentials};

use super::{OneShotCredentials, SecretOffer};

const SERVICE: &str = "sftp://nas.local:22";
const ACCOUNT: &str = "ada";

fn offered() -> StoredCredentials {
    StoredCredentials {
        username: ACCOUNT.to_string(),
        secret: "typed-just-now".to_string(),
    }
}

/// The one key the offer names comes back from memory, and ❗ the durable store
/// is never asked for it: a store that answered something else would let a stale
/// password win over the one the user just typed.
#[test]
fn the_offered_key_is_answered_from_memory() {
    let inner = Arc::new(InMemoryCredentials::new().with_entry(SERVICE, Some(ACCOUNT), ACCOUNT, "the-stale-one"));
    let wrapper = OneShotCredentials::new(inner, SERVICE, Some(ACCOUNT), offered());

    let answer = wrapper
        .credentials(SERVICE, Some(ACCOUNT))
        .expect("the offered key answers");
    assert_eq!(answer.username, ACCOUNT);
    assert_eq!(answer.secret, "typed-just-now");
}

/// Every other key forwards, so a dial that reads a second entry (a wide
/// server-level one, another account) still sees the real store.
#[test]
fn every_other_key_forwards_to_the_real_store() {
    let inner = Arc::new(InMemoryCredentials::new().with_entry(SERVICE, None, "wide", "wide-secret"));
    let wrapper = OneShotCredentials::new(inner, SERVICE, Some(ACCOUNT), offered());

    let wide = wrapper.credentials(SERVICE, None).expect("the wide entry forwards");
    assert_eq!(wide.secret, "wide-secret");
    assert!(
        wrapper.credentials("sftp://elsewhere:22", Some(ACCOUNT)).is_none(),
        "a service the offer doesn't name is the inner store's answer, not the offer's"
    );
}

/// ❗ **The wrapper never writes while the offer is live.** A `remember: false`
/// dial exists so nothing is persisted, and a backend that decided to save what
/// it authenticated with would defeat exactly that.
#[test]
fn the_wrapper_never_writes_while_the_offer_is_live() {
    let inner = Arc::new(InMemoryCredentials::new());
    let wrapper = OneShotCredentials::new(inner.clone(), SERVICE, Some(ACCOUNT), offered());

    assert!(
        wrapper.save_credentials(SERVICE, Some(ACCOUNT), &offered()).is_err(),
        "a save through the wrapper is declined, the way a store the user said no to declines"
    );
    assert!(
        inner.credentials(SERVICE, Some(ACCOUNT)).is_none(),
        "❗ and nothing reached the durable store behind it"
    );
}

/// Once the offer is over, the wrapper is the real store again, writes included:
/// a volume opened once without remembering must still be able to save a
/// password a person later asks it to keep.
#[test]
fn a_forgotten_offer_saves_through_to_the_real_store() {
    let inner = Arc::new(InMemoryCredentials::new());
    let wrapper = OneShotCredentials::new(inner.clone(), SERVICE, Some(ACCOUNT), offered());

    wrapper.forget();
    wrapper
        .save_credentials(SERVICE, Some(ACCOUNT), &offered())
        .expect("with no live offer, a save is the real store's answer");
    assert_eq!(
        inner.credentials(SERVICE, Some(ACCOUNT)).map(|c| c.secret),
        Some("typed-just-now".to_string()),
        "the save reached the durable store"
    );
}

/// ❗ **The offer ends with the attempt.** The volume the dial built keeps the
/// host it was dialed with, so the wrapper outlives the attempt; forgetting the
/// secret is what makes "not remembered" true for the reconnect that comes
/// later, which then asks a person.
#[test]
fn the_offer_is_gone_once_the_attempt_is_over() {
    let inner = Arc::new(InMemoryCredentials::new());
    let (host, guard) = super::offer_for_one_dial(
        cmdr_fs::volume::host::VolumeHost::builder().credentials(inner).build(),
        SERVICE,
        Some(ACCOUNT),
        offered(),
    );

    assert!(
        host.credentials().credentials(SERVICE, Some(ACCOUNT)).is_some(),
        "the dial this host was built for sees the offer"
    );
    drop(guard);
    assert!(
        host.credentials().credentials(SERVICE, Some(ACCOUNT)).is_none(),
        "❗ a session that outlives the attempt gets nothing"
    );
}

/// The offer reads the shape the sign-in sheet sends over IPC, switch included,
/// so the wiring decides where the secret goes from what the person chose.
#[test]
fn an_offer_reads_the_sign_in_sheets_wire_shape() {
    for remember in [true, false] {
        let offer: SecretOffer =
            serde_json::from_value(serde_json::json!({ "secret": "typed-just-now", "remember": remember }))
                .expect("the sheet's payload deserializes");
        assert_eq!(offer.secret, "typed-just-now");
        assert_eq!(offer.remember, remember, "the switch arrives as the sheet showed it");
    }
    assert!(
        serde_json::from_value::<SecretOffer>(serde_json::json!({ "secret": "typed-just-now" })).is_err(),
        "❗ a payload without the switch is refused, never read as a default"
    );
}

/// ❗ **A remembered offer is written only once the dial went through.** It used to
/// be written BEFORE the dial, so an Add cancelled at the "First time connecting"
/// step left the typed password in the store for a server nobody saved (final QA).
/// The dial answers from memory until then.
#[tokio::test]
async fn a_remembered_offer_is_written_only_when_the_dial_goes_through() {
    let _secrets = crate::test_support::isolate_secrets();
    let service = "one-shot-remember.local:22";
    let offer = SecretOffer {
        secret: "typed-just-now".to_string(),
        remember: true,
    };

    let (host, dial) = super::host_for_dial(service, ACCOUNT, Some(offer)).await;
    assert_eq!(
        host.credentials().credentials(service, Some(ACCOUNT)).map(|c| c.secret),
        Some("typed-just-now".to_string()),
        "the dial reads the typed secret"
    );
    assert!(
        crate::network::keychain::get_credentials(service, Some(ACCOUNT)).is_err(),
        "nothing is written before the dial went through"
    );

    dial.went_through().await;
    assert_eq!(
        crate::network::keychain::get_credentials(service, Some(ACCOUNT))
            .expect("written once it went through")
            .password,
        "typed-just-now"
    );
}

/// A dial that didn't go through (cancelled, refused, a host key to approve) writes nothing.
#[tokio::test]
async fn a_remembered_offer_whose_dial_did_not_go_through_writes_nothing() {
    let _secrets = crate::test_support::isolate_secrets();
    let service = "one-shot-cancelled.local:22";
    let offer = SecretOffer {
        secret: "typed-just-now".to_string(),
        remember: true,
    };

    let (_host, dial) = super::host_for_dial(service, ACCOUNT, Some(offer)).await;
    drop(dial);
    assert!(crate::network::keychain::get_credentials(service, Some(ACCOUNT)).is_err());
}
