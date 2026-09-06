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

/// ❗ **The wrapper never writes.** A `remember: false` dial exists so nothing is
/// persisted, and a backend that decided to save what it authenticated with
/// would defeat exactly that.
#[test]
fn the_wrapper_never_writes() {
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

/// The offer carries the switch the sign-in sheet showed, so the wiring above
/// decides where the secret goes rather than guessing from the shape.
#[test]
fn an_offer_says_whether_to_remember_it() {
    let offer = SecretOffer {
        secret: "typed-just-now".to_string(),
        remember: false,
    };
    assert!(!offer.remember);
}
