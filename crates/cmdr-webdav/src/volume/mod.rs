//! The WebDAV backend: a `Volume` over one HTTP client with one account's
//! credentials on it.
//!
//! There is no OS mount under this and never will be: every listing, read, and
//! write is an HTTP request. The volume's root is the collection it was opened
//! at, so the paths it hands out are root-relative remote paths (`/Photos/a.jpg`
//! under the base URL) and nothing has to be translated between two spellings
//! of the same tree.
//!
//! Nothing here names the application. What the backend needs from it arrives
//! through the [`VolumeHost`] seams handed to [`connect_webdav_volume`].
//! `CLAUDE.md` has the must-knows, `DETAILS.md` the decisions.

use std::path::Path;
use std::sync::Arc;
#[cfg(any(test, feature = "testing"))]
use std::sync::Weak;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::volume::VolumeError;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::settings::BackendName;
use cmdr_fs::volume::http_session::{HttpBackend, HttpSession};
use cmdr_fs::volume::remote_paths::RemoteRoot;
use reqwest::{RequestBuilder, Response};
use tokio_util::sync::CancellationToken;

use crate::chunking::CHUNK_SIZE;
use crate::errors::{Attempted, WebdavConnectError, map_status, map_transport_error};
use crate::params::WebdavConnectionParams;
use crate::transport::WebdavClient;

mod chunked_upload;
mod copy;
mod delete_aside;
mod mapping;
mod mutation;
mod paths;
mod query;
mod reconnect;
mod scan;
mod streams;
mod volume_impl;
mod writes;

pub use cmdr_fs::volume::http_session::UnattendedReconnect;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// This backend's settings namespace, for everything it reads through
/// [`VolumeHost::settings`].
const BACKEND: BackendName = "webdav";

/// A volume backed by a WebDAV server.
pub struct WebdavVolume {
    /// Display name, as the app chose to label the server.
    name: String,
    /// Both spellings of the collection this volume is rooted at: the app's
    /// (`webdav://ada@nas.local:443/Photos`) and the server's (`/Photos`).
    /// Immutable: a different root is a different instance. ❗ Every path
    /// translation goes through it and ❌ nothing here spells a remote path by
    /// hand (`cmdr_fs::volume::remote_paths`).
    root: RemoteRoot,
    inner: Arc<WebdavVolumeInner>,
}

/// The connection-scoped half: what every instance of this volume shares.
struct WebdavVolumeInner {
    /// The key every piece of durable per-volume state is filed under.
    volume_id: String,
    /// How to reach the server, and how to reach it again. Behind a lock
    /// because an edit to a connected place moves the root the next re-probe
    /// asks for (`WebdavVolume::set_redial_root`); the identity never moves.
    /// Read it through `params()`, a snapshot.
    params: std::sync::RwLock<WebdavConnectionParams>,
    /// The client, the connection it reports, and the switches that gate a
    /// rebuild: the HTTP lifecycle this backend shares with `cmdr-s3`
    /// (`cmdr_fs::volume::http_session`).
    session: HttpSession<WebdavVolumeInner>,
    /// Whether the server keeps creation dates and a folder's dates (a
    /// Nextcloud), as its last connect probe said (`transport.rs`'s `PROBE_BODY`).
    /// Read by `metadata_caps`, which can't wait on the client's lock.
    keeps_dates: AtomicBool,
    /// The chunk a big upload to a Nextcloud is cut into, and the size past
    /// which it is (`crate::chunking::CHUNK_SIZE`). A field rather than the
    /// constant so a cell can chunk a small file.
    upload_chunk: AtomicU64,
    /// Everything this backend asks the app around it.
    host: VolumeHost,
}

impl WebdavVolumeInner {
    /// How to reach the server again, as it stands now. A snapshot, so ❌ no
    /// guard is ever held across a probe.
    pub(super) fn params(&self) -> WebdavConnectionParams {
        self.params.read_ignore_poison().clone()
    }
}

impl WebdavVolume {
    /// The volume id every listing-cache lookup and connection event uses.
    pub fn volume_id(&self) -> &str {
        &self.inner.volume_id
    }

    /// Another instance of this volume, named `name` and rooted at the
    /// collection `remote_root` under the base URL, riding THIS instance's
    /// client: what saving an edit to a connected place installs, with no
    /// re-probe.
    ///
    /// ❗ Shares everything connection-scoped (the client, the reconnect loop,
    /// the switch, and the `Retirement`), so it goes into the registry through a
    /// replace that retires nobody. Pure: nothing is committed until the caller
    /// installs it. ❗ Not `Volume::rerooted`, for the reasons
    /// `crates/cmdr-sftp`'s twin gives: `DETAILS.md` § "The connection model".
    pub fn sharing_connection(&self, name: &str, remote_root: &Path) -> WebdavVolume {
        let prefix = {
            let params = self.inner.params.read_ignore_poison();
            cmdr_fs::volume::webdav_app_root(params.host(), params.port(), &params.username)
        };
        WebdavVolume {
            name: name.to_string(),
            root: RemoteRoot::new(prefix, remote_root),
            inner: Arc::clone(&self.inner),
        }
    }

