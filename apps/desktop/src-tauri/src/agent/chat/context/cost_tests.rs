//! What the real shapes cost, measured against the shipped assets rather than guessed.
//!
//! These are the numbers `DETAILS.md` § "What the budgets buy, measured" quotes and the budget
//! table's reasoning rests on, so they are pinned here: a change that quietly doubles what a
//! file costs fails a test instead of surprising a user mid-rename.

use serde_json::json;

use super::test_support::*;
use super::*;
use crate::agent::chat::budget::{
    DEFAULT_PROMPT_TOKEN_BUDGET, FIXED_PROMPT_OVERHEAD_TOKENS, IMAGE_FACTS_TOKENS_PER_FILE, LISTING_TOKENS_PER_FILE,
    MAX_TOOL_RESULT_TOKENS, PLAN_ROW_TOKENS_PER_FILE, PROMPT_BUDGET_60K, RENAME_TOKENS_PER_FILE, files_per_batch,
};
use crate::agent::llm::types::ToolId;

/// Average OCR text per screenshot in the real corpus the fabrication incident came from,
/// well under `image_facts`' 2,000-char per-file cap. Every per-file number below assumes
/// it; a text-dense corpus costs up to ~2.2× more.
const OCR_CHARS: usize = 900;

const FILES: usize = 100;

// ── The documented costs, in one place (`DETAILS.md` § What the budgets buy) ────
//
// Estimated tokens, `chars/4`, measured against the shipped assets. Each is pinned within a
// tenth below, so a change that doubles what a file costs fails here instead of surprising a
// user mid-rename. Change one on purpose ⇒ change that DETAILS.md section too.
//
// The per-file figures are all PRODUCTION constants (`budget.rs` sizes a rename batch from
// them, and divides the reply's own ceiling by the plan row), so they're imported rather than
// restated: this file is what keeps them honest against the real shapes.

/// Every call: the system prompt plus the 18 tool declarations, before the user has said a word.
const FIXED_OVERHEAD: usize = FIXED_PROMPT_OVERHEAD_TOKENS;
const SYSTEM_PROMPT_TOKENS: usize = 1_636;
const TOOL_DECLARATION_TOKENS: usize = 3_543;

/// One `image_facts` row at [`OCR_CHARS`] of recognized text: the dominant per-file cost, and
/// the reason a window has to be sized for the facts rather than for the plan.
const IMAGE_FACTS_PER_FILE: usize = IMAGE_FACTS_TOKENS_PER_FILE;

/// One `propose_rename_plan` row: source path, new name, and the evidence behind it.
const PLAN_ROW_PER_FILE: usize = PLAN_ROW_TOKENS_PER_FILE;

/// One `list_pane_files` entry: name, size, mtime.
const LISTING_PER_FILE: usize = LISTING_TOKENS_PER_FILE;

/// The whole 100-file rename turn, prefix included.
const HUNDRED_FILE_TURN: usize = 41_761;

const SHOTS_DIR: &str = "/Users/me/Downloads/shots";

fn shot_path(index: usize) -> String {
    format!("{SHOTS_DIR}/2026-07-21 CleanShot{index:06}@2x.png")
}

/// One `image_facts` row as the tool serializes it, at the corpus' average OCR length.
fn facts_row(index: usize) -> Value {
    json!({
        "path": shot_path(index),
        "state": "indexed",
        "text": "x".repeat(OCR_CHARS),
        "tags": [{ "label": "screenshot", "score": 0.9 }, { "label": "document", "score": 0.8 }],
    })
}

/// One `list_pane_files` entry.
fn listing_entry(index: usize) -> Value {
    json!({ "name": format!("2026-07-21 CleanShot{index:06}@2x.png"), "size": 4_302_190, "modified": 1_785_247_668 })
}

/// One `propose_rename_plan` row: path, new name, and the evidence behind it.
fn plan_row(index: usize) -> Value {
    json!({
        "sourcePath": shot_path(index),
        "volumeId": "root",
        "destinationName": format!("2026-07-21 19-36-00 cmdr-ai-rename-review-dialog-{index}.png"),
        "evidence": { "source": "imageText", "detail": "Review file renames" },
    })
}

