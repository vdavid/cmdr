# Selection AI

Natural-language → glob/regex translation for the Selection dialog. Cloud-only: small local models can't reliably fit
the folder sample plus the structured prompt. Parent: [`../CLAUDE.md`](../CLAUDE.md). Sibling pattern:
[`../../search/ai/CLAUDE.md`](../../search/ai/CLAUDE.md) (same key-value prompt shape, narrower field set, no scope or
system-dir exclusion).

## Module map

- **`mod.rs`**: re-exports `build_classification_prompt`, `parse_selection_response`,
  `build_selection_translate_result`, and the `ParsedSelectionLlmResponse` / `SelectionTranslateResult` types
- **`prompt.rs`**: `build_classification_prompt(sample_names, current_type)` + `format_sample_block`. Pure
- **`parser.rs`**: `parse_selection_response(text)` → `ParsedSelectionLlmResponse`
- **`query_builder.rs`**: `build_selection_translate_result(parsed)` + `generate_caveat` / `build_label`
- **`real_llm_eval_test.rs`**: six `#[ignore]`-gated integration tests against the live OpenAI API

The IPC entry (`translate_selection_query`) lives in `crate::commands::selection`. Prompt design, parser tolerances, and
the eval: [DETAILS.md](DETAILS.md).

## Must-knows

- **Key-value response, not JSON.** JSON generation is the #1 failure mode for smaller LLMs. Key-value lines parse
  trivially, missing lines skip individually, and one malformed line never voids the rest. Parser splits on the first
  `:` only (regex patterns contain `:`).
- **Validation is loose by design.** The parser's job is structural, not semantic; the frontend matcher re-validates (a
  malformed regex throws at compile time and the dialog surfaces the caveat). Don't add semantic validation here.
- **Don't drop `kind` to `None` when `pattern` is present.** The model often omits `kind:` for obvious globs (`*.png`);
  `build_selection_translate_result` substitutes `"glob"` in that case. Removing the default forces a re-prompt for the
  most common case and the eval flakes. Tested by `build_no_pattern_means_no_kind_either`.
- **`kind` must clear when `pattern` clears.** A `kind: regex` with no `pattern:` is a broken LLM response; the builder
  drops `kind` to `None` so the frontend doesn't compile a half-built query against a phantom pattern. Tested by
  `broken_response_returns_caveat_no_pattern`.
- **`type` is optional and leave-alone, NOT a third "both" value.** The prompt omits `type` unless the intent is clearly
  only files or only folders; the user's current type rides in as context. `type: both`/unknown maps to `None` (= keep
  the user's choice, the frontend's job). `query_builder` maps `item_type` → `is_directory` (`folder → true`,
  `file → false`, absent → `None`). Don't add a `both` wire variant.
- **Size values reject units.** The prompt asks for bytes; `5mb` drops to `None` rather than guessing. Underscores and
  commas (`1_048_576`, `5,242,880`) are stripped first. `size_min == size_max` means "exactly N" (empty files → both
  `0`); the frontend's `applySizeFromAi` maps that to the `eq` chip.
- **Never pass an empty folder sample.** `format_sample_block` returns the `(empty folder)` sentinel for empty and
  all-blank inputs; an empty block makes the model hallucinate a layout and the eval emits wild patterns. The sample
  de-dupes preserving order and truncates at `MAX_SAMPLE = 240`.
- **Caveat priority**: an LLM-emitted `note:` (sanitized to ≤200 chars, HTML angle brackets stripped) wins; the built-in
  "Couldn't translate" message fires only when nothing usable came back (no pattern, no size, no date filter).
- **Iterate the prompt against the eval, not unit tests.** Unit tests pin pure-function behavior; LLM-interpretation
  drift only shows in `real_llm_eval_test.rs`. Run the eval after any prompt edit and ship them together.
- **No `eprintln!` / `println!` / `dbg!`** (denied crate-wide). Use `log::debug!(target: "selection::ai", …)`;
  `--no-capture` runs work fine with `log::info!`.