    /// Moves the root the NEXT re-probe asks for, with no re-probe now. ❗ A
    /// reconnect probes the root, so a place whose old root was since deleted
    /// would otherwise never come back.
    pub fn set_redial_root(&self, remote_root: &Path) {
        self.inner.params.write_ignore_poison().remote_root = remote_root.to_path_buf();
    }

    /// Moves the user's "reconnect automatically" switch on a mounted volume.
    ///
    /// ❗ Switching it ON while the volume sits `Disconnected` starts the
    /// backoff loop then and there.
    pub fn set_auto_reconnect(&self, on: bool) {
        self.inner.set_auto_reconnect(on);
    }

    /// Whether an unattended reconnect can actually happen as this volume
    /// stands. Reads the secret store only when the switch is on, because a
    /// needless read is a needless Keychain prompt.
    pub async fn unattended_reconnect(&self) -> UnattendedReconnect {
        self.inner.unattended_reconnect().await
    }

    /// The live client, cloned out from under a short read guard. ❗ Clone and
    /// release: holding the guard across a request would serialize every
    /// other request behind it.
    pub(super) async fn clone_client(&self) -> Result<Arc<WebdavClient>, VolumeError> {
        self.inner.session.client().await
    }

    /// Sends one request and judges the answer in the `Volume` vocabulary:
    /// a transport failure by its typed predicates, a non-2xx by the status
    /// table, both against `path` and what the request was trying to do.
    pub(super) async fn send(
        &self,
        client: &WebdavClient,
        request: RequestBuilder,
        path: &str,
        attempted: Attempted,
    ) -> Result<Response, VolumeError> {
        let response = client
            .send(request)
            .await
            .map_err(|e| map_transport_error(&e, &self.inner.volume_id, path))?;
        if response.status().is_success() {
            Ok(response)
        } else {
            Err(map_status(response.status(), path, attempted))
        }
    }

    /// Drops the live client. There is no session to close: dropping IS the
    /// shutdown, and the connection pool goes with it.
    pub async fn disconnect(&self) {
        self.inner.session.disconnect().await;
    }

    /// Drops the live client the way a server going away would, ❗ WITHOUT
    /// starting the backoff loop, so a cell drives the recovery itself. The
    /// state stays `Connected` until an operation notices.
    #[cfg(any(test, feature = "testing"))]
    pub async fn simulate_session_loss(&self) {
        self.inner.session.simulate_loss().await;
    }

    /// Shortens the silence ladder, for a cell that runs it in real time. Takes
    /// effect from the next watch started.
    #[cfg(test)]
    pub(super) fn set_silence_timings(&self, timings: cmdr_fs::volume::liveness::Timings) {
        self.inner.session.set_silence_timings(timings);
    }

    /// The chunk a big upload to a Nextcloud is cut into.
    pub(super) fn upload_chunk_size(&self) -> u64 {
        self.inner.upload_chunk.load(Ordering::Relaxed)
    }

    /// Shrinks the upload chunk, for a cell that sends a small file through
    /// the chunked path. ❗ Nextcloud's v2 takes no chunk under 5 MB but the
    /// last.
    #[cfg(test)]
    pub(super) fn set_upload_chunk_size(&self, bytes: u64) {
        self.inner.upload_chunk.store(bytes, Ordering::Relaxed);
    }

    /// Whether the connect probe found a Nextcloud's chunked uploads.
    #[cfg(test)]
    pub(super) async fn takes_chunked_uploads(&self) -> bool {
        self.clone_client()
            .await
            .is_ok_and(|client| client.chunked_uploads().is_some())
    }

    /// A handle on the live client, for a cell asserting that nothing re-probed.
    ///
    /// A `Weak` keeps the allocation (not the client) alive, so two witnesses are
    /// `Weak::ptr_eq` exactly when they saw the same client: a reconnect installs
    /// a new allocation. `None` while there is no client.
    #[cfg(any(test, feature = "testing"))]
    pub async fn client_witness(&self) -> Option<Weak<dyn std::any::Any + Send + Sync>> {
        let client: Arc<dyn std::any::Any + Send + Sync> = self.inner.session.installed_client().await?;
        Some(Arc::downgrade(&client))
    }
}

