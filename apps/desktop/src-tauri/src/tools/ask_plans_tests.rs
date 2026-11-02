//! Tests for `ask_plans.rs`: what each plan asks about, and that every `Sensitive` tool has one.

use serde_json::json;

use super::*;
use crate::tools::registry::Access;
use crate::tools::table::{get_all_tools, tool_access, tool_has_ask_plan};

fn file(activation: Activation, name: &str) -> CursorItem {
    CursorItem {
        activation,
        path: format!("/Users/me/{name}"),
        name: name.to_string(),
        is_directory: false,
    }
}

fn bundle(activation: Activation) -> CursorItem {
    CursorItem {
        is_directory: true,
        ..file(activation, "Thing.app")
    }
}

fn asks(plan: &AskPlan) -> &AskFacts {
    match plan {
        AskPlan::Ask(facts) => facts,
        other => panic!("expected a question, got {other:?}"),
    }
}

#[test]
fn every_sensitive_tool_has_a_plan_and_nothing_else_does() {
    for tool in get_all_tools() {
        let sensitive = tool_access(&tool.name) == Some(Access::Sensitive);
        assert_eq!(tool_has_ask_plan(&tool.name), sensitive, "{}", tool.name);
    }
}

#[test]
fn a_folder_opens_with_no_question() {
    let folder = CursorItem {
        is_directory: true,
        ..file(Activation::Browse, "Photos")
    };
    assert!(matches!(open_plan(Some(&folder), None), AskPlan::NoAsk));
}

#[test]
fn no_entry_under_the_cursor_is_the_handlers_to_refuse() {
    assert!(matches!(open_plan(None, None), AskPlan::NoAsk));
}

#[test]
fn a_file_in_the_viewer_launches_nothing() {
    assert!(matches!(
        open_plan(Some(&file(Activation::Viewer, "a.txt")), None),
        AskPlan::NoAsk
    ));
}

#[test]
fn a_file_names_what_opens_it() {
    let plan = open_plan(
        Some(&file(Activation::Launch, "run.command")),
        Some("Terminal".to_string()),
    );
    assert_eq!(
        asks(&plan),
        &AskFacts::OpenFile {
            path: "/Users/me/run.command".to_string(),
            shown_path: "/Users/me/run.command".to_string(),
            name: "run.command".to_string(),
            is_bundle: false,
            opener: Some("Terminal".to_string()),
        }
    );
}

#[test]
fn a_file_nothing_claims_says_so() {
    let plan = open_plan(Some(&file(Activation::Launch, "mystery")), None);
    assert!(matches!(asks(&plan), AskFacts::OpenFile { opener: None, .. }));
}

#[test]
fn a_bundle_under_the_browse_policy_opens_with_no_question() {
    assert!(matches!(
        open_plan(Some(&bundle(Activation::Browse)), None),
        AskPlan::NoAsk
    ));
}

#[test]
fn a_bundle_under_the_open_or_ask_policy_is_a_question() {
    for activation in [Activation::Launch, Activation::Choose] {
        let plan = open_plan(Some(&bundle(activation)), None);
        assert!(matches!(asks(&plan), AskFacts::OpenFile { is_bundle: true, .. }));
    }
}

#[test]
fn the_shown_path_is_bounded_and_the_compared_one_is_exact() {
    let name = format!("{}.command", "a".repeat(2_000));
    let plan = open_plan(Some(&file(Activation::Launch, &name)), None);
    let AskFacts::OpenFile { path, shown_path, .. } = asks(&plan) else {
        panic!("an open question");
    };
    assert!(path.ends_with(&name));
    assert!(shown_path.chars().count() < path.chars().count());
}

#[test]
fn an_unknown_activation_is_a_failed_read_never_a_browse() {
    assert!(
        serde_json::from_value::<CursorItem>(json!({
            "activation": "somethingNew", "path": "/a", "name": "a", "isDirectory": false
        }))
        .is_err()
    );
}

fn setting() -> SettingDescription {
    SettingDescription {
        label: "Text editor".to_string(),
        old_value: json!("Zed"),
        ask_free: false,
        mcp_settable: json!(true),
        sensitive: false,
        locked: false,
        invalid: None,
    }
}

