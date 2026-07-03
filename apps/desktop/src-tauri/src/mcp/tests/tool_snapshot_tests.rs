//! Byte-identical characterization of the `tools/list` wire output.
//!
//! Serializes `{"tools": get_all_tools()}` and compares against a committed fixture. This
//! pins the exact tool set, order, descriptions, and JSON schemas that agents see, so the
//! registry refactor can prove it changed no wire bytes. It's a characterization test
//! (written green over existing behavior), not red→green TDD.
//!
//! To regenerate after a *deliberate* schema/tool change, run the suite with
//! `UPDATE_SNAPSHOT=1` set, review the diff, and commit the updated fixture.

use crate::mcp::tools::get_all_tools;
use serde_json::json;

const SNAPSHOT: &str = include_str!("fixtures/tools_list_snapshot.json");

#[test]
fn tools_list_matches_snapshot() {
    let mut actual = serde_json::to_string_pretty(&json!({ "tools": get_all_tools() })).unwrap();
    actual.push('\n');

    if std::env::var_os("UPDATE_SNAPSHOT").is_some() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/mcp/tests/fixtures/tools_list_snapshot.json"
        );
        std::fs::write(path, &actual).unwrap();
        return;
    }

    assert_eq!(
        actual, SNAPSHOT,
        "tools/list wire output changed. If this was intentional (a deliberate tool/schema \
         edit), regenerate the fixture with UPDATE_SNAPSHOT=1 and review the diff; otherwise \
         the registry refactor drifted the wire bytes and must be fixed."
    );
}
