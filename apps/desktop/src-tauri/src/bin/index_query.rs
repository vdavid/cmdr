//! Minimal CLI to query the Cmdr index SQLite database.
//!
//! The index DB uses a custom `platform_case` collation on the `name` column,
//! so the standard `sqlite3` CLI can't query it. This tool registers the
//! collation and runs an arbitrary SQL query.
//!
//! Usage: cargo run --bin index_query -- <db_path> <sql>

use rusqlite::{Connection, OpenFlags};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <db_path> <sql>", args[0]);
        std::process::exit(1);
    }
    let db_path = &args[1];
    let sql = &args[2];

    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY).expect("Couldn't open database");
    cmdr_lib::indexing::store::register_platform_case_collation(&conn).expect("Couldn't register collation");

    let mut stmt = conn.prepare(sql).expect("Couldn't prepare statement");
    let column_count = stmt.column_count();

    // Print header row
    let headers: Vec<&str> = (0..column_count).map(|i| stmt.column_name(i).unwrap_or("?")).collect();
    println!("{}", headers.join("\t"));

    // Print data rows
    let mut rows = stmt.query([]).expect("Couldn't execute query");
    while let Some(row) = rows.next().expect("Couldn't read row") {
        let values: Vec<String> = (0..column_count)
            .map(|i| {
                row.get::<_, rusqlite::types::Value>(i)
                    .map(|v| match v {
                        rusqlite::types::Value::Null => "NULL".to_string(),
                        rusqlite::types::Value::Integer(n) => n.to_string(),
                        rusqlite::types::Value::Real(f) => f.to_string(),
                        rusqlite::types::Value::Text(s) => s,
                        rusqlite::types::Value::Blob(b) => format!("<blob {} bytes>", b.len()),
                    })
                    .unwrap_or_else(|_| "ERROR".to_string())
            })
            .collect();
        println!("{}", values.join("\t"));
    }
}
