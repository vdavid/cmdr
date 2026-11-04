use super::*;

// ── Path reconstruction ──────────────────────────────────────────

#[test]
fn path_reconstruction() {
    let index = make_test_index();
    let path = reconstruct_path_from_index(&index, 4); // report.pdf
    assert_eq!(path, "/Users/alice/report.pdf");
}

#[test]
fn path_reconstruction_root() {
    let index = make_test_index();
    let path = reconstruct_path_from_index(&index, 1);
    assert_eq!(path, "/");
}

#[test]
fn path_reconstruction_top_level_dir() {
    let index = make_test_index();
    let path = reconstruct_path_from_index(&index, 2); // Users
    assert_eq!(path, "/Users");
}

/// The streamed hash is the ranking hot path's substitute for
/// `hash_path(reconstruct_path_from_index(..))`, so it has to agree with it for
/// EVERY entry — a drifted hash silently reads the wrong (or no) importance weight,
/// with no visible failure beyond subtly worse ranking. Covers the root sentinel, a
/// top-level dir, a nested dir, and files.
#[test]
fn streamed_hash_matches_whole_path_hash() {
    use cmdr_fs::path_hash::hash_path;

    let index = make_test_index();
    for entry in &index.entries {
        let path = reconstruct_path_from_index(&index, entry.id);
        assert_eq!(
            hash_path_from_index(&index, entry.id),
            hash_path(&path),
            "streamed hash differs for id {} ({path})",
            entry.id
        );
    }

    // An id absent from the index (an orphan) resolves to "/" both ways.
    assert_eq!(
        hash_path_from_index(&index, 9999),
        hash_path(&reconstruct_path_from_index(&index, 9999))
    );
}

#[test]
fn streamed_hash_matches_whole_path_hash_past_the_inline_depth() {
    use cmdr_fs::path_hash::hash_path;

    // A chain deeper than the names `hash_path_from_index` keeps on the stack, so the
    // part that spills to the heap has to land in the right order too.
    let mut names = String::new();
    let (root_offset, root_len) = arena_push(&mut names, "");
    let mut entries = vec![SearchEntry {
        id: ROOT_ID,
        parent_id: 0,
        name_offset: root_offset,
        name_len: root_len,
        is_directory: true,
        size: OptU64::NONE,
        modified_at: OptU64::NONE,
    }];
    for depth in 0..100_i64 {
        let (offset, len) = arena_push(&mut names, &format!("level-{depth}"));
        entries.push(SearchEntry {
            id: ROOT_ID + 1 + depth,
            parent_id: ROOT_ID + depth,
            name_offset: offset,
            name_len: len,
            is_directory: true,
            size: OptU64::NONE,
            modified_at: OptU64::NONE,
        });
    }
    let index = SearchIndex {
        names,
        entries,
        generation: 1,
    };
    for depth in [1, 63, 64, 65, 100] {
        let id = ROOT_ID + depth;
        let path = reconstruct_path_from_index(&index, id);
        assert_eq!(
            hash_path_from_index(&index, id),
            hash_path(&path),
            "depth {depth} ({path})"
        );
    }
}

// ── The `~` parent path ──────────────────────────────────────────

#[test]
fn a_parent_under_home_reads_as_tilde() {
    assert_eq!(home_relative_parent("/Users/dav/x/file.txt", Some("/Users/dav")), "~/x");
    assert_eq!(home_relative_parent("/Users/dav/file.txt", Some("/Users/dav")), "~");
}

/// A home that is a mere string prefix of the parent isn't the parent's ancestor: with
/// home `/Users/dav`, `/Users/david/x` belongs to someone else and must stay whole, not
/// come back as `~id/x`.
#[test]
fn a_home_that_only_prefixes_the_name_is_not_an_ancestor() {
    assert_eq!(
        home_relative_parent("/Users/david/x/file.txt", Some("/Users/dav")),
        "/Users/david/x"
    );
    assert_eq!(
        home_relative_parent("/Users/david/file.txt", Some("/Users/dav")),
        "/Users/david"
    );
}

#[test]
fn no_home_or_a_root_level_entry_keeps_the_literal_parent() {
    assert_eq!(home_relative_parent("/Users/dav/x/file.txt", None), "/Users/dav/x");
    assert_eq!(home_relative_parent("/file.txt", Some("/Users/dav")), "/");
}

// ── Icon ID derivation ───────────────────────────────────────────

#[test]
fn icon_id_directory() {
    assert_eq!(derive_icon_id("Documents", true), "dir");
}

#[test]
fn icon_id_file_with_extension() {
    assert_eq!(derive_icon_id("report.pdf", false), "ext:pdf");
}

#[test]
fn icon_id_file_without_extension() {
    assert_eq!(derive_icon_id("Makefile", false), "file");
}

#[test]
fn icon_id_uppercase_extension() {
    assert_eq!(derive_icon_id("Photo.JPG", false), "ext:jpg");
}
