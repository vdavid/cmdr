//! What this backend brings to the shared HTTP lifecycle
//! (`cmdr_fs::volume::http_session`, whose header has the rules): the client,
//! the PROPFIND probe and what its refusals mean, and where the secret is
//! filed.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::http_session::{CredentialKey, DialRefusal, HttpBackend, HttpSession, SessionClient};
use cmdr_fs::volume::liveness::Liveness;
use cmdr_fs::volume::{Retirement, Retires, VolumeError};
use tokio_util::sync::CancellationToken;

use super::{WebdavVolumeInner, build_and_probe};
use crate::errors::WebdavConnectError;
use crate::transport::WebdavClient;

impl SessionClient for WebdavClient {
    fn liveness(&self) -> &Arc<Liveness> {
        WebdavClient::liveness(self)
    }

    fn ping(&self) -> impl Future<Output = bool> + Send {
        WebdavClient::ping(self)
    }
}

impl Retires for WebdavVolumeInner {
    fn retirement(&self) -> &Retirement {
        self.session.connection().retirement()
    }
}

impl HttpBackend for WebdavVolumeInner {
    type Client = WebdavClient;

    const BACKEND: &'static str = "webdav";

    fn session(&self) -> &HttpSession<Self> {
        &self.session
    }

    fn host(&self) -> &VolumeHost {
        &self.host
    }

    fn credential_key(&self) -> CredentialKey {
        let params = self.params();
        CredentialKey {
            service: params.credential_service(),
            scope: params.username,
        }
    }

    async fn dial(&self, secret: &str) -> Result<WebdavClient, DialRefusal> {
        match build_and_probe(&self.params(), secret, &CancellationToken::new()).await {
            Ok((client, keeps_dates)) => {
                self.keeps_dates.store(keeps_dates, Ordering::Relaxed);
                Ok(client)
            }
            Err(WebdavConnectError::AuthenticationRejected | WebdavConnectError::AuthMethodUnsupported) => {
                Err(DialRefusal::Rejected)
            }
            Err(WebdavConnectError::NeedsCredentials) => Err(DialRefusal::NeedsUser),
            Err(WebdavConnectError::Cancelled) => Err(DialRefusal::Transient(VolumeError::Cancelled(
                self.volume_id.clone().into(),
            ))),
            Err(WebdavConnectError::TimedOut) => Err(DialRefusal::Transient(VolumeError::ConnectionTimeout(
                self.volume_id.clone().into(),
            ))),
            Err(WebdavConnectError::Unreachable(what) | WebdavConnectError::Transport(what)) => {
                Err(DialRefusal::Transient(VolumeError::DeviceDisconnected(what.into())))
            }
            Err(WebdavConnectError::CertificateUntrusted | WebdavConnectError::NotAWebdavServer) => Err(
                DialRefusal::Transient(VolumeError::DeviceDisconnected(self.volume_id.clone().into())),
            ),
        }
    }
}