/// Opens a WebDAV volume: reads the account's secret from the store, builds a
/// client, and probes the root with a PROPFIND.
///
/// `volume_id` must be the one the caller registers the volume under
/// (`cmdr_fs::volume::webdav_volume_id`). Cancelling `cancel` ends the
/// attempt where it stands and leaves ❗ nothing behind: no volume, no secret
/// written.
pub async fn connect_webdav_volume(
    name: &str,
    volume_id: &str,
    params: WebdavConnectionParams,
    host: VolumeHost,
    cancel: CancellationToken,
) -> Result<WebdavVolume, WebdavConnectError> {
    let secret = {
        let host = host.clone();
        let service = params.credential_service();
        let scope = params.username.clone();
        // ❗ On a blocking task: the store may put a Keychain prompt in front
        // of this, and a modal dialog on the async runtime stalls every volume.
        tokio::task::spawn_blocking(move || host.credentials().credentials(&service, Some(&scope)))
            .await
            .ok()
            .flatten()
    };
    let Some(secret) = secret else {
        return Err(WebdavConnectError::NeedsCredentials);
    };
    let pinned = host.certificates().pinned(params.host(), params.port());
    let (client, keeps_dates) = build_and_probe(&params, &secret.secret, pinned, &cancel).await?;
    if cancel.is_cancelled() {
        return Err(WebdavConnectError::Cancelled);
    }
    // PII-free: a WebDAV client came up. ❌ No host, account, or path crosses.
    host.analytics().record("webdav_connected", &[]);
    Ok(WebdavVolume::assemble(
        name,
        volume_id,
        params,
        (client, keeps_dates),
        host,
    ))
}

/// A client for `params` with `password`, trusting the `pinned` certificate on
/// top of the OS's, proven by one PROPFIND on the root, and whether that answer
/// says the server keeps dates (Nextcloud's `creation_time` in a 2xx
/// `propstat`; `DETAILS.md` § "Dates").
pub(super) async fn build_and_probe(
    params: &WebdavConnectionParams,
    password: &str,
    pinned: Option<String>,
    cancel: &CancellationToken,
) -> Result<(WebdavClient, bool), WebdavConnectError> {
    let mut client = WebdavClient::new(params.base_url.clone(), &params.username, password, pinned)?;
    let root = paths::root_remote_path(&params.remote_root);
    let root_url = client.url_for(&root, true);
    let entries = tokio::select! {
        () = cancel.cancelled() => Err(WebdavConnectError::Cancelled),
        probed = client.probe(root_url) => probed,
    }?;
    let keeps_dates = entries.iter().any(|entry| entry.nc_creation_time.is_some());
    Ok((client, keeps_dates))
}

impl WebdavVolume {
    fn assemble(
        name: &str,
        volume_id: &str,
        params: WebdavConnectionParams,
        (client, keeps_dates): (WebdavClient, bool),
        host: VolumeHost,
    ) -> Self {
        let root = RemoteRoot::new(
            cmdr_fs::volume::webdav_app_root(params.host(), params.port(), &params.username),
            &params.remote_root,
        );
        let auto_reconnect = params.auto_reconnect;
        Self {
            name: name.to_string(),
            root,
            inner: Arc::new_cyclic(|me| WebdavVolumeInner {
                session: HttpSession::new(me, volume_id, &host, Some(client), auto_reconnect),
                volume_id: volume_id.to_string(),
                params: std::sync::RwLock::new(params),
                keeps_dates: AtomicBool::new(keeps_dates),
                upload_chunk: AtomicU64::new(CHUNK_SIZE),
                host,
            }),
        }
    }
}

// The suites asserting on this backend's own behavior.
#[cfg(test)]
mod cancel_test;
#[cfg(test)]
mod conformance_test;
#[cfg(test)]
mod connection_drop_test;
#[cfg(test)]
mod delete_aside_test;
#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod lifecycle_test;
// ❗ Its own module because its own LANE selects it by this module path:
// `desktop-rust-webdav-nextcloud` runs `test(volume::nextcloud_test::)` and the
// shared fixture lane subtracts the same atom, so a rename here has to move
// `WebdavNextcloudTestAtom` with it (`scripts/check/checks/desktop-rust-webdav-nextcloud.go`).
#[cfg(test)]
mod fake_dav;
#[cfg(test)]
mod get_length_test;
#[cfg(test)]
mod nextcloud_test;
#[cfg(test)]
mod reconnect_test;
#[cfg(test)]
mod sharing_test;
#[cfg(test)]
mod slow_server_test;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tls_trust_test;
