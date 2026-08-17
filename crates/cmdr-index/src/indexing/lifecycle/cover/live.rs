//! One walk per patch of ground.
//!
//! Two searches over the same volume routinely want overlapping frontiers: a
//! refined query re-asks `coverage` while the first query's walk is still
//! running, and Decision 11 keeps that first walk alive. Letting both walk the
//! same directories is a data-safety bug, not a performance one — the two walks
//! allocate different ids for the same names, `insert_entries_v2_batch` is
//! `INSERT OR IGNORE` against `UNIQUE (parent_id, name_folded)`, and whichever
//! row loses takes its whole subtree with it (`scanner/DETAILS.md` § "Three scan
//! roots"). It is the same hazard `lifecycle/state.rs` names for two writers on
//! one database, one level down.
//!
//! So a holder CLAIMS the ground it is about to write, and a later one over
//! ground someone already claimed simply doesn't take it. A truncating scan and a
//! journal replay claim the volume WHOLE (they blank it, or write anywhere on
//! it); a cover walk claims the frontier roots it names. That one table is the
//! single-flight answer both scan entries read: a refusal names the KIND of
//! holder in the way, which is the whole difference between "the walk you asked
//! for is already running" and "wait for the walk holding this ground".
//!
//! A deferred search loses nothing durable: the first walk's rows land in the
//! same index, and Decision 12 makes them visible to the very next query, which
//! is exactly how Decision 11 already says a superseded query recovers the ground
//! its predecessor covered — from the index, never from a replay.
//!
//! ❌ Don't reach for a shared-subscriber fan-out instead. It would give the
//! second search live batches for the shared ground, but it needs per-subscriber
//! filtering and per-subscriber completion (root X is done while the walk moves
//! on to Y and Z), and there is no second consumer today to shape either against.
//!
//! ## What the table is, and why
//!
//! Claims are held path-keyed in a `BTreeMap` per volume, so "does anyone hold
//! ground overlapping this root" is two range questions rather than a scan of
//! everything held: the ancestor chain is a handful of lookups whatever the table
//! holds, and the descendants come out of one sorted range that costs what it
//! yields. ❌ Never a `Vec` scan — a frontier is checked root by root against the
//! roots already taken, so a linear membership test makes ONE `take` quadratic in
//! its own width, and a cold-drive search really does arrive with thousands of
//! roots (`docs/notes/claim-table-cost-2026-08-17.md`).
//!
//! ## And who is waiting for it
//!
//! A volume also remembers ONE thing beyond who holds it: whether somebody asked
//! it for a full walk it couldn't have yet. That waiter lives here rather than in
//! a set of its own because "may a rescan start" is one question about this table
//! — is anything owed, and is the ground free — and two structures answering half
//! of it each can disagree in the window between them. What the request MEANS, and
//! who runs it, is `../rescan_request.rs`; this owns only the fact.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock};

use crate::indexing::paths::path_prefix::{descendant_range_prefix, self_and_ancestors};
use cmdr_fs::ignore_poison::IgnorePoison;

/// The ground held right now, per volume id. A volume nobody is writing holds no
/// entry.
static IN_FLIGHT: OnceLock<Mutex<HashMap<String, VolumeClaims>>> = OnceLock::new();

fn in_flight() -> &'static Mutex<HashMap<String, VolumeClaims>> {
    IN_FLIGHT.get_or_init(|| Mutex::new(HashMap::new()))
}

/// How much of a volume a claim speaks for.
///
/// ⚠️ The two modes are the whole arbitration vocabulary, and deliberately so:
/// ❌ never re-entrancy or holder identity. A refusal says what KIND of holder is
/// in the way, which is all a caller needs to decide what to tell the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::indexing) enum Mode {
    /// The whole volume, whatever ground anyone else names. A truncating scan
    /// blanks the database and bumps the epoch, so every concurrent walk is
    /// writing into a table about to disappear — overlapping ground or not.
    Exclusive,
    /// Only the ground it names. Two of these compose as long as their frontiers
    /// stay off each other, which is what lets a search-driven walk run beside
    /// the phase machine covering the same volume in pieces (Decision 13).
    Additive,
}

/// What one volume's ground arbitration holds: who is writing it right now, and
/// whether anybody is waiting for all of them to leave.
#[derive(Default)]
struct VolumeClaims {
    /// Every root somebody holds, and how much of the volume that holder speaks
    /// for. Keys are unique across holders: a root is granted only when nothing
    /// overlapping it is held, and a path overlaps itself.
    roots: BTreeMap<String, Mode>,
    /// How many of `roots` are [`Mode::Exclusive`], so "is this whole volume
    /// spoken for" is a counter read rather than a scan.
    exclusive: usize,
    /// Whether a manual "Rescan now" is waiting for this volume's ground.
    ///
    /// One bit, not a queue: the request carries nothing but "this volume wants a
    /// full walk", so a second click describes the same work, and a queue would
    /// mean one truncating rebuild per click.
    owes_a_rescan: bool,
}

/// Which holders an overlap question is asked about. Every mode passes unless a
/// caller narrows it.
fn any_holder(_: Mode) -> bool {
    true
}

