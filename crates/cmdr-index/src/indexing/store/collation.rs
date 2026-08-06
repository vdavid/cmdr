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

#[cfg(not(target_os = "macos"))]
pub(super) fn platform_case_compare(a: &str, b: &str) -> std::cmp::Ordering {
    a.cmp(b)
}

/// Normalize a string for case-insensitive comparison.
#[cfg(target_os = "macos")]
pub fn normalize_for_comparison(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    s.nfd().collect::<String>().to_lowercase()
}

/// Normalize a string for case-insensitive comparison.
///
/// Case- and form-sensitive off macOS, where the filesystem is: two names that
/// differ in case are two different files, so folding them would merge rows that
/// are genuinely distinct.
#[cfg(not(target_os = "macos"))]
pub fn normalize_for_comparison(s: &str) -> String {
    s.to_string()
}
