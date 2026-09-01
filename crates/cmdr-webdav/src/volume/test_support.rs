//! A volume with no client behind it, for the cells that exercise the path
//! translation and the state machine without a server.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8};

use cmdr_fs::volume::Retirement;
use cmdr_fs::volume::host::VolumeHost;
use url::Url;

use super::{ConnectionState, WebdavVolume, WebdavVolumeInner};
use crate::params::WebdavConnectionParams;

pub(super) fn make_test_volume(root: &str) -> WebdavVolume {
    make_test_volume_with(root, VolumeHost::detached())
}

pub(super) fn make_test_volume_with(root: &str, host: VolumeHost) -> WebdavVolume {
    let params = WebdavConnectionParams::new(
        Url::parse("http://127.0.0.1:1/dav/").expect("a valid test URL"),
        "ada",
        root,
    );
    WebdavVolume {
        name: "test".to_string(),
        root: PathBuf::from(super::paths::root_remote_path(&params.remote_root)),
        inner: Arc::new_cyclic(|me| WebdavVolumeInner {
            volume_id: "webdav-test".to_string(),
            params,
            client: tokio::sync::RwLock::new(None),
            state: AtomicU8::new(ConnectionState::Connected as u8),
            retirement: Retirement::new(),
            me: me.clone(),
            reconnect_lock: tokio::sync::Mutex::new(()),
            unmounted: AtomicBool::new(false),
            auto_reconnect: AtomicBool::new(true),
            auth_attempt_spent: AtomicBool::new(false),
            host,
        }),
    }
}