fn refusal_of(plan: AskPlan) -> Value {
    match plan {
        AskPlan::Refuse(error) => error.data.expect("typed")["refusal"].clone(),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_setting_shows_its_old_and_new_value() {
    let plan = setting_plan("behavior.textEditorApp", &json!("Vim"), Some(setting()));
    assert_eq!(
        asks(&plan),
        &AskFacts::SetSetting {
            id: "behavior.textEditorApp".to_string(),
            label: "Text editor".to_string(),
            old_value: "Zed".to_string(),
            new_value: "Vim".to_string(),
        }
    );
}

#[test]
fn a_setting_outside_the_registry_refuses() {
    assert_eq!(
        refusal_of(setting_plan("made.up", &json!(1), None)),
        json!("unknownSetting")
    );
}

#[test]
fn a_setting_marked_ask_free_needs_nobody() {
    let described = SettingDescription {
        ask_free: true,
        ..setting()
    };
    assert!(matches!(
        setting_plan("theme.mode", &json!("dark"), Some(described)),
        AskPlan::NoAsk
    ));
}

#[test]
fn a_setting_that_would_not_change_needs_nobody() {
    assert!(matches!(
        setting_plan("x", &json!("Zed"), Some(setting())),
        AskPlan::NoAsk
    ));
}

#[test]
fn a_consent_answer_refuses_even_if_marked_ask_free() {
    let described = SettingDescription {
        mcp_settable: json!(false),
        ask_free: true,
        ..setting()
    };
    assert_eq!(
        refusal_of(setting_plan("x", &json!(1), Some(described))),
        json!("notSettableOverMcp")
    );
}

#[test]
fn a_data_sharing_switch_only_goes_off() {
    let off_only = SettingDescription {
        mcp_settable: json!("offOnly"),
        old_value: json!(true),
        ..setting()
    };
    assert_eq!(
        refusal_of(setting_plan("x", &json!(true), Some(off_only.clone()))),
        json!("onlyOffOverMcp")
    );
    assert!(matches!(
        setting_plan("x", &json!(false), Some(off_only)),
        AskPlan::Ask(_)
    ));
}

#[test]
fn a_locked_setting_refuses_before_any_question() {
    let described = SettingDescription {
        locked: true,
        ..setting()
    };
    assert_eq!(
        refusal_of(setting_plan("x", &json!("Vim"), Some(described))),
        json!("managedByOrganization")
    );
}

#[test]
fn a_value_the_registry_refuses_never_reaches_a_person() {
    let described = SettingDescription {
        invalid: Some("It has to be a number.".to_string()),
        ..setting()
    };
    assert_eq!(
        refusal_of(setting_plan("x", &json!("Vim"), Some(described))),
        json!("invalidValue")
    );
}

#[test]
fn a_secret_stays_masked_both_ways() {
    let described = SettingDescription {
        sensitive: true,
        old_value: json!("********"),
        ..setting()
    };
    let plan = setting_plan("x", &json!("hunter2"), Some(described));
    let AskFacts::SetSetting {
        old_value, new_value, ..
    } = asks(&plan)
    else {
        panic!("a setting question");
    };
    assert_eq!(old_value, MASKED);
    assert_eq!(new_value, MASKED);
}

#[test]
fn a_changed_old_value_makes_different_facts() {
    let before = setting_plan("x", &json!("Vim"), Some(setting()));
    let after = setting_plan(
        "x",
        &json!("Vim"),
        Some(SettingDescription {
            old_value: json!("Emacs"),
            ..setting()
        }),
    );
    assert_ne!(asks(&before), asks(&after));
}

#[test]
fn a_non_string_value_reads_as_json() {
    assert_eq!(value_text(&json!(true)), "true");
    assert_eq!(value_text(&json!(["a"])), "[\"a\"]");
    assert_eq!(value_text(&json!("a")), "a");
}

#[test]
fn a_wrong_request_id_is_not_this_requests_answer() {
    assert!(parse_cursor_item(r#"{"requestId":"other","ok":true,"item":null}"#, "mine").is_none());
    assert!(parse_setting_description(r#"{"requestId":"other","ok":true}"#, "mine").is_none());
}

#[test]
fn a_failed_description_is_an_error_never_no_setting() {
    assert!(matches!(
        parse_setting_description(r#"{"requestId":"mine","ok":false}"#, "mine"),
        Some(Err(_))
    ));
}
