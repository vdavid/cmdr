//! Server-side copy inside one account: no byte travels through the Mac.
//!
//! - **Up to the part floor (64 MiB), one `CopyObject`**; past it, a multipart
//!   upload of `UploadPartCopy` ranges, even under S3's 5 GB `CopyObject`
//!   ceiling, so progress moves per part and a pause lands between parts. One
//!   part size per copy (`multipart.rs`, R2's rule).
//! - **Up to the profile's `copy_concurrency()` parts in flight**, halved on every throttle
//!   (`SlowDown`, 503, 429) and grown back by one per finished part ([`Window`]).
//! - ❗ **The source is HEADed once**, which gives its size, its ETag (every
//!   part is pinned to that version with `x-amz-copy-source-if-match`), and its
//!   metadata. A copy keeps the source's `x-amz-meta-mtime`; a source without
//!   one has its `Last-Modified` written as the mtime, so the date survives the
//!   copy either way.
//! - ❗ **Parse every 200**: `CopyObject` and `UploadPartCopy` can fail inside
//!   one.
//! - **No-overwrite** follows the allowlist (`profile.rs`): R2's
//!   `cf-copy-destination-if-none-match` on `CopyObject`, else a HEAD first;
//!   VersityGW and Garage ignore `If-None-Match` on a copy. A multipart copy
//!   refuses at its completion, the way an upload does.
//! - **Cancel aborts the multipart upload**, recorded in the ledger before its
//!   first part and confirmed gone by listing (`multipart_upload.rs`).

use std::collections::VecDeque;
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use cmdr_fs::volume::{ServerCopyProgress, Volume, VolumeError, WriteMode};
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use log::{debug, warn};

use super::S3Volume;
use super::errors::map_s3_error;
use super::multipart_upload::{abort_upload, retry_after, upload_refusal};
use super::paths::{Target, target_of};
use super::query::{body_error, stored_mtime};
use super::writes::{WriteTarget, normalize_etag, overwrite_for, refuse_unstorable};
use crate::error::{S3Error, S3ErrorCode};
use crate::metadata::{MTIME_HEADER, WRITE_TOKEN_HEADER};
use crate::multipart::{MAX_COPY_OBJECT_SIZE, PartPlan, TooLarge, plan_parts_with_floor};
use crate::ops::{self, BuildError, CopySource, MetadataDirective, ObjectMetadata};
use crate::profile::ConditionalOp;
use crate::transport::{Answer, COMPLETE_BUDGET, S3Client, map_exchange_error};
use crate::xml::build::CompletedPart;
use crate::xml::parse_copy_result;

/// The system headers a copy restates when it can't keep them by `COPY`.
const CARRIED_SYSTEM_HEADERS: [&str; 5] = [
    "content-type",
    "cache-control",
    "content-disposition",
    "content-encoding",
    "content-language",
];

/// What one HEAD said about a copy's source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SourceObject {
    pub size: u64,
    pub etag: Option<String>,
    /// Whether it carries its own `x-amz-meta-mtime`.
    pub has_mtime: bool,
    /// Its date: `x-amz-meta-mtime` when it parses, else `Last-Modified`.
    pub mtime: Option<SystemTime>,
    /// The headers a copy that restates metadata carries over.
    pub carried: Vec<(String, String)>,
}

impl SourceObject {
    /// Reads a source's HEAD.
    pub(super) fn from_head(head: &Answer) -> Self {
        let mut carried = Vec::new();
        for (name, value) in &head.headers {
            let name = name.as_str();
            let keep = CARRIED_SYSTEM_HEADERS.contains(&name)
                || (name.starts_with("x-amz-meta-") && name != MTIME_HEADER && name != WRITE_TOKEN_HEADER);
            if keep && let Ok(text) = value.to_str() {
                carried.push((name.to_string(), text.to_string()));
            }
        }
        Self {
            size: head.object_length().unwrap_or(0),
            etag: head.header("etag").map(str::to_string),
            has_mtime: head.header(MTIME_HEADER).is_some(),
            mtime: stored_mtime(head.header(MTIME_HEADER), head.header("last-modified")),
            carried,
        }
    }

