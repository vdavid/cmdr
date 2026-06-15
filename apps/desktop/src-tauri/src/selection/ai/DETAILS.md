# Selection AI details

Depth and rationale. `CLAUDE.md` holds the must-knows; the pipeline detail, conventions, and the real-LLM eval live
here.

## Pipeline

- **`prompt.rs`**: `build_classification_prompt(sample_names, current_type)` + `format_sample_block`. Pure. Substitutes
  `{TODAY}`, `{WEEK_AGO}`, `{CURRENT_TYPE}`, `{SAMPLE}`; de-dupes the sample preserving order; truncates at
  `MAX_SAMPLE = 240`. `current_type` (`Some(true)` folders / `Some(false)` files / `None` both) renders a context line
  so the model knows the optional `type` field and the user's current choice.
- **`parser.rs`**: `parse_selection_response(text)` → `ParsedSelectionLlmResponse`. One `key: value` per line, split on
  the first `:` only. Unknown keys, blank values, and malformed `kind` / `type` / `size_*` / `modified_*` drop to
  `None`. `type` accepts only `file` / `folder`; `both`/unknown → `None`.
- **`query_builder.rs`**: `build_selection_translate_result(parsed)` plus `generate_caveat` / `build_label`. Assembles
  the camelCase IPC result. Defaults `kind` to `"glob"` when `pattern` is present and `kind` missing; clears `kind` when
  `pattern` drops. Maps `item_type` → `is_directory` (`folder → true`, `file → false`, absent → `None`).

The IPC entry point `translate_selection_query` (in `crate::commands::selection`) gates on `provider == cloud`, calls
`build_classification_prompt`, runs `chat_completion` from `crate::ai::client` with `temperature: 0.2`,
`max_tokens: 300`, `top_p: 0.9`, then parses and builds the result.

## Conventions

- **Key-value response, not JSON.** JSON generation is the #1 failure mode for smaller LLMs. Key-value lines are
  trivially produced and parsed; missing lines are individually skippable; one malformed line never voids the rest.
- **Folder sample is part of the prompt.** Intent often references filename conventions the model can't infer cold
  ("every rymd file", "everything I named `Final-*`"); the sample grounds the pattern in real filenames. The frontend
  sampler typically passes ≤240 names; `MAX_SAMPLE` is the defensive backstop.
- **Validation is loose by design.** The frontend matcher re-validates (a malformed regex throws at compile time and the
  dialog surfaces the caveat). The parser's job is structural, not semantic.
- **Caveat priority**: an LLM-emitted `note:` (sanitized to ≤200 chars, HTML angle brackets stripped) takes precedence;
  the built-in "Couldn't translate" message fires only when nothing usable came back (no pattern, no size, no date
  filter).
- **Iterate the prompt against the eval, not unit tests.** Unit tests in `prompt.rs` / `parser.rs` / `query_builder.rs`
  pin pure-function behavior; LLM-interpretation drift only shows in `real_llm_eval_test.rs`. Run the eval after any
  prompt edit and ship together.

## Real-LLM eval

`real_llm_eval_test.rs` has six `#[ignore]`-gated integration tests hitting the live OpenAI API, pinned to
`gpt-4o-mini` for repeatability (rerun against David's configured model by editing `MODEL`). They cover representative
intents:

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

A few cents per full run. The eval logs raw responses under `target: "selection::eval"`; tail
`RUST_LOG=cmdr_lib::selection::ai=debug pnpm dev` to peek at responses from the running app instead.

## Gotcha detail

- **Don't drop `kind` to `None` when pattern is present.** The model occasionally omits the `kind:` line for obvious
  globs (`*.png`, `*.log`); `build_selection_translate_result` substitutes `"glob"`. Removing the default forces a
  re-prompt for the most common case and the eval flakes.
- **`kind` must clear when `pattern` clears.** A response with `kind: regex` but no `pattern:` is broken; the builder
  drops `kind` to `None` so the frontend doesn't compile a half-built query against a phantom pattern. Tested by
  `build_no_pattern_means_no_kind_either` and `broken_response_returns_caveat_no_pattern`.
- **Size values reject units.** The prompt asks for bytes; `5mb` drops to `None` rather than guessing the unit.
  Underscores and commas in integers (`1_048_576`, `5,242,880`) are stripped first.
- **Empty-folder sample renders as `(empty folder)`.** Don't pass an empty block: the model hallucinates a folder layout
  and the eval emits wild patterns. `format_sample_block` returns the sentinel for empty and all-blank inputs alike.
- **`type` is optional and leave-alone, NOT a third "both" value.** The prompt omits `type` unless the intent is clearly
  only files or only folders; the current type rides in as context. `type: both`/unknown maps to `None` (keep the
  user's choice, the frontend's job). Don't add a `both` wire variant.
- **Exact size is prompt-wording only.** `size_min == size_max` already says "exactly N"; the prompt teaches the model
  to set them equal (empty files → both `0`). The frontend's `applySizeFromAi` maps `min == max` to the `eq` chip.
