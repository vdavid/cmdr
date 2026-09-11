//! A second instance over the same connection: what saving an edit to a
//! connected place builds.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::volume::Volume;

use crate::volume::test_support::*;

/// The prefix this crate's test volume mints.
const PREFIX: &str = "sftp://ada@127.0.0.1:12599";

#[test]
fn an_instance_sharing_the_connection_answers_to_its_own_name_and_root() {
    let narrow = make_test_volume_at("/srv/data/tmp");

    let wide = narrow.sharing_connection("ada@nas.local", Path::new("/srv/data"));

    assert_eq!(wide.name(), "ada@nas.local");
    assert_eq!(wide.root(), Path::new(&format!("{PREFIX}/srv/data")));
    assert_eq!(
        wide.to_remote_path(Path::new(&format!("{PREFIX}/srv/data/photos")))
            .expect("under the new root"),
        "/srv/data/photos",
        "every translation runs against the new root"
    );
    assert_eq!(
        narrow.root(),
        Path::new(&format!("{PREFIX}/srv/data/tmp")),
        "the instance it was built from keeps its own root: an in-flight holder still uses it"
    );
}

/// ❗ One connection, so one of everything connection-scoped: the session, the
/// reconnect loop, the switch, and the retirement flag.
#[test]
fn an_instance_sharing_the_connection_shares_its_state() {
    let narrow = make_test_volume_at("/srv/data/tmp");

    let wide = narrow.sharing_connection("data", Path::new("/srv/data"));

    assert!(Arc::ptr_eq(&wide.inner, &narrow.inner));
    assert_eq!(wide.volume_id(), narrow.volume_id());
    narrow.retirement().expect("keeps a flag").retire();
    assert!(
        wide.retirement().expect("keeps a flag").is_retired(),
        "retiring either instance retires the connection both ride"
    );
}

/// A key file or agent change reaches the NEXT unattended redial, with no redial
/// now, and the identity it dials never moves.
#[test]
fn redial_params_move_on_the_live_connection() {
    let volume = make_test_volume_at("/srv/data/tmp");
    let key = PathBuf::from("/Users/ada/.ssh/id_ed25519");

    volume.set_redial_params(Path::new("/srv/data"), Some(key.clone()), true);

    let params = volume.inner.params();
    assert_eq!(params.remote_root, PathBuf::from("/srv/data"));
    assert_eq!(params.key_file, Some(key));
    assert!(params.use_agent);
    assert_eq!(
        (params.host.as_str(), params.port, params.username.as_str()),
        ("127.0.0.1", CLOSED_PORT, "ada")
    );
}
