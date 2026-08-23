//! What the backend-neutral volume commands answer for a volume that isn't the
//! one they were written for.
//!
//! The reconnect manager calls these on whatever volume just changed state, so
//! every one of them has to have an answer for a backend with no story of its
//! own. The SFTP side of `get_volume_sign_in_state` is pinned in
//! `crates/cmdr-sftp/src/volume/reconnect_test.rs`, on real volumes.

use std::sync::Arc;

use cmdr_fs::volume::{InMemoryVolume, SignInPrompt, Volume};

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
        SignInPrompt::Password
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
        SignInPrompt::Password
    );
}
