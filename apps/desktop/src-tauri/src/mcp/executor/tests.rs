//! Tests for executor module.

use std::path::Path;

use super::app::{TabMoveAck, TabMoveRefusal, parse_tab_move_response, tab_move_result};
use super::nav::{SelectableVolume, nav_result, resolve_volume_selector, select_volume_result};
use super::search::parse_human_size;
use super::*;

#[test]
fn test_tool_error_invalid_params() {
    let err = ToolError::invalid_params("test error");
    assert_eq!(err.code, INVALID_PARAMS);
    assert_eq!(err.message, "test error");
}

#[test]
fn test_tool_error_internal() {
    let err = ToolError::internal("internal error");
    assert_eq!(err.code, INTERNAL_ERROR);
    assert_eq!(err.message, "internal error");
}

#[test]
fn test_user_path_param_expands_tilde() {
    let home = dirs::home_dir().expect("home dir").to_string_lossy().to_string();

    // Pre-fix, `~/Downloads` passed through raw and failed the existence check.
    let params = json!({"path": "~/Downloads"});
    assert_eq!(user_path_param(&params, "path").unwrap(), format!("{home}/Downloads"));

    // Bare `~` expands to home itself
    let params = json!({"path": "~"});
    assert_eq!(user_path_param(&params, "path").unwrap(), home);
}

#[test]
fn test_user_path_param_missing_param() {
    let err = user_path_param(&json!({}), "path").unwrap_err();
    assert_eq!(err.code, INVALID_PARAMS);
    assert_eq!(err.message, "Missing 'path' parameter");
}

#[test]
fn test_expand_user_path_leaves_non_tilde_paths_untouched() {
    // Absolute paths
    assert_eq!(expand_user_path("/tmp"), "/tmp");
    // Virtual paths must never be expanded
    assert_eq!(expand_user_path("mtp://device-1/DCIM"), "mtp://device-1/DCIM");
    // `~` only expands as the leading segment
    assert_eq!(expand_user_path("/tmp/~/x"), "/tmp/~/x");
    // `~user` syntax is not supported, so it passes through
    assert_eq!(expand_user_path("~root/x"), "~root/x");
}

fn pane_state_with(files: Vec<(&str, bool)>, cursor_index: usize, selected: Vec<usize>) -> PaneState {
    PaneState {
        path: "/test".to_string(),
        files: files
            .into_iter()
            .map(|(name, is_directory)| crate::mcp::pane_state::PaneFileEntry {
                name: name.to_string(),
                path: format!("/test/{name}"),
                is_directory,
                size: None,
                recursive_size: None,
                recursive_size_updating: None,
                modified: None,
                tags: vec![],
                ..Default::default()
            })
            .collect(),
        cursor_index,
        selected_indices: selected,
        total_files: 0,
        ..Default::default()
    }
}

#[test]
fn test_empty_operation_error_selection_wins() {
    let mut state = pane_state_with(vec![("..", true), ("a.txt", false)], 0, vec![1]);
    state.total_files = 2;
    assert!(file_ops::empty_operation_error(&state, "left", "copy").is_none());
}

#[test]
fn test_empty_operation_error_cursor_on_parent() {
    // Pre-fix this surfaced as a misleading 1500 ms "frontend may be stalled" timeout.
    let mut state = pane_state_with(vec![("..", true), ("a.txt", false)], 0, vec![]);
    state.total_files = 2;
    let msg = file_ops::empty_operation_error(&state, "left", "delete").expect("should reject");
    assert!(msg.contains("Nothing to delete"));
    assert!(msg.contains("parent entry"));
}

#[test]
fn test_empty_operation_error_cursor_fallback_proceeds() {
    let mut state = pane_state_with(vec![("..", true), ("a.txt", false)], 1, vec![]);
    state.total_files = 2;
    assert!(file_ops::empty_operation_error(&state, "left", "copy").is_none());
}

#[test]
fn test_empty_operation_error_empty_pane() {
    // Synced empty volume root: zero files, zero total
    let state = pane_state_with(vec![], 0, vec![]);
    let msg = file_ops::empty_operation_error(&state, "right", "move").expect("should reject");
    assert!(msg.contains("the right pane shows no files"));
}

#[test]
fn test_empty_operation_error_empty_dir_with_unrendered_parent() {
    // Synced empty dir: the FE renders the empty-state overlay (no rows, not even `..`),
    // so the push has zero files while total_files still counts the parent entry.
    let mut state = pane_state_with(vec![], 0, vec![]);
    state.total_files = 1;
    state.has_parent_row = true;
    let msg = file_ops::empty_operation_error(&state, "left", "copy").expect("should reject");
    assert!(msg.contains("shows no files"));
}

