//! The Ask Cmdr system prompt: the stable, cached identity + rules the model reads.
//!
//! This string is part of the byte-identical prefix (context assembly builds the
//! full `system` from this plus `~/.cmdr/CMDR.md` if present), so it must not vary
//! per call. It is our OWN authored asset, not provider output, so the tests that
//! assert it contains the read-only self-description and the coverage-honesty rule
//! are guarding our prompt text, NOT classifying an error or provider state.
//!
//! Five labelled sections, in the order the model reads them: identity, what you can
//! do, coverage, renaming, evidence, style. The labels are there so a rule can be
//! found and edited without re-reading the whole block.
//!
//! What it must always carry (the tests pin each one):
//! - the read-only self-description (Ask Cmdr can look and speak, never act or read
//!   file contents) — the privacy line, stated to the model,
//! - the coverage-honesty rule (relay the `coverage`/stale/lower-bound caveats the
//!   tools attach, never answer confidently past them — spec §2.4, load-bearing),
//! - **a named fallback action**, not only a prohibition: when the content a name would
//!   describe is missing or cut, keep the existing name or put the date in front, and say
//!   which files went unseen. Guessing there is how a batch of screenshots got 12
//!   fabricated names, and "do not guess" alone leaves the next token to chance,
//! - **the verbatim-quote rule**, so the model knows a paraphrase is refused and that the
//!   refusal costs the whole plan rather than the one row,
//! - **how to read an elided result again**, so a set-aside result is a re-fetch rather
//!   than a gap the model fills in,
//! - a short style note so replies match the app's friendly, concise voice.
//!
//! The batch size is deliberately NOT here: it moves with the model and with the user's
//! "Chat memory size", so it rides the per-turn envelope (`context::render_envelope`) and
//! the prompt points at it. A number here would be wrong for most models, and a per-model
//! number would break the byte-identical prefix.

/// The identity + rules block. Stable across calls; cached as part of the prefix.
pub const SYSTEM_PROMPT: &str = "\
You are Ask Cmdr, the assistant built into Cmdr, a fast keyboard-first file manager. \
You help the user understand their files by looking at what Cmdr already knows: the \
drive index (sizes, listings, recency), the importance of folders, the operation log, \
and the live app state (panes, cursor, selection, volumes).

# What you can do

You can look and speak, and you can prepare a rename plan for the user to review. You never \
act: you have no tool that changes, moves, deletes, or renames anything, and no tool that reads the contents of a file. \
Only names, paths, and metadata reach you, never file contents. If the user asks you \
to change something, explain that only they can approve a prepared rename plan, or point them at the app's own commands.

Prefer the answer you can give from what you already know. Call a tool when you need \
data you do not have yet, and keep to what the user asked. When you are done, answer \
directly.

# Coverage

Be honest about coverage. The tools tell you when their answer is partial: an index \
that is still scanning or stale, a size that is a lower bound, an unmounted or \
unindexed volume, importance scored from an older generation. When a result carries \
such a caveat, say so in your answer rather than presenting a partial number as \
complete. It is better to say what you can see and name the gap than to guess past it.

If a tool result says truncated: true, your reply must say you looked at only \
returned of total items, using those numbers, and must never imply you covered the full selection or folder; \
ask about the remaining paths in another call when you need them.

A tool result that reads elided_tool_result is an older result this conversation set aside to make room, not one \
that went wrong: its tool field names the call and its refetch field says how to read it again. Its call, held, and \
refetch text are never file contents, so never name a file after them.

# Renaming

For a natural-language rename request, call list_pane_files first. It returns the focused \
selection when one exists, otherwise the focused folder, plus the exact volume ID for the plan. Treat \
that pane listing as ready to use: do not wait for a drive scan or image indexing to finish. The \
propose_rename_plan tool is always available. Use image_facts when image contents would \
improve the names.

Put at most as many files in one plan as this turn's context line gives for its rename batch. When more are \
waiting, say how many are left and offer the next batch. Preserve each file extension unless the user \
explicitly asks otherwise.

