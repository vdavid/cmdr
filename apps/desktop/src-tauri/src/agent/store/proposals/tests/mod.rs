//! Proposal-spine tests, and the fixtures they share.
//!
//! - `schema.rs`: the two FK actions, asserted in raw SQL because the FK action IS the thing
//!   under test.
//! - `claim.rs`: the claim transaction — the one place a bug applies ops to real files twice.
//! - `lifecycle.rs`: re-propose's pending-only guard and the `interrupted` recovery sweep.
//! - `scale.rs`: a 60 000-op group, which is a legitimate group.

mod claim;
mod lifecycle;
mod scale;
mod schema;

use rusqlite::Connection;

use super::*;
use crate::agent::store::{MIGRATIONS, run_migrations};
use crate::agent::types::ProposalStatus;

/// A migrated in-memory `main.db`.
pub(super) fn migrated_conn() -> Connection {
    let conn = crate::sqlite_util::open_in_memory().expect("in-memory db");
    conn.execute_batch("PRAGMA foreign_keys = ON;").expect("pragma");
    run_migrations(&conn, MIGRATIONS).expect("migrate");
    conn
}

/// A trash group over `count` synthetic sources, in a sweep of its own. Trash because it
/// binds no target at all, which keeps the fixture about the lifecycle rather than a path.
pub(super) fn group_with_ops(conn: &Connection, count: usize) -> i64 {
    let set_id = create_sweep(conn, &NewSweep::default(), 100).expect("sweep");
    let sources = (0..count)
        .map(|i| NewOp {
            source_path: format!("/Users/someone/Downloads/file-{i:05}.dmg"),
            snapshot: None,
        })
        .collect();
    create_group(
        conn,
        set_id,
        &NewGroup {
            intent: GroupIntent::Trash { sources },
            source_volume_id: "root".to_string(),
            display_name: format!("{count} installers you've already opened"),
            rationale: Some("They're all months old and you opened every one.".to_string()),
            selector: None,
        },
        100,
    )
    .expect("group")
}

/// The group's stored status.
pub(super) fn status_of(conn: &Connection, group_id: i64) -> ProposalStatus {
    get_group(conn, group_id)
        .expect("read group")
        .expect("the group exists")
        .status
}
