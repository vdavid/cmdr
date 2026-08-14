//! What a scope still needs walked before the index alone can answer for it.
//!
//! A search over one volume has two halves: the ground the index already covers,
//! which the arena answers for free, and the ground it doesn't, which somebody has
//! to walk. This module answers the second half — the **frontier** — and nothing
//! else. The covered half is never enumerated: the tree is partitioned, so running
//! the query engine over the scope unfiltered already yields exactly the covered
//! rows, and a list of covered subtrees would be a second, weaker copy of that.
//!
//! ## The descent rule
//!
//! Both epoch fields are load-bearing, plus the `unreadable_cause` marker.
//! Descending from the scope root, each directory is exactly one of:
//!
//! - `min_subtree_epoch > 0` ⇒ **covered**. Serve from the index; don't descend.
//! - `min_subtree_epoch == 0 && listed_epoch > 0` ⇒ **listed**. This directory was
//!   read, something below it wasn't. It is itself covered ground; descend into
//!   its child directories and classify each.
//! - `listed_epoch == 0 && unreadable_cause != 0` ⇒ **unreadable**. Nothing is
//!   coming for this subtree right now, and the cause says which kind of nothing:
//!   a walk tried and was refused (permission denied), no walk will read it at all
//!   (a NAS snapshot directory, whose per-snapshot tree is the one thing the
//!   network scanner refuses on purpose), or a walk tried and gave up (a wedged
//!   mount, a vanished directory). Not frontier, reported rather than silently
//!   dropped, and reported in three lists rather than one — they reach the user as
//!   different sentences, and only the first is something they can act on.
//! - `listed_epoch == 0` ⇒ **frontier**. Cut here and hand the subtree to the walk.
//!
//! ❌ **Don't simplify this to `min_subtree_epoch` alone.** The min absorbs zero
//! upward, so one uncovered directory anywhere forces `0` on every ancestor
//! including the scope root: "the shallowest node at zero" is always the scope
//! root, and the frontier degenerates to "walk everything". Two drafts of this
//! design got that wrong. Note that the partition property alone does NOT catch
//! it — "the scope root is the whole frontier" is a perfectly valid partition, and
//! a useless answer. What catches it is `every_verdict_matches_its_directory`:
//! a frontier cut must be a directory nothing has listed.
//!
//! The four cases are disjoint and exhaustive because `min_subtree_epoch > 0`
//! implies `listed_epoch > 0`: every writer of the column starts from the
//! directory's own `listed_epoch` and 0-absorbs from there
//! (`store::recompute_min_subtree_epoch`'s `own == 0` early return, and
//! `aggregator::compute_bottom_up`'s seed). `min_subtree_epoch_implies_listed`
//! pins that premise against the real aggregator.
//!
//! ## Exclusions aren't this module's problem
//!
//! A policy-excluded child gets no `entries` row at all, and the child scan behind
//! `min_subtree_epoch` can't see a row that doesn't exist, so an excluded
//! directory drives nothing to zero and needs no case here. What that DOES need is
//! a guarantee that the rows were written under the policy this build applies,
//! which is `store::EXCLUSION_POLICY_KEY`: a stamp that doesn't match means every
//! coverage claim in the database is unknown, and the whole scope goes to the walk.

use cmdr_fs::firmlinks;
use rusqlite::{Connection, OptionalExtension, params};

use super::enrichment::get_read_pool_for;
use crate::indexing::paths::routing::index_read_path;
use crate::indexing::scanner::index_predates_exclusion_policy;
use crate::indexing::store::{IndexStore, IndexStoreError, UnreadableCause, resolve_path};

/// How deep the descent will follow the tree before it stops trusting it.
///
/// Far past any real path (macOS tops out around 100 components), so hitting it
/// means the `parent_id` chain has a cycle — corruption a user-triggered query
/// must not hang on. The node at the cap is reported as frontier, which is the
/// conservative answer: worst case somebody walks ground that was already covered.
const MAX_DESCENT_DEPTH: usize = 256;

/// Which kind of coverage a question is about.
///
/// One variant today, and the parameter exists anyway: content search will ask the
/// same question in a second dimension (a `content_epoch` sibling to
/// `listed_epoch`, propagated with the same 0-absorbing min), and the walk stages
/// that fall out of it — path-covered and content-covered, path-covered but
/// content-uncovered, path-uncovered — only work if callers were never written
/// against a single implied dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageDimension {
    /// Whether a directory's direct contents have been listed. What every walk
    /// writes today and what search's file-name matching needs.
    Listing,
}

