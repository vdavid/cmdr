//! Writing a stream into an SMB file: the inherent `write_from_stream_impl` body
//! the `write_from_stream` trait method in `volume_impl` delegates to (one
//! compound CREATE+WRITE+FLUSH frame for a write that fits it, an owned
//! `FileWriter` streaming anything bigger), and the single-shot promise that
//! frame lets the transfer layer rely on (`write_is_single_shot_impl`). Reading
//! a file as a stream is `read_stream`.

use super::SmbVolume;
use super::checkpointed_write::{Kept, Streamed};
use super::dfs_links::is_behind_dfs_link;
use super::session::{LeaseOn, SessionLease, TransferLease};
use super::stamping::{source_times, stamped};
use super::state::RanOn;
use cmdr_fs::error_detail::ErrorDetail;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{CarriedMetadata, ClassOutcome, MetadataClasses, Xattr};
use cmdr_fs::pluralize::pluralize_grouped;
use cmdr_fs::volume::{MutationEvent, StreamLength, StreamWritten, Volume, VolumeError, VolumeReadStream, WriteMode};
use log::debug;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

/// The `max_write_size` to assume when the session hasn't reported its
/// negotiated params. Every SMB2 dialect negotiates at least 64 KiB, so this is
/// the conservative floor rather than a guess.
pub(super) const ASSUMED_MAX_WRITE: u64 = 65536;

/// THE condition for the one-frame write fast path (CREATE+WRITE+FLUSH in one
/// compound, then a standalone CLOSE): the write
/// fits `limit`, and an empty file has no WRITE to compound with, so it goes to
/// the streaming writer.
///
/// One definition on purpose. `write_is_single_shot` (the transfer layer's
/// staging exemption) answers with it on smb2's `quick_write_limit`, and
/// `write_from_stream_impl` branches on it with [`one_frame_write_limit`]. If a
/// promised write could ever take the streaming path, a force-quit mid-write
/// would leave a truncated file at the user's real filename — the 2026-07-31
/// wedge over again (`docs/notes/incidents/2026-07-31-transfer-wedge/README.md`).
///
/// The limit is what the uplink moves in 250 ms, a fixed budget for how long a
/// listing may wait behind one frame (apart from the headroom smb2's adaptive
/// write-behind window learns): one upload chunk (`smb2::UPLOAD_CHUNK_SIZE`,
/// 512 KiB) on a connection that hasn't measured its uplink, then `rate × 250 ms`
/// at the uplink's measured capacity, capped at `compound_write_limit`
/// (`max_write`, lowered to what half the credit window funds). A bigger frame
/// carries the whole file with no progress, queued ahead of every listing on
/// the connection: at 375 KB/s a `stat` waited 23 s
/// behind an 8 MiB upload, 1.5 s behind a streamed one (smb2's
/// `benchmarks/read-ahead/results/adaptive-uploads.md`, smb2 0.25.0, 2026-09-23).
/// smb2 owns that arithmetic.
pub(super) fn fits_one_compound_write(limit: u64, size: u64) -> bool {
    size > 0 && size <= limit
}

/// The limit `write_from_stream` sends one compound frame up to, which must
/// cover every size `write_is_single_shot` promised.
///
/// The promise reads smb2's `quick_write_limit`, which moves between the promise
/// and the write: the upload rate it's built on changes with every upload on the
/// connection and expires after 30 s, and the credit window can shrink under it.
/// So re-reading it here could send a promised write down the streaming path.
/// Only a promised write targets the user's real name, though (every other one
/// lands on a `.cmdr-tmp-*`), which splits it cleanly:
///
/// - A staging temp takes today's limit: anything bigger streams, which is safe
///   on a temp, and a frame the window refuses streams too.
/// - The real name keeps `max_write`, the most any promise could have covered
///   (`quick_write_limit` never exceeds it). A frame the window refuses there
///   fails the write with nothing on the wire, and nothing at the name.
pub(super) fn one_frame_write_limit(dest_is_scratch: bool, max_write: u64, quick_write_limit: u64) -> u64 {
    if dest_is_scratch {
        quick_write_limit.min(max_write)
    } else {
        max_write
    }
}

