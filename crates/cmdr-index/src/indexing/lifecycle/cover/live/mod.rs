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
//! ## Asking for ground somebody else has
//!
//! A refusal that only NAMES the holder leaves a person waiting on a background
//! walk they can't reach. So a walking holder registers the token that stops it
//! when it takes its ground, and a walk somebody is waiting on can ask for that
//! ground instead of deferring: [`Claim::preempt`].
//!
//! The handover happens in the LEAVING holder's own critical section — the roots
//! it gives up go straight into the waiter's name rather than back to nobody. ❌
//! Never "release, then let the waiter take it again": between those two moments
//! any other claim can arrive, and the waiter that asked for the ground covers
//! nothing while reporting success. That race is what made preemption look
//! impossible.
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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use crate::indexing::paths::path_prefix::{descendant_range_prefix, self_and_ancestors};
use cmdr_fs::ignore_poison::IgnorePoison;

/// The ground held right now, per volume id, and the signal a holder leaving
/// sends. A volume nobody is writing holds no entry.
///
/// The condvar is what a waiting [`Claim::preempt`] sleeps on rather than
/// polling: a holder that hands ground over wakes it inside the same critical
/// section that did the handing.
static IN_FLIGHT: OnceLock<(Mutex<HashMap<String, VolumeClaims>>, Condvar)> = OnceLock::new();

fn table() -> &'static (Mutex<HashMap<String, VolumeClaims>>, Condvar) {
    IN_FLIGHT.get_or_init(|| (Mutex::new(HashMap::new()), Condvar::new()))
}

fn in_flight() -> &'static Mutex<HashMap<String, VolumeClaims>> {
    &table().0
}

/// Tells one waiter's grants from another's. ❌ Not holder identity in the
/// arbitration sense: nothing branches on it, it only says which waiter's row to
/// take back out of the table.
static NEXT_TICKET: AtomicU64 = AtomicU64::new(0);

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

/// Who a cover walk is running for.
///
/// ONE fact with two consequences, which is why it isn't two flags: a walk the
/// index started for itself leaves held ground alone and hands its own over when
/// asked, and a walk somebody is waiting on does the opposite. ⚠️ Two walks that
/// both answer [`TheUser`](WalkFor::TheUser) never stop each other, which is what
/// keeps two searches over one folder from taking turns yielding and neither
/// finishing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WalkFor {
    /// Background coverage: the phase machine filling a drive in, a resumed pass.
    TheIndex,
    /// Somebody is waiting on this walk's answer, so it outranks the background.
    TheUser,
}

/// Who is taking ground, and how to reach them.
///
/// The yield handle lives HERE rather than beside the table because a walking
/// holder without one is a holder nobody can ask to stop — and the type is what
/// makes that unrepresentable, instead of a rule saying to remember it.
#[derive(Debug, Clone)]
pub(in crate::indexing) enum Holder {
    /// A truncating scan or a journal replay: it speaks for the whole volume, and
    /// ❌ is never asked to yield. Its ground isn't being walked, it is being
    /// rewritten, and there is nothing to hand over half way.
    Rewriting,
    /// A cover walk over the ground it names.
    Walking {
        /// Stops THIS walk and nothing above it, so ground can change hands
        /// without the volume's own work stopping. ⚠️ `cover::start` derives it
        /// from the caller's token for exactly that reason.
        yield_to: CancellationToken,
        /// Whether it may be asked to hand its ground over.
        for_whom: WalkFor,
        /// The walk's own [`WalkHeartbeat`](crate::indexing::scanner::WalkHeartbeat)
        /// count of directory reads STARTED, so somebody waiting on this ground
        /// can tell a slow walk from a stopped one ([`walk_pulse`]).
        dirs_scanned: Arc<AtomicU64>,
    },
}

impl Holder {
    /// A cover walk holding ground, with the pulse whoever waits on it reads.
    pub(in crate::indexing) fn walking(
        yield_to: CancellationToken,
        for_whom: WalkFor,
        dirs_scanned: Arc<AtomicU64>,
    ) -> Self {
        Holder::Walking {
            yield_to,
            for_whom,
            dirs_scanned,
        }
    }
    /// How much of the volume this holder speaks for.
    fn mode(&self) -> Mode {
        match self {
            Holder::Rewriting => Mode::Exclusive,
            Holder::Walking { .. } => Mode::Additive,
        }
    }

    /// A cover walk whose pulse never moves, for the tests that care about the
    /// ground or the yield rather than about progress.
    #[cfg(test)]
    pub(in crate::indexing) fn a_walk(yield_to: CancellationToken, for_whom: WalkFor) -> Self {
        Holder::walking(yield_to, for_whom, Arc::new(AtomicU64::new(0)))
    }

