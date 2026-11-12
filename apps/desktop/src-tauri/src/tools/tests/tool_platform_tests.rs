//! A tool the OS can't run isn't a tool there: off every list, and unknown to a call. The model
//! never plans around something that can only refuse.

use std::collections::BTreeSet;

use serde_json::json;

use crate::agent::llm::types::ToolId;
use crate::platform::{PlatformCapabilities, capabilities};
use crate::tools::{
    Caller, Needs, ToolCtx, agent_tool_declarations, deferred_tool_lines, execute_tool, get_all_tools, meta,
    tool_needs, tool_offered, tools_listed,
};

/// Every tool that names a need, by hand: a need is what takes a tool off an OS, so a name lands
/// here on purpose, having read why its handler can't run everywhere.
const EXPECTED_NEEDS: &[(&str, Needs)] = &[
    ("tag", Needs::ColorTags),
    ("memory_diagnostics", Needs::ProcessDiagnostics),
    ("cpu_diagnostics", Needs::ProcessDiagnostics),
];

fn names(tools: Vec<crate::tools::Tool>) -> BTreeSet<String> {
    tools.into_iter().map(|tool| tool.name).collect()
}

#[test]
fn the_tools_with_a_need_are_exactly_the_expected_ones() {
    let declared: Vec<(String, Needs)> = ToolId::KNOWN
        .iter()
        .filter_map(|tool| Some((tool.as_wire_name().to_string(), tool_needs(tool.as_wire_name())?)))
        .collect();
    let expected: Vec<(String, Needs)> = EXPECTED_NEEDS
        .iter()
        .map(|(name, need)| (name.to_string(), *need))
        .collect();
    assert_eq!(declared, expected);
}

#[test]
fn each_need_reads_its_own_capability() {
    let none = PlatformCapabilities {
        color_tags: false,
        process_diagnostics: false,
        ..capabilities()
    };
    let tags_only = PlatformCapabilities {
        color_tags: true,
        ..none
    };
    let diagnostics_only = PlatformCapabilities {
        process_diagnostics: true,
        ..none
    };
    assert!(!Needs::ColorTags.met_by(&none));
    assert!(!Needs::ProcessDiagnostics.met_by(&none));
    assert!(Needs::ColorTags.met_by(&tags_only));
    assert!(!Needs::ProcessDiagnostics.met_by(&tags_only));
    assert!(!Needs::ColorTags.met_by(&diagnostics_only));
    assert!(Needs::ProcessDiagnostics.met_by(&diagnostics_only));
}

#[test]
fn an_os_that_offers_nothing_lists_none_of_the_tools_with_a_need() {
    let everything = names(tools_listed(|_| true));
    let nothing = names(tools_listed(|_| false));
    let dropped: BTreeSet<String> = everything.difference(&nothing).cloned().collect();
    let expected: BTreeSet<String> = EXPECTED_NEEDS.iter().map(|(name, _)| name.to_string()).collect();
    assert_eq!(dropped, expected);
    assert!(nothing.is_subset(&everything));
}

#[test]
fn an_os_with_color_tags_alone_lists_tag_and_no_diagnostics() {
    let listed = names(tools_listed(|need| need == Needs::ColorTags));
    assert!(listed.contains("tag"));
    assert!(!listed.contains("memory_diagnostics"));
    assert!(!listed.contains("cpu_diagnostics"));
}

/// The lists this OS really hands out, against what this OS really offers. On macOS every need is
/// met, so the Linux lane is where the "absent" half bites.
#[test]
fn this_os_lists_a_tool_exactly_when_it_offers_it() {
    let listed = names(get_all_tools());
    let declared: BTreeSet<String> = agent_tool_declarations()
        .into_iter()
        .map(|tool| tool.name.as_wire_name().to_string())
        .collect();
    let more_tools = deferred_tool_lines();
    for (name, need) in EXPECTED_NEEDS {
        let offered = need.met_here();
        assert_eq!(tool_offered(name), offered, "{name}");
        assert_eq!(listed.contains(*name), offered, "tools/list and {name}");
        assert_eq!(
            more_tools.contains(&format!("- {name}: ")),
            offered,
            "the agent's More tools list and {name}"
        );
        assert!(!declared.contains(*name) || offered, "the agent declares {name}");
        assert_eq!(
            meta::closest_tools(name).iter().any(|close| close == name),
            offered,
            "a near-miss suggestion and {name}"
        );
        assert_eq!(
            meta::schema_for_a_refusal(name).is_some(),
            offered,
            "a refusal's schema and {name}"
        );
    }
}

#[tokio::test]
async fn tool_details_calls_a_tool_this_os_lacks_unknown() {
    for (name, need) in EXPECTED_NEEDS {
        let answer = meta::execute_tool_details(&json!({ "names": [name] }))
            .await
            .expect("tool_details answers");
        let unknown = answer.get("unknown").and_then(|names| names.as_array());
        let says_unknown = unknown.is_some_and(|names| names.iter().any(|unknown| unknown == name));
        assert_eq!(says_unknown, !need.met_here(), "{name}: {answer}");
    }
}

/// A call to a tool this OS lacks gets the answer a made-up name gets, before a handler runs.
#[tokio::test]
async fn a_call_to_a_tool_this_os_lacks_is_an_unknown_tool() {
    let app = tauri::test::mock_app();
    let ctx = ToolCtx {
        caller: Caller::External { direct: true },
        cancel: tokio_util::sync::CancellationToken::new(),
        conversation_id: None,
        asked: None,
    };
    let made_up = execute_tool(app.handle(), &ctx, "definitely_not_a_tool", &json!({}))
        .await
        .expect_err("no such tool");
    for (name, need) in EXPECTED_NEEDS {
        if need.met_here() {
            continue;
        }
        let refused = execute_tool(app.handle(), &ctx, name, &json!({}))
            .await
            .expect_err("not a tool on this OS");
        assert_eq!(refused.code, made_up.code, "{name}");
    }
}
