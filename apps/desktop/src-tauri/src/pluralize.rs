//! Count + noun formatting that picks the right singular/plural form.
//!
//! Use this everywhere a log line, error message, or user-facing string
//! interpolates a number followed by a noun. Hand-rolled `format!("{n} files")`
//! reads as `"1 files"` when `n == 1`. The `pluralize-noun-check` script
//! catches new occurrences in CI.
//!
//! Both fns take `u64`; cast smaller / signed integer counts at the call site
//! (`pluralize(entries.len() as u64, "entry", "entries")`). The standard `as
//! u64` cluster reads cleaner than a generic `Into<…>` signature.

/// Returns `"1 <singular>"` when `count == 1`, otherwise `"<count> <singular>s"`.
///
/// For irregular plurals (`entry`/`entries`, `directory`/`directories`,
/// `branch`/`branches`), call [`pluralize_with`] instead.
///
/// ```ignore
/// assert_eq!(pluralize(0, "file"), "0 files");
/// assert_eq!(pluralize(1, "file"), "1 file");
/// assert_eq!(pluralize(2, "byte"), "2 bytes");
/// ```
pub fn pluralize(count: u64, singular: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {singular}s")
    }
}

/// Same as [`pluralize`], but takes an explicit `plural` form for words where
/// the regular `+s` rule doesn't apply.
///
/// ```ignore
/// assert_eq!(pluralize_with(1, "entry", "entries"), "1 entry");
/// assert_eq!(pluralize_with(3, "entry", "entries"), "3 entries");
/// assert_eq!(pluralize_with(12, "branch", "branches"), "12 branches");
/// ```
pub fn pluralize_with(count: u64, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_plural_defaults_to_plus_s() {
        assert_eq!(pluralize(0, "file"), "0 files");
        assert_eq!(pluralize(1, "file"), "1 file");
        assert_eq!(pluralize(2, "file"), "2 files");
        assert_eq!(pluralize(7, "byte"), "7 bytes");
    }

    #[test]
    fn pluralize_with_handles_irregulars() {
        assert_eq!(pluralize_with(0, "entry", "entries"), "0 entries");
        assert_eq!(pluralize_with(1, "entry", "entries"), "1 entry");
        assert_eq!(pluralize_with(3, "entry", "entries"), "3 entries");
        assert_eq!(pluralize_with(12, "branch", "branches"), "12 branches");
        assert_eq!(pluralize_with(5, "directory", "directories"), "5 directories");
    }
}