/// Which state of a volume's index a coverage answer describes.
///
/// Opaque and comparable only for equality, because that's the only question worth
/// asking: a caller takes one when it loads a snapshot of the index, takes another
/// with its coverage answer, and re-reads when the two stop matching. Two answers
/// carrying equal tokens were computed against the same rows.
///
/// It's the volume's epoch paired with the highest entry id the database has
/// handed out. Ids come from one monotonic per-volume counter, so any walk that
/// writes rows moves the pair, and both halves cost an index seek rather than a
/// scan. A volume with no index at all reports [`CoverageToken::UNINDEXED`].
///
/// **It's a watermark, not a version.** Deleting the highest-id row lowers it, so
/// an unequal token means "something changed", never "this one is newer", and a
/// delete-then-refill back to the same id at the same epoch reads as unchanged.
/// The narrow window that reaches is written up in `read/DETAILS.md`
/// § "The freshness token"; ❌ don't order two tokens or treat one as a clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverageToken {
    epoch: u64,
    high_water_id: i64,
}

impl CoverageToken {
    /// The token for a volume with no index: nothing is covered, and any index
    /// that later appears carries a different one.
    pub const UNINDEXED: Self = Self {
        epoch: 0,
        high_water_id: 0,
    };

    /// Read the current token off an open index connection.
    pub(crate) fn read(conn: &Connection) -> Result<Self, IndexStoreError> {
        Ok(Self {
            epoch: IndexStore::read_current_epoch(conn)?,
            high_water_id: IndexStore::read_high_water_id(conn)?,
        })
    }
}

/// What a scope still needs before the index alone can answer for it.
///
/// The covered half is deliberately absent: the two are complementary over the
/// same subtree, so a caller runs its query over the scope unfiltered to get the
/// covered rows and walks [`frontier`](Self::frontier) for the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageMap {
    /// The shallowest directories nothing has listed, as absolute paths in the
    /// same space as the scope that was asked about. Their subtrees are uncovered
    /// ground in full; nothing under one is covered, so a walk takes each whole.
    /// Empty means the scope is covered end to end. **Unordered** — the descent
    /// emits them as it finds them, and no caller should read the order as meaning
    /// anything.
    pub frontier: Vec<String>,
    /// Directories a walk tried to read and was REFUSED (permission denied), as
    /// absolute paths. Not offered again, and reported rather than dropped: a
    /// search over them is honestly narrow, and this is the half the user can act
    /// on — on macOS, granting Full Disk Access and searching again heals it,
    /// because the successful listing clears the mark.
    pub permission_denied: Vec<String>,
    /// Directories no walk is going to read at all, by Cmdr's own choice: a NAS
    /// snapshot tree, whose per-snapshot hardlinked copies both whole-volume
    /// scanners refuse on purpose. Nothing for the user to fix; it's here so a
    /// short answer can say why it's short. ❌ Don't merge it into
    /// [`permission_denied`](Self::permission_denied): "grant Full Disk Access"
    /// over a snapshot folder is advice that does nothing.
    pub declined: Vec<String>,
    /// Directories a walk tried and gave up on: a read that timed out, one that
    /// failed with an errno that isn't permission denied, or a task the walker's
    /// consecutive-failure budget pruned unread. The TEMPORARY half, and the
    /// reason it has a list of its own rather than joining either neighbour:
    /// nothing here is the user's to fix (so ❌ never
    /// [`permission_denied`](Self::permission_denied), which offers Full Disk
    /// Access), and Cmdr WILL come back to it (so ❌ never
    /// [`declined`](Self::declined), which is a permanent policy). The retry rides
    /// a persisted per-volume backoff (`writer/abandoned_retry.rs`), and any
    /// successful listing clears the cause on the spot.
    ///
    /// ⚠️ A caller reporting how complete its answer is has to consult this: these
    /// subtrees are no longer in [`frontier`](Self::frontier), so nothing else in
    /// the answer hints that they were skipped.
    pub abandoned: Vec<String>,
    /// Which state of the index this answer describes. Honor the answer only while
    /// the snapshot you're serving the covered half from still matches.
    pub token: CoverageToken,
    /// The [`frontier`](Self::frontier) roots a walk on this volume is covering
    /// right now, so a caller can tell "nobody has been here" from "somebody is
    /// here already". Only one walk may have a patch of ground (two allocate
    /// different ids for the same names and orphan each other's subtrees), so a
    /// caller whose whole frontier is listed here has nothing to walk: waiting for
    /// that walk is what gets it an answer, where walking anyway would corrupt one.
    /// ⚠️ A reading, not a reservation: it can go stale immediately, and the walk
    /// request stays the authority on what a walk actually took.
    pub being_walked: Vec<String>,
}

