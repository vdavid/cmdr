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

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Weak};

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::settings::BackendName;
use cmdr_fs::volume::{Retirement, VolumeError};
use reqwest::{RequestBuilder, Response};
use tokio_util::sync::CancellationToken;

use crate::errors::{Attempted, WebdavConnectError, map_status, map_transport_error};
use crate::params::WebdavConnectionParams;
use crate::transport::WebdavClient;

mod copy;
mod mapping;
mod mutation;
mod paths;
mod query;
mod reconnect;
mod scan;
mod state;
mod streams;
mod volume_impl;
mod writes;

pub use state::ConnectionState;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// This backend's settings namespace, for everything it reads through
/// [`VolumeHost::settings`].
const BACKEND: BackendName = "webdav";

/// Whether an unattended reconnect can actually happen as a volume stands.
///
/// ❗ The backend's answer to "the switch is on but nothing comes back", so ❌
/// no frontend has to derive it from a `has_credentials` call. One rung only:
/// this backend redials out of the secret store or not at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnattendedReconnect {
    /// The switch is on and the store holds a secret: a dropped connection
    /// comes back on its own (one attempt; a refusal stops it).
    Possible,
    /// The user's "reconnect automatically" switch is off. Nothing redials,
    /// however full the store.
    SwitchOff,
    /// The switch is on but nothing is remembered, so there is nothing to
    /// redial with. Only the user moves this forward.
    NoStoredSecret,
}

/// A volume backed by a WebDAV server.
pub struct WebdavVolume {
    /// Display name, as the app chose to label the server.
    name: String,
    /// The collection this volume is rooted at, under the base URL. Immutable:
    /// a different root is a different instance.
    root: PathBuf,
    inner: Arc<WebdavVolumeInner>,
}

/// The connection-scoped half: what every instance of this volume shares.
struct WebdavVolumeInner {
    /// The key every piece of durable per-volume state is filed under.
    volume_id: String,
    /// How to reach the server, and how to reach it again.
    params: WebdavConnectionParams,
    /// The live client. `None` once a request found the server gone, at which
    /// point every operation fails fast rather than each one timing out.
    client: tokio::sync::RwLock<Option<Arc<WebdavClient>>>,
    /// The state the host was last told about (`state.rs`).
    state: AtomicU8,
    /// Whether the registry still serves this volume under its id.
    retirement: Retirement,
    /// This state's own weak reference, for the background work that outlives
    /// the call that started it. Set by `Arc::new_cyclic`.
    me: Weak<WebdavVolumeInner>,
    /// Single-flight around a client rebuild.
    reconnect_lock: tokio::sync::Mutex<()>,
    /// Set by `on_unmount`. A reconnect in flight bails rather than installing
    /// a client into a volume the app has forgotten.
    unmounted: AtomicBool,
    /// Whether the user's per-server "reconnect automatically" switch is on.
    /// Live rather than read from `params`, so flipping it needs no remount.
    auto_reconnect: AtomicBool,
    /// Whether the one unattended authentication attempt has been spent
    /// (`reconnect.rs`). ❌ Never a loop: repeated refusals lock accounts.
    auth_attempt_spent: AtomicBool,
    /// Everything this backend asks the app around it.
    host: VolumeHost,
}

impl WebdavVolume {
    /// The volume id every listing-cache lookup and connection event uses.
    pub fn volume_id(&self) -> &str {
        &self.inner.volume_id
    }

    /// Moves the user's "reconnect automatically" switch on a mounted volume.
    ///
    /// ❗ Switching it ON while the volume sits `Disconnected` starts the
    /// backoff loop then and there.
    pub fn set_auto_reconnect(&self, on: bool) {
        let was = self.inner.auto_reconnect.swap(on, Ordering::Relaxed);
        if on && !was {
            self.inner.start_reconnect_loop_if_down();
        }
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
        self.inner
            .client
            .read()
            .await
            .clone()
            .ok_or_else(|| VolumeError::DeviceDisconnected(self.inner.volume_id.clone()))
    }

    /// Sends one request and judges the answer in the `Volume` vocabulary:
    /// a transport failure by its typed predicates, a non-2xx by the status
    /// table, both against `path` and what the request was trying to do.
    pub(super) async fn send(
        &self,
        request: RequestBuilder,
        path: &str,
        attempted: Attempted,
    ) -> Result<Response, VolumeError> {
        let response = request
            .send()
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
        self.inner.unmounted.store(true, Ordering::Relaxed);
        self.inner.mark_gone_silently();
        self.inner.client.write().await.take();
    }

    /// Drops the live client the way a server going away would, ❗ WITHOUT
    /// starting the backoff loop, so a cell drives the recovery itself. The
    /// state stays `Connected` until an operation notices.
    #[cfg(any(test, feature = "testing"))]
    pub async fn simulate_session_loss(&self) {
        self.inner.client.write().await.take();
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
    let client = build_and_probe(&params, &secret.secret, &cancel).await?;
    if cancel.is_cancelled() {
        return Err(WebdavConnectError::Cancelled);
    }
    // PII-free: a WebDAV client came up. ❌ No host, account, or path crosses.
    host.analytics().record("webdav_connected", &[]);
    Ok(WebdavVolume::assemble(name, volume_id, params, client, host))
}

/// A client for `params` with `password`, proven by one PROPFIND on the root.
pub(super) async fn build_and_probe(
    params: &WebdavConnectionParams,
    password: &str,
    cancel: &CancellationToken,
) -> Result<WebdavClient, WebdavConnectError> {
    let client = WebdavClient::new(params.base_url.clone(), &params.username, password)?;
    let root = paths::root_remote_path(&params.remote_root);
    let root_url = client.url_for(&root, true);
    tokio::select! {
        () = cancel.cancelled() => Err(WebdavConnectError::Cancelled),
        probed = client.probe(root_url) => probed.map(|_| client),
    }
}

impl WebdavVolume {
    fn assemble(
        name: &str,
        volume_id: &str,
        params: WebdavConnectionParams,
        client: WebdavClient,
        host: VolumeHost,
    ) -> Self {
        let root = PathBuf::from(paths::root_remote_path(&params.remote_root));
        let auto_reconnect = params.auto_reconnect;
        Self {
            name: name.to_string(),
            root,
            inner: Arc::new_cyclic(|me| WebdavVolumeInner {
                volume_id: volume_id.to_string(),
                params,
                client: tokio::sync::RwLock::new(Some(Arc::new(client))),
                state: AtomicU8::new(ConnectionState::Connected as u8),
                retirement: Retirement::new(),
                me: me.clone(),
                reconnect_lock: tokio::sync::Mutex::new(()),
                unmounted: AtomicBool::new(false),
                auto_reconnect: AtomicBool::new(auto_reconnect),
                auth_attempt_spent: AtomicBool::new(false),
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
mod integration_test;
// ❗ Its own module because its own LANE selects it by this module path:
// `desktop-rust-webdav-nextcloud` runs `test(volume::nextcloud_test::)` and the
// shared fixture lane subtracts the same atom, so a rename here has to move
// `WebdavNextcloudTestAtom` with it (`scripts/check/checks/desktop-rust-webdav-nextcloud.go`).
#[cfg(test)]
mod nextcloud_test;
#[cfg(test)]
mod test_support;
