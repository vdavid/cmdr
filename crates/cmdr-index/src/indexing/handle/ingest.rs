//! The write side of the index: what the host tells it, rather than what it
//! walks for itself.
//!
//! **These two calls are designed, not implemented.** The bodies report
//! `NotImplemented`. They exist now because the shapes below are expensive to
//! retrofit and nearly free to reserve, and because compiling them is the proof
//! that the handle's shape admits both features without a redesign:
//!
//! - **A listing enriches the index.** Showing a directory already paid for the
//!   syscalls, so feeding the result back lets the index self-correct exactly
//!   where the user is looking. [`ListingObservation`].
//! - **A folder's size, on any volume, indexed or not.** Progressive and
//!   cancelable, with an honest as-of stamp. [`SizeRequest`].
//!
//! Three things here would be painful to add later, so they're in the types now:
//!
//! 1. **An observation is about direct children at a moment.** A listing can
//!    freshen what's visible; it cannot fix a recursive size or notice a deletion
//!    three levels down. [`ListingObservation::observed_at`] plus the
//!    direct-children-only contract keeps "these rows were confirmed at T" from
//!    ever being read as "this subtree was".
//! 2. **Every listing is a free correctness audit.** The caller has just enriched
//!    the entries from the index, so it knows, at no cost, where the index and
//!    the disk disagreed. [`ListingAgreement`] carries that. It cannot be
//!    reconstructed afterwards, and it's the evidence that would one day justify
//!    serving listings from the index.
//! 3. **A volume can be watched for size invalidation without being indexed.**
//!    That's a third state next to indexed and not indexed
//!    ([`SizeRequest::keep_fresh`]), and it's why a persisted size carries
//!    [`SizeProgress::as_of`]: change notification has no coverage on a share or
//!    a phone and can drop history, so a stored total is only ever true as of a
//!    moment.

use std::path::PathBuf;

/// One entry as the host actually saw it on disk.
#[derive(Debug, Clone)]
pub struct ObservedEntry {
    /// The entry's name within the observed directory. Not a path: an
    /// observation is about one directory's direct children.
    pub name: String,
    /// Whether it's a directory.
    pub is_directory: bool,
    /// Logical size in bytes; `None` for a directory or an entry whose size the
    /// host couldn't read.
    pub size: Option<u64>,
    /// Modified time as a Unix timestamp, when the host has one.
    pub modified_at: Option<u64>,
    /// Inode, when the volume has stable ones. Lets a rename be recognized as a
    /// move rather than a delete plus an add.
    pub inode: Option<u64>,
}

/// How the index's rows compared to what the host saw, counted while the listing
/// was being enriched.
///
/// Free to collect (the caller holds both sides at that moment) and impossible to
/// reconstruct later, which is the whole reason it's on the observation.
#[derive(Debug, Clone, Copy, Default)]
pub struct ListingAgreement {
    /// Entries the index already had, with matching facts.
    pub matched: u32,
    /// Entries the index had with different facts (a size or time that moved).
    pub differed: u32,
    /// Entries on disk that the index had no row for.
    pub missing_from_index: u32,
    /// Rows the index held for this directory that the host did not see, so the
    /// index is carrying entries that are gone.
    pub stale_in_index: u32,
}

/// One directory's direct children, as the host saw them at a moment.
///
/// ❌ Never treat this as covering a subtree. It says nothing about anything
/// below the named children, and folding it in must not stamp a recursive size or
/// a subtree's coverage as confirmed.
#[derive(Debug, Clone)]
pub struct ListingObservation {
    /// Which volume's index this belongs to.
    pub volume_id: String,
    /// The absolute path of the directory that was listed.
    pub directory: PathBuf,
    /// Its direct children, as seen. Order is irrelevant.
    pub entries: Vec<ObservedEntry>,
    /// When the host read them, as a Unix timestamp. What "confirmed at T" means
    /// for exactly these rows, and nothing else.
    pub observed_at: u64,
    /// Whether the host enumerated the whole directory. A truncated listing may
    /// freshen the entries it did see but can never imply the rest are gone.
    pub complete: bool,
    /// What the index claimed versus what was there, when the caller could tell.
    /// `None` from a caller that didn't enrich from the index and so has nothing
    /// honest to report.
    pub agreement: Option<ListingAgreement>,
}

/// Why an observation couldn't be taken.
#[derive(Debug)]
pub enum IngestError {
    /// Folding listings back into the index isn't built yet.
    NotImplemented,
    /// No index is registered for the observation's volume, so there's nothing to
    /// correct.
    NotIndexed {
        /// The volume the observation named.
        volume_id: String,
    },
}

impl std::fmt::Display for IngestError {
    /// Diagnostic text for logs; the app renders its own words.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotImplemented => f.write_str("folding listings into the index isn't implemented"),
            Self::NotIndexed { volume_id } => write!(f, "no index registered for volume '{volume_id}'"),
        }
    }
}

