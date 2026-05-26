# Selection AI

Natural-language → glob/regex translation for the Selection dialog. Cloud-only;
small local models can't reliably fit the folder sample plus the structured
prompt. Parent: [`../CLAUDE.md`](../CLAUDE.md). Sibling pattern: see
[`../../search/ai/CLAUDE.md`](../../search/ai/CLAUDE.md) — same prompt-shape
(key-value lines), narrower field set, no scope or system-dir exclusion.

## File map

| File | Purpose |
|------|---------|
| `mod.rs` | Re-exports `build_classification_prompt`, `parse_selection_response`, `build_selection_translate_result`, and the `ParsedSelectionLlmResponse` / `SelectionTranslateResult` types. |
| `prompt.rs` | `build_classification_prompt(sample_names)` + `format_sample_block`. Pure. Substitutes `{TODAY}`, `{WEEK_AGO}`, `{SAMPLE}` into a static template; de-dupes the sample preserving order; truncates at `MAX_SAMPLE = 240` names with a `... (sample truncated)` sentinel. |
| `parser.rs` | `parse_selection_response(text)` → `ParsedSelectionLlmResponse`. One `key: value` per line, split on the first `:` only (regex patterns contain `:`). Unknown keys, blank values, and malformed `kind` / `size_*` / `modified_*` drop silently to `None`. |
| `query_builder.rs` | `build_selection_translate_result(parsed)` plus `generate_caveat` and `build_label` helpers. Assembles the camelCase IPC result. Defaults `kind` to `"glob"` when `pattern` is present and `kind` is missing; clears `kind` to `None` when `pattern` drops. |
| `real_llm_eval_test.rs` | Six `#[ignore]`-gated integration tests hitting the live OpenAI API. Pinned to `gpt-4o-mini` for repeatability; rerun against David's configured model by editing `MODEL`. |

The IPC entry point (`translate_selection_query`) lives in
`crate::commands::selection`; it gates on `provider == cloud`, calls
`build_classification_prompt`, runs `chat_completion` from `crate::ai::client`
with `temperature: 0.2`, `max_tokens: 300`, `top_p: 0.9`, then parses and
builds the result.

## Conventions

- **Key-value response, not JSON.** JSON generation is the #1 failure mode for
  smaller LLMs. Key-value lines are trivially produced and parsed; missing
  lines are individually skippable; one malformed line never voids the rest.
- **Folder sample is part of the prompt.** The user's intent often references
  filename conventions the model can't infer cold ("every rymd file",
  "everything I named `Final-*`"). The sample grounds the pattern in real
  filenames. The frontend sampler typically passes ≤240 names; `MAX_SAMPLE`
  is the defensive backstop.
- **Validation is loose by design.** The frontend matcher re-validates: a
  malformed regex throws at compile time and the dialog surfaces the caveat.
  The parser's job is structural, not semantic.
- **Caveat priority**: LLM-emitted `note:` (sanitized to ≤200 chars, HTML
  angle brackets stripped) takes precedence; the built-in "Couldn't translate"
  message only fires when nothing usable came back (no pattern, no size, no
  date filter).
- **Iterate the prompt against the eval, not unit tests.** Unit tests in
  `prompt.rs` / `parser.rs` / `query_builder.rs` pin pure-function behavior;
  drift in the LLM's interpretation of the prompt only shows up in
  `real_llm_eval_test.rs`. Run the eval after any prompt edit, ship together.

## Real-LLM eval

The six eval tests cover representative intents:

| Intent | Sample shape | Assertion |
|---|---|---|
| "all log files" | mixed `.log` / `.txt` / `.md` / `.png` | pattern contains `log`, `kind` set |
| "png and jpg images" | mixed image + text extensions | pattern mentions png and jpg/jpeg |
| "files bigger than 5 MB" | mixed sizes | `size_min` ∈ [4 MB, 10 MB], pattern present |
| "backups from last week" | `*-backup-*` plus noise | `modified_after` set |
| "every rymd file" | `rymd-*.pdf` plus noise | pattern matches the keyword |
| "final drafts I haven't shared" | `Final-*` files | pattern OR caveat present (no half-built query) |

Run with:

```sh
OPENAI_API_KEY=$(security find-generic-password -a "$USER" -s "OPENAI_API_KEY" -w) \
  cargo nextest run --lib --run-ignored only selection::ai::real_llm_eval_test
```

A few cents per full run. The eval logs raw responses under
`target: "selection::eval"`; tail
`RUST_LOG=cmdr_lib::selection::ai=debug pnpm dev` to peek at responses from the
running app instead.

## Gotchas

- **Don't drop `kind` to `None` when pattern is present.** The model
  occasionally omits the `kind:` line for obvious globs (`*.png`, `*.log`).
  `build_selection_translate_result` substitutes `"glob"` in that case;
  removing the default forces a re-prompt for the most common case and the
  eval starts flaking.
- **`kind` must clear when `pattern` clears.** A response with `kind: regex`
  but no `pattern:` is a broken LLM response. The builder explicitly drops
  `kind` to `None` so the frontend doesn't compile a half-built query against
  a phantom pattern. Tested by `build_no_pattern_means_no_kind_either` and
  `broken_response_returns_caveat_no_pattern`.
- **Size values reject units.** The prompt asks for bytes; `5mb` drops to
  `None` rather than guessing the unit. Underscores and commas in integers
  (`1_048_576`, `5,242,880`) are stripped first.
- **Empty-folder sample renders as `(empty folder)`.** Don't pass an empty
  block — the model hallucinates a folder layout and the eval starts
  emitting wild patterns. `format_sample_block` returns the sentinel for
  empty and all-blank inputs alike.
- **Debug-print rule applies here too.** Use `log::debug!(target:
  "selection::ai", ...)`; `eprintln!` / `println!` / `dbg!` are denied at the
  crate root. `--no-capture` test runs work fine with `log::info!`.
