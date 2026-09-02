//! Fixtures for the Docker-backed WebDAV suites, on both sides of the crate
//! boundary. Gated behind the `testing` feature, so it exists in dev targets
//! and in no shipped build. The stack itself:
//! `apps/desktop/test/webdav-servers/start.sh`.
//!
//! ❗ Every connection here goes through one resolver, [`fixture_target`], so
//! setting `CMDR_WEBDAV_TEST_URL` points the WHOLE suite at a server of your
//! own (a Nextcloud, a Synology, a Fastmail account) with no code change. What
//! that costs and which cells opt out: the fixture README's "Against a server
//! of your own".

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

/// The Nextcloud fixture's SECOND account, which keeps the stock unlimited
/// quota. `FIXTURE_USER` there carries [`FIXTURE_NEXTCLOUD_QUOTA_BYTES`]
/// instead, so the two accounts cover both RFC 4331 answers a real Nextcloud
/// gives.
pub const FIXTURE_UNLIMITED_USER: &str = "grace";

/// The quota the Nextcloud fixture gives `FIXTURE_USER`, exactly.
///
/// ❗ The same number as `QUOTA` in
/// `apps/desktop/test/webdav-servers/image-nextcloud/post-install.sh`, which
/// spells it `5GB` and means 5 GiB. `quota-available-bytes` plus
/// `quota-used-bytes` adding up to THIS rather than to the container's disk is
/// how a cell tells "the account's quota" from "the server's free space".
pub const FIXTURE_NEXTCLOUD_QUOTA_BYTES: u64 = 5 * 1024 * 1024 * 1024;

/// One fixture server: the key its port env var carries, the port its compose
/// file publishes by default, and the URL path it exports files at.
struct FixtureService {
    key: &'static str,
    port: u16,
    /// ❗ Not the same for every server, and that is the point of holding it
    /// here: Apache `mod_dav` exports one directory at `/dav/`, while Nextcloud
    /// serves the signed-in account's files under `/remote.php/webdav/`.
    dav_path: &'static str,
}

/// The fixture services and their compose defaults.
///
/// The Nextcloud entry uses the LEGACY endpoint rather than
/// `/remote.php/dav/files/<user>/`, so one base URL serves both of its
/// accounts. The three properties the sabre/dav cells read (`Range`, a chunked
/// PUT, RFC 4331 quota) answer identically on both endpoints (verified on
/// nextcloud 34.0.2-apache, by hand with `curl`, 2026-09-02).
const FIXTURE_SERVICES: [FixtureService; 3] = [
    FixtureService {
        key: "APACHE",
        port: 13480,
        dav_path: "/dav/",
    },
    FixtureService {
        key: "DIGEST",
        port: 13481,
        dav_path: "/dav/",
    },
    FixtureService {
        key: "NEXTCLOUD",
        port: 13482,
        dav_path: "/remote.php/webdav/",
    },
];

/// The env var that points the whole suite at a server of your own. The other
/// three are read only when this one is set.
pub const TEST_URL_ENV: &str = "CMDR_WEBDAV_TEST_URL";
/// The account to sign in as there. Defaults to [`FIXTURE_USER`].
pub const TEST_USERNAME_ENV: &str = "CMDR_WEBDAV_TEST_USERNAME";
/// Its password. ❗ Read into a `String` and handed straight to the credential
/// store: nothing here logs it, prints it, or puts it in an argument.
pub const TEST_PASSWORD_ENV: &str = "CMDR_WEBDAV_TEST_PASSWORD";
/// The volume root under that base URL. Defaults to [`FIXTURE_ROOT`], so the
/// suite works in the whole account unless you point it at a subdirectory.
pub const TEST_ROOT_ENV: &str = "CMDR_WEBDAV_TEST_ROOT";

/// Where one connection goes and who it goes as.
pub struct FixtureTarget {
    /// The base URL, collection-shaped (trailing slash).
    pub base_url: Url,
    /// The account to sign in as.
    pub username: String,
    /// Its password. ❗ Never logged, printed, or put in an argument.
    pub password: String,
    /// The volume root under `base_url`.
    pub root: String,
}

/// The host port a fixture service publishes: `WEBDAV_FIXTURE_{service}_PORT`,
/// else `fallback` (the compose file's own default).
pub fn fixture_port(service: &str, fallback: u16) -> u16 {
    std::env::var(format!("WEBDAV_FIXTURE_{service}_PORT"))
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(fallback)
}

/// Whether the suite is aimed at a server of your own rather than at Docker.
///
/// A cell that can only be honest against the seeded fixture (it reads a
/// landmark file, or it needs the Digest-only server) asks this and returns
/// early. The list of those cells is in the fixture README.
pub fn pointed_at_your_own_server() -> bool {
    std::env::var_os(TEST_URL_ENV).is_some_and(|v| !v.is_empty())
}