    /// A background cover walk holding ground nobody has asked it for, for the
    /// tests that care about the ground rather than about who yields.
    #[cfg(test)]
    pub(in crate::indexing) fn a_background_walk() -> Self {
        Holder::a_walk(CancellationToken::new(), WalkFor::TheIndex)
    }
}

/// What one volume's ground arbitration holds: who is writing it right now,
/// whether anybody is waiting for all of them to leave, and whoever has asked a
/// walking holder to hand its ground over.
#[derive(Default)]
struct VolumeClaims {
    /// Every root somebody holds, and who holds it. Keys are unique across
    /// holders: a root is granted only when nothing overlapping it is held, and a
    /// path overlaps itself.
    roots: BTreeMap<String, Holder>,
    /// How many of `roots` are [`Mode::Exclusive`], so "is this whole volume
    /// spoken for" is a counter read rather than a scan.
    exclusive: usize,
    /// Whether a manual "Rescan now" is waiting for this volume's ground.
    ///
    /// One bit, not a queue: the request carries nothing but "this volume wants a
    /// full walk", so a second click describes the same work, and a queue would
    /// mean one truncating rebuild per click.
    owes_a_rescan: bool,
    /// The waiters that asked a walking holder for its ground, still being served.
    /// A holder leaving hands what it gave up to these before anything else can
    /// take it.
    handovers: Vec<Handover>,
}

/// Which holders an overlap question is asked about. Every holder passes unless a
/// caller narrows it.
fn any_holder(_: &Holder) -> bool {
    true
}

/// Only the holders that are WALKING the ground they name. What a caller asking
/// "is somebody covering this" means, and what a [`Holder::Rewriting`] — which
/// speaks for a whole volume it may not be walking a step of — must not answer.
fn walking_holder(holder: &Holder) -> bool {
    matches!(holder, Holder::Walking { .. })
}

/// Only the walks that can be asked to hand their ground over: background
/// coverage, never a walk somebody is already waiting on.
fn yields_to_a_waiting_person(holder: &Holder) -> bool {
    matches!(
        holder,
        Holder::Walking {
            for_whom: WalkFor::TheIndex,
            ..
        }
    )
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
    fn overlapping_holder(&self, root: &str, keep: fn(&Holder) -> bool) -> bool {
        self_and_ancestors(root).any(|candidate| self.roots.get(candidate).is_some_and(keep))
            || self.holds_under(root, keep)
    }

    /// Whether anything strictly under `root` is held by a holder `keep` accepts.
    /// One sorted range, stopped at the first key outside the prefix.
    fn holds_under(&self, root: &str, keep: fn(&Holder) -> bool) -> bool {
        let prefix = descendant_range_prefix(root);
        // ⚠️ The root is its OWN range prefix, so without the length test a claim
        // at the volume root reads as holding ground under itself. Every strict
        // descendant's key is longer than the path it sits under.
        let self_len = root.len();
        self.roots
            .range(prefix.clone()..)
            .take_while(|(key, _)| key.starts_with(&prefix))
            .any(|(key, holder)| key.len() > self_len && keep(holder))
    }

    /// Every holder whose ground overlaps `root`, by the same rule in the same
    /// two directions. What [`overlapping_holder`](Self::overlapping_holder)
    /// answers yes-or-no, enumerated, for the caller that needs to reach them.
    fn holders_overlapping(&self, root: &str) -> Vec<&Holder> {
        let prefix = descendant_range_prefix(root);
        let self_len = root.len();
        self_and_ancestors(root)
            .filter_map(|candidate| self.roots.get(candidate))
            .chain(
                self.roots
                    .range(prefix.clone()..)
                    .take_while(|(key, _)| key.starts_with(&prefix))
                    .filter(|(key, _)| key.len() > self_len)
                    .map(|(_, holder)| holder),
            )
            .collect()
    }

    fn insert(&mut self, root: String, holder: Holder) {
        if holder.mode() == Mode::Exclusive {
            self.exclusive += 1;
        }
        self.roots.insert(root, holder);
    }

    fn remove(&mut self, root: &str) {
        if self.roots.remove(root).map(|holder| holder.mode()) == Some(Mode::Exclusive) {
            self.exclusive -= 1;
        }
    }

    /// What kind of holder a claim that got nothing was turned away by, read from
    /// the table as it stands.
    ///
    /// A [`Holder::Rewriting`] can only ever be on the volume alone (any holder
    /// refuses one, and it refuses everyone), so this reads a table that is all
    /// one kind or the other.
    fn in_the_way(&self) -> Option<Mode> {
        if self.exclusive > 0 {
            Some(Mode::Exclusive)
        } else if self.roots.is_empty() {
            None
        } else {
            Some(Mode::Additive)
        }
    }

    /// Nothing held, nobody waiting, and nobody part way through being handed
    /// ground, so the volume's whole entry can go rather than growing a map for
    /// the life of the process. ❌ Not `roots.is_empty()`: a request is recorded
    /// BEFORE its scan tries to start, which is routinely a moment when nobody
    /// holds anything, and a waiter part way through a handover holds nothing yet
    /// either.
    fn is_idle(&self) -> bool {
        self.roots.is_empty() && !self.owes_a_rescan && self.handovers.is_empty()
    }
}

