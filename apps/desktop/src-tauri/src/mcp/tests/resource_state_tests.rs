//! Tests for the `cmdr://state` builder: URI/query parsing, pane/tab/file
//! formatting. The public-API checks (resource count, URIs, mime types) live in
//! `resource_tests.rs`.

use crate::mcp::pane_state::{PaneFileEntry, PaneState, TabInfo};
use crate::mcp::resources::{
    build_pane_yaml_with_options, format_file_compact, format_tab_compact, parse_state_options, split_uri, tags_marker,
};
use crate::search::format_size;

#[test]
fn parse_state_options_defaults() {
    let opts = parse_state_options(None);
    assert!(opts.include.is_none());
    assert!(!opts.compact);
    assert!(opts.includes("panes"));
    assert!(opts.includes("anything"));
}

#[test]
fn parse_state_options_include_filters_sections() {
    let opts = parse_state_options(Some("include=panes,listings"));
    let inc = opts.include.as_ref().unwrap();
    assert_eq!(inc.len(), 2);
    assert!(opts.includes("panes"));
    assert!(opts.includes("listings"));
    assert!(!opts.includes("volumes"));
    assert!(!opts.includes("recentErrors"));
}

#[test]
fn parse_state_options_compact_truthy() {
    assert!(parse_state_options(Some("compact=true")).compact);
    assert!(parse_state_options(Some("compact=1")).compact);
    assert!(!parse_state_options(Some("compact=false")).compact);
    assert!(!parse_state_options(Some("compact=")).compact);
}

#[test]
fn split_uri_no_query() {
    let (base, q) = split_uri("cmdr://state");
    assert_eq!(base, "cmdr://state");
    assert!(q.is_none());
}

#[test]
fn split_uri_with_query() {
    let (base, q) = split_uri("cmdr://state?include=panes&compact=true");
    assert_eq!(base, "cmdr://state");
    assert_eq!(q, Some("include=panes&compact=true"));
}

#[test]
fn test_format_size() {
    assert_eq!(format_size(500), "500 B");
    assert_eq!(format_size(1024), "1 KB");
    assert_eq!(format_size(1536), "1.5 KB");
    assert_eq!(format_size(1048576), "1 MB");
    assert_eq!(format_size(1073741824), "1 GB");
}

#[test]
fn test_format_file_compact() {
    let file = PaneFileEntry {
        name: "test.txt".to_string(),
        path: "/tmp/test.txt".to_string(),
        is_directory: false,
        size: Some(1024),
        recursive_size: None,
        modified: Some("2024-01-15".to_string()),
        recursive_size_pending: None,
        tags: vec![],
        ..Default::default()
    };

    // Without details
    let formatted = format_file_compact(&file, 0, false, false, false);
    assert_eq!(formatted, "i:0 f test.txt");

    // With cursor marker
    let formatted = format_file_compact(&file, 0, true, false, false);
    assert_eq!(formatted, "i:0 f test.txt [cur]");

    // With selected marker
    let formatted = format_file_compact(&file, 0, false, true, false);
    assert_eq!(formatted, "i:0 f test.txt [sel]");

    // With details
    let formatted = format_file_compact(&file, 0, true, true, true);
    assert_eq!(formatted, "i:0 f test.txt 1 KB 2024-01-15 [cur] [sel]");

    // Directory
    let dir = PaneFileEntry {
        name: "docs".to_string(),
        path: "/tmp/docs".to_string(),
        is_directory: true,
        size: None,
        recursive_size: None,
        modified: None,
        recursive_size_pending: None,
        tags: vec![],
        ..Default::default()
    };
    let formatted = format_file_compact(&dir, 1, false, false, false);
    assert_eq!(formatted, "i:1 d docs");

    // Directory with recursive size
    let dir_with_size = PaneFileEntry {
        name: "src".to_string(),
        path: "/tmp/src".to_string(),
        is_directory: true,
        size: None,
        recursive_size: Some(169),
        modified: Some("2026-03-19T17:33:53.000Z".to_string()),
        recursive_size_pending: None,
        tags: vec![],
        ..Default::default()
    };
    let formatted = format_file_compact(&dir_with_size, 5, false, false, true);
    assert_eq!(formatted, "i:5 d src 169 B 2026-03-19T17:33:53.000Z");

    // Directory whose recursive size is mid-update gets a [size-pending] marker
    // (the "size updating" hourglass, observable without DOM access).
    let pending_dir = PaneFileEntry {
        name: "target".to_string(),
        path: "/tmp/target".to_string(),
        is_directory: true,
        size: None,
        recursive_size: Some(4096),
        modified: None,
        recursive_size_pending: Some(true),
        tags: vec![],
        ..Default::default()
    };
    let formatted = format_file_compact(&pending_dir, 2, false, false, true);
    assert_eq!(formatted, "i:2 d target 4 KB [size-pending]");
    // The marker shows even without details (it's a status, not a detail).
    let formatted = format_file_compact(&pending_dir, 2, false, false, false);
    assert_eq!(formatted, "i:2 d target [size-pending]");
}