impl std::error::Error for IngestError {}

/// How current an answer has to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeFreshness {
    /// Whatever's stored is fine, however old. The stamp on the answer says how
    /// old.
    Stored,
    /// Walk it now. Stored totals are only a starting point to report while the
    /// walk climbs.
    Recomputed,
}

/// Ask for the total size under one path.
#[derive(Debug, Clone)]
pub struct SizeRequest {
    /// Which volume the path is on. It does NOT have to be indexed.
    pub volume_id: String,
    /// The absolute path of the subtree to total up.
    pub path: PathBuf,
    /// How current the answer has to be.
    pub freshness: SizeFreshness,
    /// Keep this subtree's total current afterwards by watching it for changes,
    /// even on a volume that isn't indexed.
    ///
    /// This is the third state a volume can be in: watched for size invalidation
    /// without being indexed. Off by default, because watching costs a resource
    /// per subtree.
    pub keep_fresh: bool,
}

/// How settled a total is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeVerdict {
    /// A walk is running and the number is still climbing.
    Climbing,
    /// The walk finished and saw everything it set out to.
    Settled,
    /// The walk finished but couldn't read part of the subtree, so the total is a
    /// floor, not an answer.
    Partial,
    /// Read from storage without walking, as of the stamp on it.
    Stored,
}

/// One reading of a subtree's total.
#[derive(Debug, Clone, Copy)]
pub struct SizeProgress {
    /// Bytes seen so far.
    pub bytes: u64,
    /// Files seen so far.
    pub files: u64,
    /// Directories seen so far.
    pub directories: u64,
    /// When this total was true, as a Unix timestamp. A stored total is only ever
    /// true as of a moment: change notification has no coverage on a share or a
    /// phone and can drop history.
    pub as_of: u64,
    /// How settled the number is.
    pub verdict: SizeVerdict,
}

/// A climbing total, ending in a settled one.
///
/// Dropping the stream stops the work, the same as cancelling the token that
/// started it.
pub struct SizeStream {
    receiver: tokio::sync::mpsc::Receiver<SizeProgress>,
}

impl SizeStream {
    /// The next reading, or `None` once the total has settled and there's nothing
    /// more to report.
    pub async fn next(&mut self) -> Option<SizeProgress> {
        self.receiver.recv().await
    }
}

impl std::fmt::Debug for SizeStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SizeStream")
    }
}

/// Why a subtree's size couldn't be totalled.
#[derive(Debug)]
pub enum SizeError {
    /// Totalling a subtree on demand isn't built yet.
    NotImplemented,
    /// Nothing is mounted under the request's volume, so the path can't be
    /// reached.
    VolumeUnavailable {
        /// The volume the request named.
        volume_id: String,
    },
    /// The path isn't there.
    NotFound {
        /// The path that wasn't there.
        path: PathBuf,
    },
    /// The walk was stopped before it finished. Distinct from a settled answer:
    /// a cancelled total is not a total.
    Cancelled,
}

impl std::fmt::Display for SizeError {
    /// Diagnostic text for logs; the app renders its own words.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotImplemented => f.write_str("on-demand subtree sizes aren't implemented"),
            Self::VolumeUnavailable { volume_id } => write!(f, "nothing mounted for volume '{volume_id}'"),
            Self::NotFound { path } => write!(f, "no such path: {}", path.display()),
            Self::Cancelled => f.write_str("the size walk was stopped"),
        }
    }
}

impl std::error::Error for SizeError {}

impl super::Index {
    /// Fold a directory listing the host already performed back into the index,
    /// so the index self-corrects exactly where the user is looking.
    ///
    /// Takes ownership and returns immediately; it never touches a database lock
    /// on the caller's thread, because the caller is the listing hot path. Under
    /// pressure it drops the oldest queued batch rather than making the listing
    /// wait.
    ///
    /// Covers the named directory's direct children only. See the module docs.
    ///
    /// **Not implemented yet**: reports [`IngestError::NotImplemented`]. The shape
    /// is settled and compiled against so that building it changes no caller.
    pub fn observe_listing(&self, observation: ListingObservation) -> Result<(), IngestError> {
        let _ = observation;
        Err(IngestError::NotImplemented)
    }

    /// Total up everything under one path, on any volume, indexed or not.
    ///
    /// Reports a climbing total as it goes and stops the moment `cancel` fires.
    /// The stream ends with a [`SizeVerdict`] that says whether the number is an
    /// answer, a floor, or something read from storage as of a stamp.
    ///
    /// **Not implemented yet**: reports [`SizeError::NotImplemented`]. The shape
    /// is settled and compiled against so that building it changes no caller.
    pub fn size_of(
        &self,
        request: SizeRequest,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<SizeStream, SizeError> {
        let _ = (request, cancel);
        Err(SizeError::NotImplemented)
    }
}