/// What became of one root a waiter asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fate {
    /// Somebody else still holds it, and it may yet be handed over.
    Pending,
    /// It is the waiter's, and already in the table under the waiter's name.
    Granted,
    /// It is not coming: another holder kept it, or one of this waiter's own
    /// roots already covers it.
    Deferred,
}

/// One waiter part way through being handed ground it asked a walking holder for.
///
/// It sits in the volume's own entry so a holder leaving can serve it without
/// leaving the lock. `roots` keeps the order the caller asked in, so what the
/// claim reports back reads the same way its frontier did.
struct Handover {
    ticket: u64,
    holder: Holder,
    roots: Vec<(String, Fate)>,
}

impl Handover {
    /// Whether this waiter is still hoping for ground somebody might hand over.
    /// A root nothing but a rewrite is sitting on will never come, so it doesn't
    /// count: waiting for a truncating scan to finish is not preemption.
    fn still_hoping(&self, claimed: &VolumeClaims) -> bool {
        self.roots
            .iter()
            .any(|(root, fate)| *fate == Fate::Pending && claimed.overlapping_holder(root, yields_to_a_waiting_person))
    }
}

/// Whether the waiter holding `ticket` has ground anybody might still hand it.
///
/// `false` the moment its volume's entry, its own row, or the last yielding
/// holder over its pending roots is gone — each of which means waiting longer
/// buys nothing, so it is the whole of what [`Claim::preempt`]'s wait sleeps on.
fn a_waiter_is_still_hoping(live: &HashMap<String, VolumeClaims>, volume_id: &str, ticket: u64) -> bool {
    live.get(volume_id).is_some_and(|claimed| {
        claimed
            .handovers
            .iter()
            .find(|waiter| waiter.ticket == ticket)
            .is_some_and(|waiter| waiter.still_hoping(claimed))
    })
}

