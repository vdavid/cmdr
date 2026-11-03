//! What a streamed write stamps on what it wrote beyond the bytes (the dates
//! and Hidden, in one `FILE_BASIC_INFORMATION`), and what it reports of that.

use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{CarriedMetadata, ClassOutcome, MetadataReport, UserFlags};
use cmdr_fs::volume::{StreamWritten, VolumeError};
use log::warn;

use super::SmbVolume;
use super::mapping::attributes_of;
use super::state::{RanOn, ends_the_session};

/// What a copy stamps on what it wrote, in one `FILE_BASIC_INFORMATION`: the
/// source's `LastWriteTime` and `CreationTime`, and `HIDDEN` when the source is
/// Hidden, so neither costs a frame of its own. The server keeps its own access
/// time. `None` when the source has none of them, which leaves the server's.
///
/// Only Hidden rides here ([`SmbVolume::METADATA_CAPS`]'s `staged_flags`):
/// this lands on the staged temp, and Read-only waits for the landing. Sending
/// attributes REPLACES the settable set, so a Hidden copy doesn't wear the
/// `ARCHIVE` a fresh file gets, which nothing on a Mac reads.
pub(super) fn source_times(
    modified: Option<std::time::SystemTime>,
    created: Option<std::time::SystemTime>,
    flags: UserFlags,
) -> Option<smb2::FileTimes> {
    let staged = flags.intersection(SmbVolume::METADATA_CAPS.staged_flags);
    if modified.is_none() && created.is_none() && staged.is_empty() {
        return None;
    }
    let mut times = smb2::FileTimes::new();
    if let Some(modified) = modified {
        times = times.set_modified(modified);
    }
    if let Some(created) = created {
        times = times.set_created(created);
    }
    if !staged.is_empty() {
        times = times.set_attributes(attributes_of(staged));
    }
    Some(times)
}

impl SmbVolume {
    /// Stamps the source's date on a streaming writer's OWN handle, before
    /// `finish()`: the server rewrites the date when a writing handle closes, so
    /// a path stamp while this one is open would lose to that close. What the
    /// answer means: [`stamp_taken`](Self::stamp_taken).
    pub(super) async fn stamp_writer(
        &self,
        writer: &mut smb2::client::stream::FileWriter,
        times: smb2::FileTimes,
        smb_path: &str,
        ran_on: impl Into<RanOn>,
    ) -> Result<bool, VolumeError> {
        let result = writer.set_times(times).await;
        self.stamp_taken(smb_path, ran_on, result)
    }

    /// What a write makes of its date stamp's answer: `Ok(true)` the server
    /// took it, `Ok(false)` it refused, so the file keeps the server's dates and
    /// the copy goes on (best effort), and `Err` when the stamp met the session
    /// ending ([`ends_the_session`]), through the error funnel.
    ///
    /// ❗ A write never reports `Ok` past its own last step seeing the
    /// connection die: the transfer would land the file or start the next one on
    /// a dead session, where its outage wait never looks. Failing after the
    /// bytes landed is safe: a staged temp is cleared and the file runs again,
    /// and a single-shot create settles by comparing bytes
    /// (`settle_unanswered_create`).
    pub(super) fn stamp_taken(
        &self,
        smb_path: &str,
        ran_on: impl Into<RanOn>,
        result: Result<(), smb2::Error>,
    ) -> Result<bool, VolumeError> {
        match result {
            Ok(()) => Ok(true),
            Err(e) if ends_the_session(&e) => self
                .handle_smb_result("write_from_stream(stamp)", smb_path, ran_on, Err(e))
                .map(|()| true),
            Err(e) => {
                warn!(
                    "SmbVolume::write_from_stream: path={:?} keeps the server's date, not the source's: {e}",
                    LogPrivate::file_path(smb_path)
                );
                Ok(false)
            }
        }
    }
}

/// What a write of `bytes` reports: the creation date and the staged flags
/// rode `times`, so the server took both or refused both; the xattrs went as
/// streams of their own (`xattrs`, `Absent` for none).
pub(super) fn stamped(
    bytes: u64,
    carried: &CarriedMetadata,
    modified: Option<std::time::SystemTime>,
    refused: bool,
    xattrs: ClassOutcome,
) -> StreamWritten {
    let stamped = CarriedMetadata {
        created: carried.created,
        flags: carried.flags.intersection(SmbVolume::METADATA_CAPS.staged_flags),
        xattrs: carried.xattrs.clone(),
        ..CarriedMetadata::default()
    };
    let outcome = if refused {
        ClassOutcome::Refused
    } else {
        ClassOutcome::Kept
    };
    let report = MetadataReport {
        xattrs,
        ..MetadataReport::uniform(&stamped, modified, outcome)
    };
    StreamWritten::with_report(bytes, &report, &stamped, modified)
}

#[cfg(test)]
#[path = "stamping_test.rs"]
mod stamping_test;
