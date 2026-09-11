//! A second instance over the same client: what saving an edit to a connected
//! place builds.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::volume::Volume;

use super::test_support::make_test_volume;

/// The prefix this crate's test volume mints.
const PREFIX: &str = "webdav://ada@127.0.0.1:1";

#[test]
fn an_instance_sharing_the_client_answers_to_its_own_name_and_root() {
    let narrow = make_test_volume("/Photos/2024");

    let wide = narrow.sharing_connection("ada@127.0.0.1", Path::new("/Photos"));

    assert_eq!(wide.name(), "ada@127.0.0.1");
    assert_eq!(wide.root(), Path::new(&format!("{PREFIX}/Photos")));
    assert_eq!(
        wide.to_remote_path(Path::new(&format!("{PREFIX}/Photos/trip")))
            .expect("under the new root"),
        "/Photos/trip"
    );
    assert_eq!(
        narrow.root(),
        Path::new(&format!("{PREFIX}/Photos/2024")),
        "the instance it was built from keeps its own root: an in-flight holder still uses it"
    );
}

/// ❗ One client, so one of everything connection-scoped: the client, the
/// reconnect loop, the switch, and the retirement flag.
#[test]
fn an_instance_sharing_the_client_shares_its_state() {
    let narrow = make_test_volume("/Photos/2024");

    let wide = narrow.sharing_connection("test", Path::new("/Photos"));

    assert!(Arc::ptr_eq(&wide.inner, &narrow.inner));
    narrow.retirement().expect("keeps a flag").retire();
    assert!(
        wide.retirement().expect("keeps a flag").is_retired(),
        "retiring either instance retires the client both ride"
    );
}

/// ❗ The reconnect re-probe runs on the root, so a moved root has to reach it,
/// or a place whose old root was since deleted could never come back.
#[test]
fn the_redial_root_moves_on_the_live_client() {
    let volume = make_test_volume("/Photos/2024");

    volume.set_redial_root(Path::new("/Photos"));

    let params = volume.inner.params();
    assert_eq!(params.remote_root, PathBuf::from("/Photos"));
    assert_eq!(params.username, "ada", "the identity it dials never moves");
}
