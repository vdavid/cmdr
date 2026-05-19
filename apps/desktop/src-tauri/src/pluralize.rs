//! Count + noun formatting that picks the right singular/plural form.
//!
//! Use this everywhere a log line, error message, or user-facing string
//! interpolates a number followed by a noun. Hand-rolled `format!("{n} files")`
//! reads as `"1 files"` when `n == 1`. The `pluralize-noun-check` script
//! catches new occurrences in CI.

/// Returns `"1 <singular>"` when `count == 1`, otherwise `"<count> <singular>s"`.
///
/// For irregular plurals (`entry`/`entries`, `directory`/`directories`,
/// `branch`/`branches`), call [`pluralize_with`] instead.
///
/// `count` accepts any unsigned integer (`usize`, `u32`, `u64`, …): pass it
/// straight, no `as u64` cast at the call site.
///
/// ```ignore
/// assert_eq!(pluralize(0_u64, "file"), "0 files");
/// assert_eq!(pluralize(1_u64, "file"), "1 file");
/// assert_eq!(pluralize(2_u64, "byte"), "2 bytes");
/// ```
pub fn pluralize<N: Into<PluralCount>>(count: N, singular: &str) -> String {
    let count = count.into().0;
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
/// assert_eq!(pluralize_with(1_u64, "entry", "entries"), "1 entry");
/// assert_eq!(pluralize_with(3_u64, "entry", "entries"), "3 entries");
/// assert_eq!(pluralize_with(12_u64, "branch", "branches"), "12 branches");
/// ```
pub fn pluralize_with<N: Into<PluralCount>>(count: N, singular: &str, plural: &str) -> String {
    let count = count.into().0;
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}

/// New-type around `u64` so the pluralize fns can accept any unsigned integer
/// (including `usize`) via `Into` without forcing call sites to cast.
pub struct PluralCount(u64);

macro_rules! impl_from_for_plural_count {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl From<$ty> for PluralCount {
                fn from(value: $ty) -> Self {
                    Self(value as u64)
                }
            }
        )+
    };
}

impl_from_for_plural_count!(u8, u16, u32, u64, usize);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_plural_defaults_to_plus_s() {
        assert_eq!(pluralize(0_u64, "file"), "0 files");
        assert_eq!(pluralize(1_u64, "file"), "1 file");
        assert_eq!(pluralize(2_u64, "file"), "2 files");
        assert_eq!(pluralize(7_u64, "byte"), "7 bytes");
    }

    #[test]
    fn pluralize_with_handles_irregulars() {
        assert_eq!(pluralize_with(0_u64, "entry", "entries"), "0 entries");
        assert_eq!(pluralize_with(1_u64, "entry", "entries"), "1 entry");
        assert_eq!(pluralize_with(3_u64, "entry", "entries"), "3 entries");
        assert_eq!(pluralize_with(12_u64, "branch", "branches"), "12 branches");
        assert_eq!(pluralize_with(5_u64, "directory", "directories"), "5 directories");
    }

    #[test]
    fn accepts_smaller_integer_types() {
        assert_eq!(pluralize(1_u32, "dir"), "1 dir");
        assert_eq!(pluralize(5_u32, "dir"), "5 dirs");
        assert_eq!(pluralize(7_u16, "byte"), "7 bytes");
        assert_eq!(pluralize(2_usize, "item"), "2 items");
        assert_eq!(pluralize(1_usize, "item"), "1 item");
    }
}
