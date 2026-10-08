//! A volume with no client behind it, for the cells that exercise the path
//! translation and the state machine without a server.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8};

use cmdr_fs::volume::Retirement;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::liveness::Timings;
use cmdr_fs::volume::remote_paths::RemoteRoot;
use url::Url;

use super::{ConnectionState, S3Volume, S3VolumeInner};
use crate::params::{S3ConnectionParams, S3Provider};

/// The prefix the test volume mints, spelled out so a cell reads as the app
/// would see it.
pub(super) const PREFIX: &str = "s3://AKIATEST@127.0.0.1:1";

/// A place with no client: `bucket` is the place, `None` the account root.
pub(super) fn make_test_volume(bucket: Option<&str>) -> S3Volume {
    make_test_volume_with(bucket, VolumeHost::detached())
}

pub(super) fn make_test_volume_with(bucket: Option<&str>, host: VolumeHost) -> S3Volume {
    let provider = S3Provider::Other {
        endpoint: Url::parse("http://127.0.0.1:1").expect("a valid test URL"),
        region: None,
        path_style: true,
    };
    let params = S3ConnectionParams::new(provider, "AKIATEST", bucket).expect("a valid test provider");
    S3Volume {
        name: "test".to_string(),
        root: RemoteRoot::new(
            cmdr_fs::volume::s3_app_root(params.host(), params.port(), params.access_key_id()),
            std::path::Path::new(&params.remote_root()),
        ),
        inner: Arc::new_cyclic(|me| S3VolumeInner {
            volume_id: "s3-test".to_string(),
            params: std::sync::RwLock::new(params),
            client: tokio::sync::RwLock::new(None),
            state: AtomicU8::new(ConnectionState::Connected as u8),
            retirement: Retirement::new(),
            me: me.clone(),
            reconnect_lock: tokio::sync::Mutex::new(()),
            unmounted: AtomicBool::new(false),
            auto_reconnect: AtomicBool::new(true),
            auth_attempt_spent: AtomicBool::new(false),
            silence: std::sync::RwLock::new(Timings::PRODUCTION),
            ledger: super::upload_ledger::UploadLedger::at(host.state_dir("s3")),
            host,
            written: std::sync::Mutex::new(std::collections::HashMap::new()),
            part_floor: std::sync::atomic::AtomicU64::new(crate::multipart::MIN_PART_SIZE),
            pause_hold_ms: std::sync::atomic::AtomicU64::new(5_000),
            beside_folders: std::sync::Mutex::new(std::collections::HashSet::new()),
        }),
    }
}