#[test]
fn test_tags_marker() {
    use crate::file_system::listing::metadata::TagRef;
    let tag = |name: &str, color: u8| TagRef {
        name: name.to_string(),
        color,
    };

    // No tags → no marker (zero cost in the common case).
    assert_eq!(tags_marker(&[]), None);

    // Colored tags render as their color name (the dot the UI shows).
    assert_eq!(
        tags_marker(&[tag("Red", 6), tag("Blue", 4)]),
        Some("[tags:red,blue]".to_string())
    );

    // A colorless custom tag renders as its own name.
    assert_eq!(
        tags_marker(&[tag("Important", 0)]),
        Some("[tags:Important]".to_string())
    );

    // A custom-named colored tag still renders as its color (matches the dot).
    assert_eq!(tags_marker(&[tag("Urgent", 6)]), Some("[tags:red]".to_string()));
}

#[test]
fn test_format_file_compact_appends_tags_marker() {
    use crate::file_system::listing::metadata::TagRef;
    let file = PaneFileEntry {
        name: "photo.jpg".to_string(),
        path: "/tmp/photo.jpg".to_string(),
        is_directory: false,
        size: Some(2048),
        recursive_size: None,
        modified: None,
        recursive_size_pending: None,
        tags: vec![TagRef {
            name: "Green".to_string(),
            color: 2,
        }],
        ..Default::default()
    };
    // The tags marker trails the cursor/selected markers.
    let formatted = format_file_compact(&file, 3, true, false, false);
    assert_eq!(formatted, "i:3 f photo.jpg [cur] [tags:green]");
}

#[test]
fn test_build_pane_yaml() {
    let state = PaneState {
        path: "/Users/test".to_string(),
        volume_id: Some("root".to_string()),
        volume_name: Some("Macintosh HD".to_string()),
        files: vec![
            PaneFileEntry {
                name: "file1.txt".to_string(),
                path: "/Users/test/file1.txt".to_string(),
                is_directory: false,
                size: Some(100),
                recursive_size: None,
                modified: Some("2024-01-15".to_string()),
                recursive_size_pending: None,
                tags: vec![],
                ..Default::default()
            },
            PaneFileEntry {
                name: "folder".to_string(),
                path: "/Users/test/folder".to_string(),
                is_directory: true,
                size: None,
                recursive_size: None,
                modified: None,
                recursive_size_pending: None,
                tags: vec![],
                ..Default::default()
            },
        ],
        cursor_index: 0,
        view_mode: "brief".to_string(),
        selected_indices: vec![1],
        sort_field: "name".to_string(),
        sort_order: "asc".to_string(),
        total_files: 2,
        loaded_start: 0,
        loaded_end: 2,
        show_hidden: false,
        tabs: vec![
            TabInfo {
                id: "tab-1".to_string(),
                path: "/Users/test".to_string(),
                pinned: false,
                active: true,
            },
            TabInfo {
                id: "tab-2".to_string(),
                path: "/Users/test/Downloads".to_string(),
                pinned: true,
                active: false,
            },
        ],
        type_to_jump: None,
        mount_error: None,
    };

    let yaml = build_pane_yaml_with_options(&state, "  ", false);

    assert!(yaml.contains("volume: Macintosh HD"));
    assert!(yaml.contains("volumeId: root"));
    assert!(yaml.contains("path: /Users/test"));
    assert!(yaml.contains("view: brief"));
    assert!(yaml.contains("sort: \"name:asc\""));
    assert!(yaml.contains("totalFiles: 2"));
    assert!(yaml.contains("loadedRange: [0, 2]"));
    assert!(yaml.contains("selected: 1"));
    assert!(yaml.contains("[cur]")); // Cursor on first file
    assert!(yaml.contains("[sel]")); // Second file selected
    assert!(yaml.contains("tabs:"));
    assert!(yaml.contains("i:0 id:tab-1 [active] test (/Users/test)"));
    assert!(yaml.contains("i:1 id:tab-2 [pinned] Downloads (/Users/test/Downloads)"));
}