/// Assert a measured cost sits within a tenth of the documented figure. A per-item cost that
/// moves changes which models can do a 100-file rename at all, so it may not drift quietly:
/// the numbers here and `DETAILS.md` § "What the budgets buy, measured" are one pair.
fn assert_near(measured: usize, documented: usize, what: &str) {
    let slack = documented / 10;
    assert!(
        measured.abs_diff(documented) <= slack,
        "the measured cost of {what} is {measured}; the documented figure is {documented} (±{slack}). \
         If the change is intended, update it HERE and in agent/chat/DETAILS.md's \"what the budgets buy\" section."
    );
}

/// What every single call pays before the user's question is even in the prompt. It is why a
/// flat 8k budget left only ~4.9k for the actual work, which is how an 11-file `image_facts`
/// batch fit and a 12-file one did not.
#[test]
fn every_call_pays_about_3_500_tokens_of_fixed_overhead() {
    let tools = crate::agent::tools::agent_tool_declarations();
    assert_eq!(tools.len(), 18, "the overhead below is the cost of THESE declarations");

    let system = estimate_prompt_tokens(crate::agent::chat::system_prompt::SYSTEM_PROMPT, &[], &[]);
    let declarations = estimate_prompt_tokens("", &tools, &[]);

    assert_near(system, SYSTEM_PROMPT_TOKENS, "the system prompt");
    assert_near(declarations, TOOL_DECLARATION_TOKENS, "the 18 tool declarations");
    assert_near(system + declarations, FIXED_OVERHEAD, "the fixed per-call overhead");
}

/// The three per-file costs a bulk rename pays. `image_facts` dominates by an order of
/// magnitude, which is why the facts are what a window has to be sized for.
#[test]
fn a_bulk_rename_pays_about_350_tokens_per_file() {
    let facts = estimate_tokens_of_value(&facts_row(0));
    let plan = estimate_tokens_of_value(&plan_row(0));
    let listing = estimate_tokens_of_value(&listing_entry(0));

    assert_near(facts, IMAGE_FACTS_PER_FILE, "one image_facts row at 900 chars of OCR");
    assert_near(plan, PLAN_ROW_PER_FILE, "one plan row");
    assert_near(listing, LISTING_PER_FILE, "one pane-listing entry");
    assert_eq!(
        IMAGE_FACTS_PER_FILE + PLAN_ROW_PER_FILE + LISTING_PER_FILE,
        RENAME_TOKENS_PER_FILE,
        "budget::RENAME_TOKENS_PER_FILE sizes every batch hint from these three parts, so it is \
         their sum or the hint is fiction"
    );
    assert!(
        facts > 3 * (plan + listing),
        "the facts dominate ({facts} vs {plan} + {listing}): sizing a window for the plan rows alone \
         is how a batch overflows"
    );
}

/// The size of a 100-file content-based rename turn, measured against the REAL prefix (the
/// shipped system prompt plus every agent tool declaration) instead of a guess. This is the
/// flow the fabrication incident came from, at a size users plausibly ask for, so what it
/// costs is worth pinning: the answer decides which models can do it at all.
///
/// Shape of one such turn, all of it inside the SAME user turn (so none of it may elide):
/// the pane listing, then `image_facts` in as many pages as the per-result ceiling forces,
/// then the plan call carrying 100 rows of paths, names, and evidence.
#[test]
fn a_hundred_file_rename_turn_needs_more_than_the_default_budget() {
    let (tokens, elided, pages) = assemble_rename_turn(FILES, PROMPT_BUDGET_60K);

    assert_near(tokens, HUNDRED_FILE_TURN, "the whole 100-file turn");
    // The per-item costs above have to explain the turn, or one of them is measuring the wrong
    // thing. What they don't cover is the paths the calls name, the envelope, the user's own
    // sentence, and JSON scaffolding.
    let accounted = FIXED_OVERHEAD + FILES * (IMAGE_FACTS_PER_FILE + PLAN_ROW_PER_FILE + LISTING_PER_FILE);
    assert!(
        accounted < tokens && tokens - accounted < tokens / 10,
        "the breakdown must account for the turn: the parts add to {accounted} of {tokens} estimated tokens"
    );
    assert!(
        tokens > DEFAULT_PROMPT_TOKEN_BUDGET,
        "a 100-file rename does NOT fit the conservative default ({tokens} estimated tokens vs {DEFAULT_PROMPT_TOKEN_BUDGET}); \
         if this ever flips, the numbers below are stale"
    );
    assert!(
        tokens < PROMPT_BUDGET_60K,
        "a 100-file rename must fit the 60k budget with room to spare ({tokens} estimated tokens)"
    );
    // Nothing from this turn may have been dropped to get there: every page of facts is the
    // evidence the plan cites.
    assert_eq!(elided, 0, "the turn in flight keeps all {} of its results", pages + 1);
}

