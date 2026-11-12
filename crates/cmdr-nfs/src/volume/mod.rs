//! The NFS backend: a `Volume` over one export, through our own NFSv3 client.
//!
//! There is no OS mount under this: every listing, read, and write is an RPC on
//! a TCP connection this crate holds. The volume's root is the export, so the
//! paths it hands out are export-relative (`/Photos/a.jpg` under `/Public`), and
//! `cmdr_fs::volume::remote_paths::RemoteRoot` is the one translation to the app's
//! spelling (`nfs://testnas.local:2049/Public/Photos/a.jpg`).
//!
//! The lifecycle is `cmdr_fs::volume::http_session`'s, the one WebDAV and S3
//! share, with the connection as its client and no secret (`reconnect.rs`).
//!
//! Nothing here names the application. What the backend needs from it arrives
//! through the [`VolumeHost`] seams handed to [`connect_nfs_volume`].

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::volume::VolumeError;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::settings::BackendName;
use cmdr_fs::volume::http_session::{HttpBackend, HttpSession};
use cmdr_fs::volume::remote_paths::RemoteRoot;
use tokio_util::sync::CancellationToken;

use crate::dial::DialOptions;
use crate::errors::NfsConnectError;
use crate::params::NfsConnectionParams;

mod copy;
mod discovery;
mod handles;
mod mapping;
mod mutation;
mod paths;
mod poll;
mod query;
mod reconnect;
mod scan;
mod streams;
mod volume_impl;
mod writes;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use discovery::{NfsIdentityGuess, NfsIdentitySource, guess_identity, list_exports};
use handles::HandleCache;
use reconnect::UmntTarget;
pub(crate) use reconnect::{Dialer, NfsClient};

/// This backend's settings namespace, for everything it reads through
/// [`VolumeHost::settings`].
const BACKEND: BackendName = "nfs";

/// A volume backed by one export on an NFS server.
pub struct NfsVolume {
    /// Display name, as the app chose to label the place.
    name: String,
    /// Both spellings of the export's root: the app's
    /// (`nfs://testnas.local:2049/Public`) and the export-relative one (`/`).
    /// ❗ Every path translation goes through it, ❌ nothing here spells an app
    /// path by hand.
    root: RemoteRoot,
    inner: Arc<NfsVolumeInner>,
}

/// The connection-scoped half: what every instance of this volume shares.
struct NfsVolumeInner {
    /// The key every piece of durable per-volume state is filed under, minted
    /// from the RESOLVED export (`params.volume_id()`).
    volume_id: String,
    /// How to reach the export again, as the dial resolved it. Behind a lock
    /// because the auto-reconnect switch and the start folder move on the live
    /// volume; the identity never does. Read it through `params()`, a snapshot.
    params: std::sync::RwLock<NfsConnectionParams>,
    /// The connection, the state it reports, and the switches that gate a
    /// rebuild (`cmdr_fs::volume::http_session`).
    session: HttpSession<NfsVolumeInner>,
    /// Path → directory handle, so a deep folder isn't a LOOKUP per level on
    /// every call (`handles.rs`).
    handles: HandleCache,
    /// How a lost connection is dialed again: the real dial, or a cell's fake.
    dialer: Dialer,
    /// Where `UMNT` goes when the volume leaves, as the last dial mounted it.
    umnt: std::sync::Mutex<Option<UmntTarget>>,
    /// The change poller for the directories panes show (`poll.rs`).
    poller: poll::Poller,
    /// Everything this backend asks the app around it.
    host: VolumeHost,
}

impl NfsVolumeInner {
    /// How to reach the export again, as it stands now. A snapshot, so ❌ no
    /// guard is ever held across a call.
    fn params(&self) -> NfsConnectionParams {
        self.params.read_ignore_poison().clone()
    }
}

impl NfsVolume {
    /// The volume id every listing-cache lookup and connection event uses.
    pub fn volume_id(&self) -> &str {
        &self.inner.volume_id
    }

    /// The params this volume dials with, as the connect RESOLVED them: the real
    /// export, and the rest of a typed path as the start folder when none was
    /// given. ❗ What the caller saves, so a saved place never carries a folder
    /// in its export, and the id it registers under is this one's.
    pub fn params(&self) -> NfsConnectionParams {
        self.inner.params()
    }

    /// Another instance of this volume, named `name`, riding THIS instance's
    /// connection: what saving a name or start-folder edit to a connected place
    /// installs, with no redial. ❗ A changed uid or group is a new connection,
    /// ❌ never this.
    ///
    /// Shares everything connection-scoped (the connection, the reconnect loop,
    /// the switch, the handle cache, and the `Retirement`), so it goes into the
    /// registry through a replace that retires nobody.
    pub fn sharing_connection(&self, name: &str) -> NfsVolume {
        NfsVolume {
            name: name.to_string(),
            root: RemoteRoot::new(self.inner.params().app_root(), Path::new("/")),
            inner: Arc::clone(&self.inner),
        }
    }

