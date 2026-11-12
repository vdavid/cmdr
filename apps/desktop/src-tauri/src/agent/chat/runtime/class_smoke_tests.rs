//! One scripted turn per access class through the real gate: what a model's call does on a turn the
//! user started and on one Cmdr started, end to end, with `AppHandleDispatcher` and no fake dispatch.
//!
//! Each call carries a stray argument, so the schema gate refuses it right after `admit`. A `problem`
//! therefore proves `admit` let it through, with nothing run and nobody asked.

use tauri::test::mock_app;

use super::dispatch::AppHandleDispatcher;
use super::test_support::*;
use super::*;

/// A tool of each class, and whether `admit` lets a turn Cmdr started run it.
fn classes() -> [(&'static str, ToolId, bool); 9] {
    [
        ("Read", ToolId::ListVolumes, true),
        ("ReadContents", ToolId::InspectFile, true),
        ("Propose", ToolId::ProposeSuggestions, true),
        ("Memory", ToolId::MemoryWrite, true),
        ("Session", ToolId::ConnectVolume, false),
        ("AppState", ToolId::NavToParent, false),
        ("Initiate", ToolId::Copy, false),
        ("LightWrite", ToolId::Mkdir, false),
        ("Sensitive", ToolId::Quit, false),
    ]
}

const ALL_ORIGINS: [TurnOrigin; 4] = [
    TurnOrigin::Text,
    TurnOrigin::Resume,
    TurnOrigin::Wake,
    TurnOrigin::Outcomes,
];

/// What the model read back after its one call, on a turn with `origin`.
async fn answer_to(tool: ToolId, origin: TurnOrigin) -> Value {
    let app = mock_app();
    let conn = migrated_conn();
    let id = conversation(&conn);
    let dispatcher = AppHandleDispatcher::new(app.handle().clone(), id);
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(tool, json!({ "notAParameter": true }))],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["Done.".to_string()],
            usage: AgentUsage::default(),
        },
    ]);
    let (tx, _rx) = unbounded_channel();
    let mut params = params(id, Some("do it"));
    params.origin = origin;
    let result = run_turn(&llm, &dispatcher, &conn, &[], &params, &tx, &CancellationToken::new()).await;
    assert!(matches!(result, TurnResult::Answered { .. }), "{result:?}");

    let prompt = llm.calls_seen().into_iter().last().expect("a second call");
    let last = prompt.last().expect("the transcript ends on a tool row");
    let AgentPart::ToolResult(answer) = last.parts.first().expect("a part") else {
        panic!("expected a tool result, got {:?}", last.parts.first());
    };
    answer.content.clone()
}

#[tokio::test]
async fn every_class_is_past_admit_on_a_turn_the_user_started() {
    for (class, tool, _) in classes() {
        for origin in [TurnOrigin::Text, TurnOrigin::Resume] {
            let answer = answer_to(tool.clone(), origin).await;
            assert!(answer.get("available").is_none(), "{class}: {answer}");
            assert_eq!(
                answer["data"]["unknownProperties"],
                json!(["notAParameter"]),
                "{class}: {answer}"
            );
        }
    }
}

#[tokio::test]
async fn on_a_turn_cmdr_started_only_reads_proposals_and_memory_get_past_admit() {
    for (class, tool, runs_in_background) in classes() {
        for origin in [TurnOrigin::Wake, TurnOrigin::Outcomes] {
            let answer = answer_to(tool.clone(), origin).await;
            if runs_in_background {
                assert_eq!(
                    answer["data"]["unknownProperties"],
                    json!(["notAParameter"]),
                    "{class}: {answer}"
                );
            } else {
                assert_eq!(answer["available"], false, "{class}: {answer}");
                assert_eq!(answer["refusal"], "refusedInBackground", "{class}: {answer}");
            }
        }
    }
}