    /// The metadata every server-side copy writes: the source's date (its
    /// `x-amz-meta-mtime`, else its `Last-Modified`), its content headers, and
    /// its other user metadata, never another write's token. A one-request
    /// copy sends it as `REPLACE`, a multipart copy at `CreateMultipartUpload`,
    /// each beside a token of its own.
    pub(super) fn restated(&self) -> ObjectMetadata {
        ObjectMetadata {
            mtime: self.mtime,
            write_token: None,
            carried: self.carried.clone(),
        }
    }
}

/// How many parts may be in flight: AIMD, halved on a throttle, one more per
/// part that lands, never below one or above the ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Window {
    pub width: usize,
    ceiling: usize,
}

impl Window {
    pub(super) fn new(ceiling: usize) -> Self {
        Self {
            width: ceiling.max(1),
            ceiling: ceiling.max(1),
        }
    }

    pub(super) fn landed(&mut self) {
        self.width = (self.width + 1).min(self.ceiling);
    }

    pub(super) fn throttled(&mut self) {
        self.width = (self.width / 2).max(1);
    }
}

/// One part's copy in flight.
type PartFuture = std::pin::Pin<Box<dyn Future<Output = Result<(CompletedPart, u64), PartFailure>> + Send>>;

/// How one part's copy failed, for the loop that decides what's next.
struct PartFailure {
    number: u32,
    attempt: u32,
    error: VolumeError,
    /// The server asked us to slow down: halve the window.
    throttle: bool,
    /// Worth sending again after a back-off.
    retryable: bool,
}

/// One part's copy, owning what it needs so it can run beside the others.
struct PartCopy {
    client: Arc<S3Client>,
    bucket: String,
    key: String,
    upload_id: String,
    number: u32,
    attempt: u32,
    source_bucket: String,
    source_key: String,
    range: (u64, u64),
    source_etag: Option<String>,
    /// The source's path, what a `SourceChanged` names.
    source_remote: String,
    remote: String,
    volume_id: String,
    /// Set when the server says the upload is gone (`NoSuchUpload`).
    gone: Arc<AtomicBool>,
    /// A back-off before sending, for a part sent again.
    delay: Option<Duration>,
}

impl PartCopy {
    async fn send(self) -> Result<(CompletedPart, u64), PartFailure> {
        if let Some(delay) = self.delay {
            tokio::time::sleep(delay).await;
        }
        let length = self.range.1 - self.range.0 + 1;
        let fail = |error: VolumeError, throttle: bool, retryable: bool| PartFailure {
            number: self.number,
            attempt: self.attempt,
            error,
            throttle,
            retryable,
        };
        let source = CopySource {
            bucket: &self.source_bucket,
            key: &self.source_key,
        };
        let request = ops::upload_part_copy(
            self.client.profile(),
            &self.bucket,
            &self.key,
            &self.upload_id,
            self.number,
            source,
            self.range,
            self.source_etag.as_deref(),
        )
        .map_err(|_| fail(VolumeError::NotFound(self.remote.clone()), false, false))?;
        let answer = match self.client.exchange(request, COMPLETE_BUDGET).await {
            Ok(answer) => answer,
            Err(e) => {
                return Err(fail(map_exchange_error(&e, &self.volume_id, &self.remote), false, true));
            }
        };
        if !answer.status.is_success() {
            let error = S3Error::from_response(answer.status, &answer.text());
            return Err(fail(self.refusal(&error), error.is_throttle(), error.is_retryable()));
        }
        // ❗ A part copy can fail inside a 200.
        match parse_copy_result(&answer.text()) {
            Ok(copied) => {
                let etag = copied.etag.ok_or_else(|| {
                    fail(
                        VolumeError::IoError {
                            message: format!("{}: part {} answered without an ETag", self.remote, self.number),
                            raw_os_error: None,
                        },
                        false,
                        false,
                    )
                })?;
                Ok((
                    CompletedPart {
                        number: self.number,
                        etag,
                    },
                    length,
                ))
            }
            Err(crate::xml::BodyError::Embedded(error)) => {
                Err(fail(self.refusal(&error), error.is_throttle(), error.is_retryable()))
            }
            Err(other) => Err(fail(body_error(&other, &self.remote), false, false)),
        }
    }

