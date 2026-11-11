//! The one key two names share when they differ only in Unicode form or case.
//!
//! NFC, then lowercase. macOS holds `é` composed and decomposed side by side,
//! the kernel's SMB mount decomposes every name it lists, and an SMB server
//! matches a name's exact bytes, so "is this the same name?" needs one answer
//! everywhere Cmdr asks it: a share's identity (`volume::ids::smb_volume_id`), a
//! transfer's destination buckets, the SMB spelling resolve, and a pane landing
//! its cursor on a name spelled another way.
//!
//! ❌ A comparison key only, never sent to a volume: the server stores the bytes
//! it was given, and one directory can hold both forms.
//!
//! Two narrower answers ride beside it: [`differ_only_in_form`], the pair a
//! write guard treats as ONE name (a person can't tell them apart; case they
//! can), and [`composed`], the spelling a NEW name takes on a volume that asks.

use std::borrow::Cow;

use unicode_normalization::UnicodeNormalization;

/// `name` folded for comparison. Borrows when the name is already its own key
/// (lowercase ASCII, the overwhelmingly common case), so a hot loop over a big
/// listing allocates nothing for it.
pub fn fold_name(name: &str) -> Cow<'_, str> {
    if name.is_ascii() {
        if name.bytes().any(|b| b.is_ascii_uppercase()) {
            return Cow::Owned(name.to_ascii_lowercase());
        }
        return Cow::Borrowed(name);
    }
    Cow::Owned(name.nfc().flat_map(char::to_lowercase).collect())
}

/// Whether `a` and `b` are two spellings of ONE name that differ only in Unicode
/// form: byte-different, identical once composed. A person can't tell them
/// apart on screen, so a destination holding one is taken for the other.
///
/// Stricter than sharing [`fold_name`]'s key: `Report` and `report` share a key
/// but read as two names, and a case-sensitive destination keeps both on
/// purpose. Two names that differ only in form are never a choice anybody made.
pub fn differ_only_in_form(a: &str, b: &str) -> bool {
    // Pure ASCII has one form, so two ASCII names are equal or different names.
    a != b && !(a.is_ascii() && b.is_ascii()) && a.nfc().eq(b.nfc())
}

/// `name` in composed form (NFC), the spelling a NEW name takes on a volume that
/// asks for it ([`Volume::composes_new_names`](crate::volume::Volume::composes_new_names)).
/// Borrows when there's nothing to compose.
///
/// ❌ Never for a name that addresses an existing entry: that entry's stored
/// bytes are the only spelling that reaches it.
pub fn composed(name: &str) -> Cow<'_, str> {
    if name.is_ascii() || unicode_normalization::is_nfc(name) {
        return Cow::Borrowed(name);
    }
    Cow::Owned(name.nfc().collect())
}

/// A string folded the way THIS machine's local filesystem compares names: the
/// key the drive index stores and matches paths under.
///
/// Unlike [`fold_name`], it follows the platform rather than one rule
/// everywhere: macOS folds Unicode form and case (NFD, then lowercase, as APFS
/// does), so two spellings of one name share a key; elsewhere the filesystem is
/// case- and form-sensitive, so it changes nothing, and two names that differ
/// in case stay two genuinely distinct rows.
#[cfg(target_os = "macos")]
pub fn normalize_for_comparison(s: &str) -> String {
    s.nfd().collect::<String>().to_lowercase()
}

/// A string folded the way THIS machine's local filesystem compares names: the
/// key the drive index stores and matches paths under.
///
/// Case- and form-sensitive off macOS, where the filesystem is: two names that
/// differ in case are two different files, so folding them would merge rows that
/// are genuinely distinct.
#[cfg(any(target_os = "linux", target_family = "wasm"))]
pub fn normalize_for_comparison(s: &str) -> String {
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercase_ascii_is_its_own_key() {
        assert!(matches!(fold_name("readme.txt"), Cow::Borrowed("readme.txt")));
        assert_eq!(fold_name("README.txt"), "readme.txt");
    }

    #[test]
    fn both_unicode_forms_and_both_cases_share_one_key() {
        let composed = "R\u{e9}sz.jpg";
        let decomposed = "re\u{301}sz.jpg";
        assert_eq!(fold_name(composed), fold_name(decomposed));
        assert_eq!(fold_name(decomposed), "r\u{e9}sz.jpg");
    }

    #[test]
    fn only_a_difference_in_form_makes_a_look_alike() {
        assert!(differ_only_in_form("caf\u{e9}", "cafe\u{301}"));
        assert!(
            !differ_only_in_form("caf\u{e9}", "caf\u{e9}"),
            "one spelling is no pair"
        );
        assert!(!differ_only_in_form("Report", "report"), "case reads as two names");
        assert!(
            !differ_only_in_form("Caf\u{e9}", "cafe\u{301}"),
            "and so does case plus form"
        );
        assert!(!differ_only_in_form("a.txt", "b.txt"));
    }

    #[test]
    fn composing_borrows_what_is_already_composed() {
        assert!(matches!(composed("plain.txt"), Cow::Borrowed(_)));
        assert!(matches!(composed("caf\u{e9}"), Cow::Borrowed(_)));
        assert_eq!(composed("cafe\u{301}"), "caf\u{e9}");
    }

    #[test]
    fn folding_ascii_and_folding_through_the_normalizer_agree() {
        // The ASCII fast path is an optimization, not a second rule.
        for name in ["Notes.TXT", "a-b_c 1.txt", "UPPER", ""] {
            assert_eq!(
                fold_name(name),
                name.nfc().flat_map(char::to_lowercase).collect::<String>(),
                "the ASCII shortcut must answer what the general path answers"
            );
        }
    }
}
