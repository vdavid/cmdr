//! The byte path INTO the export: `write_from_stream` and `create_file`.
//!
//! ❗ The engine owns staging (`write_is_single_shot` keeps `false`), so a
//! `CreateOrReplace` write is one WRITE stream onto the temp it hands us, ❌
//! never staged a second time here.
//!
//! The rules that hold it up:
//!
//! - **A span at a time.** The source is taken in spans of [`COMMIT_EVERY`];
//!   each span's WRITEs go out `UNSTABLE`, [`WRITE_WINDOW_DEPTH`] in flight on
//!   the one connection, and the span ends in a COMMIT. The span stays in
//!   memory until its COMMIT is good, because it's what a resend needs.
//! - ❗ **The verifier decides what's on disk.** The reference is the first
//!   verifier this file's WRITEs answered (after a resend, the one the resent
//!   WRITEs answered). A WRITE or COMMIT answering another one means the
//!   server restarted and dropped what it hadn't committed, so the whole span
//!   is resent. A COMMIT is good only when its verifier is the one EVERY
//!   `UNSTABLE` WRITE it covers answered (RFC 1813 § 3.3.21); a span whose
//!   WRITEs all came back `FILE_SYNC` or `DATA_SYNC` is stable on its own and
//!   needs none. A verifier that changes twice in one file fails the write.
//! - ❗ **A short WRITE isn't done.** The rest is sent again from the offset the
//!   server answered, and a WRITE that takes nothing fails rather than spins.
//!   No WRITE carries more than the server's `wtmax`.
//! - **Cancel arrives only through `on_progress`'s `Break`**, reported per
//!   acknowledged chunk. Every failure, cancel included, removes the partial
//!   (`NfsVolume::remove_if_ours`).
//! - The date and mode go on after the last COMMIT (`copy.rs`), and `CREATE`
//!   carries the mode alone: RFC 1813 lets it carry owner, size, and times, but
//!   not every server applies them, and SETATTR after the last WRITE is the
//!   portable path anyway.

use std::ops::ControlFlow;
use std::path::Path;
use std::pin::Pin;

use cmdr_fs::error_detail::ErrorDetail;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{CarriedMetadata, MetadataClasses, MetadataReport, default_file_mode, routed_mode};
use cmdr_fs::os_error::OsErrorClass;
use cmdr_fs::pluralize::pluralize_grouped;
use cmdr_fs::volume::patching::patch_created;
use cmdr_fs::volume::{StreamLength, StreamWriteProgress, StreamWritten, VolumeError, VolumeReadStream, WriteMode};
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use log::debug;

use super::NfsVolume;
use super::handles::MutationDir;
use super::reconnect::NfsClient;
use crate::errors::map_nfs_error;
use crate::proto::nfs3::{CreateHow, FileHandle, FsInfo, Nfs3Client, Sattr3, Stable, WriteVerifier};
use crate::transport::MAX_PAYLOAD;

/// How many bytes of acknowledged WRITEs a COMMIT covers, and so how much of
/// the source stays in memory for a resend. Decision (revisit in review).
pub(super) const COMMIT_EVERY: usize = 8 * 1024 * 1024;

/// How many WRITEs one upload keeps in flight: the read window's depth, to be
/// re-measured against a NAS.
pub(super) const WRITE_WINDOW_DEPTH: usize = 8;

/// The mode a file wears while its bytes stream: owner only, so nobody else
/// reads a private file half-written. Its real mode goes on after the last
/// WRITE.
const STREAMING_MODE: u32 = 0o600;

/// How many verifier changes one file survives: the first is a resend, the
/// second fails the write.
const VERIFIER_CHANGES_TOLERATED: u32 = 1;