/// The batch size `budget::files_per_batch` promises has to survive contact with the real
/// shapes: a batch that size, assembled against the same budget, must fit with nothing
/// elided. Without this the hint is arithmetic nobody checked, and the model would be told to
/// take on a batch that overruns the window it was sized for.
#[test]
fn a_batch_the_hint_promises_actually_fits_its_budget() {
    for budget in [DEFAULT_PROMPT_TOKEN_BUDGET, PROMPT_BUDGET_60K] {
        let files = files_per_batch(budget);
        assert!(files > 0, "a {budget}-token budget has to be able to rename something");
        let (tokens, elided, _) = assemble_rename_turn(files, budget);
        assert!(
            tokens <= budget,
            "the hint promised a batch of {files} inside a {budget}-token budget; the real turn costs {tokens}"
        );
        assert_eq!(
            elided, 0,
            "a batch the hint promised must not need its own results dropped"
        );
    }
}

/// One content-based rename turn as the tools really serialize it, assembled against
/// `budget`: the pane listing, `image_facts` in as many pages as the per-result ceiling
/// forces, then the plan call. Returns the estimated size, how many results were elided, and
/// how many `image_facts` pages it took.
fn assemble_rename_turn(files: usize, budget: usize) -> (usize, usize, usize) {
    let listing = json!({
        "pane": "right",
        "path": SHOTS_DIR,
        "scope": "selection",
        "returned": files,
        "total": files,
        "truncated": false,
        "entries": (0..files).map(listing_entry).collect::<Vec<_>>(),
    });

    // `image_facts` pages itself to MAX_TOOL_RESULT_TOKENS, so dense rows arrive over several
    // calls — and every page stays in this turn's prompt.
    let rows_per_page = {
        let per_row = estimate_tokens_of_value(&facts_row(0));
        (MAX_TOOL_RESULT_TOKENS / per_row).max(1)
    };
    let pages: Vec<Vec<usize>> = (0..files)
        .collect::<Vec<_>>()
        .chunks(rows_per_page)
        .map(<[usize]>::to_vec)
        .collect();

    let mut transcript = vec![
        user("Rename these screenshots by their content, please.", 1_000),
        assistant_tool_call("listing", ToolId::ListPaneFiles, json!({}), 1_010),
        tool_result("listing", listing, 1_020),
    ];
    for (page, indexes) in pages.iter().enumerate() {
        let call_id = format!("facts-{page}");
        transcript.push(assistant_tool_call(
            &call_id,
            ToolId::ImageFacts,
            json!({ "volumeId": "root", "paths": indexes.iter().map(|i| shot_path(*i)).collect::<Vec<_>>() }),
            1_030 + page as i64,
        ));
        transcript.push(tool_result(
            &call_id,
            json!({ "status": "ok", "coverage": [], "facts": indexes.iter().map(|i| facts_row(*i)).collect::<Vec<_>>() }),
            1_040 + page as i64,
        ));
    }
    transcript.push(assistant_tool_call(
        "plan",
        ToolId::ProposeRenamePlan,
        json!({ "renames": (0..files).map(plan_row).collect::<Vec<_>>() }),
        1_100,
    ));

    let tools = crate::agent::tools::agent_tool_declarations();
    let real_prefix = PrefixInputs {
        system_prompt: crate::agent::chat::system_prompt::SYSTEM_PROMPT,
        cmdr_md: None,
        memory: None,
        tools: &tools,
    };
    let assembled = assemble_prompt(&real_prefix, &transcript, &envelope_at(1_000), offset(), budget);
    let tokens = estimate_prompt_tokens(&assembled.system, &assembled.tools, &assembled.messages);
    (tokens, assembled.elision.elided_results, pages.len())
}