/// Only the holders that are WALKING the ground they name. What a caller asking
/// "is somebody covering this" means, and what an `Exclusive` holder — which
/// speaks for a whole volume it may not be walking a step of — must not answer.
fn walking_holder(mode: Mode) -> bool {
    mode == Mode::Additive
}

impl VolumeClaims {
    /// Whether any held root would cover `root`'s ground, in either direction.
    ///
    /// A requested root overlaps a held one in EITHER direction — a descendant of
    /// a live root is already being walked, and an ancestor of one would walk
    /// straight through it. The second case shouldn't arise from a coverage
    /// answer (a frontier node's ancestors are listed, or they'd be the frontier
    /// instead), which is exactly why it's handled rather than assumed.
    fn overlapping(&self, root: &str) -> bool {
        self.overlapping_holder(root, any_holder)
    }

    /// The same question asked of some of the holders: whichever `keep` accepts.
    fn overlapping_holder(&self, root: &str, keep: fn(Mode) -> bool) -> bool {
        self_and_ancestors(root).any(|candidate| self.roots.get(candidate).is_some_and(|mode| keep(*mode)))
            || self.holds_under(root, keep)
    }

    /// Whether anything strictly under `root` is held by a holder `keep` accepts.
    /// One sorted range, stopped at the first key outside the prefix.
    fn holds_under(&self, root: &str, keep: fn(Mode) -> bool) -> bool {
        let prefix = descendant_range_prefix(root);
        // ⚠️ The root is its OWN range prefix, so without the length test a claim
        // at the volume root reads as holding ground under itself. Every strict
        // descendant's key is longer than the path it sits under.
        let self_len = root.len();
        self.roots
            .range(prefix.clone()..)
            .take_while(|(key, _)| key.starts_with(&prefix))
            .any(|(key, mode)| key.len() > self_len && keep(*mode))
    }

    fn insert(&mut self, root: String, mode: Mode) {
        if mode == Mode::Exclusive {
            self.exclusive += 1;
        }
        self.roots.insert(root, mode);
    }

    fn remove(&mut self, root: &str) {
        if self.roots.remove(root) == Some(Mode::Exclusive) {
            self.exclusive -= 1;
        }
    }

    /// Nothing held and nobody waiting, so the volume's whole entry can go rather
    /// than growing a map for the life of the process. ❌ Not `roots.is_empty()`:
    /// a request is recorded BEFORE its scan tries to start, which is routinely a
    /// moment when nobody holds anything.
    fn is_idle(&self) -> bool {
        self.roots.is_empty() && !self.owes_a_rescan
    }
}

/// One holder's grip on the ground it may write, released when it ends.
///
/// Whoever holds it owns it for the whole of their work, so the roots free up on
/// the completion path, the cancel path, and a panic alike. A cover walk's lives
/// on its own thread; a scan's and a replay's travel into the task that ends
/// them, since both outlive the call that started them.
pub(in crate::indexing) struct Claim {
    volume_id: String,
    /// The roots this holder took.
    mine: Vec<String>,
    /// The roots it didn't, because another holder on this volume has them.
    deferred: Vec<String>,
    /// What kind of holder was in the way, for a claim that took NOTHING. See
    /// [`Claim::refused_by`].
    refused_by: Option<Mode>,
}

impl Claim {
    /// Split `frontier` into the roots this holder may take and the roots another
    /// one already owns.
    ///
    /// The overlap rule is [`VolumeClaims::overlapping`], and it deduplicates a
    /// frontier that overlaps ITSELF by the same test: each root is checked
    /// against the ones this call has already taken, so one holder can't
    /// double-write its own ground either.
    pub(in crate::indexing) fn take(volume_id: &str, frontier: Vec<String>, mode: Mode) -> Self {
        let mut live = in_flight().lock_ignore_poison();
        let claimed = live.entry(volume_id.to_string()).or_default();

        // ⚠️ Read BEFORE this claim takes anything, so the volume-wide rule is
        // about OTHER holders. Asked per root instead, an `Exclusive` claim over
        // several roots would refuse its own second root the moment its first
        // landed.
        let volume_is_spoken_for = !claimed.roots.is_empty() && (mode == Mode::Exclusive || claimed.exclusive > 0);

        // And WHAT KIND those other holders are, read at the same moment and for
        // the same reason: a property of the table BEFORE this call, which is what
        // keeps a claim's own roots out of the answer. An `Exclusive` holder can
        // only ever be on the volume alone (any holder refuses one, and it refuses
        // everyone), so this reads a table that is all one kind or the other.
        let in_the_way = if claimed.exclusive > 0 {
            Some(Mode::Exclusive)
        } else if claimed.roots.is_empty() {
            None
        } else {
            Some(Mode::Additive)
        };

        let mut mine = Vec::with_capacity(frontier.len());
        let mut deferred = Vec::new();
        for root in frontier {
            if volume_is_spoken_for || claimed.overlapping(&root) {
                deferred.push(root);
            } else {
                claimed.insert(root.clone(), mode);
                mine.push(root);
            }
        }

        if claimed.is_idle() {
            live.remove(volume_id);
        }
        if !deferred.is_empty() {
            log::debug!(
                "Cover: leaving {} frontier root(s) on '{volume_id}' to the holder already covering them",
                deferred.len()
            );
        }
        // A claim that took nothing was turned away by whoever was already here:
        // the FIRST root of a frontier can't be refused by roots this same call
        // took, since it took none yet. That's what makes this exact rather than
        // "somebody else is on the volume".
        let refused_by = if mine.is_empty() && !deferred.is_empty() {
            in_the_way
        } else {
            None
        };
        Self {
            volume_id: volume_id.to_string(),
            mine,
            deferred,
            refused_by,
        }
    }