/// Hand freed ground to whoever asked for it, before anything else can take it.
///
/// Called from inside the leaving holder's own critical section, which is the
/// whole point: a root goes straight from the holder that gave it up into the
/// waiter's name, so no claim arriving in between can take it. ❌ Never
/// "release, then let the waiter ask again" — that gap is what made preemption
/// look impossible.
fn hand_over_freed_ground(claimed: &mut VolumeClaims) {
    // Out and back so the grants can be written into the table the waiters are
    // being checked against; they are the same structure.
    let mut handovers = std::mem::take(&mut claimed.handovers);
    for waiter in &mut handovers {
        for (root, fate) in &mut waiter.roots {
            if *fate != Fate::Pending || claimed.overlapping(root) {
                continue;
            }
            claimed.insert(root.clone(), waiter.holder.clone());
            *fate = Fate::Granted;
        }
    }
    claimed.handovers = handovers;
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
    ///
    /// ❌ Never waits. A background walk that queued behind a user's would stop
    /// making progress the moment somebody kept searching; [`preempt`](Self::preempt)
    /// is the door for the walk that may.
    pub(in crate::indexing) fn take(volume_id: &str, frontier: Vec<String>, holder: Holder) -> Self {
        let mut live = in_flight().lock_ignore_poison();
        let claimed = live.entry(volume_id.to_string()).or_default();

        // ⚠️ Read BEFORE this claim takes anything, so the volume-wide rule is
        // about OTHER holders. Asked per root instead, an `Exclusive` claim over
        // several roots would refuse its own second root the moment its first
        // landed.
        let volume_is_spoken_for =
            !claimed.roots.is_empty() && (holder.mode() == Mode::Exclusive || claimed.exclusive > 0);

        // And WHAT KIND those other holders are, read at the same moment and for
        // the same reason: a property of the table BEFORE this call, which is what
        // keeps a claim's own roots out of the answer.
        let in_the_way = claimed.in_the_way();

        let mut mine = Vec::with_capacity(frontier.len());
        let mut deferred = Vec::new();
        for root in frontier {
            if volume_is_spoken_for || claimed.overlapping(&root) {
                deferred.push(root);
            } else {
                claimed.insert(root.clone(), holder.clone());
                mine.push(root);
            }
        }

        if claimed.is_idle() {
            live.remove(volume_id);
        }
        Self::report(volume_id, mine, deferred, in_the_way)
    }

    /// Take `frontier`, asking whoever is walking that ground to hand it over.
    ///
    /// For the walk somebody is waiting on. Every background walk over ground this
    /// frontier names is asked to stop, and the roots they let go of come straight
    /// to this claim inside the leaving holder's own critical section. Waits up to
    /// `wait_for` for that to happen, then takes whatever is free and reports the
    /// rest as deferred — so a holder that won't stop costs a bounded wait and the
    /// answer a plain [`take`](Self::take) would have given.
    ///
    /// ⚠️ It blocks the calling thread, which is why the caller is `cover::start`
    /// on a search's own thread and ❌ never the phase machine's driver.
    pub(in crate::indexing) fn preempt(
        volume_id: &str,
        frontier: Vec<String>,
        holder: Holder,
        wait_for: Duration,
    ) -> Self {
        let (live, freed) = table();
        let mut guard = live.lock_ignore_poison();
        let ticket = NEXT_TICKET.fetch_add(1, Ordering::Relaxed);
        let claimed = guard.entry(volume_id.to_string()).or_default();

        // A frontier that overlaps ITSELF is settled before anyone is asked for
        // anything: those roots are this claim's own, and no holder can hand over
        // what this walk is about to cover from its own ancestor.
        let mut own = VolumeClaims::default();
        let roots: Vec<(String, Fate)> = frontier
            .into_iter()
            .map(|root| {
                if own.overlapping(&root) {
                    return (root, Fate::Deferred);
                }
                own.insert(root.clone(), holder.clone());
                (root, Fate::Pending)
            })
            .collect();

        claimed.handovers.push(Handover { ticket, holder, roots });
        // Whatever is free right now is this claim's immediately; only the rest is
        // worth asking anyone for.
        hand_over_freed_ground(claimed);
        ask_the_background_walks_to_yield(claimed, ticket);

        let deadline = Instant::now() + wait_for;
        while a_waiter_is_still_hoping(&guard, volume_id, ticket) {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                log::debug!(
                    "Cover: '{volume_id}' didn't hand its ground over within {wait_for:.1?}, so this walk takes what it can"
                );
                break;
            }
            let (next, _) = freed
                .wait_timeout(guard, left)
                .expect("the claim table's handover condvar");
            guard = next;
        }

        // Collect what the handover was served, and stop being a waiter in the
        // same breath — a row nobody is reading would hold the volume's entry open
        // and take ground the next holder should get.
        let mut mine = Vec::new();
        let mut deferred = Vec::new();
        let mut in_the_way = None;
        if let Some(claimed) = guard.get_mut(volume_id) {
            if let Some(index) = claimed.handovers.iter().position(|w| w.ticket == ticket) {
                for (root, fate) in claimed.handovers.remove(index).roots {
                    match fate {
                        Fate::Granted => mine.push(root),
                        Fate::Pending | Fate::Deferred => deferred.push(root),
                    }
                }
            }
            // Read once the waiter's own grants are the only thing this claim
            // holds, so a refusal names another holder and never itself.
            if mine.is_empty() {
                in_the_way = claimed.in_the_way();
            }
            if claimed.is_idle() {
                guard.remove(volume_id);
            }
        }
        Self::report(volume_id, mine, deferred, in_the_way)
    }

    /// What a finished take reports, however it got there.
    ///
    /// A claim that took nothing was turned away by whoever was already here: the
    /// FIRST root of a frontier can't be refused by roots this same call took,
    /// since it took none yet. That's what makes the answer exact rather than
    /// "somebody else is on the volume".
    fn report(volume_id: &str, mine: Vec<String>, deferred: Vec<String>, in_the_way: Option<Mode>) -> Self {
        if !deferred.is_empty() {
            log::debug!(
                "Cover: leaving {} frontier root(s) on '{volume_id}' to the holder already covering them",
                deferred.len()
            );
        }
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
        let (live, freed) = table();
        let mut guard = live.lock_ignore_poison();
        if let Some(claimed) = guard.get_mut(&self.volume_id) {
            for root in &self.mine {
                claimed.remove(root);
            }
            // Before anything else can ask for it. See `hand_over_freed_ground`.
            hand_over_freed_ground(claimed);
            if claimed.is_idle() {
                guard.remove(&self.volume_id);
            }
        }
        drop(guard);
        freed.notify_all();
    }
}