    fn refusal(&self, error: &S3Error) -> VolumeError {
        part_refusal(
            error,
            self.source_etag.is_some(),
            &self.source_remote,
            &self.remote,
            &self.gone,
        )
    }
}

/// A refused part copy in the `Volume` vocabulary. ❗ A failed precondition is
/// the source's ETag pin (the only precondition a part copy carries): the
/// source changed since its HEAD, ❌ never the destination being taken. So is
/// `InvalidRange`: every range comes from that HEAD's size, so the source
/// shrank, and on a provider that ignores the pin (Spaces, Hetzner) that's how
/// a smaller replacement shows.
pub(super) fn part_refusal(
    error: &S3Error,
    pinned: bool,
    source_remote: &str,
    remote: &str,
    gone: &AtomicBool,
) -> VolumeError {
    if (pinned && error.is_precondition_failed()) || error.code == S3ErrorCode::InvalidRange {
        return VolumeError::SourceChanged(source_remote.to_string());
    }
    upload_refusal(error, remote, gone)
}

/// Where a copy reads from, resolved to a bucket, a key, and what its HEAD said.
pub(super) struct CopyFrom<'a> {
    pub bucket: &'a str,
    pub key: &'a str,
    /// The source's server-side path, what an error about it names.
    pub remote: &'a str,
    pub object: &'a SourceObject,
}

impl S3Volume {
    /// The engine's server-side copy: `from` on `source` to `to` here, when
    /// `source` is a place of this same account and the provider can copy
    /// between the two buckets. `NotSupported` sends the engine to streaming.
    pub(super) async fn copy_on_server_impl(
        &self,
        source: &dyn Volume,
        from: &Path,
        to: &Path,
        mode: WriteMode,
        progress: &dyn ServerCopyProgress,
    ) -> Result<u64, VolumeError> {
        let Some(peer) = source.as_any().downcast_ref::<S3Volume>() else {
            return Err(VolumeError::NotSupported);
        };
        // ❗ One account (endpoint and key id): a copy names its source by
        // bucket and key, which mean something else on another account.
        if peer.inner.account() != self.inner.account() {
            return Err(VolumeError::NotSupported);
        }
        let from_remote = peer.to_remote_path(from)?;
        let to_remote = self.to_remote_path(to)?;
        let (
            Target::Key {
                bucket: from_bucket,
                key: from_key,
            },
            Target::Key {
                bucket: to_bucket,
                key: to_key,
            },
        ) = (target_of(&from_remote), target_of(&to_remote))
        else {
            return Err(VolumeError::IsADirectory(from_remote));
        };
        let client = self.clone_client().await?;
        refuse_unstorable(&client, to_key, &to_remote)?;
        if from_bucket != to_bucket && !client.profile().cross_bucket_copy() {
            // A provider that copies within one bucket only: stream it instead.
            return Err(VolumeError::NotSupported);
        }
        let head = self
            .head_object(&client, from_bucket, from_key, &from_remote)
            .await?
            .ok_or_else(|| VolumeError::NotFound(from_remote.clone()))?;
        let object = SourceObject::from_head(&head);
        let copy_from = CopyFrom {
            bucket: from_bucket,
            key: from_key,
            remote: &from_remote,
            object: &object,
        };
        self.copy_key(&client, &copy_from, to_bucket, to_key, &to_remote, mode, progress)
            .await
    }