/// What a write learned closing its file, for its report.
pub(super) struct ClosedWriter {
    pub(super) xattrs_kept: ClassOutcome,
    /// The server refused the dates.
    pub(super) times_refused: bool,
    /// The bytes the server acknowledged once `finish()` drained the window.
    pub(super) confirmed: u64,
}

/// What a write puts on its file beside the bytes, asked of the source stream
/// once, before its first chunk.
pub(super) struct CarriedForWrite {
    carried: CarriedMetadata,
    times: Option<smb2::FileTimes>,
}

/// What [`SmbVolume::close_writer`] puts on a streamed-into file, and the
/// session it does that on beside the writer.
pub(super) struct WriteCarries<'a> {
    pub(super) tree: &'a smb2::client::Tree,
    /// For the xattrs beside the writer, which owns its own connection.
    pub(super) side_conn: &'a mut smb2::client::Connection,
    pub(super) carried: &'a CarriedForWrite,
}

/// A source that yielded `got` bytes after declaring `StreamLength::Known(declared)`,
/// refused the way the other backends' uploads refuse it. The copy engine holds
/// its streams to the same promise before this sees them
/// (`crates/cmdr-operations/src/transfer/volume/source_reads.rs`); this is the
/// backend's own guard for every other caller.
fn source_broke_its_length(smb_path: &str, got: u64, declared: u64) -> VolumeError {
    VolumeError::IoError {
        message: ErrorDetail::path_then(
            smb_path,
            format!(
                ": the source declared {} and yielded {}",
                pluralize_grouped(declared, "byte"),
                pluralize_grouped(got, "byte")
            ),
        ),
        raw_os_error: None,
    }
}

/// Whether `dest` is one of Cmdr's scratch names (a `.cmdr-tmp-*` staging
/// temp), which a partial write can never make look like the user's file.
fn is_scratch_name(dest: &Path) -> bool {
    dest.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(cmdr_fs::staging::is_staging_temp_name)
}

/// Whether a compound write failed AFTER the server had created the file.
///
/// smb2 reports which command in the chain returned the non-success NTSTATUS. A
/// `Create` failure means nothing of ours reached the destination path (and any
/// file already sitting there is untouched, so it must NOT be deleted); anything
/// later means the CREATE landed, truncating the name to whatever the server
/// accepted. A transport error says nothing either way, so it counts as "not
/// ours to clean up".
fn create_succeeded_but_write_failed<T>(result: &Result<T, smb2::Error>) -> bool {
    matches!(result, Err(smb2::Error::Protocol { command, .. }) if *command != smb2::types::Command::Create)
}

/// Opens the streaming writer `mode` asks for: `FileCreate` for a name that
/// must be new, `FileOverwriteIf` for one the caller may replace.
async fn open_file_writer(
    tree: &Arc<smb2::client::Tree>,
    conn: smb2::client::Connection,
    path: &str,
    mode: WriteMode,
) -> Result<smb2::client::stream::FileWriter, smb2::Error> {
    match mode {
        WriteMode::CreateNew | WriteMode::CreateNewInFreshFolder => tree.create_file_writer_exclusive(conn, path).await,
        WriteMode::CreateOrReplace => tree.create_file_writer(conn, path).await,
    }
}

impl SmbVolume {
    /// The negotiated `max_write_size` for the live session, or `None` when
    /// there isn't one.
    ///
    /// Reads the client's own connection rather than cloning a session: the
    /// clones `clone_session` hands out share the connection's negotiated
    /// params, so this is the same number the upload will see, for the price of
    /// a brief uncontended mutex and no wire traffic. Only tests size a payload
    /// off it, so a release build without `testing` leaves it out.
    #[cfg(any(test, feature = "testing"))]
    pub(super) async fn negotiated_max_write(&self) -> Option<u64> {
        let guard = self.inner.session.lock().await;
        guard
            .as_ref()
            .and_then(|s| s.client.params())
            .map(|p| p.max_write_size as u64)
    }

