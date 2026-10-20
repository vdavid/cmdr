# Agent prompts (`agent/prompts/`)

The agent's authored prompts as Markdown files with YAML frontmatter, rendered by `mod.rs`. Edit a prompt's text here,
never in Rust.

## Files

- `system.md`: Ask Cmdr's system prompt (what it must carry, and the tests pinning each rule:
  `../chat/system_prompt.rs`).
- `followup.md`: the opener of the turn an answered sweep earns.
- `mod.rs`: the loader, `{{variable}}` substitution, and one typed function per prompt.

## Must-knows

- **Dev builds read the file on each use, release builds embed it.** An edit shows on the next turn of `pnpm dev`, no
  rebuild. A turn reads the system prompt ONCE (`runtime/turn.rs`), so its calls share one prefix.
- **The system prompt's bytes are the provider's cache key.** The body is everything after the closing `---`, minus
  trailing newlines; ❌ never put anything per-call (a date, a count, a model name) in it. Per-turn facts ride the
  envelope.
- **Every frontmatter key is required** (`name` = file stem, `purpose`, `model`, `version`, `variables`).
  `variables:` is a comma-separated list, and the `render` call in `mod.rs` must pass exactly those names, in the
  literal `render(&CONST, &[("name", value), …])` shape. `pnpm check prompt-lint` enforces all of it.
- **Plain substitution only, one pass**: a value is never read as a template. A prompt that needs a conditional or a
  loop is the moment to evaluate `minijinja`.

Why each piece is shaped this way, and how to add a prompt: `DETAILS.md`.