/// One directory's verdict during the descent.
///
/// [`Listed`](Self::Listed) is the interior case: the directory itself is covered
/// ground, and its children each get their own verdict. The other three are cuts —
/// the whole subtree gets the verdict and the descent stops. Together they
/// partition the scope's directories, which is what makes "the index answers the
/// rest" true rather than hopeful.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// The subtree is covered end to end. Serve it from the index.
    Covered,
    /// This directory was listed but something below it wasn't. Covered itself;
    /// its children are classified individually.
    Listed,
    /// Nothing has listed this directory. The whole subtree goes to the walk.
    Frontier,
    /// Nothing is coming for this directory, and the cause says which kind of
    /// nothing: a walk was refused, or no walk will read it at all.
    Unreadable(UnreadableCause),
}

/// One directory's coverage state, as the descent reads it.
struct DirCoverage {
    id: i64,
    listed_epoch: u64,
    unreadable_cause: Option<UnreadableCause>,
    min_subtree_epoch: u64,
}

impl DirCoverage {
    /// The descent rule, applied to one directory. See the module docs.
    fn verdict(&self) -> Verdict {
        if self.min_subtree_epoch > 0 {
            return Verdict::Covered;
        }
        if self.listed_epoch > 0 {
            return Verdict::Listed;
        }
        if let Some(cause) = self.unreadable_cause {
            return Verdict::Unreadable(cause);
        }
        Verdict::Frontier
    }
}

/// The frontier for one scope on one volume.
///
/// A volume with no registered index reports the scope itself as the whole
/// frontier and [`CoverageToken::UNINDEXED`]: nothing is covered, which is the
/// honest answer and exactly what a cold drive needs. A scope that isn't on the
/// volume at all reports the same, since the index can't speak for it either.
///
/// Paths come back firmlink-normalized (`/tmp` reads as `/private/tmp`), because
/// that's the space the index stores and the walk has to work in.
pub(crate) fn coverage_on_volume(
    volume_id: &str,
    scope_path: &str,
    dimension: CoverageDimension,
) -> Result<CoverageMap, String> {
    let normalized = firmlinks::normalize_path(scope_path);
    let uncovered = || CoverageMap {
        frontier: vec![normalized.clone()],
        permission_denied: Vec::new(),
        declined: Vec::new(),
        abandoned: Vec::new(),
        token: CoverageToken::UNINDEXED,
        being_walked: Vec::new(),
    };

    let Some(pool) = get_read_pool_for(volume_id) else {
        return Ok(uncovered());
    };
    let Some(index_path) = index_read_path(volume_id, &normalized) else {
        return Ok(uncovered());
    };
    pool.with_conn(|conn| {
        coverage_for_scope(conn, &index_path, &normalized, dimension)
            .map_err(|e| format!("Couldn't read coverage for '{normalized}': {e}"))
    })?
}

/// Which state of a volume's index is current right now, so a caller can take one
/// alongside the snapshot it's loading and compare it against a coverage answer's.
/// [`CoverageToken::UNINDEXED`] when the volume has no index.
pub(crate) fn coverage_token_on_volume(volume_id: &str) -> CoverageToken {
    let Some(pool) = get_read_pool_for(volume_id) else {
        return CoverageToken::UNINDEXED;
    };
    pool.with_conn(CoverageToken::read)
        .ok()
        .and_then(|inner| inner.ok())
        .unwrap_or(CoverageToken::UNINDEXED)
}

/// The frontier for one scope, read off an open index connection.
///
/// `scope_index_path` is the scope in the volume's own index path space (what
/// `paths::routing::index_read_path` produces); `scope_path` is the same folder as
/// the caller named it, and every path in the answer is built from it, so the
/// answer comes back in the space the caller asked in.
pub(crate) fn coverage_for_scope(
    conn: &Connection,
    scope_index_path: &str,
    scope_path: &str,
    dimension: CoverageDimension,
) -> Result<CoverageMap, IndexStoreError> {
    // Deliberately an irrefutable `let` rather than an ignored parameter: adding a
    // second dimension makes this a compile error at every place that has to grow a
    // case, which is the whole reason the parameter exists this early.
    let CoverageDimension::Listing = dimension;
    let mut frontier = Vec::new();
    let mut permission_denied = Vec::new();
    let mut declined = Vec::new();
    let mut abandoned = Vec::new();
    let token = walk_coverage(conn, scope_index_path, scope_path, &mut |verdict, path| match verdict {
        Verdict::Frontier => frontier.push(path.to_string()),
        Verdict::Unreadable(UnreadableCause::Denied) => permission_denied.push(path.to_string()),
        Verdict::Unreadable(UnreadableCause::Declined) => declined.push(path.to_string()),
        Verdict::Unreadable(UnreadableCause::Abandoned) => abandoned.push(path.to_string()),
        Verdict::Covered | Verdict::Listed => {}
    })?;
    Ok(CoverageMap {
        frontier,
        permission_denied,
        declined,
        abandoned,
        token,
        // Filled by `Index::coverage`, which sits above the walks: this half is a
        // read of one database, and who is walking right now is process state.
        being_walked: Vec::new(),
    })
}

