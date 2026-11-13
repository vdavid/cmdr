# Agent prompts: details

## Adding a prompt

1. Write `<name>.md` here: the five frontmatter keys, then the body. Name each slot `{{lowercase_name}}` and list the
   names in `variables:`.
2. In `mod.rs`, declare `const NAME: PromptFile = prompt!("<name>");` and one typed function calling
   `render(&NAME, &[("var", value), …])`. Callers use the function, never `render`.
3. `pnpm check prompt-lint`, then the tests that pin the prompt's rules (put them beside its caller, the way
   `../chat/system_prompt.rs` pins the system prompt).

## Decisions

- **Decision: Markdown with frontmatter, not Rust string literals.** Why: a word change used to mean a rebuild, and the
  prompts are about to multiply (the planner and the summarizer). The frontmatter says what each one is for and which
  model class it was written against, which a `const` could only say in a comment.
- **Decision: plain `{{variable}}`, no `minijinja`.** Why: neither prompt needs a conditional or a loop. The one list
  (the follow-up's decisions) is rendered in Rust, where its shape is typed, and arrives as one value. A template engine
  is a dependency and a second language for an editor to learn, paid for nothing yet.
- **Decision: dev reads disk, release embeds.** Why: iteration speed where it's wanted, and a shipped prompt that is
  exactly the reviewed one. The dev path falls back to the embedded copy when the read fails, so a moved checkout still
  runs. `.taurignore` excludes `*.md`, so saving a prompt doesn't restart `pnpm dev`.
- **Decision: the lint reads call sites by a literal shape, and fails closed.** Why: the check runner is Go and can't
  type-check Rust, so it reads `render(&CONST, &[("name", …)])` with a regex. A call in any other shape is counted as
  unreadable and fails the check rather than passing unchecked. Comments are stripped first, so the docs showing the
  shape aren't read as calls.
- **Decision: the follow-up opener no longer ends in a newline.** Why: a body drops the file's trailing newlines (an
  editor adds or strips them at will, and the system prompt's bytes must not depend on that). The opener is the last
  text of a user message, where the newline carried nothing. The system prompt's bytes are unchanged by the move
  (verified by a byte-for-byte comparison against the old `const` before it was deleted, 2026-10-20).
- **Decision: the system prompt asks for one sentence before every batch of tool calls.** Why: the rail renders a
  turn's narration between the tool lines it explains, and without the ask the model narrated nothing. Measured live on
  `claude-sonnet-5-5` (dev build, 2026-10-26): two tool-running turns (three rounds) with no ask put no text before any of them;
  a softer "Before a batch of tool calls, say in one short sentence what you are checking." still got none; the current
  "Every time you call tools, first write one short sentence…" narrated every round ("I'll check the biggest folders on
  your Desktop first.", "Next, the contents of vo-test."). Cost: about 31 tokens of prefix. The fixed overhead
  re-measured at 2,114 (system) + 4,275 (declarations) = 6,389 against the then-pinned 6,263, inside `cost_tests.rs`'s ±10%.

- **Decision: the card verdict has two whole prompt files, not one with a conditional.** Why: whether the model words
  what removing an item would cost is the one place it can hand out a green light, and the verdict eval decides it per
  model class. Two static files keep each byte-identical (cacheable) and reviewable as the text that ships;
  `about::verdict::prompt::PromptVariant::for_provider` picks one. The fence around the item's facts is built in Rust
  (`about/verdict/prompt.rs`), beside its defanging, for the same reason the memory fence lives in `chat/context`.

## Not here

- The digest a wake opens with is DATA (`llm::types::WakeDigest`), rendered from structure on purpose (its folder
  counts must outlive every locale pass), so it has no prose to move. The memory fence's header stays in
  `../chat/context.rs`: it's security-critical text inside the pure prefix core, which reads no files.
