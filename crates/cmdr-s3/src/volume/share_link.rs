//! "Copy share link": a presigned GET for one object, signed offline with the
//! account's keys, so minting one is free and sends nothing.
//!
//! ❗ The URL is a credential (anyone holding it reads the object until it
//! expires), so it travels as a `ShareLink`, whose `Debug` prints nothing, and
//! ❌ nothing here logs it.

use std::path::Path;

use cmdr_fs::volume::{ShareLink, ShareLinkExpiry, VolumeError};

use super::S3Volume;
use super::paths::{Holder, Resolved, Target, target_of};
use crate::ops::ShareLinkError;

impl S3Volume {
    /// A link to the object at `path`, valid for `expires_in`. The account
    /// root and a bucket's top are folders. A key that turns out to be a folder
    /// (a prefix) gets a link that answers 404: telling the two apart would cost
    /// a request, and the UI only offers this on a file row.
    pub(super) async fn share_link_impl(
        &self,
        path: &Path,
        expires_in: ShareLinkExpiry,
    ) -> Result<ShareLink, VolumeError> {
        let Resolved { remote, holder } = self.resolve(path)?;
        let Target::Key { bucket, key } = target_of(&remote) else {
            return Err(VolumeError::IsADirectory(remote));
        };
        // The folder row beside a `<name> (file)` row: never that file's link.
        if holder == Holder::Folder {
            return Err(VolumeError::IsADirectory(remote));
        }
        let client = self.clone_client().await?;
        match client.share_link(bucket, key, expires_in.duration()).await {
            Ok(url) => Ok(ShareLink::new(url.into())),
            // A key this stack can't address (a `.` or `..` segment).
            Err(ShareLinkError::Build(_)) => Err(VolumeError::NotFound(remote)),
            // Every `ShareLinkExpiry` fits inside the seven-day ceiling.
            Err(ShareLinkError::ExpiryOutOfRange) => Err(VolumeError::NotSupported),
        }
    }
}
