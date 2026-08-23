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

// ── Calling a connect off ────────────────────────────────────────────

/// The address every dial below aims at: reserved for documentation (RFC 5737),
/// routed nowhere, so a TCP connect to it hangs until something stops it.
///
/// ❗ That hang IS the subject. It's what a typo'd hostname does to a sign-in
/// dialog, and before the token it held the dialog for the whole budget.
const BLACK_HOLE: &str = "192.0.2.1";

/// Cancelling a phase ends it long before its budget does.
#[tokio::test(start_paused = true)]
async fn a_cancel_ends_a_phase_before_its_budget_does() {
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    tokio::spawn(async move {
        // allowed-test-sleep: the canceller's head start; virtual under `start_paused`
        tokio::time::sleep(Duration::from_millis(100)).await;
        token.cancel();
    });

    let started = tokio::time::Instant::now();
    let outcome = phase(&cancel, std::future::pending::<()>()).await;

    assert!(matches!(outcome, Err(SftpConnectError::Cancelled)));
    assert!(
        started.elapsed() < HANDSHAKE_TIMEOUT,
        "the whole point is that a cancel doesn't wait out the budget"
    );
}

/// A phase nobody calls off still gives up when its budget runs out. ❗ The
/// backstop the token doesn't replace: nobody is watching a reconnect.
#[tokio::test(start_paused = true)]
async fn a_phase_nobody_cancels_still_ends_at_its_budget() {
    let outcome = phase(&CancellationToken::new(), std::future::pending::<()>()).await;
    assert!(matches!(outcome, Err(SftpConnectError::TimedOut)));
}

/// A phase that finishes hands its value straight back.
#[tokio::test]
async fn a_phase_that_finishes_hands_its_value_back() {
    assert!(matches!(
        phase(&CancellationToken::new(), std::future::ready(7)).await,
        Ok(7)
    ));
}

/// A cancel that landed before the phase started wins without the work being
/// polled at all.
///
/// ❗ What `biased` buys: a connect the user already called off must not put a
/// packet on the wire, spend an authentication attempt, or read the secret store.
#[tokio::test]
async fn a_cancel_that_landed_first_costs_no_packet() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let cancel = CancellationToken::new();
    cancel.cancel();
    let polled = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&polled);
    let work = async move {
        flag.store(true, Ordering::Relaxed);
        std::future::pending::<()>().await
    };

    assert!(matches!(phase(&cancel, work).await, Err(SftpConnectError::Cancelled)));
    assert!(!polled.load(Ordering::Relaxed), "the work must never have been polled");
}

/// A cancel mid-flight stops a dial that would otherwise hang for the whole
/// handshake budget.
#[tokio::test]
async fn a_cancel_stops_a_dial_that_would_otherwise_hang() {
    let params = SftpConnectionParams::new(BLACK_HOLE, 22, "nobody", "/").without_agent();
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    tokio::spawn(async move {
        // allowed-test-sleep: the canceller's head start, so the cancel lands mid-dial
        tokio::time::sleep(Duration::from_millis(150)).await;
        token.cancel();
    });

    let started = std::time::Instant::now();
    let outcome = dial(params, VolumeHost::detached(), None, cancel).await;
    let took = started.elapsed();

    assert!(matches!(outcome, Err(SftpConnectError::Cancelled)));
    assert!(
        took < Duration::from_secs(2),
        "a black-holed address holds the dial for {HANDSHAKE_TIMEOUT:?} without a cancel; this one took {took:?}"
    );
}

/// A connect called off before it starts never leaves the machine.
#[tokio::test]
async fn a_connect_called_off_before_it_starts_answers_at_once() {
    let params = SftpConnectionParams::new(BLACK_HOLE, 22, "nobody", "/").without_agent();
    let cancel = CancellationToken::new();
    cancel.cancel();

    let started = std::time::Instant::now();
    let outcome = dial(params, VolumeHost::detached(), None, cancel).await;

    assert!(matches!(outcome, Err(SftpConnectError::Cancelled)));
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "nothing should have been attempted at all"
    );
}