#[test]
fn a_parentless_pane_holding_one_row_is_not_an_empty_pane() {
    // `total_files == 1` reads as "only the parent entry" ONLY on a pane that has
    // one. A search-results snapshot has no `..` row, and neither does a pane at a
    // volume root, so one counted row there is one real file. Refusing the delete
    // would be a false "shows no files" over a file the user can see.
    let mut state = pane_state_with(vec![], 0, vec![]);
    state.total_files = 1;
    state.has_parent_row = false;
    assert!(file_ops::empty_operation_error(&state, "left", "delete").is_none());
}

#[test]
fn a_snapshot_pane_with_rows_and_no_selection_falls_back_to_its_cursor_row() {
    // The shape a search-results pane pushes: no `..` row, real result rows, the
    // cursor on one of them, nothing selected. The op resolves the cursor row, so
    // the gate must let it through. Before the pane synced to MCP at all, the store
    // still held the directory it came FROM, and a cursor parked on that pane's `..`
    // refused an MCP delete the user could plainly see a target for.
    let mut state = pane_state_with(vec![("a.txt", false), ("b.txt", false)], 1, vec![]);
    state.path = "search-results://snap-1".to_string();
    state.total_files = 2;
    state.has_parent_row = false;
    assert!(file_ops::empty_operation_error(&state, "left", "delete").is_none());
}

#[test]
fn an_empty_snapshot_pane_still_reports_it_has_nothing_to_act_on() {
    let mut state = pane_state_with(vec![], 0, vec![]);
    state.path = "search-results://snap-empty".to_string();
    state.total_files = 0;
    state.has_parent_row = false;
    let msg = file_ops::empty_operation_error(&state, "left", "delete").expect("should reject");
    assert!(msg.contains("shows no files"));
}

#[test]
fn test_empty_operation_error_unsynced_state_passes_through() {
    // Default state (no push yet, path empty): the FE is the authority, don't reject.
    let mut state = pane_state_with(vec![], 0, vec![]);
    state.path = String::new();
    assert!(file_ops::empty_operation_error(&state, "left", "copy").is_none());
}

#[test]
fn test_empty_operation_error_cursor_outside_loaded_window() {
    // Cursor at global index 5 but the loaded window starts at 100: we can't see the
    // entry, so the FE stays the authority and the operation proceeds.
    let mut state = pane_state_with(vec![("z.txt", false)], 5, vec![]);
    state.loaded_start = 100;
    state.loaded_end = 101;
    state.total_files = 200;
    assert!(file_ops::empty_operation_error(&state, "left", "copy").is_none());
}

#[test]
fn test_optional_pane_param() {
    // Absent → None (the FE defaults to its focused pane).
    assert_eq!(optional_pane_param(&json!({})).unwrap(), None);
    // Valid values pass through.
    assert_eq!(optional_pane_param(&json!({"pane": "left"})).unwrap(), Some("left"));
    assert_eq!(optional_pane_param(&json!({"pane": "right"})).unwrap(), Some("right"));
    // An unknown value is an honest error, not a silent default.
    let err = optional_pane_param(&json!({"pane": "middle"})).unwrap_err();
    assert_eq!(err.code, INVALID_PARAMS);
}

#[test]
fn test_is_virtual_path() {
    // Scheme-prefixed virtual paths skip the local existence check
    assert!(is_virtual_path("mtp://device-1/DCIM"));
    assert!(is_virtual_path("smb://nas.local/share/folder"));
    // Local paths don't
    assert!(!is_virtual_path("/Users/jane/Documents"));
    assert!(!is_virtual_path("~/Downloads"));
    assert!(!is_virtual_path(""));
    // A "://" that isn't a scheme prefix isn't virtual
    assert!(!is_virtual_path("/tmp/weird://name"));
    assert!(!is_virtual_path("://no-scheme"));
}

#[tokio::test]
async fn test_validate_path_exists() {
    // Local existing path passes
    assert!(validate_path_exists("/tmp").await.is_ok());
    // Local missing path is invalid_params
    let err = validate_path_exists("/nonexistent/path/xyz").await.unwrap_err();
    assert_eq!(err.code, INVALID_PARAMS);
    // Virtual paths skip the check entirely
    assert!(validate_path_exists("mtp://device/DCIM").await.is_ok());
    assert!(validate_path_exists("smb://server/share/missing").await.is_ok());
}

