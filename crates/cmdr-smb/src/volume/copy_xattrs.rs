//! A copy's xattrs (Finder tags, comments, FinderInfo, resource forks) to and
//! from a share, as the named streams macOS's own client stores them in.
//! `DETAILS.md` § "Xattrs on copies".
//!
//! Both halves ride frames the copy already sends wherever the protocol allows:
//! a read lists the file's streams in its own frame, and only a file that has
//! some costs a round trip more (their values, overlapped); a write puts them on
//! the staged temp, overlapped, before its dates and its landing rename.

use super::SmbVolume;
use super::session::SessionLease;
use super::state::{RanOn, ends_the_session};
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{CarriedMetadata, ClassOutcome, MetadataAsk, MetadataClasses, Xattr};
use cmdr_fs::volume::VolumeError;
use log::{debug, warn};
use smb2::StreamInfo;
use smb2::client::{Connection, Tree};
use std::ffi::OsString;
use std::path::Path;

/// The xattr macOS stamps on every file a process writes. `CarriedMetadata`
/// never carries it (a copy can't set it), so a read doesn't spend a READ on it.
const PROVENANCE: &str = "com.apple.provenance";

/// What a copy wants a read to learn of the file's xattrs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum XattrAsk {
    /// Nothing: no listing goes out. Every read that isn't a copy's, a copy whose
    /// pair never asks about xattrs, and a share without named streams.
    Nothing,
    /// Their names, for the loss note: the listing in the read's own frame, no
    /// value read.
    Names,
    /// Their values, for a destination that keeps them: the listing, then one
    /// overlapped read of whatever it named.
    Values,
}

impl XattrAsk {
    /// What `ask` needs of a share that does (`stores_xattrs`) or doesn't have
    /// named streams.
    pub(super) fn of(ask: MetadataAsk, stores_xattrs: bool) -> Self {
        if !stores_xattrs || !ask.mentions(MetadataClasses::XATTRS) {
            Self::Nothing
        } else if ask.values.contains(MetadataClasses::XATTRS) {
            Self::Values
        } else {
            Self::Names
        }
    }

    pub(super) fn lists(self) -> bool {
        self != Self::Nothing
    }
}

/// What a read learned of its file's xattrs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) enum SourceXattrs {
    /// Nothing asked, or nothing there.
    #[default]
    None,
    /// The whole xattrs, for `VolumeReadStream::metadata`.
    Values(Vec<Xattr>),
    /// Names with empty values, for `metadata_outline` only.
    Names(Vec<Xattr>),
}

impl SourceXattrs {
    /// The values `metadata` may hand out: none for a names-only answer, so an
    /// empty value is never written as a real one.
    pub(super) fn values(&self) -> Vec<Xattr> {
        match self {
            Self::Values(xattrs) => xattrs.clone(),
            Self::None | Self::Names(_) => Vec::new(),
        }
    }

    /// What `metadata_outline` reports: every xattr the read learned of.
    pub(super) fn outline(&self) -> Vec<Xattr> {
        match self {
            Self::Values(xattrs) | Self::Names(xattrs) => xattrs.clone(),
            Self::None => Vec::new(),
        }
    }
}

/// Whether a copy carries the stream `name` as an xattr: all but provenance.
fn carries_stream(name: &str) -> bool {
    smb2::stream_xattr_name(name) != PROVENANCE
}

/// The streams of `listed` a copy carries.
fn named(listed: &[StreamInfo]) -> Vec<StreamInfo> {
    listed
        .iter()
        .filter(|stream| carries_stream(&stream.name))
        .cloned()
        .collect()
}

/// What a read makes of the listing its open got back (`None`: not answered),
/// reading the values when `ask` wants them. A value that can't be read stays
/// behind with a `warn!`, and the copy goes on.
pub(super) async fn source_xattrs(
    tree: &Tree,
    conn: &mut Connection,
    smb_path: &str,
    listed: Option<&[StreamInfo]>,
    ask: XattrAsk,
) -> SourceXattrs {
    let Some(listed) = listed else {
        if ask.lists() {
            debug!(
                "smb: the read of path={:?} got no stream listing back; its xattrs stay unknown",
                LogPrivate::file_path(smb_path)
            );
        }
        return SourceXattrs::None;
    };
    let streams = named(listed);
    if streams.is_empty() {
        return SourceXattrs::None;
    }
    match ask {
        XattrAsk::Nothing => SourceXattrs::None,
        XattrAsk::Names => SourceXattrs::Names(
            streams
                .iter()
                .map(|stream| Xattr {
                    name: OsString::from(smb2::stream_xattr_name(&stream.name)),
                    value: Vec::new(),
                })
                .collect(),
        ),
        XattrAsk::Values => {
            let mut xattrs = Vec::with_capacity(streams.len());
            for (name, value) in tree.get_xattrs(conn, smb_path, &streams).await {
                match value {
                    Ok(value) => xattrs.push(Xattr {
                        name: OsString::from(name),
                        value,
                    }),
                    Err(e) => warn!(
                        // allowed-log-private: an xattr key (`com.apple.…`), the triage clue, not a file's name
                        "smb: a copy of path={:?} leaves its xattr {name:?} behind, it didn't read: {e}",
                        LogPrivate::file_path(smb_path)
                    ),
                }
            }
            SourceXattrs::Values(xattrs)
        }
    }
}

