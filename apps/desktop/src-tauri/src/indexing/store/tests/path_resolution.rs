//! Resolving a path to an entry id and back, plus the `platform_case` collation
//! every one of those queries rides on (case folding, NFC≡NFD, comparator laws).

use super::*;

#[test]
fn resolve_path_basic() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // Root resolves to ROOT_ID
    assert_eq!(resolve_path(&conn, "/").unwrap(), Some(ROOT_ID));

    // Insert /Users/test
    let users_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();
    let test_id = IndexStore::insert_entry_v2(&conn, users_id, "test", true, false, None, None, None, None).unwrap();

    assert_eq!(resolve_path(&conn, "/Users").unwrap(), Some(users_id));
    assert_eq!(resolve_path(&conn, "/Users/test").unwrap(), Some(test_id));
    assert_eq!(resolve_path(&conn, "/nonexistent").unwrap(), None);
    assert_eq!(resolve_path(&conn, "/Users/nonexistent").unwrap(), None);
}

/// `resolve_path_under` walks from an ARBITRARY root id, not just `ROOT_ID`.
///
/// This is the network/MTP case: the index is rooted at the volume root, so a
/// deep dir must resolve relative to a non-`/` root. The tree here mimics a share
/// whose mount root is `share` (id `share_id`); `sub/deep` resolves under it, a
/// leading-slash variant resolves identically, an empty path resolves to the root
/// itself, and a missing component returns `None`.
#[test]
fn resolve_path_under_walks_from_a_non_root_id() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // /share/sub/deep, plus a sibling /share/other to prove we don't wander.
    let share_id = insert_entry(&conn, ROOT_ID, "share", true, None);
    let sub_id = insert_entry(&conn, share_id, "sub", true, None);
    let deep_id = insert_entry(&conn, sub_id, "deep", true, None);
    insert_entry(&conn, share_id, "other", true, None);

    // Resolve a deep dir RELATIVE to `share_id` (the index's volume root would be
    // `share_id` for a non-`/`-rooted index).
    assert_eq!(resolve_path_under(&conn, share_id, "sub/deep").unwrap(), Some(deep_id));
    // A leading slash is relative to the given root, not the index root.
    assert_eq!(resolve_path_under(&conn, share_id, "/sub/deep").unwrap(), Some(deep_id));
    // The empty path and "/" resolve to the root id itself.
    assert_eq!(resolve_path_under(&conn, share_id, "").unwrap(), Some(share_id));
    assert_eq!(resolve_path_under(&conn, share_id, "/").unwrap(), Some(share_id));
    // One level under the root resolves.
    assert_eq!(resolve_path_under(&conn, share_id, "sub").unwrap(), Some(sub_id));
    // A missing component returns None.
    assert_eq!(resolve_path_under(&conn, share_id, "sub/missing").unwrap(), None);
    // The absolute path that `resolve_path` would use FAILS at the first
    // component (the volume root isn't `/`), which is exactly the gap
    // `resolve_path_under` closes.
    assert_eq!(resolve_path(&conn, "/sub/deep").unwrap(), None);
}

#[test]
fn resolve_path_trailing_slash() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let users_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();
    assert_eq!(resolve_path(&conn, "/Users/").unwrap(), Some(users_id));
}

#[test]
fn reconstruct_path_test() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    assert_eq!(IndexStore::reconstruct_path(&conn, ROOT_ID).unwrap(), "/");

    let users = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();
    let foo = IndexStore::insert_entry_v2(&conn, users, "foo", true, false, None, None, None, None).unwrap();
    let file =
        IndexStore::insert_entry_v2(&conn, foo, "bar.txt", false, false, Some(10), Some(10), None, None).unwrap();

    assert_eq!(IndexStore::reconstruct_path(&conn, users).unwrap(), "/Users");
    assert_eq!(IndexStore::reconstruct_path(&conn, foo).unwrap(), "/Users/foo");
    assert_eq!(IndexStore::reconstruct_path(&conn, file).unwrap(), "/Users/foo/bar.txt");
}

#[test]
fn resolve_component_test() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let users = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();
    assert_eq!(
        IndexStore::resolve_component(&conn, ROOT_ID, "Users").unwrap(),
        Some(users)
    );
    assert_eq!(
        IndexStore::resolve_component(&conn, ROOT_ID, "nonexistent").unwrap(),
        None
    );
}

#[test]
fn get_parent_id_test() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let users = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();
    assert_eq!(IndexStore::get_parent_id(&conn, users).unwrap(), Some(ROOT_ID));
    assert_eq!(IndexStore::get_parent_id(&conn, ROOT_ID).unwrap(), Some(ROOT_PARENT_ID));
    assert_eq!(IndexStore::get_parent_id(&conn, 999999).unwrap(), None);
}