#[test]
fn test_brief_cursor_detail_respects_loaded_window() {
    // Scrolled large directory: the files vec holds the loaded window
    // [loaded_start, loaded_end), while cursor_index is GLOBAL. The brief-mode
    // cursor detail must index window-relative, or it describes the wrong file.
    let state = PaneState {
        path: "/big".to_string(),
        volume_id: Some("root".to_string()),
        volume_name: Some("Macintosh HD".to_string()),
        files: vec![
            PaneFileEntry {
                name: "window-first.txt".to_string(),
                path: "/big/window-first.txt".to_string(),
                is_directory: false,
                size: Some(1),
                recursive_size: None,
                modified: None,
                recursive_size_pending: None,
                tags: vec![],
                ..Default::default()
            },
            PaneFileEntry {
                name: "under-cursor.txt".to_string(),
                path: "/big/under-cursor.txt".to_string(),
                is_directory: false,
                size: Some(2),
                recursive_size: None,
                modified: None,
                recursive_size_pending: None,
                tags: vec![],
                ..Default::default()
            },
        ],
        cursor_index: 101, // global; window-relative index 1
        view_mode: "brief".to_string(),
        selected_indices: vec![],
        sort_field: "name".to_string(),
        sort_order: "asc".to_string(),
        total_files: 50_000,
        loaded_start: 100,
        loaded_end: 102,
        show_hidden: false,
        tabs: vec![],
        type_to_jump: None,
        mount_error: None,
    };

    let yaml = build_pane_yaml_with_options(&state, "  ", false);
    assert!(
        yaml.contains("name: under-cursor.txt"),
        "cursor detail should name the file under the cursor, got:\n{yaml}"
    );
    assert!(
        !yaml.contains("name: window-first.txt"),
        "cursor detail must not name a different file in the window, got:\n{yaml}"
    );

    // Cursor outside the loaded window: no detail lines rather than a wrong file.
    let mut outside = state;
    outside.cursor_index = 5;
    let yaml = build_pane_yaml_with_options(&outside, "  ", false);
    assert!(
        !yaml.contains("name: window-first.txt") && !yaml.contains("name: under-cursor.txt"),
        "cursor outside the window must not show any file's details, got:\n{yaml}"
    );
}

#[test]
fn test_format_tab_compact_active() {
    let tab = TabInfo {
        id: "t1".to_string(),
        path: "/Users/foo/Documents".to_string(),
        pinned: false,
        active: true,
    };
    assert_eq!(
        format_tab_compact(&tab, 0),
        "i:0 id:t1 [active] Documents (/Users/foo/Documents)"
    );
}

#[test]
fn test_format_tab_compact_pinned() {
    let tab = TabInfo {
        id: "t2".to_string(),
        path: "/Users/foo/Downloads".to_string(),
        pinned: true,
        active: false,
    };
    assert_eq!(
        format_tab_compact(&tab, 1),
        "i:1 id:t2 [pinned] Downloads (/Users/foo/Downloads)"
    );
}

