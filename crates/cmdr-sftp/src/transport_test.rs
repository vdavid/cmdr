//! The two decisions the transport makes before a byte moves: what to pin, and
//! how big a window to advertise.
//!
//! The rest of this module needs a server, and lives in the Docker cells.

use super::*;

#[test]
fn first_contact_leaves_the_default_algorithm_order_alone() {
    let default = client::Config::default();
    let config = build_config(&[]);
    assert_eq!(
        config.preferred.key, default.preferred.key,
        "nothing is trusted yet, so there is nothing to pin to"
    );
}

#[test]
fn a_trusted_algorithm_narrows_the_offer_to_itself() {
    // ❗ The half that makes a changed key mean something: without it, an
    // attacker offering a type we hold no entry for lands on the first-contact
    // path and collects a one-click approval.
    let config = build_config(&["ssh-ed25519".to_string()]);
    assert_eq!(
        config.preferred.key.iter().map(Algorithm::as_str).collect::<Vec<_>>(),
        vec!["ssh-ed25519"]
    );
}

#[test]
fn the_pin_keeps_the_libraries_preference_order() {
    // Rebuilding the list from the stored names would let an rsa entry outrank
    // an ed25519 one just because of how they sorted.
    let default = client::Config::default();
    let default_order: Vec<&str> = default.preferred.key.iter().map(Algorithm::as_str).collect();
    let pinned = build_config(&["ssh-ed25519".to_string(), "rsa-sha2-512".to_string()]);
    let pinned_order: Vec<&str> = pinned.preferred.key.iter().map(Algorithm::as_str).collect();

    let expected: Vec<&str> = default_order
        .into_iter()
        .filter(|name| *name == "ssh-ed25519" || *name == "rsa-sha2-512")
        .collect();
    assert_eq!(pinned_order, expected);
}

#[test]
fn an_unparseable_stored_algorithm_narrows_nothing() {
    // A store carrying a name this russh doesn't know (an older entry, a newer
    // key type) must not leave the offer EMPTY, which would refuse every
    // connection to a host we do trust.
    let default = client::Config::default();
    let config = build_config(&["ssh-something-from-the-future".to_string()]);
    assert_eq!(config.preferred.key, default.preferred.key);
}

#[test]
fn the_channel_window_is_raised_well_past_the_library_default() {
    // At the 2 MiB default the request window buys nothing: eight 255 KiB reads
    // already fill the channel, so depth 8 and depth 32 measure the same.
    let default_window = client::Config::default().window_size;
    assert!(
        build_config(&[]).window_size > default_window,
        "the read window is capped by the channel window, so this has to move first"
    );
    assert_eq!(build_config(&[]).window_size, CHANNEL_WINDOW_BYTES);
}

#[test]
fn rsa_signs_with_a_hash_openssh_still_accepts() {
    // `None` maps to the legacy SHA-1 `ssh-rsa`, which OpenSSH has refused since
    // 8.8, so an RSA-only server would reject every key we offered.
    assert_eq!(rsa_hash_alg(Algorithm::Rsa { hash: None }), Some(HashAlg::Sha512));
    assert_eq!(rsa_hash_alg(Algorithm::Ed25519), None);
}