/// A path a ROUTE serves has no inode, so `Path::exists()` answers a confident
/// false and would refuse every `nav` into a repo's snapshots or into a zip.
#[tokio::test]
async fn validate_path_exists_lets_a_routed_path_through() {
    use crate::file_system::git;

    git::wiring::set_virtual_portal_enabled(true);
    assert!(
        validate_path_exists("/tmp/some-repo/.git/branches/main").await.is_ok(),
        "a snapshot path is the portal's to answer for"
    );
    assert!(
        validate_path_exists("/tmp/bundle.zip/inner.txt").await.is_ok(),
        "an archive-inner path is the archive volume's"
    );

    // The real files under `.git/` are ordinary local files, so they keep the check.
    let err = validate_path_exists("/tmp/some-repo/.git/config").await.unwrap_err();
    assert_eq!(err.code, INVALID_PARAMS);

    // And with the portal off, a snapshot path is an ordinary missing local path.
    git::wiring::set_virtual_portal_enabled(false);
    let err = validate_path_exists("/tmp/some-repo/.git/branches/main")
        .await
        .unwrap_err();
    assert_eq!(err.code, INVALID_PARAMS);
    git::wiring::set_virtual_portal_enabled(true);
}

#[test]
fn test_path_exists_validation() {
    // Test that Path::new().exists() works as expected for our validation
    assert!(Path::new("/").exists(), "Root should exist");
    assert!(Path::new("/tmp").exists(), "Temp dir should exist");
    assert!(
        !Path::new("/nonexistent/path/that/does/not/exist").exists(),
        "Nonexistent path should not exist"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn test_volume_list_not_empty() {
    // Verify we can list volumes for validation
    let locations = crate::volumes::list_locations();
    assert!(!locations.is_empty(), "Should have at least one volume");
    // Should have a main volume
    assert!(
        locations
            .iter()
            .any(|l| l.category == crate::volumes::LocationCategory::MainVolume),
        "Should have main volume"
    );
}

#[test]
fn test_parse_human_size_with_space() {
    // SI symbols are base 1000, the way Cmdr's SI sizes and the "MB" in its copy mean them.
    assert_eq!(parse_human_size("1 MB").unwrap(), 1_000_000);
    assert_eq!(parse_human_size("500 kB").unwrap(), 500_000);
    assert_eq!(parse_human_size("2 GB").unwrap(), 2_000_000_000);
    assert_eq!(parse_human_size("1 TB").unwrap(), 1_000_000_000_000);
    assert_eq!(parse_human_size("100 B").unwrap(), 100);
}

#[test]
fn test_parse_human_size_no_space() {
    assert_eq!(parse_human_size("1MB").unwrap(), 1_000_000);
    assert_eq!(parse_human_size("500KB").unwrap(), 500_000);
    assert_eq!(parse_human_size("2GB").unwrap(), 2_000_000_000);
}

#[test]
fn test_parse_human_size_case_insensitive() {
    // Uppercase "KB" (no i) is read leniently as the SI kilobyte, like "kB".
    assert_eq!(parse_human_size("1 mb").unwrap(), 1_000_000);
    assert_eq!(parse_human_size("500 kb").unwrap(), 500_000);
    assert_eq!(parse_human_size("500 KB").unwrap(), 500_000);
    assert_eq!(parse_human_size("1 Mb").unwrap(), 1_000_000);
}

#[test]
fn test_parse_human_size_iec() {
    // The IEC binary symbols Cmdr's own sizes read in (`search::format_size`), so a size the
    // model read off a listing parses back to the same bytes.
    assert_eq!(parse_human_size("1 KiB").unwrap(), 1_024);
    assert_eq!(parse_human_size("1.5 MiB").unwrap(), 1_572_864);
    assert_eq!(parse_human_size("2GiB").unwrap(), 2_147_483_648);
    assert_eq!(parse_human_size("1 tib").unwrap(), 1_099_511_627_776);
}

#[test]
fn test_parse_human_size_decimal() {
    assert_eq!(parse_human_size("1.5 MB").unwrap(), 1_500_000);
    assert_eq!(parse_human_size("0.5 GiB").unwrap(), 536_870_912);
}

#[test]
fn test_parse_human_size_invalid() {
    assert!(parse_human_size("abc").is_err());
    assert!(parse_human_size("MB").is_err());
}

// === parse_mcp_response: the per-request completion signal ===
//
// Round-trip tools (`refresh`, `select`, `move_cursor`, `nav_to_path`, …) wait for
// an `mcp-response` event whose `requestId` matches the one they generated. This
// correlation is what makes the ack per-request: a state push (or its dedupe) is
// irrelevant, and a reply to some OTHER request must never satisfy ours.

#[test]
fn parse_mcp_response_accepts_matching_ok() {
    let payload = r#"{"requestId":"r-1","ok":true}"#;
    assert_eq!(parse_mcp_response(payload, "r-1"), Some(Ok(())));
}

#[test]
fn parse_mcp_response_maps_failure_to_error_message() {
    let payload = r#"{"requestId":"r-1","ok":false,"error":"Refresh timed out — the volume may be unresponsive"}"#;
    assert_eq!(
        parse_mcp_response(payload, "r-1"),
        Some(Err("Refresh timed out — the volume may be unresponsive".to_string()))
    );
}

#[test]
fn parse_mcp_response_failure_without_message_is_unknown_error() {
    let payload = r#"{"requestId":"r-1","ok":false}"#;
    assert_eq!(
        parse_mcp_response(payload, "r-1"),
        Some(Err("Unknown error".to_string()))
    );
}

#[test]
fn parse_mcp_response_missing_ok_field_is_a_failure_not_a_success() {
    // `ok` defaults to false: a malformed reply must never turn into a false-positive OK.
    let payload = r#"{"requestId":"r-1"}"#;
    assert_eq!(
        parse_mcp_response(payload, "r-1"),
        Some(Err("Unknown error".to_string()))
    );
}

#[test]
fn parse_mcp_response_ignores_other_requests() {
    // The heart of the per-request contract: someone else's reply is not ours.
    let payload = r#"{"requestId":"r-2","ok":true}"#;
    assert_eq!(parse_mcp_response(payload, "r-1"), None);
}

#[test]
fn parse_mcp_response_ignores_malformed_payloads() {
    assert_eq!(parse_mcp_response("not json", "r-1"), None);
    assert_eq!(parse_mcp_response(r#"{"requestId":42,"ok":true}"#, "r-1"), None);
    assert_eq!(parse_mcp_response(r#"{"ok":true}"#, "r-1"), None);
}

// === parse_nav_response + nav_result: the ack says what the pane DID ===
//
// `navigate()`'s volume-switch arm resolves its `settled` promise before the new
// volume lists anything, so "the FE replied" never meant "the pane got there": a
// cross-volume `nav_to_path` acked `OK: Navigated …` while an MTP-fatal fallback
// quietly moved the pane home. The FE now names the outcome and the backend words
// it, branching on the discriminant and never on message text.

#[test]
fn parse_nav_response_reads_the_landing_the_frontend_reported() {
    let landed = r#"{"requestId":"r-1","ok":true,"outcome":"navigated","path":"/Users/david"}"#;
    assert_eq!(
        parse_nav_response(landed, "r-1"),
        Some(Ok(NavAck::Navigated {
            path: "/Users/david".to_string()
        }))
    );

    let fell_back = r#"{"requestId":"r-1","ok":false,"outcome":"fell-back","path":"/Users/david"}"#;
    assert_eq!(
        parse_nav_response(fell_back, "r-1"),
        Some(Ok(NavAck::FellBack {
            path: "/Users/david".to_string()
        }))
    );

    let unsettled = r#"{"requestId":"r-1","ok":false,"outcome":"did-not-settle","path":"smb://nas/share"}"#;
    assert_eq!(
        parse_nav_response(unsettled, "r-1"),
        Some(Ok(NavAck::DidNotSettle {
            path: "smb://nas/share".to_string()
        }))
    );
}

#[test]
fn parse_nav_response_keeps_a_pre_move_refusal_verbatim() {
    // The declines that happen before the pane moves (no explorer, an unresolvable
    // path, a synchronous refusal) carry no outcome and keep their exact message.
    let payload = r#"{"requestId":"r-1","ok":false,"error":"Pane is on the Network volume."}"#;
    assert_eq!(
        parse_nav_response(payload, "r-1"),
        Some(Err("Pane is on the Network volume.".to_string()))
    );
}

#[test]
fn parse_nav_response_never_turns_a_malformed_reply_into_an_arrival() {
    // No outcome and no `ok` is a failure, same rule as `parse_mcp_response`.
    assert_eq!(
        parse_nav_response(r#"{"requestId":"r-1"}"#, "r-1"),
        Some(Err("Unknown error".to_string()))
    );
    // An unknown outcome is not a landing either.
    assert_eq!(
        parse_nav_response(r#"{"requestId":"r-1","outcome":"teleported"}"#, "r-1"),
        Some(Err("Unknown error".to_string()))
    );
    // And someone else's reply is still not ours.
    assert_eq!(
        parse_nav_response(r#"{"requestId":"r-2","ok":true,"outcome":"navigated"}"#, "r-1"),
        None
    );
}

#[test]
fn nav_result_reports_the_landing_place_not_the_request() {
    let ok = nav_result(
        "left",
        "/tmp/link",
        NavAck::Navigated {
            path: "/tmp/link".to_string(),
        },
    )
    .expect("navigated is a success");
    assert_eq!(ok, json!("OK: Navigated left pane to /tmp/link"));

    let fell_back = nav_result(
        "left",
        "mtp://phone/DCIM",
        NavAck::FellBack {
            path: "/Users/david".to_string(),
        },
    )
    .expect_err("a fallback is not an OK");
    assert!(fell_back.message.contains("mtp://phone/DCIM"), "names the request");
    assert!(fell_back.message.contains("/Users/david"), "names where it landed");

    let unsettled = nav_result(
        "right",
        "smb://nas/share",
        NavAck::DidNotSettle {
            path: "smb://nas/share".to_string(),
        },
    )
    .expect_err("an unsettled pane is not an OK");
    assert!(unsettled.message.contains("didn't settle"));
}

// === A stalled folder answers at once, not after the 30 s budget ===
//
// A folder whose server stopped answering keeps its listing alive and retrying, so
// the pane never came to rest and `nav_to_path` used to wait out its whole budget.
// The FE now replies `stalled` the moment the pane shows the stall, and the result
// says so in a shape an agent can branch on.

#[test]
fn parse_nav_response_reads_a_stalled_folder() {
    let stalled = r#"{"requestId":"r-1","ok":false,"outcome":"stalled","path":"/Volumes/nas/photos"}"#;
    assert_eq!(
        parse_nav_response(stalled, "r-1"),
        Some(Ok(NavAck::Stalled {
            path: "/Volumes/nas/photos".to_string()
        }))
    );
}

#[test]
fn nav_result_says_the_folder_is_stalled() {
    let stalled = nav_result(
        "left",
        "/Volumes/nas/photos",
        NavAck::Stalled {
            path: "/Volumes/nas/photos".to_string(),
        },
    )
    .expect_err("a stalled folder is not an OK");
    assert!(stalled.message.contains("/Volumes/nas/photos"), "names the folder");
    assert!(stalled.message.contains("isn't answering"), "says why");
    assert_eq!(
        stalled.data,
        Some(json!({ "reason": "folderStalled", "path": "/Volumes/nas/photos" }))
    );
}

#[test]
fn select_volume_result_says_the_folder_is_stalled() {
    let stalled = select_volume_result(
        "right",
        "naspi",
        NavAck::Stalled {
            path: "/Volumes/naspi/photos".to_string(),
        },
    )
    .expect_err("a stalled folder is not an OK");
    assert!(stalled.message.contains("naspi"), "names the request");
    assert!(stalled.message.contains("isn't answering"), "says why");
    assert_eq!(
        stalled.data,
        Some(json!({ "reason": "folderStalled", "path": "/Volumes/naspi/photos" }))
    );
}

// === select_volume_result: the ack says where the switch left the pane ===
//
// A volume select reopens the folder last used on that volume, after a background check
// that can take a moment. Acking once the pane merely showed the volume let that check
// land after the reply and supersede the agent's next `nav_to_path`. The FE now replies
// once the pane has come to rest, and the result names the folder it opened.

#[test]
fn select_volume_result_names_the_folder_the_pane_opened() {
    let ok = select_volume_result(
        "left",
        "Internal Storage",
        NavAck::Navigated {
            path: "mtp://1/65537/Documents".to_string(),
        },
    )
    .expect("navigated is a success");
    assert_eq!(
        ok,
        json!("OK: Switched left pane to volume Internal Storage, at mtp://1/65537/Documents")
    );
}

#[test]
fn select_volume_result_is_not_an_ok_when_the_pane_rests_elsewhere_or_never_settles() {
    let fell_back = select_volume_result(
        "left",
        "Internal Storage",
        NavAck::FellBack {
            path: "/Users/david".to_string(),
        },
    )
    .expect_err("a fallback is not an OK");
    assert!(fell_back.message.contains("Internal Storage"), "names the request");
    assert!(fell_back.message.contains("/Users/david"), "names where it landed");

    let unsettled = select_volume_result(
        "right",
        "Naspolya",
        NavAck::DidNotSettle {
            path: "sftp://ada@nas.local:22/srv/data".to_string(),
        },
    )
    .expect_err("an unsettled pane is not an OK");
    assert!(unsettled.message.contains("Naspolya"), "names the request");
    assert!(unsettled.message.contains("didn't settle"));
}

#[test]
fn select_volume_result_leaves_out_the_folder_when_the_reply_names_none() {
    let ok = select_volume_result("left", "Macintosh HD", NavAck::Navigated { path: String::new() })
        .expect("still a success");
    assert_eq!(ok, json!("OK: Switched left pane to volume Macintosh HD"));
}

#[test]
fn volume_id_selects_the_adb_connection_when_mtp_has_the_same_name() {
    let rows = [
        SelectableVolume {
            id: "mtp-pixel:1".to_string(),
            name: "Pixel 9".to_string(),
            is_favorite: false,
        },
        SelectableVolume {
            id: "adb-pixel".to_string(),
            name: "Pixel 9".to_string(),
            is_favorite: false,
        },
    ];

    let selected = resolve_volume_selector(&rows, Some("adb-pixel"), Some("stale name"))
        .expect("the stable id wins over display copy");
    assert_eq!(selected.id, "adb-pixel");
}

#[test]
fn duplicate_volume_name_is_refused_with_the_ids_that_disambiguate_it() {
    let rows = [
        SelectableVolume {
            id: "mtp-pixel:1".to_string(),
            name: "Pixel 9".to_string(),
            is_favorite: false,
        },
        SelectableVolume {
            id: "adb-pixel".to_string(),
            name: "Pixel 9".to_string(),
            is_favorite: false,
        },
    ];

    let err = resolve_volume_selector(&rows, None, Some("Pixel 9")).expect_err("a duplicate name is ambiguous");
    assert_eq!(
        err.data,
        Some(json!({
            "reason": "ambiguousVolumeName",
            "matchingVolumeIds": ["mtp-pixel:1", "adb-pixel"]
        }))
    );
}

#[test]
fn unknown_volume_id_reports_the_available_stable_references() {
    let rows = [SelectableVolume {
        id: "adb-pixel".to_string(),
        name: "Pixel 9".to_string(),
        is_favorite: false,
    }];

    let err = resolve_volume_selector(&rows, Some("missing"), None).expect_err("an unknown id is not selectable");
    assert_eq!(
        err.data,
        Some(json!({
            "reason": "volumeNotFound",
            "availableVolumes": ["Pixel 9 (adb-pixel)", "Servers (network)"]
        }))
    );
}

#[test]
fn nav_result_falls_back_to_the_requested_path_when_the_reply_names_none() {
    let ok = nav_result("left", "/Users", NavAck::Navigated { path: String::new() }).expect("still a success");
    assert_eq!(ok, json!("OK: Navigated left pane to /Users"));
}

// === parse_operation_start_response: the autoConfirm-op correlation ===
//
// The auto-confirmed copy/move/delete/compress paths wait for the FE to reply
// with the spawned `operationId`, so the tool's OK carries the exact id (and a
// follow-up `queue` / `await operation_complete` is directly sequenced). This is
// the parser under that round-trip.

#[test]
fn parse_operation_start_response_extracts_the_spawned_operation_id() {
    let payload = r#"{"requestId":"r-1","ok":true,"operationId":"op-42"}"#;
    assert_eq!(
        parse_operation_start_response(payload, "r-1"),
        Some(Ok(OperationStartAck::Started {
            operation_id: Some("op-42".to_string())
        }))
    );
}

#[test]
fn parse_operation_start_response_ok_without_id_is_a_spawnless_ack() {
    // Compress auto-confirm on an existing target keeps its dialog open and mints
    // no op: an OK with no operationId, not a failure.
    let payload = r#"{"requestId":"r-1","ok":true}"#;
    assert_eq!(
        parse_operation_start_response(payload, "r-1"),
        Some(Ok(OperationStartAck::Started { operation_id: None }))
    );
}

#[test]
fn parse_operation_start_response_maps_failure_to_error() {
    let payload = r#"{"requestId":"r-1","ok":false,"error":"Nothing to copy"}"#;
    assert_eq!(
        parse_operation_start_response(payload, "r-1"),
        Some(Err("Nothing to copy".to_string()))
    );
    // A `blockedBy` that isn't a dialog id names nothing, so the sentence is all there is.
    let malformed = r#"{"requestId":"r-1","ok":false,"blockedBy":7,"error":"Nothing to copy"}"#;
    assert_eq!(
        parse_operation_start_response(malformed, "r-1"),
        Some(Err("Nothing to copy".to_string()))
    );
}

#[test]
fn parse_operation_start_response_keeps_the_frontend_dialog_refusal_typed() {
    // The FE's own gate runs after the Rust pre-check, so a dialog that opens in between
    // is caught there and named in `blockedBy`. Read as a plain failure, the agent got
    // the sentence and no `data.blockingDialog`.
    let payload = r#"{"requestId":"r-1","ok":false,"blockedBy":"transfer-progress","error":"The transfer-progress dialog is open, so nothing new can start. Close it first, then try again."}"#;
    assert_eq!(
        parse_operation_start_response(payload, "r-1"),
        Some(Ok(OperationStartAck::Blocked {
            blocking_dialog: "transfer-progress".to_string()
        }))
    );
}

#[test]
fn parse_operation_start_response_ignores_other_requests_and_junk() {
    assert_eq!(
        parse_operation_start_response(r#"{"requestId":"r-2","ok":true,"operationId":"op-9"}"#, "r-1"),
        None
    );
    assert_eq!(parse_operation_start_response("not json", "r-1"), None);
}

// ── Fitting a result to the caller's context ──────────────────────────────────

#[test]
fn fit_to_result_budget_keeps_everything_that_fits() {
    let small: Vec<String> = (0..20).map(|i| format!("row-{i}")).collect();
    let fitted = fit_to_result_budget(small);
    assert_eq!(fitted.items.len(), 20);
    assert_eq!(fitted.total, 20);
    assert!(!fitted.truncated, "a small page is never cut");
}

#[test]
fn fit_to_result_budget_cuts_at_the_ceiling_and_reports_the_counts() {
    use crate::agent::chat::budget::CHARS_PER_TOKEN_ESTIMATE;

    // Rows of ~1k estimated tokens each: a handful fit, the rest must be reported as cut
    // rather than shipped and pushed out of the model's context downstream.
    let row_chars = 1_000 * CHARS_PER_TOKEN_ESTIMATE;
    let rows: Vec<String> = (0..50).map(|_| "x".repeat(row_chars)).collect();
    let fitted = fit_to_result_budget(rows);

    assert_eq!(fitted.total, 50);
    assert!(fitted.truncated, "the page must say it was cut");
    assert!(!fitted.items.is_empty(), "it always returns something");
    assert!(fitted.items.len() < 50, "and not everything");
    let spent: usize = fitted.items.iter().map(estimate_serialized_tokens).sum();
    assert!(
        spent <= MAX_TOOL_RESULT_TOKENS,
        "what it keeps must fit the ceiling (spent {spent})"
    );
}

#[test]
fn fit_to_result_budget_never_returns_an_empty_page() {
    use crate::agent::chat::budget::CHARS_PER_TOKEN_ESTIMATE;

    // One row bigger than the whole ceiling: keep it (truncated to one) rather than answer
    // with nothing, which would tell the model neither what it found nor that it exists.
    let huge = "y".repeat(MAX_TOOL_RESULT_TOKENS * CHARS_PER_TOKEN_ESTIMATE * 2);
    let fitted = fit_to_result_budget(vec![huge, "second".to_string()]);
    assert_eq!(fitted.items.len(), 1);
    assert_eq!(fitted.total, 2);
    assert!(fitted.truncated);
}

// ── The operation-start gate ──────────────────────────────────────────────────
//
// An agent asked to copy while a dialog is up used to get one of two useless
// answers: silence for ten seconds (the auto-confirm round-trip waiting for an
// operation the FE would never start), or a generic ack timeout. Now it gets the
// reason immediately, and the dialog's identity in a field it can act on.
//
// Which dialogs block lives in the FE's `dialog-registry.ts` and rides over IPC as
// `KnownDialog::blocks_operations`; `SoftDialogTracker::blocking_dialog` picks the
// topmost open one (tested in `mcp/dialog_state.rs`).

#[test]
fn a_blocked_operation_names_the_dialog_in_a_typed_field() {
    let err = dialog_block_error("copy", "transfer-progress");

    // ❌ The agent must never have to parse the sentence to learn this.
    assert_eq!(
        err.data.as_ref().and_then(|d| d.get("blockingDialog")),
        Some(&json!("transfer-progress"))
    );
}

#[test]
fn a_blocked_operation_says_what_to_do_about_it() {
    let err = dialog_block_error("delete", "search");

    assert!(err.message.contains("search"), "it names the dialog: {}", err.message);
    assert!(
        err.message.contains("Close it first"),
        "and the way out: {}",
        err.message
    );
    // Refusing a request the caller can fix is a bad-input answer, not a server fault.
    assert_eq!(err.code, INVALID_PARAMS);
}

#[test]
fn a_frontend_dialog_refusal_reaches_the_agent_exactly_like_the_rust_pre_check() {
    let from_frontend = operation_start_result(
        "copy",
        OperationStartAck::Blocked {
            blocking_dialog: "transfer-progress".to_string(),
        },
    )
    .expect_err("a blocked start is not an OK");
    let from_pre_check = dialog_block_error("copy", "transfer-progress");

    assert_eq!(from_frontend.code, from_pre_check.code);
    assert_eq!(from_frontend.message, from_pre_check.message);
    assert_eq!(from_frontend.data, from_pre_check.data);
}

#[test]
fn a_started_operation_passes_its_id_through() {
    let started = operation_start_result(
        "copy",
        OperationStartAck::Started {
            operation_id: Some("op-42".to_string()),
        },
    )
    .expect("a start is an OK");
    assert_eq!(started, Some("op-42".to_string()));
}

// === parse_tab_move_response + tab_move_result: a tab move names what it did ===
//
// The frontend owns a move's rules (one `moveTab`, shared with the tab drag), so its
// reply carries a typed outcome and the backend only words it.

#[test]
fn parse_tab_move_response_reads_the_outcome_the_frontend_reported() {
    let moved = r#"{"requestId":"r-1","ok":true,"outcome":"moved","toIndex":2}"#;
    assert_eq!(
        parse_tab_move_response(moved, "r-1"),
        Some(Ok(TabMoveAck::Moved { to_index: 2 }))
    );

    let unchanged = r#"{"requestId":"r-1","ok":true,"outcome":"unchanged"}"#;
    assert_eq!(
        parse_tab_move_response(unchanged, "r-1"),
        Some(Ok(TabMoveAck::Unchanged))
    );

    for (outcome, refusal) in [
        ("pinned", TabMoveRefusal::Pinned),
        ("onlyTab", TabMoveRefusal::OnlyTab),
        ("targetFull", TabMoveRefusal::TargetFull),
        ("notFound", TabMoveRefusal::NotFound),
    ] {
        let payload = format!(r#"{{"requestId":"r-1","ok":false,"outcome":"{outcome}"}}"#);
        assert_eq!(
            parse_tab_move_response(&payload, "r-1"),
            Some(Ok(TabMoveAck::Refused(refusal))),
            "{outcome}"
        );
    }
}

#[test]
fn parse_tab_move_response_ignores_a_reply_meant_for_another_request() {
    let payload = r#"{"requestId":"r-2","ok":true,"outcome":"moved","toIndex":0}"#;
    assert_eq!(parse_tab_move_response(payload, "r-1"), None);
    assert_eq!(parse_tab_move_response("not json", "r-1"), None);
}

#[test]
fn parse_tab_move_response_keeps_a_pre_move_decline_verbatim() {
    let payload = r#"{"requestId":"r-1","ok":false,"error":"Explorer is not ready"}"#;
    assert_eq!(
        parse_tab_move_response(payload, "r-1"),
        Some(Err("Explorer is not ready".to_string()))
    );
}

#[test]
fn parse_tab_move_response_never_turns_a_malformed_reply_into_an_ok() {
    // An `ok` that doesn't say what happened, and a "moved" that doesn't say where.
    let no_outcome = r#"{"requestId":"r-1","ok":true}"#;
    assert!(matches!(parse_tab_move_response(no_outcome, "r-1"), Some(Err(_))));

    let no_index = r#"{"requestId":"r-1","ok":true,"outcome":"moved"}"#;
    assert!(matches!(parse_tab_move_response(no_index, "r-1"), Some(Err(_))));
}

#[test]
fn tab_move_result_says_where_the_tab_landed() {
    let moved = tab_move_result("t1", "left", "right", TabMoveAck::Moved { to_index: 3 }).expect("a move is OK");
    assert_eq!(moved, json!("OK: Moved tab t1 to index 3 in right pane"));

    let unchanged = tab_move_result("t1", "left", "left", TabMoveAck::Unchanged).expect("a no-op is OK");
    assert!(unchanged.as_str().is_some_and(|text| text.starts_with("OK:")));
}

#[test]
fn tab_move_result_refuses_with_a_typed_reason() {
    for (refusal, reason) in [
        (TabMoveRefusal::Pinned, "tabPinned"),
        (TabMoveRefusal::OnlyTab, "onlyTab"),
        (TabMoveRefusal::TargetFull, "tabLimitReached"),
        (TabMoveRefusal::NotFound, "tabNotFound"),
    ] {
        let err =
            tab_move_result("t1", "left", "right", TabMoveAck::Refused(refusal)).expect_err("a refusal is an error");
        assert_eq!(err.code, INVALID_PARAMS);
        assert_eq!(err.data, Some(json!({ "reason": reason })));
    }
}
