//! Fixtures for the Docker-backed WebDAV suites, on both sides of the crate
//! boundary. Gated behind the `testing` feature, so it exists in dev targets
//! and in no shipped build. The stack itself:
//! `apps/desktop/test/webdav-servers/start.sh`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmdr_fs::volume::Volume;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::credentials::InMemoryCredentials;
use cmdr_fs::volume::host::events::{RecordingVolumeEvents, VolumeEventSink};
use tokio_util::sync::CancellationToken;
use url::Url;

use super::{WebdavVolume, connect_webdav_volume};
use crate::params::WebdavConnectionParams;

/// The volume root under the base URL.
pub const FIXTURE_ROOT: &str = "/";

/// The file every fixture carries for the byte path to read. Self-describing:
/// each 16-byte line holds its own line number, so every position says where
/// it belongs.
pub const FIXTURE_LARGE_FILE: &str = "large.bin";

/// What `large.bin` holds, for its first `len` bytes.
pub fn fixture_large_bytes(len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut line = 0u64;
    while out.len() < len {
        out.extend_from_slice(format!("{line:015}\n").as_bytes());
        line += 1;
    }
    out.truncate(len);
    out
}

/// Fails with the first offset where two byte runs differ, and what sits
/// around it. ❗ Not `assert_eq!` on the two buffers: a 4 MiB mismatch would
/// bury the one number that says what went wrong.
pub fn assert_same_bytes(read: &[u8], expected: &[u8], what: &str) {
    assert_eq!(
        read.len(),
        expected.len(),
        "{what}: the wrong number of bytes came back"
    );
    let Some(at) = read.iter().zip(expected).position(|(left, right)| left != right) else {
        return;
    };
    let from = at.saturating_sub(16);
    panic!(
        "{what}: the bytes differ from offset {at}\n  read:     {:?}\n  expected: {:?}",
        String::from_utf8_lossy(&read[from..(from + 48).min(read.len())]),
        String::from_utf8_lossy(&expected[from..(from + 48).min(expected.len())]),
    );
}

/// The account every fixture server runs as.
pub const FIXTURE_USER: &str = "ada";
/// Its password.
pub const FIXTURE_PASSWORD: &str = "openthedoor";

/// The fixture services and the ports their compose file publishes by default.
const FIXTURE_SERVICES: [(&str, u16); 2] = [("APACHE", 13480), ("DIGEST", 13481)];

/// The host port a fixture service publishes: `WEBDAV_FIXTURE_{service}_PORT`,
/// else `fallback` (the compose file's own default).
pub fn fixture_port(service: &str, fallback: u16) -> u16 {
    std::env::var(format!("WEBDAV_FIXTURE_{service}_PORT"))
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(fallback)
}

/// `http://127.0.0.1:{port}/dav/` for one fixture service.
pub fn fixture_base_url(service: &str, fallback_port: u16) -> Url {
    Url::parse(&format!(
        "http://127.0.0.1:{}/dav/",
        fixture_port(service, fallback_port)
    ))
    .expect("a fixture URL is well-formed by construction")
}

/// A host with the fixture account's secret preloaded for every fixture
/// service, and a recording event sink. Detached otherwise.
pub fn fixture_host() -> VolumeHost {
    let mut credentials = InMemoryCredentials::new();
    for (service, port) in FIXTURE_SERVICES {
        let params = WebdavConnectionParams::new(fixture_base_url(service, port), FIXTURE_USER, FIXTURE_ROOT);
        credentials = credentials.with_entry(
            &params.credential_service(),
            Some(FIXTURE_USER),
            FIXTURE_USER,
            FIXTURE_PASSWORD,
        );
    }
    VolumeHost::builder()
        .credentials(Arc::new(credentials))
        .events(Arc::new(RecordingVolumeEvents::new()) as Arc<dyn VolumeEventSink>)
        .build()
}

/// Connects to one fixture service, panicking with a pointer at the stack
/// script if it isn't up.
pub async fn connect_fixture(service: &str, fallback_port: u16) -> WebdavVolume {
    let params = WebdavConnectionParams::new(fixture_base_url(service, fallback_port), FIXTURE_USER, FIXTURE_ROOT);
    match connect_webdav_volume(
        "fixture",
        &format!("webdav-test-{}", service.to_ascii_lowercase()),
        params,
        fixture_host(),
        CancellationToken::new(),
    )
    .await
    {
        Ok(volume) => volume,
        Err(e) => panic!(
            "the WebDAV fixture {service} refused a connection ({e:?}); is the stack up? apps/desktop/test/webdav-servers/start.sh"
        ),
    }
}

/// Creates a uniquely named directory under the root and returns its volume
/// path. The process id keeps two `cargo` runs apart, the counter two cells.
pub async fn scratch_dir(volume: &WebdavVolume) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = Path::new("/").join(format!(
        "cmdr-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    volume
        .create_directory(&path)
        .await
        .unwrap_or_else(|e| panic!("creating the scratch dir {}: {e:?}", path.display()));
    path
}