/// The bytes one WRITE carries: the server's preference, within its `wtmax`
/// and our own record cap. A server answering 0 for either gets 64 KiB.
pub(super) fn write_chunk_bytes(fsinfo: &FsInfo) -> usize {
    let preferred = if fsinfo.wtpref == 0 { 64 * 1024 } else { fsinfo.wtpref };
    let allowed = if fsinfo.wtmax == 0 { MAX_PAYLOAD } else { fsinfo.wtmax };
    preferred.clamp(4096, MAX_PAYLOAD).min(allowed).max(1) as usize
}

/// How durable each WRITE asks to be: `UNSTABLE` plus COMMIT, unless the
/// server's FSINFO says nothing useful about writes, where `FILE_SYNC` is the
/// safe answer.
fn write_stability(fsinfo: &FsInfo) -> Stable {
    if fsinfo.wtmax == 0 && fsinfo.wtpref == 0 {
        Stable::FileSync
    } else {
        Stable::Unstable
    }
}

/// One file being written: where, and how.
struct Upload<'a> {
    nfs: &'a Nfs3Client,
    handle: &'a FileHandle,
    remote: &'a str,
    chunk: usize,
    stable: Stable,
}

/// What a span's WRITEs said about durability.
enum SpanAnswer {
    /// Every WRITE was stable on its own: nothing to COMMIT.
    Stable,
    /// Every `UNSTABLE` WRITE answered this verifier.
    One(WriteVerifier),
    /// The `UNSTABLE` WRITEs disagreed: the server restarted among them. The
    /// verifier the last one to land answered.
    Changed(WriteVerifier),
}

/// The verifiers the WRITEs of one span answered, in the order they landed.
#[derive(Default)]
struct Verifiers(Vec<WriteVerifier>);

impl Verifiers {
    fn answer(&self) -> SpanAnswer {
        match self.0.split_last() {
            None => SpanAnswer::Stable,
            Some((last, rest)) if rest.iter().all(|seen| seen == last) => SpanAnswer::One(*last),
            Some((last, _)) => SpanAnswer::Changed(*last),
        }
    }
}

/// How the bytes acknowledged so far are reported, and where a cancel comes
/// in.
struct Progress<'a> {
    on_progress: &'a (dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    length: StreamLength,
    /// The most ever reported: a resent span doesn't move the bar back.
    reported: u64,
}

impl Progress<'_> {
    fn landed(&mut self, upto: u64, remote: &str) -> Result<(), VolumeError> {
        if upto <= self.reported {
            return Ok(());
        }
        self.reported = upto;
        // ❗ Cancellation arrives ONLY here: the engine says stop by answering
        // `Break`, and a backend that never called back would be uncancelable.
        if (self.on_progress)(StreamWriteProgress::new(upto, self.length)).is_break() {
            return Err(VolumeError::Cancelled(ErrorDetail::path(remote)));
        }
        Ok(())
    }
}

/// The verifier bookkeeping across one file's spans.
#[derive(Default)]
struct Reference {
    verifier: Option<WriteVerifier>,
    changes: u32,
}

impl Reference {
    /// `seen` came back: fine when it's the reference (or the first), else one
    /// more change, which fails the write past [`VERIFIER_CHANGES_TOLERATED`].
    /// `Ok(true)` when the span must go again.
    fn check(&mut self, seen: WriteVerifier, remote: &str) -> Result<bool, VolumeError> {
        match self.verifier {
            None => {
                self.verifier = Some(seen);
                Ok(false)
            }
            Some(reference) if reference == seen => Ok(false),
            Some(_) => self.changed(seen, remote).map(|()| true),
        }
    }

    fn changed(&mut self, now: WriteVerifier, remote: &str) -> Result<(), VolumeError> {
        self.changes += 1;
        self.verifier = Some(now);
        if self.changes > VERIFIER_CHANGES_TOLERATED {
            return Err(VolumeError::io_of_class(
                ErrorDetail::path_then(
                    remote,
                    " couldn't be written: the server restarted twice during the write and lost what it hadn't stored",
                ),
                OsErrorClass::Io,
            ));
        }
        debug!(target: "nfs", "the write verifier of {} changed; resending since the last good COMMIT", LogPrivate::file_path(remote));
        Ok(())
    }
}

