//! Copying inside one server without the bytes leaving it: one COPY request.
//!
//! The caller stages it exactly as it stages a streamed write, so the bytes
//! land on a `.cmdr-tmp-*` sibling and take the user's filename at the end.
//! A COPY is one request with no progress inside it, so progress is reported
//! once, at the end, with the size a PROPFIND on the source gave.

use std::ops::ControlFlow;
use std::path::Path;

use cmdr_fs::volume::VolumeError;

use super::WebdavVolume;
use crate::errors::Attempted;
use crate::transport::method;

impl WebdavVolume {
    /// Copies one resource inside this server.
    pub(super) async fn copy_within_impl(
        &self,
        from: &Path,
        to: &Path,
        on_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    ) -> Result<u64, VolumeError> {
        let remote_from = self.to_remote_path(from)?;
        let remote_to = self.to_remote_path(to)?;
        let client = self.clone_client().await?;
        let source = self.stat(&client, &remote_from).await?;
        let total = source.size.unwrap_or(0);
        let request = client
            .request(method("COPY"), client.url_for(&remote_from, source.is_collection))
            .header("Destination", client.url_for(&remote_to, source.is_collection).as_str())
            .header("Overwrite", "T")
            .header("Depth", "infinity");
        self.send(request, &remote_to, Attempted::Reaching).await?;
        if on_progress(total, total).is_break() {
            self.remove_best_effort(&remote_to).await;
            return Err(VolumeError::Cancelled(self.volume_id().to_string()));
        }
        Ok(total)
    }
}
