//! Dump recent operations and their items from `operation-log.db`, read-only.
//!
//! Unlike the drive index and `importance.db`, the operation log registers NO
//! custom collation, so a stock `sqlite3` CLI can already open it — but this tool
//! renders it through the SAME library read functions the app uses (decoding the
//! typed enum tokens, reconstructing interned dir paths), so what you see matches
//! what the app reads, never a re-implementation.
//!
//! Usage:
//!   cargo run -p index-query --bin operation-log-dump -- <operation-log.db> [limit]
//!
//! Point it at the app's `operation-log.db` in the app data dir. It opens
//! read-only; copy the file first if the app might be mid-write (a read-only
//! open still risks reading a half-written page).

use std::path::PathBuf;

use cmdr_lib::operation_log::store::{
    OperationItemRow, open_read_connection, read_operation_items, reconstruct_dir_path, recent_operations,
};
use rusqlite::Connection;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args.len() > 3 {
        eprintln!("Usage: {} <operation-log.db> [limit]", args[0]);
        std::process::exit(1);
    }
    let db_path = PathBuf::from(&args[1]);
    let limit: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20);

    if !db_path.exists() {
        eprintln!("operation-log DB not found: {}", db_path.display());
        std::process::exit(1);
    }

    let conn = match open_read_connection(&db_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("couldn't open {}: {e}", db_path.display());
            std::process::exit(1);
        }
    };

    let operations = match recent_operations(&conn, limit) {
        Ok(ops) => ops,
        Err(e) => {
            eprintln!("couldn't read operations: {e}");
            std::process::exit(1);
        }
    };

    if operations.is_empty() {
        println!("(no operations recorded)");
        return;
    }

    for op in &operations {
        let subkind = op.archive_subkind.map(|s| format!("/{}", s.as_token())).unwrap_or_default();
        let reason = op
            .not_rollbackable_reason
            .map(|r| format!(" reason={}", r.as_token()))
            .unwrap_or_default();
        let coverage = op
            .search_coverage_reason
            .map(|r| format!("{}:{}", op.search_coverage.as_token(), r.as_token()))
            .unwrap_or_else(|| op.search_coverage.as_token().to_string());
        println!(
            "\n{} {}{}  by {}  [{} / {}{}]  items {}/{}  bytes {}  coverage {}",
            op.op_id,
            op.kind.as_token(),
            subkind,
            op.initiator.as_token(),
            op.execution_status.as_token(),
            op.rollback_state.as_token(),
            reason,
            op.items_done,
            op.item_count,
            op.bytes_total,
            coverage,
        );
        if let Some(rolls_back) = &op.rolls_back_op_id {
            println!("  rolls back: {rolls_back}");
        }
        if let Some(summary) = &op.dev_summary {
            println!("  summary: {summary}");
        }

        match read_operation_items(&conn, &op.op_id, 1000) {
            Ok(items) => {
                for item in &items {
                    print_item(&conn, item);
                }
            }
            Err(e) => println!("  (couldn't read items: {e})"),
        }
    }
}

fn print_item(conn: &Connection, item: &OperationItemRow) {
    let source = match reconstruct_dir_path(conn, item.source_dir_id) {
        Ok(dir) => join_path(&dir, &item.source_name),
        Err(_) => format!("dir#{}/{}", item.source_dir_id, item.source_name),
    };
    let dest = match (item.dest_dir_id, &item.dest_name) {
        (Some(dir_id), Some(name)) => match reconstruct_dir_path(conn, dir_id) {
            Ok(dir) => format!(" -> {}", join_path(&dir, name)),
            Err(_) => format!(" -> dir#{dir_id}/{name}"),
        },
        _ => String::new(),
    };
    let overwrote = if item.overwrote { " (overwrote)" } else { "" };
    let size = item.size.map(|s| format!(" {s}B")).unwrap_or_default();
    println!(
        "  #{:>4} {}/{} {}{}{}{} [{}]",
        item.seq,
        item.entry_type.as_token(),
        item.row_role.as_token(),
        source,
        dest,
        size,
        overwrote,
        item.outcome.as_token(),
    );
}

/// Join a reconstructed dir path (`/a/b` or `/`) with a leaf name.
fn join_path(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}