/// Answers true, having said why, when the calling cell can't be honest against
/// the server the run is pointed at. `needs` names what it would want there.
///
/// ❗ Prints rather than fails. A cell that asserts on `large.bin` is not wrong
/// when someone points the suite at their Fastmail; it just has nothing to say.
/// Run nextest with `--success-output immediate` to see these go by, which is
/// also what puts the test's own name beside the line.
pub fn not_for_your_own_server(needs: &str) -> bool {
    if !pointed_at_your_own_server() {
        return false;
    }
    eprintln!("skipped: {TEST_URL_ENV} is set, and this cell needs {needs}");
    true
}

/// Where a connection to `service` as `username` should go: the Docker fixture
/// by default, or the server `CMDR_WEBDAV_TEST_URL` names.
///
/// ❗ The single resolver every fixture connection goes through, which is what
/// makes the override one variable rather than an edit per call site.
pub fn fixture_target(service: &str, fallback_port: u16, username: &str) -> FixtureTarget {
    if let Ok(url) = std::env::var(TEST_URL_ENV)
        && !url.is_empty()
    {
        let mut base_url = Url::parse(&url)
            .unwrap_or_else(|e| panic!("{TEST_URL_ENV} is not a URL ({e}); it wants something like https://cloud.example.com/remote.php/dav/files/you/"));
        // ❗ A base URL names a COLLECTION, so it ends in a slash. Without one,
        // every `Url::join` against it drops the last segment, which turns a URL
        // that reads correctly into requests one directory too high.
        if !base_url.path().ends_with('/') {
            let with_slash = format!("{}/", base_url.path());
            base_url.set_path(&with_slash);
        }
        return FixtureTarget {
            base_url,
            username: std::env::var(TEST_USERNAME_ENV).unwrap_or_else(|_| username.to_string()),
            password: std::env::var(TEST_PASSWORD_ENV)
                .unwrap_or_else(|_| panic!("{TEST_URL_ENV} is set but {TEST_PASSWORD_ENV} is not")),
            root: std::env::var(TEST_ROOT_ENV).unwrap_or_else(|_| FIXTURE_ROOT.to_string()),
        };
    }
    // ❗ A panic, not a default: the two servers here export at different URL
    // paths, so a typo'd key that quietly got `/dav/` would 404 against
    // Nextcloud and read as a backend bug.
    let dav_path = FIXTURE_SERVICES
        .iter()
        .find(|s| s.key == service)
        .unwrap_or_else(|| {
            panic!(
                "no fixture service is called {service}; the stack serves {:?}",
                FIXTURE_SERVICES.map(|s| s.key)
            )
        })
        .dav_path;
    FixtureTarget {
        base_url: Url::parse(&format!(
            "http://127.0.0.1:{}{dav_path}",
            fixture_port(service, fallback_port)
        ))
        .expect("a fixture URL is well-formed by construction"),
        username: username.to_string(),
        password: FIXTURE_PASSWORD.to_string(),
        root: FIXTURE_ROOT.to_string(),
    }
}

/// The base URL for one fixture service, as [`FIXTURE_USER`].
pub fn fixture_base_url(service: &str, fallback_port: u16) -> Url {
    fixture_target(service, fallback_port, FIXTURE_USER).base_url
}

impl FixtureTarget {
    /// The connection parameters this target describes.
    pub fn params(&self) -> WebdavConnectionParams {
        WebdavConnectionParams::new(self.base_url.clone(), &self.username, &self.root)
    }
}

/// A host with the secret preloaded for every account the suite signs in as,
/// and a recording event sink. Detached otherwise.
pub fn fixture_host() -> VolumeHost {
    let mut credentials = InMemoryCredentials::new();
    for service in &FIXTURE_SERVICES {
        for username in [FIXTURE_USER, FIXTURE_UNLIMITED_USER] {
            let target = fixture_target(service.key, service.port, username);
            let params = target.params();
            credentials = credentials.with_entry(
                &params.credential_service(),
                Some(&target.username),
                &target.username,
                &target.password,
            );
        }
    }
    VolumeHost::builder()
        .credentials(Arc::new(credentials))
        .events(Arc::new(RecordingVolumeEvents::new()) as Arc<dyn VolumeEventSink>)
        .build()
}

/// Connects to one fixture service as [`FIXTURE_USER`], panicking with a
/// pointer at the stack script if it isn't up.
pub async fn connect_fixture(service: &str, fallback_port: u16) -> WebdavVolume {
    connect_fixture_as(service, fallback_port, FIXTURE_USER).await
}

/// Connects to one fixture service as a named account.
///
/// Only the Nextcloud fixture has a second account
/// ([`FIXTURE_UNLIMITED_USER`]), and it exists so both RFC 4331 answers are
/// reachable from a cell.
pub async fn connect_fixture_as(service: &str, fallback_port: u16, username: &str) -> WebdavVolume {
    let target = fixture_target(service, fallback_port, username);
    match connect_webdav_volume(
        "fixture",
        &format!("webdav-test-{}-{}", service.to_ascii_lowercase(), target.username),
        target.params(),
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