#[cfg(target_os = "macos")]
#[test]
fn platform_case_collation_macos() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // Insert "Users" dir
    let users_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();

    // Resolve with different case should work on macOS
    assert_eq!(resolve_path(&conn, "/users").unwrap(), Some(users_id));
    assert_eq!(resolve_path(&conn, "/USERS").unwrap(), Some(users_id));
    assert_eq!(resolve_path(&conn, "/Users").unwrap(), Some(users_id));

    // Schema v12 reinstated UNIQUE on (parent_id, name_folded). On macOS
    // `normalize_for_comparison("Users") == normalize_for_comparison("users")`
    // (NFD + case fold), so this insert must collide.
    let result = IndexStore::insert_entry_v2(&conn, ROOT_ID, "users", true, false, None, None, None, None);
    assert!(
        result.is_err(),
        "case-variant insert must collide on the UNIQUE (parent_id, name_folded) index; got {result:?}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn resolve_component_case_insensitive() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    let users_id = IndexStore::insert_entry_v2(&conn, ROOT_ID, "Users", true, false, None, None, None, None).unwrap();

    // Different casings should all resolve to the same ID
    assert_eq!(
        IndexStore::resolve_component(&conn, ROOT_ID, "users").unwrap(),
        Some(users_id)
    );
    assert_eq!(
        IndexStore::resolve_component(&conn, ROOT_ID, "USERS").unwrap(),
        Some(users_id)
    );
    assert_eq!(
        IndexStore::resolve_component(&conn, ROOT_ID, "uSeRs").unwrap(),
        Some(users_id)
    );

    // Nonexistent name returns None
    assert_eq!(
        IndexStore::resolve_component(&conn, ROOT_ID, "nonexistent").unwrap(),
        None
    );
}

#[test]
fn deeply_nested_path_resolution() {
    let (_store, dir) = open_temp_store();
    let db_path = dir.path().join("test-index.db");
    let conn = IndexStore::open_write_connection(&db_path).unwrap();

    // Create /a/b/c/d/e/f/g/h/i/j (10 levels deep)
    let mut parent_id = ROOT_ID;
    let names = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];
    let mut ids = Vec::new();
    for name in &names {
        let id = IndexStore::insert_entry_v2(&conn, parent_id, name, true, false, None, None, None, None).unwrap();
        ids.push(id);
        parent_id = id;
    }

    // Resolve full path
    let path = "/a/b/c/d/e/f/g/h/i/j";
    assert_eq!(resolve_path(&conn, path).unwrap(), Some(*ids.last().unwrap()));

    // Reconstruct from deepest
    let reconstructed = IndexStore::reconstruct_path(&conn, *ids.last().unwrap()).unwrap();
    assert_eq!(reconstructed, path);

    // Partial path
    assert_eq!(resolve_path(&conn, "/a/b/c").unwrap(), Some(ids[2]));
}

// ====================================================================
// platform_case_compare / normalize_for_comparison
//
// The collation function backs SQLite's `platform_case` collation, which
// every path-resolution query relies on. cargo-mutants showed the
// structural mutants `platform_case_compare -> Default::default()` and
// `normalize_for_comparison -> String::new() / "xyzzy".into()` survive
// when the only test exercises one direction of equality.
// ====================================================================

#[cfg(target_os = "macos")]
#[test]
fn platform_case_compare_distinguishes_distinct_names() {
    // Kills: replace platform_case_compare -> Default::default() (which is
    // Ordering::Equal, so every comparison would say "equal"; sort order
    // and SQLite's collation-driven uniqueness would collapse).
    assert_eq!(platform_case_compare("a", "a"), std::cmp::Ordering::Equal);
    assert_eq!(platform_case_compare("a", "b"), std::cmp::Ordering::Less);
    assert_eq!(platform_case_compare("b", "a"), std::cmp::Ordering::Greater);
}

#[cfg(target_os = "macos")]
#[test]
fn platform_case_compare_case_insensitive_on_macos() {
    // APFS is case-preserving but case-insensitive by default. The
    // collation must report equality across case variants for path
    // resolution to work.
    assert_eq!(platform_case_compare("Users", "users"), std::cmp::Ordering::Equal);
    assert_eq!(
        platform_case_compare("README.MD", "readme.md"),
        std::cmp::Ordering::Equal
    );
}

