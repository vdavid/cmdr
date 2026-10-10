//! Greek to Latin per ELOT 743, as file names need it.

use super::transliterate::greek_to_latin;

#[test]
fn letters_and_accents() {
    assert_eq!(greek_to_latin("Αθήνα"), "Athina");
    assert_eq!(greek_to_latin("Θεσσαλονίκη"), "Thessaloniki");
    assert_eq!(greek_to_latin("ψυχή"), "psychi");
    assert_eq!(greek_to_latin("Χανιά"), "Chania");
}

#[test]
fn digraphs_read_as_one_sound() {
    assert_eq!(greek_to_latin("Κουρούτα"), "Kourouta");
    assert_eq!(greek_to_latin("αυτό"), "afto", "αυ before an unvoiced consonant");
    assert_eq!(greek_to_latin("Ευρώπη"), "Evropi", "ευ before a voiced consonant");
    assert_eq!(greek_to_latin("άγγελος"), "angelos");
    assert_eq!(greek_to_latin("Σφίγξ"), "Sfinx");
}

#[test]
fn a_dialytika_splits_the_pair() {
    assert_eq!(greek_to_latin("Ταΰγετος"), "Taygetos");
}

#[test]
fn capitals_follow_their_neighbours() {
    assert_eq!(greek_to_latin("ΘΕΣΗ"), "THESI");
    assert_eq!(greek_to_latin("Θέση"), "Thesi");
}

#[test]
fn everything_else_passes_through() {
    assert_eq!(greek_to_latin("Report 2026 Žádost.pdf"), "Report 2026 Žádost.pdf");
    assert_eq!(greek_to_latin("photo_Ρόδος_01.jpg"), "photo_Rodos_01.jpg");
}
