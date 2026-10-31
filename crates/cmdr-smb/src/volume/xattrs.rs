//! Extended attributes on a share, over smb2's named streams: what `Volume`'s
//! `*_xattr*` methods do here. `DETAILS.md` § "Extended attributes".
//!
//! Every call goes over the session, so none of this needs the macOS mount.

use super::SmbVolume;
use super::dfs_links::{LinkCall, is_behind_dfs_link};
use super::session::SessionLease;
use cmdr_fs::volume::{VolumeError, XattrAnswer, XattrReadMode};
use log::debug;
use smb2::SmbClient;
use smb2::client::tree::Tree;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

/// [`SmbVolume::or_through_link`]'s `op` for one xattr read, after a batch item
/// came back behind a DFS link. `name` is owned because the link's call outlives
/// the borrow the signature can name.
fn link_get_xattr<'c>(
    client: &'c mut SmbClient,
    tree: &'c mut Tree,
    path: &'c str,
    name: String,
) -> LinkCall<'c, Option<Vec<u8>>> {
    Box::pin(async move { client.get_xattr(tree, path, &name).await })
}

/// What `xattr_refused` means for one smb2 error: the two ways a server says it
/// can't hold what was asked (`Error::NamedStreamsUnsupported`, a plain Samba; and
/// `Error::StreamDeleteIgnored`, a QNAP keeping a stream it reported deleted). Both
/// are the backend's answer to the question, not a transport failure.
fn is_refused_by_the_store(err: &smb2::Error) -> bool {
    matches!(
        err,
        smb2::Error::NamedStreamsUnsupported { .. } | smb2::Error::StreamDeleteIgnored { .. }
    )
}

impl SmbVolume {
    /// Asks the share whether it has named streams, once at connect. One round trip.
    /// A failed answer keeps the optimistic default: a write then says what's true.
    pub(super) async fn learn_xattr_support(&self) {
        let Ok(SessionLease { tree, mut conn, .. }) = self.clone_session().await else {
            return;
        };
        match tree.fs_attributes(&mut conn).await {
            Ok(attributes) => self
                .inner
                .stores_xattrs
                .store(attributes.supports_named_streams, Ordering::Relaxed),
            Err(e) => debug!("smb: couldn't ask whether the share has named streams: {e}"),
        }
    }

    /// One pipelined batch on the main session (smb2's window keeps it fair to what
    /// else is on that connection), then a serial retry for whatever sits behind a
    /// DFS link. Every failure goes through the funnel with its lease's generation.
    pub(super) async fn read_xattr_many_impl(
        &self,
        paths: &[PathBuf],
        name: &str,
        mode: XattrReadMode,
    ) -> Vec<XattrAnswer> {
        let converted: Vec<Result<String, VolumeError>> = paths.iter().map(|path| self.to_smb_path(path)).collect();
        let SessionLease {
            tree,
            mut conn,
            generation,
        } = match self.clone_session().await {
            Ok(lease) => lease,
            Err(e) => return paths.iter().map(|_| Err(e.clone())).collect(),
        };
        let wire: Vec<&str> = converted.iter().filter_map(|path| path.as_deref().ok()).collect();
        let options = smb2::BatchOptions::new().assume_paths_exist(mode == XattrReadMode::AssumePathsExist);
        let mut answers = tree.get_xattr_many(&mut conn, &wire, name, options).await.into_iter();

        let mut results = Vec::with_capacity(paths.len());
        for path in converted {
            let answer = match path {
                Err(e) => Err(e),
                Ok(smb_path) => {
                    let answer = answers
                        .next()
                        .expect("smb2 answers each path it was given, exactly once");
                    let answer = match answer {
                        Err(e) if is_behind_dfs_link(&e) => {
                            let owned = name.to_string();
                            self.or_through_link(generation, &smb_path, std::future::ready(Err(e)), move |c, t, p| {
                                link_get_xattr(c, t, p, owned)
                            })
                            .await
                        }
                        answered => answered,
                    };
                    self.handle_smb_result("read_xattr_many", &smb_path, generation, answer)
                }
            };
            results.push(answer);
        }
        results
    }

    pub(super) async fn write_xattr_impl(&self, path: &Path, name: &str, value: &[u8]) -> Result<(), VolumeError> {
        let smb_path = self.to_smb_path(path)?;
        let SessionLease {
            tree,
            mut conn,
            generation,
        } = self.clone_session().await?;
        let (owned_name, owned_value) = (name.to_string(), value.to_vec());
        let answer = self
            .or_through_link(
                generation,
                &smb_path,
                tree.set_xattr(&mut conn, &smb_path, name, value),
                move |client, tree, path| {
                    Box::pin(async move { client.set_xattr(tree, path, &owned_name, &owned_value).await })
                },
            )
            .await;
        self.answer_for_the_store("write_xattr", &smb_path, generation, answer)
    }

    pub(super) async fn remove_xattr_impl(&self, path: &Path, name: &str) -> Result<(), VolumeError> {
        let smb_path = self.to_smb_path(path)?;
        let SessionLease {
            tree,
            mut conn,
            generation,
        } = self.clone_session().await?;
        let owned_name = name.to_string();
        let answer = self
            .or_through_link(
                generation,
                &smb_path,
                tree.remove_xattr(&mut conn, &smb_path, name),
                move |client, tree, path| Box::pin(async move { client.remove_xattr(tree, path, &owned_name).await }),
            )
            .await;
        self.answer_for_the_store("remove_xattr", &smb_path, generation, answer)
    }

    /// The funnel, with a store's refusal as the typed answer the trait promises.
    fn answer_for_the_store(
        &self,
        op: &str,
        smb_path: &str,
        generation: super::state::SessionGeneration,
        answer: Result<(), smb2::Error>,
    ) -> Result<(), VolumeError> {
        match answer {
            Err(e) if is_refused_by_the_store(&e) => Err(VolumeError::NotSupported),
            other => self.handle_smb_result(op, smb_path, generation, other),
        }
    }
}
