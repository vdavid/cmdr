//! The agent's authored prompts, as Markdown files with YAML frontmatter beside this module.
//!
//! - **Release builds embed them** (`include_str!`), so a shipped prompt is the one that was
//!   reviewed and nothing on disk can change it.
//! - **Dev builds read them from disk on each use**, so a prompt edit takes effect on the next
//!   turn without a rebuild (the repo-root `.taurignore` already keeps an `.md` save from
//!   restarting `pnpm dev`). A file that can't be read falls back to the embedded copy.
//! - **Substitution is plain `{{variable}}`**, one pass: a value is never scanned again, so a
//!   path containing `{{` reaches the model verbatim. No conditionals or loops; a prompt that
//!   needs them is a reason to reach for `minijinja`, not to grow this.
//! - **`prompt-lint` (the Go check runner) keeps the two halves in step**: every file's
//!   frontmatter is complete, every `{{name}}` in a body is declared in its `variables:`, and
//!   the `render` call for each prompt below passes exactly those names. Keep every call in
//!   the literal `render(&CONST, &[("name", value), …])` shape the lint reads.
//!
//! The body is everything after the frontmatter's closing `---` line, minus the file's
//! trailing newlines. Nothing else is trimmed, so the system prompt stays byte-identical
//! across turns and builds, which is what keeps it inside the provider's prompt cache.

use std::borrow::Cow;

/// One prompt file: its name (the file stem and the frontmatter's `name:`) and the copy
/// compiled into the binary.
pub(crate) struct PromptFile {
    name: &'static str,
    embedded: &'static str,
}

macro_rules! prompt {
    ($name:literal) => {
        PromptFile {
            name: $name,
            embedded: include_str!(concat!($name, ".md")),
        }
    };
}

/// Ask Cmdr's identity and rules: the stable, cached system prompt of every turn.
const SYSTEM: PromptFile = prompt!("system");
/// The opener of the one turn a rejected (or answered) sweep earns.
const FOLLOWUP: PromptFile = prompt!("followup");

/// Ask Cmdr's system prompt. Call it once per turn and hold the result: in a dev build each
/// call reads the file again.
pub fn system_prompt() -> Cow<'static, str> {
    render(&SYSTEM, &[])
}

/// The follow-up turn's opener, around `decisions` (one rendered decision per line, each
/// ending in a newline).
pub fn followup(decisions: &str) -> String {
    render(&FOLLOWUP, &[("decisions", decisions)]).into_owned()
}

fn render(file: &PromptFile, vars: &[(&str, &str)]) -> Cow<'static, str> {
    match source_of(file) {
        Cow::Borrowed(text) => substitute(body_of(text), vars, file.name),
        Cow::Owned(text) => Cow::Owned(substitute(body_of(&text), vars, file.name).into_owned()),
    }
}

/// The file's text: from disk in a dev build, so an edit shows on the next turn; else, or
/// when the read fails, the embedded copy.
fn source_of(file: &PromptFile) -> Cow<'static, str> {
    #[cfg(debug_assertions)]
    {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/agent/prompts")
            .join(format!("{}.md", file.name));
        if let Ok(text) = std::fs::read_to_string(&path) {
            return Cow::Owned(text);
        }
    }
    Cow::Borrowed(file.embedded)
}

/// Everything after the frontmatter's closing `---` line, minus trailing newlines. A file
/// with no frontmatter is all body (the lint refuses one; this just doesn't lose the text).
fn body_of(source: &str) -> &str {
    let body = source
        .strip_prefix("---\n")
        .and_then(|rest| rest.find("\n---\n").map(|end| &rest[end + "\n---\n".len()..]))
        .unwrap_or(source);
    body.trim_end_matches('\n')
}

/// `{{name}}` → its value, in one pass over `template`. An unknown name stays in the text and
/// is logged: the lint keeps that from shipping, and leaving it visible beats a silent blank.
fn substitute<'a>(template: &'a str, vars: &[(&str, &str)], prompt: &str) -> Cow<'a, str> {
    if !template.contains("{{") {
        return Cow::Borrowed(template);
    }
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        let Some(close) = after.find("}}") else {
            out.push_str(&rest[open..]);
            rest = "";
            break;
        };
        let name = after[..close].trim();
        match vars.iter().find(|(key, _)| *key == name) {
            Some((_, value)) => out.push_str(value),
            None => {
                // allowed-log-private: both values are authored prompt text (a file stem and a slot name), never the user's
                log::warn!(target: "agent::prompts", "prompt '{prompt}' names an unknown variable '{name}'");
                out.push_str(&rest[open..open + 2 + close + 2]);
            }
        }
        rest = &after[close + 2..];
    }
    out.push_str(rest);
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_is_what_follows_the_frontmatter_minus_trailing_newlines() {
        let source = "---\nname: x\nvariables:\n---\nLine one.\n\nLine two.\n\n";
        assert_eq!(body_of(source), "Line one.\n\nLine two.");
    }

    #[test]
    fn substitution_is_one_pass_so_a_value_is_never_read_as_a_template() {
        let out = substitute("a {{x}} b {{ y }} c", &[("x", "{{y}}"), ("y", "Y")], "test");
        assert_eq!(out, "a {{y}} b Y c");
    }

    #[test]
    fn an_unknown_or_unclosed_variable_stays_visible() {
        assert_eq!(substitute("a {{nope}} b", &[], "test"), "a {{nope}} b");
        assert_eq!(substitute("a {{open", &[("open", "x")], "test"), "a {{open");
    }

    /// Prompt caching keys on the exact prefix bytes, so two reads must agree to the byte.
    #[test]
    fn the_system_prompt_is_byte_identical_across_calls() {
        assert_eq!(system_prompt(), system_prompt());
        assert!(
            !system_prompt().starts_with("---"),
            "the frontmatter never reaches the model"
        );
    }

    #[test]
    fn the_followup_wraps_the_decisions_it_is_given() {
        let out = followup("turned down: trash 3 item(s) under /tmp\n");
        assert!(
            out.starts_with("The user has answered suggestions you made:\nturned down: trash"),
            "{out}"
        );
        assert!(out.contains("memory_write"), "{out}");
        assert!(!out.contains("{{"), "{out}");
    }
}
