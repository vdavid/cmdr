//! Which operation put the node at a path there: the journal read behind "Tell me
//! about this" (`DETAILS.md` § "Where an item landed").
//!
//! A path alone proves nothing. The file Cmdr copied to `~/Documents/report.pdf`
//! last month may have been replaced since by a download of the same name, and
//! "you copied this from the NAS" about that download is a false statement of
//! where a file came from. So a row answers only for the node it recorded landing
//! (`dest_inode` + `dest_birth_ns`), and a row that recorded none answers nothing.

use std::path::Path;

use cmdr_fs::volume::NodeIdentity;
use rusqlite::Connection;

use crate::store::{OperationLogStoreError, decode, find_dir, fold_name, join_leaf, resolve_dir};
use crate::types::{Initiator, OpKind};

/// How many rows at one path are looked through, newest first. A path collects a
/// row per operation that landed something there; past this many the older ones
/// go unread, and the answer for them is the same as if retention had pruned them.
const NEWEST_ROWS_READ: u32 = 64;

/// What the journal knows about how the node now at a path got there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LandedAt {
    /// An operation landed this very node at the path: the newest one that did.
    ByOperation {
        /// What the operation was.
        kind: OpKind,
        /// Who started it.
        initiator: Initiator,
        /// When it ended, or when it started if it hasn't yet. Unix seconds.
        at: i64,
        /// The volume the item came from.
        source_volume_id: String,
        /// Where on that volume it came from.
        source_path: String,
    },
    /// Operations landed something at the path, and none of the nodes they
    /// recorded is the one there now: it was replaced since.
    NodeChangedSince,
    /// Nothing ties this node to an operation: no row lands at the path, or the
    /// rows that do recorded no node (written before the journal kept one, or
    /// landed where no lasting identity exists), or retention pruned them.
    NoRecord,
}

/// Which operation put `node`, the node now at `path` on `volume_id`, there.
///
/// `path` is matched the way it was recorded: its folder by the exact names the
/// `dirs` table interned, its own name folded with [`fold_name`], through the
/// `operation_items_dest_name` index. Only the node decides a claim, so a name
/// that matches loosely costs a row read and never a wrong answer.
///
/// Reads the database only, never the disk: the caller brings the node.
pub fn landed_at(
    conn: &Connection,
    volume_id: &str,
    path: &Path,
    node: NodeIdentity,
) -> Result<LandedAt, OperationLogStoreError> {
    let (Some(folder), Some(name)) = (path.parent(), path.file_name()) else {
        return Ok(LandedAt::NoRecord);
    };
    let Some(dir_id) = find_dir(conn, volume_id, &folder.to_string_lossy())? else {
        return Ok(LandedAt::NoRecord);
    };

    let mut recorded_another_node = false;
    let landed = {
        let mut stmt = conn.prepare_cached(LANDED_AT_SQL)?;
        let mut rows = stmt.query(rusqlite::params![
            fold_name(&name.to_string_lossy()),
            dir_id,
            NEWEST_ROWS_READ
        ])?;
        let mut landed = None;
        while let Some(row) = rows.next()? {
            let (Some(inode), Some(birth_nanos)) = (row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?)
            else {
                continue;
            };
            // The inode is stored bit-cast to SQLite's signed integer.
            if (inode as u64, birth_nanos) != (node.inode, node.birth_nanos) {
                recorded_another_node = true;
                continue;
            }
            landed = Some(Landing {
                source_dir_id: row.get(2)?,
                source_name: row.get(3)?,
                kind: decode(row.get(4)?, OpKind::from_token, "kind")?,
                initiator: decode(row.get(5)?, Initiator::from_token, "initiator")?,
                at: row.get::<_, Option<i64>>(7)?.unwrap_or(row.get(6)?),
            });
            break;
        }
        landed
    };
    // Resolve the source's folder after the query drained: both use the
    // connection's statement cache.
    match landed {
        Some(landing) => {
            let (source_volume_id, source_folder) = resolve_dir(conn, landing.source_dir_id)?;
            Ok(LandedAt::ByOperation {
                kind: landing.kind,
                initiator: landing.initiator,
                at: landing.at,
                source_volume_id,
                source_path: join_leaf(&source_folder, &landing.source_name)
                    .to_string_lossy()
                    .into_owned(),
            })
        }
        None if recorded_another_node => Ok(LandedAt::NodeChangedSince),
        None => Ok(LandedAt::NoRecord),
    }
}

/// The newest rows landing at one name in one folder. A rollback's own rows are
/// left out: they record no node, and its original carries the story.
const LANDED_AT_SQL: &str = "SELECT i.dest_inode, i.dest_birth_ns, i.source_dir_id, i.source_name, \
            o.kind, o.initiator, o.started_at, o.ended_at \
     FROM operation_items i JOIN operations o ON o.op_id = i.op_id \
     WHERE i.dest_name_folded = ?1 AND i.dest_dir_id = ?2 AND o.rolls_back_op_id IS NULL \
     ORDER BY o.started_at DESC, o.op_id DESC, i.seq DESC LIMIT ?3";

/// The matching row, before its source folder is resolved to a path.
struct Landing {
    source_dir_id: i64,
    source_name: String,
    kind: OpKind,
    initiator: Initiator,
    at: i64,
}

#[cfg(test)]
mod tests;
