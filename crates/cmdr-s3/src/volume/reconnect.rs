//! What this backend brings to the shared HTTP lifecycle
//! (`cmdr_fs::volume::http_session`, whose header has the rules): the signed
//! client, the connect probe and what its refusals mean, where the secret is
//! filed, and the unfinished-upload sweep a rebuilt client starts.

use std::sync::Arc;

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::http_session::{CredentialKey, DialRefusal, HttpBackend, HttpSession, SessionClient};
use cmdr_fs::volume::liveness::Liveness;
use cmdr_fs::volume::{Retirement, Retires, VolumeError};
use tokio_util::sync::CancellationToken;

use super::{S3VolumeInner, build_and_probe};
use crate::refusal::S3ConnectError;
use crate::transport::S3Client;

impl SessionClient for S3Client {
    fn liveness(&self) -> &Arc<Liveness> {
        S3Client::liveness(self)
    }

    fn ping(&self) -> impl Future<Output = bool> + Send {
        S3Client::ping(self)
    }
}

impl Retires for S3VolumeInner {
    fn retirement(&self) -> &Retirement {
        self.session.connection().retirement()
    }
}

impl HttpBackend for S3VolumeInner {
    type Client = S3Client;

    const BACKEND: &'static str = "s3";

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
            scope: params.access_key_id().to_string(),
        }
    }

    async fn dial(&self, secret: &str) -> Result<S3Client, DialRefusal> {
        match build_and_probe(&self.params(), secret, &CancellationToken::new()).await {
            Ok(client) => Ok(client),
            Err(
                S3ConnectError::KeysRejected
                | S3ConnectError::AccessDenied
                | S3ConnectError::BucketListRefused
                | S3ConnectError::NeedsCredentials,
            ) => Err(DialRefusal::Rejected),
            Err(S3ConnectError::Cancelled) => {
                Err(DialRefusal::Transient(VolumeError::Cancelled(self.volume_id.clone())))
            }
            Err(S3ConnectError::TimedOut) => Err(DialRefusal::Transient(VolumeError::ConnectionTimeout(
                self.volume_id.clone(),
            ))),
            Err(
                S3ConnectError::Unreachable(_)
                | S3ConnectError::Transport(_)
                | S3ConnectError::InvalidProvider
                | S3ConnectError::NoSuchBucket
                | S3ConnectError::WrongRegion { .. }
                | S3ConnectError::ClockSkewed
                | S3ConnectError::CertificateUntrusted
                | S3ConnectError::NotAnS3Endpoint,
            ) => Err(DialRefusal::Transient(VolumeError::DeviceDisconnected(
                self.volume_id.clone(),
            ))),
        }
    }

    /// An upload the outage cut off couldn't be aborted then.
    fn client_installed(&self) {
        self.spawn_upload_sweep();
    }
}