/// What the model read back after one `read_resource` call on a turn with `origin`.
async fn read_resource_answer(uri: &str, origin: TurnOrigin) -> Value {
    let app = mock_app();
    let conn = migrated_conn();
    let id = conversation(&conn);
    let dispatcher = AppHandleDispatcher::new(app.handle().clone(), id);
    let llm = ProgrammableLlm::new(vec![
        Program::Tools {
            calls: vec![(ToolId::ReadResource, json!({ "uri": uri }))],
            usage: AgentUsage::default(),
        },
        Program::Answer {
            chunks: vec!["Done.".to_string()],
            usage: AgentUsage::default(),
        },
    ]);
    let (tx, _rx) = unbounded_channel();
    let mut params = params(id, Some("what do the logs say?"));
    params.origin = origin;
    let result = run_turn(&llm, &dispatcher, &conn, &[], &params, &tx, &CancellationToken::new()).await;
    assert!(matches!(result, TurnResult::Answered { .. }), "{result:?}");
    let prompt = llm.calls_seen().into_iter().last().expect("a second call");
    let AgentPart::ToolResult(answer) = prompt.last().expect("a tool row").parts.first().expect("a part") else {
        panic!("expected a tool result");
    };
    answer.content.clone()
}

/// The agent reads the logs, on a turn the user started and on one Cmdr started, and what reaches the
/// model is the redacted line. nextest gives each test its own process, so the log dir is this test's.
#[tokio::test]
async fn a_scripted_turn_reads_the_logs_redacted_on_any_origin() {
    let dir = crate::test_support::TestDir::new("agent-reads-logs");
    crate::logging::set_log_dir(dir.to_path_buf());
    std::fs::write(
        dir.join("cmdr.log"),
        "2026-11-02T08:00:00.000+01:00 WARN listing /Users/test_user_8/SecretProject/plan.pdf failed\n",
    )
    .expect("write the log");

    for origin in ALL_ORIGINS {
        let answer = read_resource_answer("cmdr://logs", origin).await;
        let text = answer["text"]
            .as_str()
            .unwrap_or_else(|| panic!("{origin:?}: {answer}"));
        assert!(text.contains("WARN listing"), "{origin:?}: {text}");
        assert!(!text.contains("test_user_8"), "{origin:?}: {text}");
        assert!(!text.contains("SecretProject"), "{origin:?}: {text}");
        assert_eq!(answer["truncated"], false, "{origin:?}");
    }
}

/// A key in the log refuses typed, so the line says why and the model relays it.
#[tokio::test]
async fn a_scripted_turn_hears_a_typed_refusal_when_the_log_holds_a_key() {
    let dir = crate::test_support::TestDir::new("agent-reads-a-key");
    crate::logging::set_log_dir(dir.to_path_buf());
    std::fs::write(
        dir.join("cmdr.log"),
        "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXk\n-----END OPENSSH PRIVATE KEY-----\n",
    )
    .expect("write the log");

    let answer = read_resource_answer("cmdr://logs?limit=1", TurnOrigin::Text).await;
    assert_eq!(answer["data"]["refusal"], "protected", "{answer}");
    assert!(!answer.to_string().contains("b3BlbnNzaC1rZXk"), "{answer}");
}

/// `DevOnly` refuses on every origin, before the schema gate.
#[tokio::test]
async fn dev_only_refuses_on_every_origin() {
    for tool in [ToolId::DialogConfirm, ToolId::ResolveConflict, ToolId::QueueMoveLocked] {
        for origin in ALL_ORIGINS {
            let answer = answer_to(tool.clone(), origin).await;
            assert_eq!(answer["available"], false, "{tool:?} on {origin:?}: {answer}");
            assert_eq!(answer["refusal"], "devOnly", "{tool:?} on {origin:?}: {answer}");
        }
    }
}

/// A propose answer stays honest about nothing being staged, on the way out of the real gate.
#[tokio::test]
async fn a_refused_proposal_still_says_nothing_is_waiting() {
    let answer = answer_to(ToolId::MultiRename, TurnOrigin::Text).await;
    assert_eq!(answer["readyForReview"], false, "{answer}");
}