    /// The largest write worth sending as one compound frame right now (smb2's
    /// `quick_write_limit`, see [`fits_one_compound_write`]) on the connection a
    /// write would ride: the transfer session's while one is up, else the main
    /// session's. `None` when there's neither. No clone, no wire.
    ///
    /// A promise made on the main session's limit and then written on a freshly
    /// opened transfer session is covered: the real name keeps `max_write`, the
    /// same on both sessions of one server (`one_frame_write_limit`).
    async fn quick_write_limit(&self) -> Option<u64> {
        if let Some(limit) = self
            .inner
            .transfer
            .live
            .with_connection(|c| c.params().is_some().then(|| c.quick_write_limit()))
            .flatten()
        {
            return Some(limit);
        }
        let guard = self.inner.session.lock().await;
        guard
            .as_ref()
            .map(|s| &s.client)
            .filter(|c| c.params().is_some())
            .map(|c| c.connection().quick_write_limit())
    }

    /// [`open_file_writer`] on the lease's tree, or through the DFS link the
    /// path sits behind (`dfs_links.rs`): first, under a known link, skipping
    /// the refused open.
    async fn open_writer(
        &self,
        tree: &Arc<smb2::client::Tree>,
        conn: smb2::client::Connection,
        lease: (RanOn, &LeaseOn),
        path: &str,
        mode: WriteMode,
    ) -> Result<smb2::client::stream::FileWriter, smb2::Error> {
        if self.inner.link_paths.covers(path)
            && let Some(opened) = self.inner.open_link_writer(lease.0, lease.1, tree, path, mode).await
        {
            return opened;
        }
        match open_file_writer(tree, conn, path, mode).await {
            Err(e) if is_behind_dfs_link(&e) => self
                .inner
                .open_link_writer(lease.0, lease.1, tree, path, mode)
                .await
                .unwrap_or(Err(e)),
            opened => opened,
        }
    }

    /// Closes a streamed-into `writer`: the copied xattrs, then its dates, then
    /// `finish()`. Beside the open writer (a stream opens past its no-sharing
    /// handle) and before its dates, which then outlast the stream writes' own
    /// bump on `streams_xattr` (`DETAILS.md` § "Xattrs on copies"). On any
    /// failure it closes the handle first (`abort()`, or `finish()` already
    /// consumed it) and then runs `delete_partial`, so a half-written file never
    /// keeps the user's name, unless `kept` lets a transport fault leave it
    /// for a continuation (`SmbVolume::settle_partial`).
    pub(super) async fn close_writer<F: Future<Output = ()>>(
        &self,
        mut writer: smb2::client::stream::FileWriter,
        smb_path: &str,
        ran_on: RanOn,
        WriteCarries {
            tree,
            side_conn,
            carried,
        }: WriteCarries<'_>,
        kept: Option<Kept>,
        delete_partial: &impl Fn() -> F,
    ) -> Result<ClosedWriter, VolumeError> {
        let xattrs: &[Xattr] = &carried.carried.xattrs;
        let xattrs_kept = match self
            .put_copied_xattrs(tree, side_conn, smb_path, ran_on, xattrs, false)
            .await
        {
            Ok(kept) => kept,
            Err(ve) => {
                let _ = writer.abort().await;
                self.settle_partial(&ve, kept, smb_path, delete_partial).await;
                return Err(ve);
            }
        };
        let mut times_refused = false;
        if let Some(times) = carried.times {
            match self.stamp_writer(&mut writer, times, smb_path, ran_on).await {
                Ok(took) => times_refused = !took,
                Err(ve) => {
                    // The connection died under the stamp: the same cleanup as
                    // a failed chunk.
                    let _ = writer.abort().await;
                    self.settle_partial(&ve, kept, smb_path, delete_partial).await;
                    return Err(ve);
                }
            }
        }
        let finish_result = writer.finish().await;
        match self.handle_smb_result("write_from_stream(finish)", smb_path, ran_on, finish_result) {
            Ok(confirmed) => Ok(ClosedWriter {
                xattrs_kept,
                times_refused,
                confirmed,
            }),
            Err(ve) => {
                self.settle_partial(&ve, kept, smb_path, delete_partial).await;
                Err(ve)
            }
        }
    }

