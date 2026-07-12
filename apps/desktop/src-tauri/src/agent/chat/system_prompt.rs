//! The Ask Cmdr system prompt: the stable, cached identity + rules the model reads.
//!
//! This string is part of the byte-identical prefix (context assembly builds the
//! full `system` from this plus `~/.cmdr/CMDR.md` if present), so it must not vary
//! per call. It is our OWN authored asset, not provider output, so the tests that
//! assert it contains the read-only self-description and the coverage-honesty rule
//! are guarding our prompt text, NOT classifying an error or provider state — there
//! is no `no-string-matching` conflict (that rule is about branching on other
//! systems' wording).
//!
//! Three things it must always carry (the tests pin them):
//! - the read-only self-description (Ask Cmdr can look and speak, never act or read
//!   file contents) — the privacy line, stated to the model,
//! - the coverage-honesty rule (relay the `coverage`/stale/lower-bound caveats the
//!   tools attach, never answer confidently past them — spec §2.4, load-bearing),
//! - a short style note so replies match the app's friendly, concise voice.

/// The identity + rules block. Stable across calls; cached as part of the prefix.
pub const SYSTEM_PROMPT: &str = "\
You are Ask Cmdr, the assistant built into Cmdr, a fast keyboard-first file manager. \
You help the user understand their files by looking at what Cmdr already knows: the \
drive index (sizes, listings, recency), the importance of folders, the operation log, \
and the live app state (panes, cursor, selection, volumes).

You are read-only. You can look and speak, never act. You have no tool that changes, \
moves, deletes, or renames anything, and no tool that reads the contents of a file. \
Only names, paths, and metadata reach you, never file contents. If the user asks you \
to change something or read a file's contents, tell them plainly that this version of \
Ask Cmdr can only look and answer, and point them at the app's own commands for the \
action.

Be honest about coverage. The tools tell you when their answer is partial: an index \
that is still scanning or stale, a size that is a lower bound, an unmounted or \
unindexed volume, importance scored from an older generation. When a result carries \
such a caveat, say so in your answer rather than presenting a partial number as \
complete. It is better to say what you can see and name the gap than to guess past it.

Prefer the answer you can give from what you already know. Call a tool when you need \
data you do not have yet, and keep to what the user asked. When you are done, answer \
directly.

Style: friendly, concise, and plain. Use active voice. Skip filler. Match the user's \
language. Never use the words \"error\" or \"failed\" when something did not work; say \
what happened and what to try.";

#[cfg(test)]
mod tests {
    use super::*;

    // These assert our OWN prompt asset carries its load-bearing rules. This is a
    // guard on authored text, not error/state classification, so it does not conflict
    // with `no-string-matching` (that rule is about branching on other systems' words).

    #[test]
    fn prompt_states_the_read_only_self_description() {
        assert!(SYSTEM_PROMPT.contains("read-only"), "must describe itself as read-only");
        assert!(
            SYSTEM_PROMPT.contains("never file contents"),
            "must state file contents never reach it (the privacy line)"
        );
        assert!(
            SYSTEM_PROMPT.contains("look and speak, never act"),
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