    /// Copies one object to `to_key` in `to_bucket`, whole or in parts by
    /// size, refusing an occupied key under `CreateNew`. Verified by a HEAD,
    /// like every write. Returns the bytes copied.
    #[allow(
        clippy::too_many_arguments,
        reason = "one copy's whole context: the client, the source, the destination's bucket, key, and path, the mode, and the progress hook"
    )]
    pub(super) async fn copy_key(
        &self,
        client: &Arc<S3Client>,
        from: &CopyFrom<'_>,
        to_bucket: &str,
        to_key: &str,
        to_remote: &str,
        mode: WriteMode,
        progress: &dyn ServerCopyProgress,
    ) -> Result<u64, VolumeError> {
        let size = from.object.size;
        let in_parts = client.profile().copies_in_parts;
        if !in_parts && size > MAX_COPY_OBJECT_SIZE {
            // No `UploadPartCopy` (GCS), and too big for one `CopyObject`:
            // the engine streams it.
            return Err(VolumeError::NotSupported);
        }
        if self.copies_whole(size) || !in_parts {
            return self
                .copy_whole(client, from, to_bucket, to_key, to_remote, mode, progress)
                .await;
        }
        let plan = plan_parts_with_floor(size, self.part_floor(), client.profile().short_tail).map_err(|TooLarge| {
            VolumeError::IoError {
                message: format!("{to_remote}: too big for one S3 object (10,000 parts of 5 GiB)"),
                raw_os_error: None,
            }
        })?;
        // This copy's own token rides in the creation metadata, so a completion
        // whose answer is lost can still prove the destination is ours and whole
        // (`landed_whole`). The source's token is never carried (`from_head`).
        let metadata = ObjectMetadata {
            write_token: Some(crate::metadata::write_token()),
            ..from.object.restated()
        };
        let target = WriteTarget {
            bucket: to_bucket,
            key: to_key,
            remote: to_remote,
            mode,
            metadata: &metadata,
        };
        self.copy_in_parts(client, from, &target, plan, progress).await
    }

    /// Whether an object of `size` copies in one `CopyObject`: up to the part
    /// floor. Past it, a copy runs in parts (with progress and pause), and a
    /// rename of it is the transfer engine's job (`RenameWork::CopyThenDelete`).
    pub(super) fn copies_whole(&self, size: u64) -> bool {
        size <= self.part_floor()
    }

    /// One `CopyObject`.
    #[allow(
        clippy::too_many_arguments,
        reason = "one copy's whole context, as `copy_key` carries it"
    )]
    async fn copy_whole(
        &self,
        client: &S3Client,
        from: &CopyFrom<'_>,
        to_bucket: &str,
        to_key: &str,
        to_remote: &str,
        mode: WriteMode,
        progress: &dyn ServerCopyProgress,
    ) -> Result<u64, VolumeError> {
        let size = from.object.size;
        // ❗ Always `REPLACE`, restating the source's date and headers beside
        // this copy's own token, so a copy whose answer is lost can prove the
        // destination is ours and whole (`landed_whole`) instead of reporting a
        // failure the engine would then stream over, and refuse, its own copy
        // (live, Hetzner, 2026-10-02). Same one request as a `COPY`.
        let metadata = ObjectMetadata {
            write_token: Some(crate::metadata::write_token()),
            ..from.object.restated()
        };
        let directive = MetadataDirective::Replace(metadata.clone());
        let source = CopySource {
            bucket: from.bucket,
            key: from.key,
        };
        let target = WriteTarget {
            bucket: to_bucket,
            key: to_key,
            remote: to_remote,
            mode,
            metadata: &metadata,
        };
        let mut retried_without_header = false;
        loop {
            let built = match ops::copy_object(
                client.profile(),
                source,
                from.object.etag.as_deref(),
                to_bucket,
                to_key,
                overwrite_for(mode),
                &directive,
            ) {
                Ok(built) => built,
                Err(BuildError::CrossBucketCopy) => return Err(VolumeError::NotSupported),
                Err(_) => return Err(VolumeError::NotFound(to_remote.to_string())),
            };
            let conditional = mode.refuses_occupied() && !built.check_first;
            if built.check_first {
                self.refuse_if_taken(client, &target).await?;
            }
            // The copy is what publishes: a pause waits here, a Cancel stops it.
            if progress.checkpoint().await.is_break() || progress.advanced(0, size).is_break() {
                return Err(VolumeError::Cancelled(self.volume_id().to_string()));
            }
            let answer = match client.exchange(built.request, COMPLETE_BUDGET).await {
                Ok(answer) => answer,
                Err(e) => {
                    let failure = map_exchange_error(&e, self.volume_id(), to_remote);
                    return self
                        .landed_after_all(client, &target, size, progress)
                        .await
                        .ok_or(failure);
                }
            };
            if !answer.status.is_success() {
                let error = S3Error::from_response(answer.status, &answer.text());
                if conditional && error.is_not_implemented() && !retried_without_header {
                    client.profile().downgrade(ConditionalOp::Copy);
                    retried_without_header = true;
                    continue;
                }
                if from.object.etag.is_some()
                    && error.is_precondition_failed()
                    && self.source_moved_on(client, from, conditional).await
                {
                    warn!(target: "volume", "s3: {} changed since its HEAD; nothing was copied", from.remote);
                    return Err(VolumeError::SourceChanged(from.remote.to_string()));
                }
                let failure = map_s3_error(&error, to_remote);
                // A server fault may come after the copy applied (S3 says a
                // 500 can mean either).
                if error.is_retryable() {
                    return self
                        .landed_after_all(client, &target, size, progress)
                        .await
                        .ok_or(failure);
                }
                return Err(failure);
            }
            // ❗ A copy can fail inside a 200.
            let copied = parse_copy_result(&answer.text()).map_err(|e| body_error(&e, to_remote))?;
            let _ = progress.advanced(size, size);
            // ❗ Not under `CreateNewInFreshFolder`: the folder's creation proved
            // it empty moments ago, which is all this HEAD could add (another
            // writer at the name). Same accepted window as the skipped
            // no-overwrite HEAD (`DETAILS.md` § "No-overwrite writes").
            if mode != WriteMode::CreateNewInFreshFolder {
                self.verify_landing(client, &target, size, copied.etag.as_deref())
                    .await?;
            }
            debug!(target: "volume", "s3 copied {} bytes to {to_remote} in one request", size);
            return Ok(size);
        }
    }

    /// A `412` to a pinned copy names either the source pin or the
    /// no-overwrite condition (R2's `cf-copy-destination-if-none-match`, GCS's
    /// generation match). Without the latter it's the pin; with both, one HEAD
    /// of the source says which: a source no longer at its pinned ETag moved on.
    async fn source_moved_on(&self, client: &S3Client, from: &CopyFrom<'_>, conditional: bool) -> bool {
        if !conditional {
            return true;
        }
        match self.head_object(client, from.bucket, from.key, from.remote).await {
            Ok(Some(head)) => match (head.header("etag"), from.object.etag.as_deref()) {
                (Some(now), Some(then)) => normalize_etag(now) != normalize_etag(then),
                _ => false,
            },
            Ok(None) => true,
            Err(_) => false,
        }
    }

    /// After a `CopyObject` whose answer never came (or came as a fault): our
    /// whole copy at the key, carrying this copy's token, means the server
    /// applied it, so it's reported as copied. One HEAD, ❌ never a delete.
    async fn landed_after_all(
        &self,
        client: &S3Client,
        target: &WriteTarget<'_>,
        size: u64,
        progress: &dyn ServerCopyProgress,
    ) -> Option<u64> {
        let head = self.landed_whole(client, target, size).await?;
        warn!(
            target: "volume",
            "s3: the answer to a copy into {} never came, but the server applied it",
            target.remote
        );
        self.remember_written(target, &head);
        let _ = progress.advanced(size, size);
        Some(size)
    }

    /// A multipart upload of `UploadPartCopy` ranges, recorded in the ledger
    /// before its first part and aborted on any failure or cancel.
    async fn copy_in_parts(
        &self,
        client: &Arc<S3Client>,
        from: &CopyFrom<'_>,
        target: &WriteTarget<'_>,
        plan: PartPlan,
        progress: &dyn ServerCopyProgress,
    ) -> Result<u64, VolumeError> {
        if target.mode.refuses_occupied()
            && client.profile().no_overwrite(ConditionalOp::CompleteMultipart)
                == crate::profile::NoOverwrite::CheckThenWrite
        {
            self.refuse_if_taken(client, target).await?;
        }
        if progress.checkpoint().await.is_break() {
            return Err(VolumeError::Cancelled(self.volume_id().to_string()));
        }
        let (upload_id, mut guard) = self.start_recorded_upload(client, target).await?;
        let gone = Arc::new(AtomicBool::new(false));
        let copied = self
            .copy_parts(client, from, target, &upload_id, plan, progress, &gone)
            .await;
        // ❗ The completion publishes, so a Cancel that came in while the last
        // part was in flight is honoured before it.
        let copied = copied.and_then(|parts| match progress.advanced(plan.total, plan.total) {
            ControlFlow::Break(()) => Err(VolumeError::Cancelled(self.volume_id().to_string())),
            ControlFlow::Continue(()) => Ok(parts),
        });
        let copied = match copied {
            Ok(parts) => self.source_unchanged(client, from).await.map(|()| parts),
            Err(e) => Err(e),
        };
        let outcome = match copied {
            Ok(parts) => match self.complete(client, target, &upload_id, &parts, &gone).await {
                Ok(etag) => Ok(etag),
                // ❗ The link can die after the server completed: our whole copy
                // is at the key and the upload is gone, so the copy landed.
                Err(e) => match self.landed_whole(client, target, plan.total).await {
                    Some(head) => {
                        self.inner.ledger.finished(&guard.upload);
                        guard.settled = true;
                        let _ = progress.advanced(plan.total, plan.total);
                        warn!(
                            target: "volume",
                            "s3: the answer to a copy into {} never came, but the server completed it",
                            target.remote
                        );
                        self.remember_written(target, &head);
                        return Ok(plan.total);
                    }
                    None => Err(e),
                },
            },
            Err(e) => Err(e),
        };
        match outcome {
            Ok(etag) => {
                self.inner.ledger.finished(&guard.upload);
                guard.settled = true;
                self.verify_landing(client, target, plan.total, etag.as_deref()).await?;
                debug!(
                    target: "volume",
                    "s3 copied {} bytes to {} in {} parts",
                    plan.total, target.remote, plan.part_count
                );
                Ok(plan.total)
            }
            Err(e) => {
                abort_upload(client, &self.inner.ledger, &guard.upload).await;
                guard.settled = true;
                if gone.load(Ordering::Relaxed)
                    && target.mode.refuses_occupied()
                    && matches!(
                        self.head_object(client, target.bucket, target.key, target.remote).await,
                        Ok(Some(_))
                    )
                {
                    return Err(VolumeError::AlreadyExists(target.remote.to_string()));
                }
                Err(e)
            }
        }
    }

    /// ❗ Where the provider ignores the parts' ETag pin (Hetzner, Spaces), a
    /// source replaced mid-copy would be stitched from two versions, and a
    /// move would then delete the new source. So right before the completion
    /// that publishes, one HEAD asks whether the source is still the version
    /// the copy started from; anything else is `SourceChanged`, and the caller
    /// aborts. One request per multipart copy, none where the pin holds. A
    /// replacement after this HEAD and before the completion stays blind.
    async fn source_unchanged(&self, client: &S3Client, from: &CopyFrom<'_>) -> Result<(), VolumeError> {
        if client.profile().enforces_copy_source_pin {
            return Ok(());
        }
        let now = self.head_object(client, from.bucket, from.key, from.remote).await?;
        let unchanged = match (&now, from.object.etag.as_deref()) {
            (None, _) => false,
            // A source that answered no ETag at the start has nothing to compare.
            (Some(_), None) => true,
            (Some(head), Some(then)) => head.header("etag").map(normalize_etag) == Some(normalize_etag(then)),
        };
        if unchanged {
            return Ok(());
        }
        warn!(target: "volume", "s3: {} changed during a copy; nothing was published", from.remote);
        Err(VolumeError::SourceChanged(from.remote.to_string()))
    }

    /// Copies every part, up to the window's width at once, asking the
    /// progress hook's checkpoint before starting each one. Answers the parts
    /// in order. ❗ Leaves the upload for the caller to complete or abort.
    #[allow(
        clippy::too_many_arguments,
        reason = "one copy's whole context: the client, the source, the target, the upload id, the plan, progress, and the upload-gone flag"
    )]
    async fn copy_parts(
        &self,
        client: &Arc<S3Client>,
        from: &CopyFrom<'_>,
        target: &WriteTarget<'_>,
        upload_id: &str,
        plan: PartPlan,
        progress: &dyn ServerCopyProgress,
        gone: &Arc<AtomicBool>,
    ) -> Result<Vec<CompletedPart>, VolumeError> {
        let part = |number: u32, attempt: u32, delay: Option<Duration>| PartCopy {
            client: Arc::clone(client),
            bucket: target.bucket.to_string(),
            key: target.key.to_string(),
            upload_id: upload_id.to_string(),
            number,
            attempt,
            source_bucket: from.bucket.to_string(),
            source_key: from.key.to_string(),
            range: plan.range(number),
            source_etag: from.object.etag.clone(),
            source_remote: from.remote.to_string(),
            remote: target.remote.to_string(),
            volume_id: self.volume_id().to_string(),
            gone: Arc::clone(gone),
            delay,
        };
        let mut waiting: VecDeque<u32> = (1..=plan.part_count).collect();
        let mut in_flight: FuturesUnordered<PartFuture> = FuturesUnordered::new();
        let volume_id = self.volume_id();
        let mut window = Window::new(client.profile().copy_concurrency());
        let mut parts: Vec<CompletedPart> = Vec::with_capacity(plan.part_count as usize);
        let mut done_bytes = 0u64;
        loop {
            while in_flight.len() < window.width
                && let Some(&number) = waiting.front()
            {
                // ❗ A pause lands here, between parts, while the parts already
                // in flight finish: their requests keep being driven.
                let flow = {
                    let gate = progress.checkpoint();
                    tokio::pin!(gate);
                    loop {
                        tokio::select! {
                            biased;
                            flow = &mut gate => break flow,
                            Some(finished) = in_flight.next(), if !in_flight.is_empty() => {
                                let mut copy = CopyState { parts: &mut parts, done_bytes: &mut done_bytes, window: &mut window, in_flight: &mut in_flight };
                                settle_part(finished, &mut copy, &part, plan.total, progress, volume_id)?;
                            }
                        }
                    }
                };
                if flow.is_break() {
                    return Err(VolumeError::Cancelled(self.volume_id().to_string()));
                }
                waiting.pop_front();
                in_flight.push(Box::pin(part(number, 1, None).send()));
            }
            let Some(finished) = in_flight.next().await else {
                break;
            };
            let mut copy = CopyState {
                parts: &mut parts,
                done_bytes: &mut done_bytes,
                window: &mut window,
                in_flight: &mut in_flight,
            };
            settle_part(finished, &mut copy, &part, plan.total, progress, volume_id)?;
        }
        parts.sort_by_key(|part| part.number);
        Ok(parts)
    }
}

