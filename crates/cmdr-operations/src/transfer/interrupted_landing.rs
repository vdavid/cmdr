//! What a landing or a single-shot write the transport cut off did, settled
//! once the destination is back (`volume_outage.rs`).
//!
//! **The ambiguity.** A rename or a delete over a network share whose reply
//! never arrives may or may not have happened: the drop can land before the
//! server saw the request, or after it performed it. Asked again blind, a rename
//! that happened meets our OWN file at the name, and a landing onto a name the
//! caller believed free refuses that as a clash nobody resolved
//! (`staged_write.rs::land`): a copy that waited out the outage then failed with
//! `DestinationExists` over the file it had just written. The live-SMB cell
//! `operations_host/backend_suites/smb_outage_test.rs::smb_integration_a_copy_waits_out_a_dropped_network_and_finishes`
//! hit it whenever the cut fell inside a landing. So the landing stops at the
//! step the transport cut off (`staged_write::LandingError::CutOff`), and
//! `CutOffLanding::resume` settles it from what the destination shows.
//!
//! **A temp still whole is landed again, ❌ never streamed again.** The settle
//! picks up where the cut left off: after a claimed name's clearing delete went
//! through (the temp is then the only copy at the destination), the rename
//! alone once the name is empty (a file someone put there meanwhile makes it
//! refuse, and is never deleted), the clearing delete again only while the name
//! still holds the very entry the landing noted before its delete
//! ([`NameHeld`]; anything else there is a save that came after our delete, so
//! the rename alone); after a swap's move aside went through, the rename alone,
//! and the move again when the aside isn't there.
//!
//! **The proof a gone temp was ours.** The temp is a `.cmdr-tmp-<uuid>` name
//! this operation minted, and only two things ever take it away: our landing
//! rename, and our own discard. So once the destination is back, a landing
//! whose temp is gone counts as done exactly when all three hold:
//!
//! - nothing of ours discarded it (the landing notes every discard it asks
//!   for, which is why `land` never discards after a transport fault: on a
//!   stale error the delete would reach a fresh session and find the landed
//!   temp gone);
//! - the temp is gone from the destination, so something moved it, and the only
//!   move ever asked for it was ours, onto `final_path`;
//! - `final_path` holds what we wrote, so nothing replaced it while the share
//!   was away: a file of exactly our size for a rename-first landing, which
//!   cleared nothing; ❗ exactly the source's bytes, compared in full
//!   ([`holds_the_source`]), once the way may have been cleared or the
//!   original moved, since a swap whose temp someone removed can leave the
//!   user's OLD file at the name, maybe at the same size. With no source to
//!   compare against (a one-pass extract), a gone temp after a cleared way or
//!   a move aside never counts as landed.
//!
//! Anything short of that (a stat that fails, a size that differs, a folder at
//! the name) is NOT proof, and the file runs again the way it did before: a
//! name someone else took meets the landing's ordinary clash refusal. ❌ Never
//! loosen the checks into "something is at the name": a file claimed by mistake
//! goes into the rollback ledger as ours, and an undo would delete it.
//!
//! **A write with no temp.** A SINGLE-SHOT write (`WriteMode::CreateNew` at the
//! final name: SMB's compound CREATE+WRITE+FLUSH+CLOSE for a small file, an
//! object store's PUT) has the same ambiguity and no temp to read it from. Run
//! again blind, its `CreateNew` meets our own file and refuses it. So the next
//! attempt first asks [`settle_unanswered_create`], which counts the file done
//! only when the name holds a file of exactly our size whose bytes are the
//! source's, compared in full. The name was free when the conflict check
//! looked, our create to it went unanswered, and what's there now is
//! byte-for-byte what we sent. ❌ Never on size alone, and never on the mtime:
//! the compound write stamps the source's dates in a separate request after it,
//! which an unanswered write never reached. A 0-byte file our CREATE left
//! without its WRITE, or a file someone else saved, isn't the source's bytes,
//! so it stays the ordinary clash. For the same reason a settled create hands
//! back the source's [`WriteStamp`]: the caller puts it on by path (idempotent),
//! so the file never keeps the server's date silently.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::metadata::CarriedMetadata;
use cmdr_fs::volume::{StreamWritten, Volume, VolumeError, VolumeReadStream, WriteMode};

/// Whether this attempt's write, made in `write_mode` (`StagedWrite::write_mode`),
/// was a create at the FINAL name (a single-shot `CreateNew`) that failed on the
/// transport, so the server may have completed it anyway: the next attempt must
/// [`settle_unanswered_create`] first.
///
/// Takes the mode, ❌ not the `StagedWrite`: the landing in
/// `staged_write_landing.rs` calls into this module, so naming the type here
/// would weld the two into a module cycle.
pub(super) fn create_went_unanswered(write_mode: WriteMode, result: &Result<StreamWritten, VolumeError>) -> bool {
    matches!(write_mode, WriteMode::CreateNew | WriteMode::CreateNewInFreshFolder)
        && matches!(result, Err(e) if e.transport_fault().is_some())
}

/// What a write puts on its file beyond the bytes, as the destination's own
/// write would have: the source's date and the classes the destination keeps on
/// a write, flags narrowed to the ones it stages (the rest wait for the landing,
/// `landed_flags.rs`).
pub(super) struct WriteStamp {
    pub(super) carried: CarriedMetadata,
    pub(super) modified: Option<std::time::SystemTime>,
}