impl SmbVolume {
    /// Puts `xattrs` on the file or folder a copy has just made at `smb_path`,
    /// overlapped (one round trip for small ones). `Absent` with nothing to put
    /// on, `Kept` when the server took them all, `Refused` when it took fewer
    /// (a share without streams, a value past its limit, a name that isn't
    /// UTF-8, a path behind a DFS link): a reported loss, ❌ never a failed copy.
    /// Only the session ending under it fails the write, through the funnel,
    /// for the reason `stamp_taken` gives.
    ///
    /// ❗ Only on a path the copy itself just created: smb2's
    /// `set_xattrs_on_new_file` skips the existence check, and a stream write
    /// to a missing file creates it.
    pub(super) async fn put_copied_xattrs(
        &self,
        tree: &Tree,
        conn: &mut Connection,
        smb_path: &str,
        ran_on: impl Into<RanOn>,
        xattrs: &[Xattr],
        behind_a_link: bool,
    ) -> Result<ClassOutcome, VolumeError> {
        if xattrs.is_empty() {
            return Ok(ClassOutcome::Absent);
        }
        if behind_a_link || self.inner.link_paths.covers(smb_path) {
            warn!(
                "smb: path={:?} sits behind a DFS link, so a copy leaves its xattrs behind",
                LogPrivate::file_path(smb_path)
            );
            return Ok(ClassOutcome::Refused);
        }
        let named: Vec<(&str, &[u8])> = xattrs
            .iter()
            .filter_map(|xattr| xattr.name.to_str().map(|name| (name, xattr.value.as_slice())))
            .collect();
        for (name, _) in &named {
            self.inner.sidecars.note_written_xattr(name);
        }
        let mut outcome = match named.len() == xattrs.len() {
            true => ClassOutcome::Kept,
            false => ClassOutcome::Refused,
        };
        for (result, (name, _)) in tree
            .set_xattrs_on_new_file(conn, smb_path, &named)
            .await
            .into_iter()
            .zip(&named)
        {
            match result {
                Ok(()) => {}
                Err(e) if ends_the_session(&e) => {
                    return self
                        .handle_smb_result("write_from_stream(xattrs)", smb_path, ran_on, Err(e))
                        .map(|()| ClassOutcome::Refused);
                }
                Err(e) => {
                    warn!(
                        // allowed-log-private: an xattr key (`com.apple.…`), the triage clue, not a file's name
                        "smb: path={:?} doesn't keep its xattr {name:?}: {e}",
                        LogPrivate::file_path(smb_path)
                    );
                    outcome = ClassOutcome::Refused;
                }
            }
        }
        Ok(outcome)
    }
}

impl SmbVolume {
    /// A folder's xattrs, for a copy whose destination keeps them: a stream
    /// listing, then one overlapped read of what it names. A folder behind a
    /// DFS link, or one that can't be listed, reports none.
    pub(super) async fn folder_xattrs(&self, path: &Path) -> Vec<Xattr> {
        let read = async {
            let smb_path = self.to_smb_path(path)?;
            let SessionLease {
                tree,
                mut conn,
                generation,
            } = self.clone_session().await?;
            let listed = tree.list_streams(&mut conn, &smb_path).await;
            let listed = self.handle_smb_result("folder_metadata(streams)", &smb_path, generation, listed)?;
            let read = source_xattrs(&tree, &mut conn, &smb_path, Some(&listed), XattrAsk::Values).await;
            Ok::<_, VolumeError>(read.values())
        };
        read.await.unwrap_or_else(|e| {
            debug!(
                "SmbVolume::folder_metadata: {} reports no xattrs: {e}",
                LogPrivate::dir_path(path),
                e = LogError(&e)
            );
            Vec::new()
        })
    }

    /// `carried`'s xattrs on the existing item at `path`, each through smb2's
    /// `set_xattr`, which checks the item is there first (unlike a copy's
    /// fresh temp, `apply_metadata` may reach a path someone just deleted, and
    /// a stream write would create it). Overlapped, so a folder's few cost
    /// about two round trips together.
    pub(super) async fn apply_xattrs_by_path(&self, path: &Path, carried: &CarriedMetadata) -> ClassOutcome {
        let lease = async {
            let smb_path = self.to_smb_path(path)?;
            let lease = self.clone_session().await?;
            Ok::<_, VolumeError>((smb_path, lease))
        };
        let (smb_path, SessionLease { tree, conn, generation }) = match lease.await {
            Ok(lease) => lease,
            Err(e) => {
                warn!(
                    "SmbVolume::apply_metadata: {} keeps no xattrs: {e}",
                    LogPrivate::path(path),
                    e = LogError(&e)
                );
                return ClassOutcome::Refused;
            }
        };
        if self.inner.link_paths.covers(&smb_path) {
            return ClassOutcome::Refused;
        }
        for name in carried.xattrs.iter().filter_map(|xattr| xattr.name.to_str()) {
            self.inner.sidecars.note_written_xattr(name);
        }
        let writes = carried.xattrs.iter().map(|xattr| {
            let (tree, mut conn, smb_path) = (tree.clone(), conn.clone(), smb_path.clone());
            async move {
                let Some(name) = xattr.name.to_str() else {
                    return Ok(false);
                };
                tree.set_xattr(&mut conn, &smb_path, name, &xattr.value)
                    .await
                    .map(|()| true)
            }
        });
        let mut outcome = ClassOutcome::Kept;
        for written in futures_util::future::join_all(writes).await {
            match written {
                Ok(true) => {}
                Ok(false) => outcome = ClassOutcome::Refused,
                Err(e) => {
                    warn!(
                        "SmbVolume::apply_metadata: {} doesn't keep an xattr: {e}",
                        LogPrivate::path(path)
                    );
                    // Through the funnel, so a dead session still counts.
                    let _ = self.handle_smb_result("apply_metadata(xattrs)", &smb_path, generation, Err::<(), _>(e));
                    outcome = ClassOutcome::Refused;
                }
            }
        }
        outcome
    }
}

#[cfg(test)]
#[path = "copy_xattrs_test.rs"]
mod copy_xattrs_test;