/// The copy loop's running state, as one settle step updates it.
struct CopyState<'a> {
    parts: &'a mut Vec<CompletedPart>,
    done_bytes: &'a mut u64,
    window: &'a mut Window,
    in_flight: &'a mut FuturesUnordered<PartFuture>,
}

/// Folds one finished part into the copy: a landed part moves the bar and
/// widens the window; a throttle halves it and sends the part again after a
/// back-off, as does any other retryable failure; anything else ends the copy.
fn settle_part(
    finished: Result<(CompletedPart, u64), PartFailure>,
    copy: &mut CopyState<'_>,
    part: &impl Fn(u32, u32, Option<Duration>) -> PartCopy,
    total: u64,
    progress: &dyn ServerCopyProgress,
    volume_id: &str,
) -> Result<(), VolumeError> {
    match finished {
        Ok((landed, length)) => {
            *copy.done_bytes += length;
            copy.parts.push(landed);
            copy.window.landed();
            if progress.advanced(*copy.done_bytes, total).is_break() {
                return Err(VolumeError::Cancelled(volume_id.to_string()));
            }
            Ok(())
        }
        Err(failure) if failure.retryable => {
            if failure.throttle {
                copy.window.throttled();
            }
            let Some(wait) = retry_after(failure.attempt) else {
                return Err(failure.error);
            };
            warn!(
                target: "volume",
                "s3: part {} of a server-side copy failed (attempt {}): {}; again in {wait:?}, {} in flight at most",
                failure.number, failure.attempt, failure.error, copy.window.width
            );
            copy.in_flight
                .push(Box::pin(part(failure.number, failure.attempt + 1, Some(wait)).send()));
            Ok(())
        }
        Err(failure) => Err(failure.error),
    }
}

#[cfg(test)]
#[path = "server_copy_test.rs"]
mod server_copy_test;
