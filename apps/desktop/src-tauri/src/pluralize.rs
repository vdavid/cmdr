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

/// A count with thousands separators, so a six- or seven-figure number is
/// readable at a glance in a log line: `1649321` → `1,649,321`.
pub fn grouped(count: u64) -> String {
    let digits = count.to_string();
    let bytes = digits.as_bytes();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(b as char);
    }
    out
}

/// [`pluralize`] with the count grouped by [`grouped`]: `"620,483 events"`.
/// Use it wherever the number can plausibly reach six figures.
pub fn pluralize_grouped(count: u64, singular: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{} {singular}s", grouped(count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouped_formats_thousands_separators() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(42), "42");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(1_649_321), "1,649,321");
        assert_eq!(grouped(1_000_000), "1,000,000");
    }

    #[test]
    fn pluralize_grouped_groups_and_pluralizes() {
        assert_eq!(pluralize_grouped(1, "event"), "1 event");
        assert_eq!(pluralize_grouped(0, "event"), "0 events");
        assert_eq!(pluralize_grouped(831_060, "event"), "831,060 events");
    }

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