/// Pulls up to `want` bytes out of `stream`, coalescing what the source hands
/// over and keeping the overflow in `pending`. Short only at end of stream.
async fn take_span(
    stream: &mut Box<dyn VolumeReadStream>,
    pending: &mut Vec<u8>,
    want: usize,
) -> Result<Vec<u8>, VolumeError> {
    while pending.len() < want {
        match stream.next_chunk().await {
            None => break,
            Some(Ok(bytes)) => pending.extend_from_slice(&bytes),
            Some(Err(err)) => return Err(err),
        }
    }
    let take = want.min(pending.len());
    let rest = pending.split_off(take);
    Ok(std::mem::replace(pending, rest))
}

/// Writes all of `bytes` at `offset`, as many WRITEs as the server makes it
/// take, answering the verifiers its `UNSTABLE` ones came back with.
async fn write_all_at(up: &Upload<'_>, offset: u64, bytes: &[u8]) -> Result<Vec<WriteVerifier>, VolumeError> {
    let mut done = 0usize;
    let mut verifiers = Vec::new();
    while done < bytes.len() {
        let answered = up
            .nfs
            .write(up.handle, offset + done as u64, up.stable, &bytes[done..])
            .await
            .map_err(|err| map_nfs_error(&err, up.remote))?;
        let took = (answered.count as usize).min(bytes.len() - done);
        if took == 0 {
            // ❗ Not a short write to loop on: a server that takes nothing
            // takes nothing again, and looping would hang the upload.
            return Err(VolumeError::io_of_class(
                ErrorDetail::path_then(
                    up.remote,
                    format!(": the server took none of a {}-byte write", bytes.len() - done), // allowed-pluralize-noun: a log-only diagnostic of a WRITE's size, never 1 in practice.
                ),
                OsErrorClass::Io,
            ));
        }
        if answered.committed == Stable::Unstable {
            verifiers.push(answered.verifier);
        }
        done += took;
    }
    Ok(verifiers)
}

/// One span's WRITEs, [`WRITE_WINDOW_DEPTH`] in flight, progress reported as
/// each chunk lands.
async fn write_span(
    up: &Upload<'_>,
    start: u64,
    span: &[u8],
    progress: &mut Progress<'_>,
) -> Result<Verifiers, VolumeError> {
    #[allow(
        clippy::type_complexity,
        reason = "boxing the write future is what makes the window a nameable type; one allocation per chunk"
    )]
    let mut in_flight: FuturesUnordered<
        Pin<Box<dyn Future<Output = Result<(u64, Vec<WriteVerifier>), VolumeError>> + Send + '_>>,
    > = FuturesUnordered::new();
    let mut chunks = span.chunks(up.chunk).enumerate();
    let mut verifiers = Verifiers::default();
    let mut acked = 0u64;
    loop {
        while in_flight.len() < WRITE_WINDOW_DEPTH {
            let Some((index, chunk)) = chunks.next() else { break };
            let offset = start + (index * up.chunk) as u64;
            in_flight.push(Box::pin(async move {
                let seen = write_all_at(up, offset, chunk).await?;
                Ok((chunk.len() as u64, seen))
            }));
        }
        let Some(landed) = in_flight.next().await else {
            return Ok(verifiers);
        };
        let (len, seen) = landed?;
        verifiers.0.extend(seen);
        acked += len;
        progress.landed(start + acked, up.remote)?;
    }
}

