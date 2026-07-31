//! Corpus anonymization tests — the privacy contract (TDD target). These pin
//! WHICH names survive a dump verbatim (only classification-relevant, non-personal
//! ones) and that everything else becomes a stable, unrecoverable placeholder. A
//! regression here could leak a personal folder name, so the coverage is thorough.

use super::*;

#[test]
fn denylisted_and_marker_names_survive_verbatim() {
    // Machine-output names the denylist floors must survive (the score depends on
    // them) — and they carry no PII.
    for name in ["node_modules", "target", "dist", "build"] {
        // Only assert the ones actually denylisted in this build; node_modules and
        // target are known members. Guard with the classifier so the test tracks
        // the real list.
        if is_denylisted(name) {
            assert_eq!(anonymize_name(name, false), name, "{name} is denylisted ⇒ kept");
        }
    }
    // VCS markers (dot-prefixed) survive.
    for name in [".git", ".hg", ".svn"] {
        assert_eq!(anonymize_name(name, false), name, "{name} is a marker ⇒ kept");
    }
}

#[test]
fn dotfiles_survive_verbatim_for_hidden_detection() {
    // Any leading-dot name drives hidden/system classification, so it's kept —
    // and a dotfile name is a convention, not personal content.
    assert_eq!(anonymize_name(".config", false), ".config");
    assert_eq!(anonymize_name(".cache", false), ".cache");
}

#[test]
fn path_class_anchors_survive_only_as_home_children() {
    // As a direct child of home, an anchor name classifies the subtree, so it's
    // kept.
    for anchor in ["Downloads", "Desktop", "Documents", "Library"] {
        assert_eq!(
            anonymize_name(anchor, true),
            anchor,
            "{anchor} as a home child drives path-class ⇒ kept"
        );
    }
    // The SAME name deeper in the tree (not a home child) carries no path-class
    // meaning, so it's anonymized — a folder a person named "Documents" three
    // levels down is potentially personal and doesn't affect classification.
    let deep = anonymize_name("Documents", false);
    assert_ne!(deep, "Documents", "a non-home-child anchor name is anonymized");
    assert!(deep.starts_with("dir-"), "…to a placeholder");
}

#[test]
fn ordinary_names_become_stable_placeholders() {
    // A personal folder name is replaced by a placeholder — never leaked.
    let a = anonymize_name("Taxes 2025 - Personal", false);
    assert!(a.starts_with("dir-"), "personal name ⇒ placeholder, got {a}");
    assert!(!a.contains("Taxes"), "the original must not survive in the placeholder");

    // Stable: the same input maps to the same placeholder (structure stays legible).
    let b = anonymize_name("Taxes 2025 - Personal", false);
    assert_eq!(a, b, "anonymization is deterministic");

    // Distinct inputs (almost always) map to distinct placeholders.
    let c = anonymize_name("Something Else", false);
    assert_ne!(a, c, "different names ⇒ different placeholders");
}

#[test]
fn placeholder_shape_is_dir_plus_8_hex() {
    let p = anonymize_name("whatever personal name", false);
    assert!(p.starts_with("dir-"));
    let hex = &p["dir-".len()..];
    assert_eq!(hex.len(), 8, "8 hex chars");
    assert!(hex.chars().all(|c| c.is_ascii_hexdigit()), "all hex: {hex}");
}

#[test]
fn classification_relevance_matches_the_scorer_inputs() {
    // The predicate is the load-bearing gate: it must say "relevant" for exactly
    // the names the scorer classifies and "not" for personal names.
    assert!(name_is_classification_relevant(".git", false));
    assert!(name_is_classification_relevant("node_modules", false));
    assert!(name_is_classification_relevant("Library", true));
    assert!(!name_is_classification_relevant("Library", false), "not a home child");
    assert!(!name_is_classification_relevant("My Vacation Photos", false));
    assert!(!name_is_classification_relevant("clientwork", true));
}