    /// What the write takes from the stream beside its bytes. The creation
    /// date and Hidden ride the dates' frame; Read-only waits for the landing
    /// (the copy's `apply_metadata`). The xattrs go on as named streams before
    /// the dates, on a share that has them (`copy_xattrs.rs`). A share has no
    /// mode.
    pub(super) fn carried_for_write(&self, stream: &dyn VolumeReadStream) -> CarriedForWrite {
        let wanted = self
            .metadata_caps()
            .stores
            .intersection(MetadataClasses::CREATED | MetadataClasses::FLAGS | MetadataClasses::XATTRS);
        let carried = stream.metadata(wanted);
        let times = source_times(stream.modified_at(), carried.created, carried.flags);
        CarriedForWrite { carried, times }
    }

    /// Best-effort delete of a partial file on a FRESH cloned session.
    ///
    /// Once a `FileWriter` is open and bytes have streamed into it, an early
    /// error (source read error, `write_chunk` / `finish` failure) would
    /// otherwise leave a half-written file at the real destination name
    /// (AGENTS.md principle #4: a failed copy must not leave corrupt bytes under
    /// the user's intended name). The delete runs on a fresh session because the
    /// writer's own connection may be gone. The caller MUST close the leaked
    /// write handle first (`writer.abort()` where the writer is still owned),
    /// else this delete hits a sharing violation against the still-open handle.
    pub(super) fn partial_deleter<'a>(
        &'a self,
        smb_path: &'a str,
    ) -> impl Fn() -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> + 'a {
        move || {
            Box::pin(async move {
                if let Ok(SessionLease {
                    tree,
                    mut conn,
                    generation,
                }) = self.clone_session().await
                {
                    match tree.delete_file(&mut conn, smb_path).await {
                        Ok(()) => self.delete_apple_double_sidecar(&tree, &conn, smb_path).await,
                        Err(e) if is_behind_dfs_link(&e) => {
                            let _ = self
                                .inner
                                .link_delete_file(generation.into(), &LeaseOn::Main, &tree, smb_path)
                                .await;
                        }
                        Err(_) => {}
                    }
                }
            })
        }
    }

    /// A write's last step: the listing patch, then its report.
    ///
    /// The patch comes from local knowledge so the destination pane sees the
    /// new file without waiting for a CHANGE_NOTIFY round-trip. The SMB watcher
    /// has a loss window between consecutive `next_events()` calls; relying on
    /// it alone left bulk cross-volume copies showing only a subset of the
    /// just-copied files until the user navigated away and back.
    pub(super) async fn written(
        &self,
        dest: &Path,
        closed: ClosedWriter,
        carried: &CarriedForWrite,
        modified_at: Option<std::time::SystemTime>,
    ) -> StreamWritten {
        if let (Some(parent), Some(name)) = (dest.parent(), dest.file_name())
            && let Some(parent_display) = self.display_path_for(parent)
        {
            self.notify_mutation(
                &self.inner.volume_id,
                &parent_display,
                MutationEvent::Created(name.to_string_lossy().to_string()),
            )
            .await;
        }
        stamped(
            closed.confirmed,
            &carried.carried,
            modified_at,
            closed.times_refused,
            closed.xattrs_kept,
        )
    }

    /// Inherent body for the `write_is_single_shot` trait method (thin delegator
    /// in `volume_impl`): whether a write of `size` bytes takes the compound
    /// fast path below, which is what makes it all-or-nothing.
    pub(super) async fn write_is_single_shot_impl(&self, size: u64) -> bool {
        match self.quick_write_limit().await {
            Some(limit) => fits_one_compound_write(limit, size),
            // No live session: no promise. The transfer stages, as it would for
            // any backend without the guarantee.
            None => false,
        }
    }

    /// Inherent body for the `write_from_stream` trait method (thin delegator in `volume_impl`).
    pub(super) fn write_from_stream_impl<'a>(
        &'a self,
        dest: &'a Path,
        mode: WriteMode,
        length: StreamLength,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(cmdr_fs::volume::StreamWriteProgress) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<StreamWritten, VolumeError>> + Send + 'a>> {
        // Lock-free streaming write path.
        //
        // Both branches below drive the upload on a cloned `Connection`
        // (cheap `Arc::clone`) and an `Arc<Tree>`. The client mutex is
        // held only for the few microseconds of `clone_session()`, never
        // for the upload itself. With smb2 0.9's owned `FileWriter`, N
        // concurrent `write_from_stream` calls on one `SmbVolume`
        // pipeline N WRITE chains over a single SMB session: smb2's
        // receiver task multiplexes responses by `MessageId`.
        //
        // This collapses the historical two-phase pattern (brief
        // `clone_session` for the fast-path → drop → long
        // session-mutex hold for the streaming fallback) into a single
        // clone. The old shape deadlocked under sustained concurrent
        // pressure; the regression test
        // `smb_integration_concurrent_streaming_writes_no_deadlock`
        // pins this shape.
        Box::pin(async move {
            let smb_path = self.to_smb_path(dest)?;

            debug!(
                "SmbVolume::write_from_stream: share={:?}, path={:?}, length={length:?}",
                LogPrivate::volume(&self.inner.share_name),
                LogPrivate::file_path(&smb_path)
            );

            // Acquire a lease once, up front, on the transfer session
            // (`transfer_session.rs`). Both the compound fast-path and the
            // streaming fallback drive their write on this same clone, and the
            // lease lives to the end of the write.
            let TransferLease {
                tree,
                mut conn,
                ran_on,
                on,
            } = self.transfer_lease().await?;
            let carried = self.carried_for_write(&*stream);
            let delete_partial = self.partial_deleter(&smb_path);
            // A temp of a staged write: never left behind by a checkpoint it
            // didn't reach, and carried on past one (`checkpointed_write.rs`).
            self.inner.write_checkpoints.forget(&smb_path);

            // Compound fast-path: when the caller promised a size that fits
            // in one WRITE, drain the source stream into a buffer and send
            // CREATE+WRITE+FLUSH as a single compound frame, then a standalone
            // CLOSE (2 RTTs instead of 4; smb2 keeps the FLUSH last in its
            // chain because servers may fail an async op mid-chain, and Samba
            // before 4.18 crashes on it). Small files are the hot case; we
            // fall through to the streaming writer for anything larger.
            //
            // The frame is also what makes this write ALL-OR-NOTHING, which is
            // why `write_is_single_shot` lets the transfer layer skip its
            // `.cmdr-tmp-*` staging for exactly these writes: the server either
            // gets the whole length-prefixed frame (every byte of the file) and
            // runs it, or discards it and creates nothing. The CLOSE only
            // releases the handle, which the server also does when the session
            // dies, so a force-quit between the two can't truncate anything.
            // ❌ So keep the drained buffer on
            // this path whenever it still fits one WRITE — dropping into the
            // multi-round-trip streaming writer after promising one shot is what
            // would put a truncated file at the user's real filename.
            //
            // Which limit applies depends on the destination
            // (`one_frame_write_limit`): a staging temp follows the quick-write
            // limit as it is now, the user's real name keeps the limit any
            // promise could have been made under.
            let dest_is_scratch = is_scratch_name(dest);
            let closed = 'write: {
                let max_write = conn
                    .params()
                    .map(|p| p.max_write_size as u64)
                    .unwrap_or(ASSUMED_MAX_WRITE);
                let limit = one_frame_write_limit(dest_is_scratch, max_write, conn.quick_write_limit());
                if length.known().is_some_and(|size| fits_one_compound_write(limit, size)) {
                    let size = length.known().expect("the compound branch requires a known length");
                    let mut buffer = Vec::with_capacity(size as usize);
                    while let Some(chunk_result) = stream.next_chunk().await {
                        // Compound drain buffers in memory; no writer/handle
                        // is open yet, so a source error here can't leave a
                        // partial on the server. Just propagate.
                        let chunk = chunk_result?;
                        buffer.extend_from_slice(&chunk);
                        // A source still growing (a log, a download finishing)
                        // is refused the moment it outruns its promise, before
                        // anything is opened and without buffering the rest.
                        if buffer.len() as u64 > size {
                            return Err(source_broke_its_length(&smb_path, buffer.len() as u64, size));
                        }
                        // Fire progress per chunk AND honor cancellation, so
                        // the fast-path has the same cancel/progress contract
                        // as the streaming fallback below. Cancel here aborts
                        // before the compound WRITE touches the wire: the
                        // destination never sees a partial file.
                        if on_progress(cmdr_fs::volume::StreamWriteProgress::new(buffer.len() as u64, length))
                            .is_break()
                        {
                            return Err(VolumeError::Cancelled("Operation cancelled by user".into()));
                        }
                    }
                    // A source that came up short is refused too: `Known(size)`
                    // promises exactly `size` bytes, and the frame below is
                    // the file the user gets under that name (#520).
                    if buffer.len() as u64 != size {
                        return Err(source_broke_its_length(&smb_path, buffer.len() as u64, size));
                    }
                    debug!(
                        "SmbVolume::write_from_stream: using compound fast-path ({} bytes)",
                        buffer.len()
                    );
                    // `CreateNew` asks the server for `FileCreate`, so a name
                    // someone else took while we drained is refused whole
                    // (`STATUS_OBJECT_NAME_COLLISION` on the CREATE, nothing
                    // opened, nothing written) and never replaced. That
                    // refusal is a CREATE failure, so the cleanup below leaves
                    // their file alone.
                    let mut write_result = match mode {
                        WriteMode::CreateNew | WriteMode::CreateNewInFreshFolder => {
                            tree.write_file_compound_exclusive(&mut conn, &smb_path, &buffer).await
                        }
                        WriteMode::CreateOrReplace => tree.write_file_compound(&mut conn, &smb_path, &buffer).await,
                    };
                    // Behind a DFS link the CREATE was refused and nothing was
                    // written, so the same frame goes to the link's target.
                    let through_link = matches!(&write_result, Err(e) if is_behind_dfs_link(e));
                    if through_link
                        && let Some(linked) = self
                            .inner
                            .link_write_compound(ran_on, &on, &tree, &smb_path, &buffer, mode)
                            .await
                    {
                        write_result = linked;
                    }
                    if !(dest_is_scratch && matches!(write_result, Err(smb2::Error::CreditStarvation { .. }))) {
                        if create_succeeded_but_write_failed(&write_result) {
                            // The server created (hence truncated) the file and
                            // then refused the bytes: out of space, over quota,
                            // a lost lease. What's left is a 0-byte file that
                            // may be wearing the user's real filename, since
                            // this write was allowed to skip staging. Take it
                            // away. A CREATE failure needs no cleanup: nothing
                            // was created, and any existing file at that name
                            // is still untouched, so a delete there would be
                            // data loss.
                            delete_partial().await;
                        }
                        let written =
                            self.handle_smb_result("write_from_stream(compound)", &smb_path, ran_on, write_result)?;
                        // Before the dates: a stream write moves the file's
                        // date on `streams_xattr`. A lost connection fails the
                        // write with no cleanup, as the stamp's does below.
                        let xattrs_kept = self
                            .put_copied_xattrs(
                                &tree,
                                &mut conn,
                                &smb_path,
                                ran_on,
                                &carried.carried.xattrs,
                                through_link,
                            )
                            .await?;
                        let mut times_refused = false;
                        // The frame's CLOSE already stamped the server's own
                        // dates, so these go by path, after it. A refusal
                        // keeps the server's; a lost connection fails the
                        // write (`stamp_taken`). ❌ No cleanup on that: the
                        // CLOSE already ran and the name may be the user's
                        // real one, so the transfer settles it
                        // (`settle_unanswered_create`).
                        if let Some(times) = carried.times {
                            let stamp = match through_link {
                                false => tree.set_times(&mut conn, &smb_path, times).await,
                                true => match self.inner.link_set_times(ran_on, &on, &tree, &smb_path, times).await {
                                    Some(stamp) => stamp,
                                    // The lease's session was replaced: the
                                    // path's own refusal, which keeps the
                                    // server's dates.
                                    None => tree.set_times(&mut conn, &smb_path, times).await,
                                },
                            };
                            times_refused = !self.stamp_taken(&smb_path, ran_on, stamp)?;
                        }
                        break 'write ClosedWriter {
                            xattrs_kept,
                            times_refused,
                            confirmed: written,
                        };
                    }
                    // The window shrank after the limit was read, and smb2
                    // refused the frame before anything reached the wire. A
                    // staging temp can take the bytes streamed (the streaming
                    // writer below honors `mode` too); the real name can't, so
                    // that one surfaced the error above with nothing written,
                    // whichever `mode`. ❌ This is the ONLY way a drained
                    // buffer reaches the streaming writer: a real name never
                    // streams, or a drop mid-stream leaves a truncated file
                    // wearing the user's filename.
                    debug!(
                        "SmbVolume::write_from_stream: the credit window can't fund one compound write; streaming the drained {} bytes to the staging temp instead",
                        buffer.len()
                    );
                    // For the xattrs beside the writer, which owns `conn`.
                    let mut side_conn = conn.clone();
                    let writer_result = self.open_writer(&tree, conn, (ran_on, &on), &smb_path, mode).await;
                    let mut writer =
                        self.handle_smb_result("write_from_stream(open)", &smb_path, ran_on, writer_result)?;
                    if !buffer.is_empty() {
                        let write_result = writer.write_chunk(&buffer).await;
                        if let Err(ve) =
                            self.handle_smb_result("write_from_stream(write_chunk)", &smb_path, ran_on, write_result)
                        {
                            // Writer still owned: abort (closes the leaked
                            // handle) then delete the partial, mirroring the
                            // cancel branch, then propagate the original error.
                            let _ = writer.abort().await;
                            delete_partial().await;
                            return Err(ve);
                        }
                    }
                    // The source signalled end-of-stream by returning None
                    // above (we exited the drain loop). No further chunks.
                    // One frame's worth: no checkpoint to keep, so nothing
                    // for a continuation.
                    let carries = WriteCarries {
                        tree: &tree,
                        side_conn: &mut side_conn,
                        carried: &carried,
                    };
                    let closed = self
                        .close_writer(writer, &smb_path, ran_on, carries, None, &delete_partial)
                        .await?;
                    break 'write ClosedWriter {
                        confirmed: buffer.len() as u64,
                        ..closed
                    };
                }

                // Streaming path for large / unknown-size writes. Drives the
                // owned `FileWriter` on the cloned `Connection` directly —
                // no client mutex is held while WRITEs are in flight, so N
                // concurrent large copies pipeline over one SMB session.
                // For the xattrs beside the writer, which owns `conn`.
                let mut side_conn = conn.clone();
                let writer_result = self.open_writer(&tree, conn, (ran_on, &on), &smb_path, mode).await;
                let writer = self.handle_smb_result("write_from_stream(open)", &smb_path, ran_on, writer_result)?;
                let streamed = Streamed {
                    smb_path: &smb_path,
                    ran_on,
                    length,
                    on_progress,
                    // A temp checkpoints so an outage can carry it on; the
                    // user's real name never streams a partial worth keeping.
                    continuable: dest_is_scratch,
                    created: None,
                };
                let carries = WriteCarries {
                    tree: &tree,
                    side_conn: &mut side_conn,
                    carried: &carried,
                };
                self.stream_into_writer(writer, &streamed, &mut *stream, carries, &delete_partial)
                    .await?
            };

            Ok(self.written(dest, closed, &carried, stream.modified_at()).await)
        })
    }
}

#[cfg(test)]
#[path = "write_stream_test.rs"]
mod write_stream_test;
