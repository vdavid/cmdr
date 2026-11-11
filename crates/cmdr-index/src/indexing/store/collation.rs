//! The `platform_case` collation the `name` column sorts and compares under, and
//! the same folding as a plain function.
//!
//! Custom collations aren't persisted in the DB file, so every connection has to
//! register this one before any table creation or query (`connection.rs` does).

use rusqlite::Connection;

use super::IndexStoreError;

/// Register the `platform_case` collation on a connection.
///
/// Must be called on every connection before any table creation or query,
/// because custom collations are not persisted in the DB file.
pub fn register_platform_case_collation(conn: &Connection) -> Result<(), IndexStoreError> {
    conn.create_collation("platform_case", platform_case_compare)?;
    Ok(())
}

/// Compare two strings using the platform's filesystem case/normalization rules.
///
/// - **macOS**: NFD-normalize then case-fold (matching APFS behavior).
/// - **Linux**: binary comparison (matching ext4/btrfs).
#[cfg(target_os = "macos")]
pub(super) fn platform_case_compare(a: &str, b: &str) -> std::cmp::Ordering {
    use unicode_normalization::UnicodeNormalization;
    let a_norm: String = a.nfd().collect::<String>().to_lowercase();
    let b_norm: String = b.nfd().collect::<String>().to_lowercase();
    a_norm.cmp(&b_norm)
}

#[cfg(target_os = "linux")]
pub(super) fn platform_case_compare(a: &str, b: &str) -> std::cmp::Ordering {
    a.cmp(b)
}

/// The same folding as a plain function, owned by `cmdr-fs` so a crate that
/// matches paths against the index (the write engine) shares one key without
/// depending on the index.
pub use cmdr_fs::name_fold::normalize_for_comparison;