/// One span onto the server, resent until a COMMIT vouches for it (or the
/// verifier changed too often).
async fn land_span(
    up: &Upload<'_>,
    start: u64,
    span: &[u8],
    reference: &mut Reference,
    progress: &mut Progress<'_>,
) -> Result<(), VolumeError> {
    loop {
        let wrote = match write_span(up, start, span, progress).await?.answer() {
            SpanAnswer::Stable => return Ok(()),
            SpanAnswer::Changed(now) => {
                reference.changed(now, up.remote)?;
                continue;
            }
            SpanAnswer::One(verifier) => verifier,
        };
        if reference.check(wrote, up.remote)? {
            continue;
        }
        let count = u32::try_from(span.len()).unwrap_or(u32::MAX);
        let committed = up
            .nfs
            .commit(up.handle, start, count)
            .await
            .map_err(|err| map_nfs_error(&err, up.remote))?;
        if !reference.check(committed.verifier, up.remote)? {
            return Ok(());
        }
    }
}

/// The source yielded a different byte count than its declared length.
fn size_mismatch(remote: &str, got: u64, declared: u64) -> VolumeError {
    VolumeError::IoError {
        message: ErrorDetail::path_then(
            remote,
            format!(
                ": the source declared {} and yielded {}",
                pluralize_grouped(declared, "byte"),
                pluralize_grouped(got, "byte")
            ),
        ),
        raw_os_error: None,
    }
}

/// A file this call created, and what removes it again.
struct Created {
    dir: MutationDir,
    handle: FileHandle,
    fileid: u64,
}

impl NfsVolume {
    /// Creates `name` in its proven folder, `GUARDED` (refusing a taken name)
    /// or `UNCHECKED` and truncated (the caller's to replace), with `mode`
    /// alone in its attributes.
    async fn create_for_write(
        &self,
        client: &NfsClient,
        remote: &str,
        guarded: bool,
        mode: u32,
    ) -> Result<Created, VolumeError> {
        let (dir, name) = self.parent_for_mutation(client, remote).await?;
        let how = if guarded {
            CreateHow::Guarded(Sattr3 {
                mode: Some(mode),
                ..Sattr3::default()
            })
        } else {
            CreateHow::Unchecked(Sattr3 {
                mode: Some(mode),
                size: Some(0),
                ..Sattr3::default()
            })
        };
        let made = client
            .nfs
            .create(&dir.handle, name.as_bytes(), &how)
            .await
            .map_err(|err| map_nfs_error(&err, remote))?;
        self.inner.handles.note_dir_changed(&dir.real, &made.dir_wcc);
        let (handle, fileid) = match (made.handle, made.attributes) {
            (Some(handle), Some(attrs)) => (handle, attrs.fileid),
            _ => {
                let found = client
                    .nfs
                    .lookup(&dir.handle, name.as_bytes())
                    .await
                    .map_err(|err| map_nfs_error(&err, remote))?;
                let fileid = match found.attributes {
                    Some(attrs) => attrs.fileid,
                    None => {
                        client
                            .nfs
                            .getattr(&found.handle)
                            .await
                            .map_err(|err| map_nfs_error(&err, remote))?
                            .fileid
                    }
                };
                (found.handle, fileid)
            }
        };
        Ok(Created { dir, handle, fileid })
    }

    /// Takes a failed write's partial away, best effort: the engine's staging
    /// removes its temp too, and ❌ a failure here never replaces the error
    /// that caused it.
    async fn remove_partial(&self, client: &NfsClient, created: &Created, remote: &str) {
        let Some((_, name)) = super::mutation::split_parent(remote) else {
            return;
        };
        self.remove_if_ours(client, &created.dir, name, remote, created.fileid)
            .await;
    }