impl WriteStamp {
    /// What `dest`'s write of `stream` would have stamped.
    fn of(stream: &dyn VolumeReadStream, dest: &dyn Volume) -> Self {
        let caps = dest.metadata_caps();
        let mut carried = stream.metadata(caps.stores);
        carried.flags = carried.flags.intersection(caps.staged_flags);
        Self {
            carried,
            modified: stream.modified_at(),
        }
    }
}

/// What stood at a claimed name as the landing set out to clear it, so the
/// settle of a clearing delete whose reply was lost can tell "it never reached
/// the server" (the same entry is still there) from "it went through, and
/// someone saved at the name during the outage" (anything else, which the
/// settle ❌ never clears).
///
/// Both stats ask the SAME backend about the SAME name the same way
/// (`get_metadata`), so they report the same fields, and a coarse clock
/// truncates both alike: whole-struct equality is exact, and a field a backend
/// never reports is `None` on both sides. What it can miss is a same-size save
/// inside one tick of the destination's clock (whole seconds) on a backend with
/// no node id, which needs our delete AND their save inside that one second.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NameHeld {
    is_directory: bool,
    is_symlink: bool,
    size: Option<u64>,
    modified_at: Option<u64>,
    inode: Option<u64>,
}

impl NameHeld {
    pub(super) fn of(entry: &FileEntry) -> Self {
        Self {
            is_directory: entry.is_directory,
            is_symlink: entry.is_symlink,
            size: entry.size,
            modified_at: entry.modified_at,
            inode: entry.inode,
        }
    }

    /// Whether it's a regular file of exactly zero bytes: the only shape a
    /// `Rename` pick's `O_EXCL` placeholder has, and so the only thing a
    /// claimed name's landing may clear.
    pub(super) fn is_an_empty_file(&self) -> bool {
        !self.is_directory && !self.is_symlink && self.size == Some(0)
    }
}

/// Settles a single-shot create of `size` bytes at `final_path` whose reply never
/// came: `Some` only when the file there is exactly the source's bytes, with the
/// [`WriteStamp`] the write may not have put on (its stamp is a later request,
/// and the drop may have cut it off). `None` leaves the name alone, so a file
/// that isn't provably ours meets the next attempt's `CreateNew` refusal.
pub(super) async fn settle_unanswered_create(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    size: u64,
    dest_volume: &Arc<dyn Volume>,
    final_path: &Path,
) -> Option<WriteStamp> {
    let (ours, at_the_name) = same_size_streams(source_volume, source_path, size, dest_volume, final_path).await?;
    // Read before the compare consumes the stream: the open already holds it.
    let stamp = WriteStamp::of(&*ours, &**dest_volume);
    same_bytes(ours, at_the_name).await.then_some(stamp)
}

/// Whether `final_path` holds exactly the `size` bytes of `source_path`,
/// compared in full by re-reading both: the proof a landing whose temp is gone
/// after the way was cleared put our file there. A read that fails on either
/// side is "can't prove it".
pub(super) async fn holds_the_source(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    size: u64,
    dest_volume: &Arc<dyn Volume>,
    final_path: &Path,
) -> bool {
    match same_size_streams(source_volume, source_path, size, dest_volume, final_path).await {
        Some((ours, at_the_name)) => same_bytes(ours, at_the_name).await,
        None => false,
    }
}

/// Both files open for a compare, when `final_path` is a file of exactly
/// `size` bytes (the cheap check first).
async fn same_size_streams(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    size: u64,
    dest_volume: &Arc<dyn Volume>,
    final_path: &Path,
) -> Option<(Box<dyn VolumeReadStream>, Box<dyn VolumeReadStream>)> {
    let same_size = matches!(
        dest_volume.get_metadata(final_path).await,
        Ok(entry) if !entry.is_directory && !entry.is_symlink && entry.size == Some(size)
    );
    if !same_size {
        return None;
    }
    match (
        source_volume.open_read_stream_with_hint(source_path, Some(size)).await,
        dest_volume.open_read_stream(final_path).await,
    ) {
        (Ok(ours), Ok(at_the_name)) => Some((ours, at_the_name)),
        _ => None,
    }
}

/// Whether two streams yield the same bytes, whatever their chunking. A read
/// error on either side is "can't prove it", so `false`.
async fn same_bytes(mut a: Box<dyn VolumeReadStream>, mut b: Box<dyn VolumeReadStream>) -> bool {
    let (mut a_buf, mut a_at, mut b_buf, mut b_at) = (Vec::new(), 0, Vec::new(), 0);
    loop {
        let Some(a_more) = refill(&mut *a, &mut a_buf, &mut a_at).await else {
            return false;
        };
        let Some(b_more) = refill(&mut *b, &mut b_buf, &mut b_at).await else {
            return false;
        };
        if !a_more || !b_more {
            return !a_more && !b_more;
        }
        let n = (a_buf.len() - a_at).min(b_buf.len() - b_at);
        if a_buf[a_at..a_at + n] != b_buf[b_at..b_at + n] {
            return false;
        }
        a_at += n;
        b_at += n;
    }
}

/// Makes sure `buf[*at..]` holds unread bytes. `Some(false)` at the end of the
/// stream, `None` on a read error.
async fn refill(stream: &mut dyn VolumeReadStream, buf: &mut Vec<u8>, at: &mut usize) -> Option<bool> {
    while *at >= buf.len() {
        match stream.next_chunk().await {
            Some(Ok(chunk)) => {
                *buf = chunk;
                *at = 0;
            }
            Some(Err(_)) => return None,
            None => return Some(false),
        }
    }
    Some(true)
}
