//! Hidden and Locked on a share: the DOS attributes `HIDDEN` and `READONLY` a
//! copy reads off a source and puts on what it lands, and what a reversal
//! clears to remove a Read-only copy of its own. `DETAILS.md` § "Hidden and
//! Locked on copies".
//!
//! A FILE's read and write carry them in frames they already send (the read's
//! CREATE response, the write's dates SET_INFO: `read_stream.rs`, `write_stream.rs`). What lives here
//! goes by path: a landed item's whole set with its dates, a folder's read,
//! and the unlock.

use super::SmbVolume;
use super::dfs_links::link_bare_stat;
use super::mapping::{attributes_of, user_flags_of};
use super::session::SessionLease;
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{
    CarriedMetadata, ClassOutcome, LandedMetadata, MetadataClasses, MetadataReport, Unblock, UserFlags,
};
use cmdr_fs::volume::VolumeError;
use log::{debug, warn};
use std::path::Path;

impl SmbVolume {
    /// The DOS attributes of the file or folder at `path`: one `stat` round
    /// trip. What a folder's flags and a reversal's unlock ask; a FILE a copy
    /// reads reports its own off the read's CREATE response instead.
    pub(super) async fn attributes_impl(&self, path: &Path) -> Result<smb2::FileAttributes, VolumeError> {
        let smb_path = self.to_smb_path(path)?;
        let SessionLease {
            tree,
            mut conn,
            generation,
        } = self.clone_session().await?;
        let r = self
            .or_through_link(generation, &smb_path, tree.stat(&mut conn, &smb_path), link_bare_stat)
            .await;
        Ok(self
            .handle_smb_result("attributes", &smb_path, generation, r)?
            .attributes)
    }

    /// The xattrs (as named streams, on a share that has them), then the
    /// creation and modification dates and Hidden and Locked in the one frame a
    /// date alone costs: the xattrs first, since a stream write moves the date
    /// on `streams_xattr`. The flags go as the whole attribute set (`HIDDEN`,
    /// `READONLY`), which REPLACES the item's settable ones; a set with neither
    /// (append-only, no-dump) has nothing to send and answers `NotApplicable`.
    /// A share has no mode.
    pub(super) async fn apply_metadata_impl(
        &self,
        path: &Path,
        carried: &CarriedMetadata,
        modified: Option<std::time::SystemTime>,
    ) -> MetadataReport {
        let mut report = MetadataReport::none_applicable(carried, modified);
        if !carried.xattrs.is_empty() && self.stores_xattrs() {
            report.xattrs = self.apply_xattrs_by_path(path, carried).await;
        }
        let attributes = Some(attributes_of(carried.flags)).filter(|attributes| attributes.bits() != 0);
        if carried.created.is_none() && modified.is_none() && attributes.is_none() {
            return report;
        }
        let mut times = smb2::FileTimes::new();
        if let Some(created) = carried.created {
            times = times.set_created(created);
        }
        if let Some(modified) = modified {
            times = times.set_modified(modified);
        }
        if let Some(attributes) = attributes {
            times = times.set_attributes(attributes);
        }
        let outcome = match self.set_basic_info_impl(path, times).await {
            Ok(()) => ClassOutcome::Kept,
            Err(e) => {
                warn!(
                    "SmbVolume::apply_metadata: {} keeps the server's dates and attributes, not the source's: {e}",
                    LogPrivate::path(path),
                    e = LogError(&e)
                );
                ClassOutcome::Refused
            }
        };
        if carried.created.is_some() {
            report.created = outcome;
        }
        if modified.is_some() {
            report.modified = outcome;
        }
        if attributes.is_some() {
            report.flags = outcome;
        }
        report
    }

    /// A folder's Hidden and Locked and its xattrs, each only when `wanted`:
    /// the flags in one `stat` round trip, since a listing entry has nowhere to
    /// carry them (the copy asks only when its destination keeps a folder's
    /// flags, `folder_metadata.rs` in the app), and the xattrs in a stream
    /// listing plus one overlapped read of what it names, on a share that has
    /// them.
    pub(super) async fn folder_metadata_impl(&self, path: &Path, wanted: MetadataClasses) -> CarriedMetadata {
        let xattrs = match wanted.contains(MetadataClasses::XATTRS) && self.stores_xattrs() {
            true => self.folder_xattrs(path).await,
            false => Vec::new(),
        };
        CarriedMetadata {
            xattrs,
            ..self.folder_flags(path, wanted).await
        }
    }

    async fn folder_flags(&self, path: &Path, wanted: MetadataClasses) -> CarriedMetadata {
        if !wanted.contains(MetadataClasses::FLAGS) {
            return CarriedMetadata::default();
        }
        match self.attributes_impl(path).await {
            Ok(attributes) => {
                let name = path.file_name().map(|name| name.to_string_lossy()).unwrap_or_default();
                CarriedMetadata {
                    flags: user_flags_of(attributes, &name),
                    ..CarriedMetadata::default()
                }
            }
            Err(e) => {
                debug!(
                    "SmbVolume::folder_metadata: {} reports no flags: {e}",
                    LogPrivate::dir_path(path),
                    e = LogError(&e)
                );
                CarriedMetadata::default()
            }
        }
    }

    /// Clears `READONLY`, the one attribute that refuses a removal
    /// (`STATUS_CANNOT_DELETE`), keeping the rest: a `stat` and a SET_INFO
    /// compound, spent only after a reversal's removal was refused.
    pub(super) async fn clear_blockers_impl(&self, path: &Path) -> Unblock {
        let attributes = match self.attributes_impl(path).await {
            Ok(attributes) if attributes.is_readonly() => attributes,
            Ok(_) => return Unblock::NothingToClear,
            Err(e) => {
                debug!(
                    "SmbVolume::clear_blockers: {} can't be read: {e}",
                    LogPrivate::path(path),
                    e = LogError(&e)
                );
                return Unblock::NothingToClear;
            }
        };
        let unlocked = smb2::FileAttributes::new(attributes.bits() & !smb2::FileAttributes::READONLY);
        match self
            .set_basic_info_impl(path, smb2::FileTimes::new().set_attributes(unlocked))
            .await
        {
            Ok(()) => Unblock::Cleared(LandedMetadata::default().with_flags(UserFlags::LOCKED)),
            Err(e) => {
                warn!(
                    "SmbVolume::clear_blockers: {} stays Read-only: {e}",
                    LogPrivate::path(path),
                    e = LogError(&e)
                );
                Unblock::Refused
            }
        }
    }
}