    /// Streams `stream` (`length` bytes) to `dest`, reporting each acknowledged
    /// chunk through `on_progress`, whose `Break` cancels.
    pub(super) async fn write_from_stream_impl(
        &self,
        dest: &Path,
        mode: WriteMode,
        length: StreamLength,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &(dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    ) -> Result<StreamWritten, VolumeError> {
        let remote = self.to_remote_path(dest)?;
        let client = self.client().await?;
        debug!(target: "nfs", "write_from_stream: {}, length={length:?}", LogPrivate::file_path(&remote));
        // Asked before the first chunk, while the stream still holds what its
        // open learned.
        let landed_mode = stream
            .metadata(MetadataClasses::MODE)
            .mode
            .and_then(|mode| routed_mode(mode, default_file_mode()));
        let created = self
            .create_for_write(&client, &remote, mode.refuses_occupied(), STREAMING_MODE)
            .await?;
        let up = Upload {
            nfs: &client.nfs,
            handle: &created.handle,
            remote: &remote,
            chunk: write_chunk_bytes(&client.fsinfo),
            stable: write_stability(&client.fsinfo),
        };
        let mut progress = Progress {
            on_progress,
            length,
            reported: 0,
        };
        let written = match pump(&up, &mut stream, length, &mut progress).await {
            Ok(written) => written,
            Err(err) => {
                self.remove_partial(&client, &created, &remote).await;
                return Err(err);
            }
        };
        let modified = stream.modified_at();
        let set = self
            .set_attrs(
                &client,
                &created.handle,
                &remote,
                Some(landed_mode.unwrap_or_else(default_file_mode)),
                modified,
            )
            .await;
        patch_created(self, dest).await;
        let carried = CarriedMetadata {
            mode: landed_mode,
            ..CarriedMetadata::default()
        };
        let mut report = MetadataReport::none_applicable(&carried, modified);
        if landed_mode.is_some() {
            report.mode = set.mode;
        }
        report.modified = set.modified;
        Ok(StreamWritten::with_report(written, &report, &carried, modified))
    }

    /// Creates `path` with `content`, refusing a taken name (`CREATE GUARDED`):
    /// ❌ never truncates.
    pub(super) async fn create_file_impl(&self, path: &Path, content: &[u8]) -> Result<(), VolumeError> {
        let remote = self.to_remote_path(path)?;
        let client = self.client().await?;
        let created = self
            .create_for_write(&client, &remote, true, default_file_mode())
            .await?;
        let up = Upload {
            nfs: &client.nfs,
            handle: &created.handle,
            remote: &remote,
            chunk: write_chunk_bytes(&client.fsinfo),
            // A few bytes from the app, so each WRITE is stable on its own and
            // needs no COMMIT.
            stable: Stable::FileSync,
        };
        let keep_going = |_: StreamWriteProgress| ControlFlow::Continue(());
        let mut progress = Progress {
            on_progress: &keep_going,
            length: StreamLength::Known(content.len() as u64),
            reported: 0,
        };
        let mut reference = Reference::default();
        for (index, span) in content.chunks(COMMIT_EVERY).enumerate() {
            let start = (index * COMMIT_EVERY) as u64;
            if let Err(err) = land_span(&up, start, span, &mut reference, &mut progress).await {
                self.remove_partial(&client, &created, &remote).await;
                return Err(err);
            }
        }
        patch_created(self, path).await;
        Ok(())
    }
}

/// The whole source onto `up`, a span at a time, and the byte count checked
/// against what the source declared.
async fn pump(
    up: &Upload<'_>,
    stream: &mut Box<dyn VolumeReadStream>,
    length: StreamLength,
    progress: &mut Progress<'_>,
) -> Result<u64, VolumeError> {
    let mut pending = Vec::new();
    let mut reference = Reference::default();
    let mut written = 0u64;
    loop {
        let span = take_span(stream, &mut pending, COMMIT_EVERY).await?;
        if span.is_empty() {
            break;
        }
        land_span(up, written, &span, &mut reference, progress).await?;
        written += span.len() as u64;
    }
    // `Known(n)` promises exactly `n` bytes: a source that shrank or grew
    // while it was read fails here, and the caller takes the partial away.
    if let StreamLength::Known(declared) = length
        && written != declared
    {
        return Err(size_mismatch(up.remote, written, declared));
    }
    Ok(written)
}

#[cfg(test)]
#[path = "writes_test.rs"]
mod writes_test;
