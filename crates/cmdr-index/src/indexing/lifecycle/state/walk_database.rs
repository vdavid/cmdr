//! The database a search-driven walk writes into.
//!
//! A `WriterOnly` start is the only one with no scan behind it to truncate,
//! re-stamp, and seed, so it does that itself: drop an index whose coverage this
//! build refuses to trust, and give the empty one that replaces it the epoch,
//! mount root, and exclusion-policy stamp a scan start would have given it.

use std::path::Path;

use super::clear_index;
use crate::indexing::store::IndexStore;

/// Drop a database whose coverage claims this build refuses to trust, so the walk
/// about to run fills a clean one instead of walking on top of it forever
/// (`docs/specs/unindexed-search-plan.md` Decision 17).
///
/// The exclusion-policy stamp records which policy an index's rows were written
/// under, and a mismatch means NOTHING in it counts as covered
/// (`scanner::index_predates_exclusion_policy`): an excluded directory gets no
/// row, so its parents keep claiming coverage they no longer have. A full scan
/// repairs that by truncating and re-stamping — but this is a WRITER-ONLY start,
/// which is by definition a volume no scan is coming for. Left alone, every
/// search of that drive re-walks the whole scope, each frontier root landing on
/// the slow non-virgin repair path, and the stamp is never rewritten: it never
/// converges again. Evicting costs one walk and restores convergence, because
/// `prepare_database_for_a_walk` stamps a database that provably holds nothing.
///
/// ❌ Not for an `IndexTheVolume` start: a full scan truncates and re-stamps by
/// itself, and throwing its database away would cost a resumable index for
/// nothing.
///
/// Eviction goes through [`clear_index`] rather than unlinking the file: that's
/// what withdraws the volume's read handles, invalidates their connections, and
/// drops the walked-branch set, which describes rows that are about to stop
/// existing.
///
/// Best-effort: a database it can't read, or can't delete, is left standing and
/// behaves exactly as it did before this existed.
pub(super) fn evict_an_index_no_walk_can_trust(volume_id: &str, db_path: &Path) {
    if !db_path.exists() {
        return;
    }
    let predates = match IndexStore::open_read_connection(db_path) {
        Ok(conn) => {
            // An empty database satisfies any policy trivially, and the walk's own
            // bootstrap is about to stamp it. Only rows written under a policy this
            // build no longer applies are worth deleting a file over.
            let holds_rows = IndexStore::get_entry_count(&conn).unwrap_or(0) > 1;
            holds_rows && crate::indexing::scanner::index_predates_exclusion_policy(&conn)
        }
        Err(e) => {
            log::warn!("start_indexing_for('{volume_id}'): couldn't check the index's exclusion policy: {e}");
            return;
        }
    };
    if !predates {
        return;
    }
    log::info!(
        "start_indexing_for('{volume_id}'): the index predates this build's exclusion policy, so nothing in it counts \
         as covered; dropping it for the walk to rebuild"
    );
    if let Err(e) = clear_index(volume_id) {
        log::warn!("start_indexing_for('{volume_id}'): dropping the untrusted index failed: {e}");
    }
}

/// Give a database a search-driven walk is about to write into the three things a
/// scan start would have given it.
///
/// 1. **The epoch** every directory the walk lists is stamped with. A cold
///    database has no `current_epoch`, and a walk only ever stamps the value it
///    reads; seeding it here is what makes epoch 1 mean "this walk covered it".
/// 2. **The mount root** (`volume_path`), which is what lets a reader prefix this
///    index's mount-relative paths back to absolute ones. Search falls back to
///    the live volume registry when it's absent, so this is what keeps a
///    walk-built external index readable once the drive is offline.
/// 3. **The exclusion policy stamp**, but ONLY while the database provably holds
///    nothing (the `ROOT` sentinel alone). It records which policy the rows were
///    written under, and an empty database satisfies any policy trivially — the
///    same argument as stamping right after a `TruncateData`, which is the only
///    other moment that holds. Without it `coverage` trusts NOTHING the walk
///    writes and every later search re-walks the same ground, so convergence on a
///    cold drive lives or dies here (`store::EXCLUSION_POLICY_KEY`).
///
/// ❌ A database that already holds rows is left unstamped: those rows came from
/// somewhere this call can't vouch for.
///
/// Best-effort throughout: a failure costs coverage claims, never correctness, so
/// it's logged rather than propagated into a refusal to walk at all.
pub(super) fn prepare_database_for_a_walk(volume_id: &str, db_path: &Path, volume_root: &Path) {
    let conn = match IndexStore::open_write_connection(db_path) {
        Ok(conn) => conn,
        Err(e) => {
            log::warn!("start_indexing_for('{volume_id}'): couldn't prepare the index for a walk: {e}");
            return;
        }
    };
    if let Err(e) = IndexStore::seed_current_epoch(&conn) {
        log::warn!("start_indexing_for('{volume_id}'): seeding the epoch failed: {e}");
    }
    if let Err(e) = IndexStore::update_meta(&conn, "volume_path", &volume_root.to_string_lossy()) {
        log::warn!("start_indexing_for('{volume_id}'): recording the mount root failed: {e}");
    }
    match IndexStore::get_entry_count(&conn) {
        // 1 is the `ROOT` sentinel `create_tables` inserts, so this is an index
        // that has never held an entry — the same shape a `TruncateData` leaves.
        Ok(count) if count <= 1 => {
            if let Err(e) = IndexStore::update_meta(
                &conn,
                crate::indexing::store::EXCLUSION_POLICY_KEY,
                &crate::indexing::scanner::exclusion_policy_fingerprint(),
            ) {
                log::warn!("start_indexing_for('{volume_id}'): stamping the exclusion policy failed: {e}");
            }
        }
        Ok(_) => log::debug!(
            "start_indexing_for('{volume_id}'): the index already holds rows, so its exclusion-policy stamp stands as it is"
        ),
        Err(e) => log::warn!("start_indexing_for('{volume_id}'): couldn't count the index's entries: {e}"),
    }
}