#[test]
fn test_format_tab_compact_active_and_pinned() {
    let tab = TabInfo {
        id: "t3".to_string(),
        path: "/Users/foo/Projects".to_string(),
        pinned: true,
        active: true,
    };
    assert_eq!(
        format_tab_compact(&tab, 2),
        "i:2 id:t3 [active] [pinned] Projects (/Users/foo/Projects)"
    );
}

#[test]
fn test_format_tab_compact_plain() {
    let tab = TabInfo {
        id: "t4".to_string(),
        path: "/Users/foo/Desktop".to_string(),
        pinned: false,
        active: false,
    };
    assert_eq!(format_tab_compact(&tab, 3), "i:3 id:t4 Desktop (/Users/foo/Desktop)");
}

#[test]
fn test_format_tab_compact_root_path() {
    let tab = TabInfo {
        id: "t5".to_string(),
        path: "/".to_string(),
        pinned: false,
        active: true,
    };
    // Root path has no non-empty segment after splitting by '/', so falls back to full path
    assert_eq!(format_tab_compact(&tab, 0), "i:0 id:t5 [active] / (/)");
}

#[test]
fn test_pane_yaml_no_tabs_when_empty() {
    let state = PaneState {
        path: "/tmp".to_string(),
        volume_name: Some("Disk".to_string()),
        tabs: vec![],
        ..PaneState::default()
    };
    let yaml = build_pane_yaml_with_options(&state, "  ", false);
    assert!(!yaml.contains("tabs:"));
}

/// A directory whose subtree the indexer hasn't finished covering carries a
/// LOWER BOUND, not a total. `cmdr://state` must say so with the same `≥` the
/// UI shows: an agent that reads a bare number acts on it, and a partial sum
/// presented as settled is how you conclude a 129 GB tree is 28.8 GB.
#[test]
fn incomplete_recursive_size_renders_as_a_lower_bound() {
    let partial = PaneFileEntry {
        name: "deps".to_string(),
        path: "/tmp/deps".to_string(),
        is_directory: true,
        recursive_size: Some(4096),
        recursive_size_complete: Some(false),
        ..Default::default()
    };
    assert_eq!(format_file_compact(&partial, 2, false, false, true), "i:2 d deps ≥4 KB");

    // A covered subtree is an exact total, so it renders bare.
    let complete = PaneFileEntry {
        recursive_size_complete: Some(true),
        ..partial.clone()
    };
    assert_eq!(format_file_compact(&complete, 2, false, false, true), "i:2 d deps 4 KB");

    // Absent flag ⇒ treat as exact (fixtures, and volumes with no index).
    let unknown = PaneFileEntry {
        recursive_size_complete: None,
        ..partial.clone()
    };
    assert_eq!(format_file_compact(&unknown, 2, false, false, true), "i:2 d deps 4 KB");
}

/// Incomplete AND nothing known below yet: `≥0 B` would be worse than silence,
/// so drop the size entirely. Mirrors the UI's `<dir>` placeholder.
#[test]
fn an_incomplete_dir_with_nothing_known_yet_shows_no_size() {
    let nothing_yet = PaneFileEntry {
        name: "scanning".to_string(),
        path: "/tmp/scanning".to_string(),
        is_directory: true,
        recursive_size: Some(0),
        recursive_size_complete: Some(false),
        ..Default::default()
    };
    assert_eq!(
        format_file_compact(&nothing_yet, 3, false, false, true),
        "i:3 d scanning"
    );
}

/// An exact size computed at an older volume epoch is still exact, just old.
/// It's a status, so it shows with or without details.
#[test]
fn a_stale_recursive_size_is_marked() {
    let stale = PaneFileEntry {
        name: "old".to_string(),
        path: "/tmp/old".to_string(),
        is_directory: true,
        recursive_size: Some(2048),
        recursive_size_complete: Some(true),
        recursive_size_stale: Some(true),
        ..Default::default()
    };
    assert_eq!(
        format_file_compact(&stale, 4, false, false, true),
        "i:4 d old 2 KB [size-stale]"
    );
    assert_eq!(
        format_file_compact(&stale, 4, false, false, false),
        "i:4 d old [size-stale]"
    );
}