Before a follow-up batch in the same folder, call list_pane_files again and match the naming the already-renamed \
files show. The folder is what actually happened; your earlier messages are only what you proposed. If the context \
line lists names the user turned down, do not propose them again, and do not offer a variation of the same \
shape.

Never invent what an image contains. When a file's facts are missing, not indexed, or cut short, do this \
instead: keep that file's existing name, or put its date in front as \"<date> <existing-name>\", and list in your \
reply which files you could not see. A name describing contents you were not shown is worse than a plain one.

Submit the final plan with propose_rename_plan; never claim a rename happened before the user reviews it.

# Evidence

State what each proposed name is based on, per file: the text or tags image_facts returned, the old filename, \
other metadata, or the user's own instruction. Claim image text or tags only for a path image_facts actually \
returned content for; for every other file say which of the rest you used.

Quote image text verbatim: copy the characters image_facts returned for that path, and do not paraphrase, \
translate, or tidy them. For image tags, name only labels that call returned, separated by commas. A quote or \
label that was not delivered is refused, and the refusal takes the whole plan rather than the one row, so every \
row has to hold.

# Style

Friendly, concise, and plain. Use active voice. Skip filler. Match the user's \
language. Never use the words \"error\" or \"failed\" when something did not work; say \
what happened and what to try.";

#[cfg(test)]
mod tests {
    use super::*;

    // These assert our OWN prompt asset carries its load-bearing rules. This is a
    // guard on authored text, not error/state classification.

    #[test]
    fn prompt_states_the_read_only_self_description() {
        assert!(
            SYSTEM_PROMPT.contains("prepare a rename plan"),
            "must describe its proposal-only power"
        );
        assert!(
            SYSTEM_PROMPT.contains("never file contents"),
            "must state file contents never reach it (the privacy line)"
        );
        assert!(
            SYSTEM_PROMPT.contains("never act"),
            "must state it can look and speak but never act"
        );
    }

    #[test]
    fn prompt_carries_the_coverage_honesty_rule() {
        assert!(
            SYSTEM_PROMPT.contains("honest about coverage"),
            "must carry the coverage-honesty rule (spec §2.4)"
        );
        assert!(
            SYSTEM_PROMPT.contains("lower bound") && SYSTEM_PROMPT.contains("stale"),
            "must name the partial-coverage cases the model has to relay"
        );
    }

    #[test]
    fn prompt_requires_exact_truncation_disclosure() {
        assert!(
            SYSTEM_PROMPT.contains("truncated: true")
                && SYSTEM_PROMPT.contains("returned of total")
                && SYSTEM_PROMPT.contains("must never imply you covered the full"),
            "a capped tool result must be disclosed with exact returned and total counts"
        );
    }

    /// The prompt-side half of the fabricated-rename fix: budget pressure or a paged tool
    /// result can leave the model without the content it was asked to name files by, and the
    /// old wording ("continue with names, dates, and other available metadata") read as a
    /// licence to fill the gap from imagination.
    #[test]
    fn prompt_forbids_inventing_image_contents() {
        assert!(
            SYSTEM_PROMPT.contains("Never invent what an image contains"),
            "missing content must never be filled in by guessing"
        );
        assert!(
            SYSTEM_PROMPT.contains("which files you could not see"),
            "the reply must name the files it couldn't see"
        );
    }

    /// `propose_rename_plan` requires typed evidence per item and verifies content claims
    /// against what `image_facts` actually delivered, so the prompt has to state the same
    /// contract: a claim the model can't back gets the whole plan refused.
    #[test]
    fn prompt_requires_naming_what_each_rename_is_based_on() {
        assert!(
            SYSTEM_PROMPT.contains("State what each proposed name is based on"),
            "the model must say what each name rests on"
        );
        assert!(
            SYSTEM_PROMPT.contains("Claim image text or tags only for a path image_facts actually"),
            "content claims are limited to paths image_facts answered"
        );
    }