/// Ask every background walk sitting on ground this waiter is still pending on to
/// stop.
///
/// ⚠️ Only [`WalkFor::TheIndex`] holders are asked: a walk somebody is waiting on
/// outranks nothing, and two of them asking each other would take turns stopping
/// and neither would cover its ground. A [`Holder::Rewriting`] is never asked at
/// all — it is blanking the volume, and half a truncate is not a thing to hand
/// over.
fn ask_the_background_walks_to_yield(claimed: &VolumeClaims, ticket: u64) {
    let Some(waiter) = claimed.handovers.iter().find(|w| w.ticket == ticket) else {
        return;
    };
    let mut asked = 0;
    for (root, fate) in &waiter.roots {
        if *fate != Fate::Pending {
            continue;
        }
        for holder in claimed.holders_overlapping(root) {
            if let Holder::Walking {
                yield_to,
                for_whom: WalkFor::TheIndex,
                ..
            } = holder
            {
                if !yield_to.is_cancelled() {
                    asked += 1;
                }
                yield_to.cancel();
            }
        }
    }
    if asked > 0 {
        log::debug!("Cover: asked {asked} background walk(s) to hand their ground to a walk somebody is waiting on");
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

/// Whether somebody is part way through being handed ground on this volume.
///
/// Test-only, and the one moment a preemption test has to line itself up against:
/// between "the holder has been asked" and "the holder has let go" there is
/// nothing else observable, by design — the handover happens inside the release's
/// own critical section.
#[cfg(test)]
pub(in crate::indexing) fn somebody_is_asking_for_ground(volume_id: &str) -> bool {
    in_flight()
        .lock_ignore_poison()
        .get(volume_id)
        .is_some_and(|claimed| !claimed.handovers.is_empty())
}

/// Which of `frontier`'s roots a WALK on this volume is covering RIGHT NOW.
///
/// The same overlap rule [`Claim::take`] would apply, asked without taking
/// anything — so a caller can find out that the ground it wants is spoken for
/// before it commits to a walk that would take none of it. ❌ Not a reservation
/// and not a promise: the answer can go stale the moment it's read, which is why
/// `Claim::take` stays the authority and reports what it left behind.
///
/// ⚠️ Only [`Holder::Walking`] answers, and that's what makes the question honest
/// rather than a proxy for "is anything held". A scan takes the volume root as a
/// [`Holder::Rewriting`], so an unfiltered answer would name every root of every
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

/// How much walking the walks holding `frontier` have done, as one number.
///
/// ⚠️ It means something only by CHANGING. The value is a sum of per-walk
/// counters, so it says nothing on its own: a caller reads it twice and learns
/// whether ANY walk over its ground started a directory read in between. A walk
/// that is merely slow keeps moving it (a cover walk keeps up to 64 listings in
/// flight, so a hung directory doesn't stop the others starting); one whose
/// concurrency has collapsed onto a dead mount doesn't. ❌ Never show it: it isn't
/// a folder count, and it drops when a walk lets go.
///
/// 0 when nothing is walking that ground, which is also what a walk that has
/// started no read yet reports — harmless, because the caller only compares
/// readings, and [`ground_being_walked`] is what says whether anyone is there.
pub(in crate::indexing) fn walk_pulse(volume_id: &str, frontier: &[String]) -> u64 {
    let live = in_flight().lock_ignore_poison();
    let Some(claimed) = live.get(volume_id) else {
        return 0;
    };
    // One walk holds one counter however many roots it took, so the same `Arc`
    // comes back once per root it holds. Counted once: a pulse that jumped with
    // the SHAPE of a frontier rather than with the walking would read as progress
    // nobody made.
    let mut counted: Vec<*const AtomicU64> = Vec::new();
    let mut pulse = 0;
    for root in frontier {
        for holder in claimed.holders_overlapping(root) {
            let Holder::Walking { dirs_scanned, .. } = holder else {
                continue;
            };
            if counted.contains(&Arc::as_ptr(dirs_scanned)) {
                continue;
            }
            counted.push(Arc::as_ptr(dirs_scanned));
            pulse += dirs_scanned.load(Ordering::Relaxed);
        }
    }
    pulse
}

/// What a claim costs at frontier scale. `#[ignore]`d.
#[cfg(test)]
mod bench;
#[cfg(test)]
mod tests;