    /// Moves the place's "Reconnect automatically" switch on the live volume.
    ///
    /// ❗ Switching it ON while the volume sits `Disconnected` starts the
    /// backoff loop then and there.
    pub fn set_auto_reconnect(&self, on: bool) {
        self.inner.params.write_ignore_poison().auto_reconnect = on;
        self.inner.set_auto_reconnect(on);
    }

    /// Moves the place's start folder (export-relative, `/photos`; `None` for
    /// the export itself) on the live volume, and on every instance sharing its
    /// connection: what [`params`](Self::params) answers from now on, with no
    /// redial. What saving a start-folder edit to a connected place calls.
    pub fn set_start_folder(&self, folder: Option<String>) {
        self.inner.params.write_ignore_poison().start_folder = folder;
    }

    /// The live connection, cloned out from under a short read guard. ❗ Clone
    /// and release: holding the guard across a call would serialize every
    /// other call behind it.
    async fn client(&self) -> Result<Arc<NfsClient>, VolumeError> {
        self.inner.session.client().await
    }

    /// Drops the live connection and stops reconnecting, silently.
    #[cfg(any(test, feature = "testing"))]
    pub async fn disconnect(&self) {
        self.inner.session.disconnect().await;
    }

    /// Drops the live connection the way a server going away would, ❗ WITHOUT
    /// starting the backoff loop, so a cell drives the recovery itself.
    #[cfg(any(test, feature = "testing"))]
    pub async fn simulate_session_loss(&self) {
        self.inner.session.simulate_loss().await;
    }

    /// Builds a volume over an already-dialed connection.
    fn assemble(name: &str, params: NfsConnectionParams, client: NfsClient, dialer: Dialer, host: VolumeHost) -> Self {
        let volume_id = params.volume_id();
        let root = RemoteRoot::new(params.app_root(), Path::new("/"));
        let auto_reconnect = params.auto_reconnect;
        let silence = dialer.options().transport.timings.liveness;
        let umnt = client.umnt_target();
        Self {
            name: name.to_string(),
            root,
            inner: Arc::new_cyclic(|me| NfsVolumeInner {
                session: HttpSession::new(me, &volume_id, &host, Some(client), auto_reconnect)
                    .with_silence_timings(silence),
                volume_id,
                params: std::sync::RwLock::new(params),
                handles: HandleCache::new(),
                dialer,
                umnt: std::sync::Mutex::new(umnt),
                poller: poll::Poller::default(),
                host,
            }),
        }
    }
}

/// Opens an NFS export: dials the server, splits `params`' export into the real
/// export and a folder under it, mounts the export as the params' identity, and
/// proves the root handle.
///
/// ❗ **The split happens here, before an id exists.** A typed
/// `/share/Public/photos` resolves by longest match against the server's
/// exports (or, when it won't list them, by `MNT` on ever-shorter prefixes),
/// and the rest becomes the start folder when none was given. So the caller
/// registers under the returned volume's [`NfsVolume::volume_id`] and saves its
/// [`NfsVolume::params`], ❌ never the id of what it passed in. Cancelling
/// `cancel` ends the attempt where it stands and leaves ❗ nothing behind.
pub async fn connect_nfs_volume(
    name: &str,
    params: NfsConnectionParams,
    host: VolumeHost,
    cancel: CancellationToken,
) -> Result<NfsVolume, NfsConnectError> {
    connect_with(name, params, host, Dialer::over_tcp(DialOptions::PRODUCTION), cancel).await
}

/// [`connect_nfs_volume`] through `dialer`, which a cell points at a fake.
async fn connect_with(
    name: &str,
    params: NfsConnectionParams,
    host: VolumeHost,
    dialer: Dialer,
    cancel: CancellationToken,
) -> Result<NfsVolume, NfsConnectError> {
    let (client, resolved) = dialer.first(&params, &cancel).await?;
    if cancel.is_cancelled() {
        client.close();
        return Err(NfsConnectError::Cancelled);
    }
    let mut saved = NfsConnectionParams::new(params.server.clone(), &resolved.export, params.identity.clone());
    saved.start_folder = params.start_folder.clone().or(resolved.folder);
    saved.auto_reconnect = params.auto_reconnect;
    // PII-free: an NFS export came up. ❌ No host, export, uid, or path crosses.
    host.analytics().record("nfs_connected", &[]);
    let volume = NfsVolume::assemble(name, saved, client, dialer, host);
    // Panes may already show this id (a redial that replaces another
    // instance); with none, the poller stops a tick later, having sent nothing.
    volume.inner.start_polling();
    Ok(volume)
}

// The suites asserting on this backend's own behavior.
#[cfg(test)]
mod connection_drop_test;
#[cfg(test)]
mod fake_nfs;
#[cfg(test)]
mod fake_volume_test;
#[cfg(test)]
mod fixture_test;
#[cfg(test)]
mod reconnect_test;
// The cross-backend promises, against the fixture's `/insecure` export.
#[cfg(test)]
mod conformance_test;
