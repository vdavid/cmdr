//! A folder's DELETE, closed against a child landing mid-way: the folder moves
//! aside to a name only this call knows, is counted there, then deleted or
//! moved back. Why the protocol's own guards (an `If` ETag, a `LOCK`) don't do:
//! `DETAILS.md` § "A folder's DELETE".

use std::path::PathBuf;
use std::sync::Arc;

use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::staging::{StagingTemp, deleting_sibling_name, is_deleting_aside_name, is_staging_temp_in_flight};
use cmdr_fs::volume::VolumeError;
use log::debug;
use reqwest::Method;

use super::WebdavVolume;
use super::mutation::still_holds;
use super::paths::child_of;
use crate::errors::Attempted;
use crate::transport::{Depth, MUTATION_BUDGET, WebdavClient, method};

/// How many `<name> (kept by Cmdr N)` names a folder that can't go back tries.
const KEEP_TRIES: u32 = 20;

impl WebdavVolume {
    /// Deletes the collection at `remote`, which a `Depth: 1` listing has just
    /// found empty.
    ///
    /// ❗ On a task of its own: once the folder is aside, only finishing puts it
    /// right, so a caller that stops waiting (a silent server's cut, a dropped
    /// future) must not strand it under the aside name. Every request still has
    /// its `MUTATION_BUDGET`, so the task ends.
    pub(super) async fn delete_collection(&self, client: Arc<WebdavClient>, remote: &str) -> Result<(), VolumeError> {
        let volume = self.twin();
        let remote = remote.to_string();
        self.inner
            .host
            .runtime()
            .spawn(async move { volume.delete_by_moving_aside(&client, &remote).await })
            .await
            .unwrap_or_else(|stopped| {
                Err(VolumeError::IoError {
                    message: format!("the folder delete stopped: {stopped}"),
                    raw_os_error: None,
                })
            })
    }

    async fn delete_by_moving_aside(&self, client: &WebdavClient, remote: &str) -> Result<(), VolumeError> {
        let (parent, name) = remote.rsplit_once('/').unwrap_or(("", remote));
        let aside = child_of(parent, &deleting_sibling_name(name));
        // Out of this app's listings while it's here; a leftover nobody holds shows.
        let _held = StagingTemp::adopt(PathBuf::from(&aside), None);
        match self
            .move_collection(client, remote, &aside, (remote, Attempted::Reaching))
            .await
        {
            Ok(()) => {}
            Err(VolumeError::NotSupported | VolumeError::PermissionDenied { .. }) => {
                debug!(
                    "WebdavVolume::delete: the server won't move {remote} aside, deleting it in place",
                    remote = LogPrivate::path(remote)
                );
                return self.delete_collection_at(client, remote, remote).await;
            }
            Err(e) => return Err(e),
        }
        let held = match self.propfind(client, &aside, true, Depth::One).await {
            Ok(listing) => listing.len().saturating_sub(1),
            Err(e) => return Err(self.put_back(client, &aside, remote, e).await),
        };
        if held > 0 {
            return Err(self.put_back(client, &aside, remote, still_holds(remote, held)).await);
        }
        match self.delete_collection_at(client, &aside, remote).await {
            // Gone either way, which is what was asked.
            Ok(()) | Err(VolumeError::NotFound(_)) => Ok(()),
            Err(e) => Err(self.put_back(client, &aside, remote, e).await),
        }
    }

    /// Moves the folder at `aside` back to `remote`, and answers `why` the
    /// delete stopped. When `remote` was taken meanwhile, the folder stays
    /// beside it under a name a person can read, and the answer says where.
    /// (`pub(super)` for the real-server cells, which can't time the race.)
    pub(super) async fn put_back(
        &self,
        client: &WebdavClient,
        aside: &str,
        remote: &str,
        why: VolumeError,
    ) -> VolumeError {
        let back = self
            .move_collection(client, aside, remote, (remote, Attempted::TakingAName))
            .await;
        let taken = match back {
            Ok(()) => return why,
            Err(VolumeError::AlreadyExists(_)) => true,
            Err(_) => false,
        };
        if taken {
            for attempt in 1..=KEEP_TRIES {
                let kept = kept_name(remote, attempt);
                match self
                    .move_collection(client, aside, &kept, (&kept, Attempted::TakingAName))
                    .await
                {
                    Ok(()) => return noted(why, &format!("a new {remote} took its name, so it's kept as {kept}")),
                    Err(VolumeError::AlreadyExists(_)) => {}
                    Err(_) => break,
                }
            }
        }
        noted(why, &format!("it couldn't go back, so it's kept as {aside}"))
    }

    /// A collection MOVE that never replaces anything (`Overwrite: F`), its
    /// refusal judged against `named`.
    async fn move_collection(
        &self,
        client: &WebdavClient,
        from: &str,
        to: &str,
        (named, attempted): (&str, Attempted),
    ) -> Result<(), VolumeError> {
        let request = client
            .request(method("MOVE"), client.url_for(from, true))
            .header("Destination", client.url_for(to, true).as_str())
            .header("Overwrite", "F")
            .timeout(MUTATION_BUDGET);
        self.send(client, request, named, attempted).await.map(|_| ())
    }

    /// One DELETE of the collection at `remote`, its refusal judged against
    /// `named`.
    async fn delete_collection_at(&self, client: &WebdavClient, remote: &str, named: &str) -> Result<(), VolumeError> {
        let request = client
            .request(Method::DELETE, client.url_for(remote, true))
            .timeout(MUTATION_BUDGET);
        self.send(client, request, named, Attempted::Reaching).await.map(|_| ())
    }

    /// Whether `remote`, a listing's child called `name`, is an EMPTY aside a
    /// delete left behind (a crash between its steps) that is now deleted.
    ///
    /// ❗ Empty only, ❌ never one with something in it: that one is a folder
    /// whose move back didn't happen, and its name still says which it was.
    /// Best effort: anything in the way leaves it listed.
    pub(super) async fn tidied_leftover(&self, client: &WebdavClient, remote: &str, name: &str) -> bool {
        if !is_deleting_aside_name(name) || is_staging_temp_in_flight(name) {
            return false;
        }
        match self.propfind(client, remote, true, Depth::One).await {
            Ok(listing) if listing.len() <= 1 => {}
            Err(VolumeError::NotFound(_)) => return true,
            _ => return false,
        }
        debug!(
            "WebdavVolume: tidying an empty leftover aside, {remote}",
            remote = LogPrivate::path(remote)
        );
        matches!(
            self.delete_collection_at(client, remote, remote).await,
            Ok(()) | Err(VolumeError::NotFound(_))
        )
    }

    /// Another handle on this volume, for work that must outlive the call.
    fn twin(&self) -> Self {
        Self {
            name: self.name.clone(),
            root: self.root.clone(),
            inner: Arc::clone(&self.inner),
        }
    }
}

/// `<name> (kept by Cmdr)` beside `remote`, numbered from the second try.
fn kept_name(remote: &str, attempt: u32) -> String {
    if attempt == 1 {
        format!("{remote} (kept by Cmdr)")
    } else {
        format!("{remote} (kept by Cmdr {attempt})")
    }
}

/// `why`, with `note` on where the folder is now. Keeps an `IoError`'s errno,
/// which is what the app renders from.
fn noted(why: VolumeError, note: &str) -> VolumeError {
    match why {
        VolumeError::IoError { message, raw_os_error } => VolumeError::IoError {
            message: format!("{message}; {note}"),
            raw_os_error,
        },
        other => VolumeError::IoError {
            message: format!("{other}; {note}"),
            raw_os_error: None,
        },
    }
}