    /// The roots this holder is covering.
    pub(in crate::indexing) fn mine(&self) -> &[String] {
        &self.mine
    }

    /// The roots it left to another holder.
    pub(in crate::indexing) fn deferred(&self) -> &[String] {
        &self.deferred
    }

    /// What kind of holder turned this claim away, when it got NO ground at all.
    ///
    /// The whole of what a refused caller is told, and deliberately so: a mode
    /// says whether the ground is being blanked ([`Mode::Exclusive`]) or walked
    /// ([`Mode::Additive`]), which is the difference between "the walk you asked
    /// for is already happening" and "wait for the walk that's holding it". ❌ Not
    /// WHICH holder — identity is re-entrancy's vocabulary, and this table
    /// deliberately doesn't speak it.
    ///
    /// `None` whenever the claim took something, a partial grant included: ground
    /// in hand is not a refusal.
    pub(in crate::indexing) fn refused_by(&self) -> Option<Mode> {
        self.refused_by
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        if self.mine.is_empty() {
            return;
        }
        let mut live = in_flight().lock_ignore_poison();
        if let Some(claimed) = live.get_mut(&self.volume_id) {
            for root in &self.mine {
                claimed.remove(root);
            }
            if claimed.is_idle() {
                live.remove(&self.volume_id);
            }
        }
    }
}

// ── The one walk a volume is waiting for ─────────────────────────────

/// Remember that this volume wants a full walk it couldn't have yet. Idempotent:
/// a second click describes the same work.
pub(in crate::indexing) fn remember_rescan(volume_id: &str) {
    in_flight()
        .lock_ignore_poison()
        .entry(volume_id.to_string())
        .or_default()
        .owes_a_rescan = true;
}

/// Take the request, if there is one. Taking is what SPENDING it looks like, so a
/// caller that takes and then can't start has to [`remember_rescan`] again.
pub(in crate::indexing) fn take_rescan(volume_id: &str) -> bool {
    let mut live = in_flight().lock_ignore_poison();
    let Some(claimed) = live.get_mut(volume_id) else {
        return false;
    };
    let owed = std::mem::replace(&mut claimed.owes_a_rescan, false);
    if claimed.is_idle() {
        live.remove(volume_id);
    }
    owed
}

/// Drop what a volume was waiting for: its scan started, or it stopped indexing.
pub(in crate::indexing) fn forget_rescan(volume_id: &str) {
    let _ = take_rescan(volume_id);
}

/// Whether a rescan this volume is owed could actually START right now: somebody
/// is waiting, and nobody holds the ground any more.
///
/// ⚠️ Both halves in ONE look at the table, which is the reason the waiter lives
/// here. Asked as two questions of two structures, a walk that has just let go can
/// answer "the ground is free" while another holder is still on the volume, and
/// the scan that spawns on the strength of it gets refused and re-queued for
/// nothing. ❌ Not a reservation: the answer can go stale the moment it's read, so
/// `force_scan` re-asks for real when it starts.
pub(in crate::indexing) fn a_rescan_can_start(volume_id: &str) -> bool {
    let live = in_flight().lock_ignore_poison();
    live.get(volume_id)
        .is_some_and(|claimed| claimed.owes_a_rescan && claimed.roots.is_empty())
}

/// Which of `frontier`'s roots a WALK on this volume is covering RIGHT NOW.
///
/// The same overlap rule [`Claim::take`] would apply, asked without taking
/// anything — so a caller can find out that the ground it wants is spoken for
/// before it commits to a walk that would take none of it. ❌ Not a reservation
/// and not a promise: the answer can go stale the moment it's read, which is why
/// `Claim::take` stays the authority and reports what it left behind.
///
/// ⚠️ Only [`Mode::Additive`] holders answer, and that's what makes the question
/// honest rather than a proxy for "is anything held". A scan takes the volume
/// root `Exclusive`ly, so an unfiltered answer would name every root of every
/// frontier for as long as one runs — telling a search to wait for a walk that
/// will never cover its ground.
pub(in crate::indexing) fn ground_being_walked(volume_id: &str, frontier: &[String]) -> Vec<String> {
    let live = in_flight().lock_ignore_poison();
    let Some(claimed) = live.get(volume_id) else {
        return Vec::new();
    };
    frontier
        .iter()
        .filter(|root| claimed.overlapping_holder(root, walking_holder))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests;