    /// A prohibition leaves the next token to chance; the fabricated-rename incident is what
    /// that looks like in production. So the no-invention rule has to be followed by the
    /// action to take instead, spelled out concretely enough to copy.
    #[test]
    fn prompt_names_the_fallback_action_not_only_the_prohibition() {
        assert!(
            SYSTEM_PROMPT.contains("keep that file's existing name"),
            "the prompt must name what to do instead of guessing"
        );
        assert!(
            SYSTEM_PROMPT.contains("<date> <existing-name>"),
            "the date-prefix form must be given literally, so it can be copied"
        );
    }

    /// The batch size is per-model and per-setting, so it can't live in the cached prefix. The
    /// prompt has to send the model to the envelope for it instead of naming a number that
    /// would be wrong for most models (`context::render_envelope` writes it).
    #[test]
    fn prompt_points_at_the_envelopes_batch_size_rather_than_a_number() {
        assert!(
            SYSTEM_PROMPT.contains("this turn's context line gives for its rename batch"),
            "the prompt must point at the per-turn batch size"
        );
        for hardcoded in ["50 files", "100 files", "101 files", "200 files"] {
            assert!(
                !SYSTEM_PROMPT.contains(hardcoded),
                "the prefix must not hardcode a batch size, found {hardcoded}"
            );
        }
    }

    /// `propose_rename_plan` matches a quote against the delivered text and refuses the PLAN,
    /// not the row, when it doesn't check out. A model that doesn't know the cost paraphrases
    /// and loses 50 good rows with the one bad one.
    #[test]
    fn prompt_requires_a_verbatim_quote_and_names_the_cost() {
        assert!(
            SYSTEM_PROMPT.contains("Quote image text verbatim"),
            "the quote must be required verbatim"
        );
        assert!(
            SYSTEM_PROMPT.contains("do not paraphrase"),
            "paraphrasing must be called out, since it's the failure the check catches"
        );
        assert!(
            SYSTEM_PROMPT.contains("takes the whole plan rather than the one row"),
            "the model must know a refusal costs the whole plan"
        );
    }

    /// M5's stubs carry a `refetch` hint, which is worth nothing if the model reads a stub as
    /// a failure or, worse, as content it may name a file after.
    #[test]
    fn prompt_says_how_to_read_a_set_aside_result_again() {
        assert!(
            SYSTEM_PROMPT.contains("elided_tool_result"),
            "the model must recognize the stub by its marker"
        );
        assert!(
            SYSTEM_PROMPT.contains("refetch field says how to"),
            "the stub's re-fetch hint must be pointed at"
        );
        assert!(
            SYSTEM_PROMPT.contains("never file contents, so never name a file after them"),
            "a digest must not be usable as evidence (invariant 6)"
        );
    }

    /// A multi-batch job must re-derive its convention from the FOLDER, not from its own
    /// transcript: the transcript says what the model proposed, the folder says what the user
    /// actually kept, including their hand edits and denials.
    #[test]
    fn prompt_re_derives_a_follow_up_batchs_convention_from_the_folder() {
        assert!(
            SYSTEM_PROMPT.contains("call list_pane_files again and match the naming"),
            "a follow-up batch must read the folder rather than trust its own transcript"
        );
        assert!(
            SYSTEM_PROMPT.contains("only what you proposed"),
            "the reason has to be stated, or the rule reads as a redundant extra call"
        );
    }

    /// The envelope lists names the user turned down. Without this rule the next batch happily
    /// re-proposes a rejected style, which is the same argument had twice.
    #[test]
    fn prompt_says_not_to_re_propose_a_denied_name() {
        assert!(
            SYSTEM_PROMPT.contains("names the user turned down, do not propose them again"),
            "a denied name must not come back"
        );
        assert!(
            SYSTEM_PROMPT.contains("variation of the same shape"),
            "nor a near-miss of the style that was denied"
        );
    }

    #[test]
    fn prompt_forbids_the_error_and_failed_words() {
        // The prompt instructs the model to avoid "error"/"failed"; it may quote the
        // words while forbidding them, but must not use them as its own voice. We
        // assert the forbidding instruction is present.
        assert!(
            SYSTEM_PROMPT.contains("Never use the words"),
            "must instruct the model to avoid the error/failed words"
        );
    }
}
