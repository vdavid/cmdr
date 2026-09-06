//! What the backend-neutral volume commands answer for a volume that isn't the
//! one they were written for.
//!
//! The reconnect manager calls these on whatever volume just changed state, so
//! every one of them has to have an answer for a backend with no story of its
//! own. The SFTP side of `get_volume_sign_in_state` is pinned in
//! `crates/cmdr-sftp/src/volume/reconnect_test.rs`, on real volumes.

use std::sync::Arc;

use cmdr_fs::volume::{InMemoryVolume, SignInShape, Volume};

use super::get_volume_sign_in_state;

/// ❗ **The fallback is "ask for a password", ❌ never "there's nothing to ask
/// for".**
///
/// This is only ever called on a volume that just reported `needs_credentials`,
/// which is a backend saying it wants a person. Answering `Nothing` there is the
/// exact dead end this command exists to close: a banner with no way in. A
/// needless password box is recoverable; a missing one isn't.
#[tokio::test]
async fn a_volume_with_no_sign_in_story_of_its_own_asks_for_a_password() {
    let volume_id = "sign-in-state-plain-volume";
    let manager = crate::file_system::volume::manager::get_volume_manager();
    manager.register(volume_id, Arc::new(InMemoryVolume::new("Plain")) as Arc<dyn Volume>);

    assert_eq!(
        get_volume_sign_in_state(volume_id.to_string()).await,
        SignInShape::Password
    );

    manager.unregister(volume_id);
}

/// An id nothing is registered under gets the same fallback, ❌ not a panic and
/// ❌ not a silent `Nothing`.
///
/// A volume can be unregistered between the event and the banner rendering, and
/// the honest answer to "what would signing in ask for" is still the one that
/// leaves a way forward.
#[tokio::test]
async fn an_id_nothing_is_registered_under_asks_for_a_password() {
    assert_eq!(
        get_volume_sign_in_state("sign-in-state-nothing-is-here".to_string()).await,
        SignInShape::Password
    );
}

/// The wire shape is internally tagged on `kind`, so the frontend reads it as a
/// discriminated union and a variant that grows a field doesn't change how the
/// other variants parse.
///
/// Pinned here because this is an IPC contract: `bindings.ts` generates the TS
/// union from it, and the sheet switches on `kind`.
#[test]
fn the_sign_in_shape_rides_the_wire_tagged_on_kind() {
    use cmdr_fs::volume::SignInShape;

    assert_eq!(
        serde_json::to_value(SignInShape::Nothing).unwrap(),
        serde_json::json!({ "kind": "nothing" })
    );
    assert_eq!(
        serde_json::to_value(SignInShape::KeyPassphrase).unwrap(),
        serde_json::json!({ "kind": "key_passphrase" })
    );
    assert_eq!(
        serde_json::to_value(SignInShape::UsernamePassword { guest_allowed: true }).unwrap(),
        serde_json::json!({ "kind": "username_password", "guestAllowed": true }),
        "fields are camelCase on the wire, like every other IPC payload",
    );
}
