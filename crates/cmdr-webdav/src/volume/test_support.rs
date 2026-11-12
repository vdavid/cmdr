//! A volume with no client behind it, for the cells that exercise the path
//! translation and the state machine without a server.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::remote_paths::RemoteRoot;
use url::Url;

use super::{WebdavVolume, WebdavVolumeInner};
use crate::params::WebdavConnectionParams;
use cmdr_fs::volume::http_session::HttpSession;

pub(super) fn make_test_volume(root: &str) -> WebdavVolume {
    make_test_volume_with(root, VolumeHost::detached())
}

pub(super) fn make_test_volume_with(root: &str, host: VolumeHost) -> WebdavVolume {
    let params = WebdavConnectionParams::new(
        Url::parse("http://127.0.0.1:1/dav/").expect("a valid test URL"),
        "test_user_1",
        root,
    );
    WebdavVolume {
        name: "test".to_string(),
        root: RemoteRoot::new(
            cmdr_fs::volume::webdav_app_root(params.host(), params.port(), &params.username),
            &params.remote_root,
        ),
        inner: Arc::new_cyclic(|me| WebdavVolumeInner {
            session: HttpSession::new(me, "webdav-test", &host, None, true),
            volume_id: "webdav-test".to_string(),
            params: std::sync::RwLock::new(params),
            keeps_dates: AtomicBool::new(false),
            upload_chunk: AtomicU64::new(crate::chunking::CHUNK_SIZE),
            host,
        }),
    }
}