#[cfg(target_os = "macos")]
#[test]
fn platform_case_compare_normalizes_unicode_nfc_to_nfd() {
    // "é" can be one codepoint (NFC, U+00E9) or two (NFD, U+0065 U+0301).
    // APFS stores NFD; the collation must treat the two representations
    // as equal so a user typing NFC resolves NFD-stored entries.
    let nfc = "café"; // typically NFC in Rust source
    let nfd = "cafe\u{0301}"; // 'e' + combining acute
    // Make sure they're actually different byte sequences (sanity check).
    assert_ne!(nfc.as_bytes(), nfd.as_bytes());
    assert_eq!(
        platform_case_compare(nfc, nfd),
        std::cmp::Ordering::Equal,
        "NFC and NFD forms of 'café' must compare equal on APFS"
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn platform_case_compare_is_binary_off_macos() {
    // Linux ext4/btrfs: exact byte comparison, NOT case-folded.
    assert_eq!(platform_case_compare("a", "a"), std::cmp::Ordering::Equal);
    assert_eq!(platform_case_compare("Users", "users"), std::cmp::Ordering::Less);
    // ('U' = 0x55, 'u' = 0x75 → 'U' < 'u' in ASCII, so "Users" < "users".)
}

#[cfg(target_os = "macos")]
#[test]
fn normalize_for_comparison_lowercases_and_nfd_normalizes() {
    // Kills: replace normalize_for_comparison -> String::new() / "xyzzy".
    assert_eq!(normalize_for_comparison("Users"), "users");
    let nfc = "café";
    let nfd = "cafe\u{0301}";
    // After normalization, both should be NFD-lowercased and equal.
    assert_eq!(normalize_for_comparison(nfc), normalize_for_comparison(nfd));
    assert!(
        !normalize_for_comparison("hello").is_empty(),
        "normalize_for_comparison must not return an empty string for non-empty input"
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn normalize_for_comparison_is_identity_off_macos() {
    assert_eq!(normalize_for_comparison("Users"), "Users");
    assert_eq!(normalize_for_comparison("hello"), "hello");
}

// ── platform_case_compare (property-based) ───────────────────────
//
// The collation is used on every `entries.name` comparison in the
// SQLite index. A bug in the comparator would corrupt the index's
// sort order and, worse, cause `resolve_path` to fail to find
// entries the user typed in a different case or Unicode form.
// These properties pin the comparator algebra (reflexivity,
// antisymmetry, transitivity) plus the platform-specific normalization
// semantics (NFC≡NFD on macOS, byte-equal off macOS).

mod platform_case_proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Reflexivity: `cmp(a, a) == Equal` for any string.
        #[test]
        fn reflexivity(s in ".*") {
            prop_assert_eq!(platform_case_compare(&s, &s), std::cmp::Ordering::Equal);
        }

        /// Antisymmetry: `cmp(a, b)` and `cmp(b, a)` must be reverses
        /// of each other.
        #[test]
        fn antisymmetry(a in ".*", b in ".*") {
            let ab = platform_case_compare(&a, &b);
            let ba = platform_case_compare(&b, &a);
            prop_assert_eq!(
                ab,
                ba.reverse(),
                "cmp({:?}, {:?}) = {:?} but cmp({:?}, {:?}) = {:?} should be its reverse",
                a, b, ab, b, a, ba
            );
        }

        /// Transitivity: if `cmp(a, b) <= 0` and `cmp(b, c) <= 0`,
        /// then `cmp(a, c) <= 0`. We also check the strict-less and
        /// equal flavors.
        #[test]
        fn transitivity(a in ".*", b in ".*", c in ".*") {
            use std::cmp::Ordering::*;
            let ab = platform_case_compare(&a, &b);
            let bc = platform_case_compare(&b, &c);
            let ac = platform_case_compare(&a, &c);
            if ab != Greater && bc != Greater {
                prop_assert!(
                    ac != Greater,
                    "transitivity violated: cmp(a,b)={:?} cmp(b,c)={:?} cmp(a,c)={:?} for a={:?} b={:?} c={:?}",
                    ab, bc, ac, a, b, c
                );
            }
            if ab != Less && bc != Less {
                prop_assert!(
                    ac != Less,
                    "transitivity violated (>=): cmp(a,b)={:?} cmp(b,c)={:?} cmp(a,c)={:?}",
                    ab, bc, ac
                );
            }
        }
    }

    // On macOS, NFC and NFD forms of the same logical string must
    // compare equal: APFS stores NFD, but users may type NFC, and
    // `resolve_path` must find the stored entry either way.
    #[cfg(target_os = "macos")]
    proptest! {
        #[test]
        fn nfc_equals_nfd_on_macos(s in ".*") {
            use unicode_normalization::UnicodeNormalization;
            let nfc: String = s.nfc().collect();
            let nfd: String = s.nfd().collect();
            prop_assert_eq!(
                platform_case_compare(&nfc, &nfd),
                std::cmp::Ordering::Equal,
                "NFC {:?} and NFD {:?} of {:?} must compare equal on APFS",
                nfc, nfd, s
            );
        }
    }

    // Off macOS, the comparator is exact byte comparison. We pin
    // this by checking that the result matches `str::cmp`.
    #[cfg(not(target_os = "macos"))]
    proptest! {
        #[test]
        fn matches_byte_cmp_off_macos(a in ".*", b in ".*") {
            prop_assert_eq!(platform_case_compare(&a, &b), a.cmp(&b));
        }
    }
}