/// The descent itself, reporting every directory it classifies.
///
/// Separate from [`coverage_for_scope`] because the partition property is over ALL
/// four verdicts and the public answer carries two of them: a test can watch the
/// covered cuts go by without the query paying to build their paths in production.
///
/// The whole read runs in one deferred transaction, so the frontier and the token
/// describe the same database state rather than two states either side of a
/// committing writer.
pub(crate) fn walk_coverage(
    conn: &Connection,
    scope_index_path: &str,
    scope_path: &str,
    on_verdict: &mut impl FnMut(Verdict, &str),
) -> Result<CoverageToken, IndexStoreError> {
    let tx = conn.unchecked_transaction()?;
    let token = CoverageToken::read(&tx)?;

    // Rows written under a policy this build no longer applies can't be trusted as
    // covered, whatever their epochs say, so the scope goes to the walk whole.
    if index_predates_exclusion_policy(&tx) {
        on_verdict(Verdict::Frontier, scope_path);
        return Ok(token);
    }

    let Some(root) = read_dir_coverage(&tx, scope_index_path)? else {
        // No `entries` row: a cold volume, or a path this index has never seen.
        // Either way the scope root itself is the whole frontier.
        on_verdict(Verdict::Frontier, scope_path);
        return Ok(token);
    };

    let mut stack = vec![(root, scope_path.to_string(), 0usize)];
    while let Some((dir, path, depth)) = stack.pop() {
        let verdict = dir.verdict();
        if verdict != Verdict::Listed {
            on_verdict(verdict, &path);
            continue;
        }
        if depth >= MAX_DESCENT_DEPTH {
            log::warn!(
                target: "indexing",
                "coverage: stopping the descent below depth {MAX_DESCENT_DEPTH} under \"{scope_path}\"; \
                 the parent chain looks cyclic, so \"{path}\" goes to the walk"
            );
            on_verdict(Verdict::Frontier, &path);
            continue;
        }
        on_verdict(Verdict::Listed, &path);
        for (child, name) in read_child_dir_coverage(&tx, dir.id)? {
            stack.push((child, join_path(&path, &name), depth + 1));
        }
    }
    Ok(token)
}

/// One directory's coverage columns, by path. `None` when the path has no row, or
/// has one that isn't a directory.
fn read_dir_coverage(conn: &Connection, index_path: &str) -> Result<Option<DirCoverage>, IndexStoreError> {
    let Some(id) = resolve_path(conn, index_path)? else {
        return Ok(None);
    };
    let mut stmt = conn.prepare_cached(
        "SELECT e.listed_epoch, e.unreadable_cause, COALESCE(ds.min_subtree_epoch, 0)
         FROM entries e LEFT JOIN dir_stats ds ON ds.entry_id = e.id
         WHERE e.id = ?1 AND e.is_directory = 1",
    )?;
    let row = stmt
        .query_row(params![id], |row| {
            Ok(DirCoverage {
                id,
                listed_epoch: row.get(0)?,
                unreadable_cause: UnreadableCause::from_stored(row.get::<_, i64>(1)?),
                min_subtree_epoch: row.get(2)?,
            })
        })
        .optional()?;
    Ok(row)
}

/// Every child DIRECTORY's coverage columns plus its name, for one parent.
///
/// Files are skipped: coverage is a property of directories, and a listed
/// directory's files came with the listing. Served by `idx_parent_name_folded`'s
/// leading `parent_id`.
fn read_child_dir_coverage(conn: &Connection, parent_id: i64) -> Result<Vec<(DirCoverage, String)>, IndexStoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT c.id, c.name, c.listed_epoch, c.unreadable_cause, COALESCE(ds.min_subtree_epoch, 0)
         FROM entries c LEFT JOIN dir_stats ds ON ds.entry_id = c.id
         WHERE c.parent_id = ?1 AND c.is_directory = 1",
    )?;
    let rows = stmt.query_map(params![parent_id], |row| {
        Ok((
            DirCoverage {
                id: row.get(0)?,
                listed_epoch: row.get(2)?,
                unreadable_cause: UnreadableCause::from_stored(row.get::<_, i64>(3)?),
                min_subtree_epoch: row.get(4)?,
            },
            row.get::<_, String>(1)?,
        ))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

/// Append one component to a path, without doubling the separator at the root.
fn join_path(parent: &str, name: &str) -> String {
    if parent.ends_with('/') {
        format!("{parent}{name}")
    } else {
        format!("{parent}/{name}")
    }
}

#[cfg(test)]
mod tests;