/// On-disk size rides along ONLY when it diverges enough to change a decision
/// (compression, sparse files, cloud placeholders). Same threshold as the UI's
/// `hasSizeMismatch`, so the two surfaces never disagree about what's worth
/// mentioning; below it, the second number is pure tokens.
#[test]
fn on_disk_size_shows_only_when_it_diverges_enough_to_matter() {
    let sparse = PaneFileEntry {
        name: "sparse".to_string(),
        path: "/tmp/sparse".to_string(),
        is_directory: true,
        recursive_size: Some(4 * 1024 * 1024 * 1024),
        recursive_size_complete: Some(true),
        recursive_physical_size: Some(1024 * 1024 * 1024),
        ..Default::default()
    };
    assert_eq!(
        format_file_compact(&sparse, 5, false, false, true),
        "i:5 d sparse 4 GB (1 GB on disk)"
    );

    // Same relative gap, but too small in absolute terms to be worth a word.
    let tiny_gap = PaneFileEntry {
        recursive_size: Some(1000),
        recursive_physical_size: Some(200),
        ..sparse.clone()
    };
    assert_eq!(
        format_file_compact(&tiny_gap, 5, false, false, true),
        "i:5 d sparse 1000 B"
    );

    // Big absolute gap, but under half: ordinary block rounding, not news.
    let small_relative_gap = PaneFileEntry {
        recursive_size: Some(10 * 1024 * 1024 * 1024),
        recursive_physical_size: Some(9 * 1024 * 1024 * 1024),
        ..sparse.clone()
    };
    assert_eq!(
        format_file_compact(&small_relative_gap, 5, false, false, true),
        "i:5 d sparse 10 GB"
    );

    // A lower-bound total keeps its `≥`, and the on-disk figure follows it.
    let partial_sparse = PaneFileEntry {
        recursive_size_complete: Some(false),
        ..sparse.clone()
    };
    assert_eq!(
        format_file_compact(&partial_sparse, 5, false, false, true),
        "i:5 d sparse ≥4 GB (1 GB on disk)"
    );

    // Details off ⇒ no sizes at all, on-disk included.
    assert_eq!(format_file_compact(&sparse, 5, false, false, false), "i:5 d sparse");
}

/// A pane whose mount didn't go through must say so here.
///
/// The error pane replaces the share list, but the pane's `path` and `files`
/// still describe that list, so without `mountError` a failed mount reads from
/// this resource as a pane that simply didn't move — which is exactly how a
/// mount that couldn't build its URL for a non-ASCII share got mistaken for
/// silence. The message is the same sentence the pane shows.
#[test]
fn a_pane_showing_a_mount_failure_reports_it() {
    let state = PaneState {
        path: "smb://localhost/".to_string(),
        volume_id: Some("network".to_string()),
        volume_name: Some("Network > SMB Test (Unicode)".to_string()),
        view_mode: "full".to_string(),
        mount_error: Some(crate::mcp::pane_state::MountErrorInfo {
            share: "caf\u{e9}".to_string(),
            message: "Connection to \"localhost\" timed out".to_string(),
        }),
        ..Default::default()
    };

    let yaml = build_pane_yaml_with_options(&state, "  ", false);

    assert!(yaml.contains("mountError:"), "expected a mountError section:\n{yaml}");
    assert!(
        yaml.contains("share: \"caf\u{e9}\""),
        "expected the share name:\n{yaml}"
    );
    assert!(
        yaml.contains("message: \"Connection to \\\"localhost\\\" timed out\""),
        "expected the pane's own sentence:\n{yaml}"
    );

    // And it stays out of the way when the pane is showing a directory.
    let ok = PaneState {
        path: "/Users/test".to_string(),
        view_mode: "full".to_string(),
        ..Default::default()
    };
    assert!(!build_pane_yaml_with_options(&ok, "  ", false).contains("mountError"));
}
