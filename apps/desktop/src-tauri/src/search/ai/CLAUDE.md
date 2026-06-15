# Search AI

Natural-language → `SearchQuery` translation pipeline: a classification prompt, a key-value line parser, deterministic enum-to-value mappings, and an assembler. The LLM only classifies intent; Rust computes every value. Parent: [`../CLAUDE.md`](../CLAUDE.md).

## File map

- **`mod.rs`**: Re-exports the public surface (`build_classification_prompt`, `parse_llm_response`, `build_search_query`, `build_translate_display`, `build_translated_query`, `generate_caveat`, `iso_date_to_timestamp`, `fallback_keywords`, `ParsedLlmResponse`)
- **`prompt.rs`**: `CLASSIFICATION_PROMPT` const + `build_classification_prompt(current_type)`. Substitutes `{TODAY}` + `{CURRENT_TYPE}` at runtime. Instructs the LLM to emit `keywords / type / time / size / scope / exclude / folders / note / label` one-per-line. `folders` is the file-vs-folder dimension and is OPTIONAL: `current_type` (`Some(true)` folders / `Some(false)` files / `None` both) is surfaced as a context line so the model knows the user's current choice and that omitting `folders` keeps it
- **`parser.rs`**: `ParsedLlmResponse` + `parse_llm_response()`. First-colon split, enum validation (`validate_type` / `_time` / `_size` / `_folders`), unknown keys silently skipped. `fallback_keywords()` for total LLM failure: top-3 longest tokens > 2 chars
- **`query_builder.rs`**: Assembles `SearchQuery` from a `ParsedLlmResponse` by invoking `mappings/`. Also `generate_caveat`, `build_translate_display`, `build_translated_query`, `build_label`, and `iso_date_to_timestamp` (also called from `crate::mcp::executor`)
- **`mappings/`**: Pure LLM-enum → value conversions split by domain (`type_mapping`, `time_mapping`, `size_scope_mapping`, `keyword_mapping`) plus shared `KB/MB/GB` and `KNOWN_EXTENSIONS`. Single re-export hub via `mappings/mod.rs`

## Conventions

- **LLM classifies, Rust computes.** The LLM never produces regex, ISO dates, or byte counts. It picks a token from a closed enum (`last_week`, `huge`, `documents`, …) and Rust maps it deterministically in `mappings/`. New filter dimensions extend both the prompt enum and a matching mapping function in lockstep.
- **Key-value lines, not JSON.** One `key: value` line per dimension. The parser splits on the first `:` (values may contain colons — see the scope test). Missing lines stay `None` and apply no filter; malformed lines are individually droppable.
- **Validation lives in the parser, not the assembler.** `validate_type` / `_time` / `_size` / `_folders` discard unknown enum values at parse time, so `build_search_query` only ever sees vocabulary it recognizes. `keywords`, `scope`, `exclude`, `note`, `label` pass through unvalidated (free text by design).
- **Caveat priority**: LLM-provided `note` (sanitized: strip `<>`, truncate 200 chars) wins over any Rust-inferred caveat. The "no filename filter" fallback fires only when both `name_pattern` is `None` and the LLM omitted `note`.
- **Label truncation mirrors the frontend.** `LABEL_MAX_CHARS = 40` here must stay equal to `AI_LABEL_MAX_CHARS` in `lib/search/snapshot-label.ts`. Truncation is Unicode-scalar based (emoji-safe) and reserves one char for the `…` ellipsis. Blank or missing label → `None` → frontend falls back to the original prompt.

## Key decisions

**Decision**: Key-value line output, not JSON.
**Why**: JSON generation is the #1 failure mode for small LLMs (≈13% parse failure on 2B models). Lines like `keywords: rymd\ntype: documents\ntime: recent` are trivial for the model to produce and for `split_once(':')` to parse. A malformed line drops one field, not the whole response.

**Decision**: LLM picks enum tokens, Rust computes values.
**Why**: Even 2B models reliably map "last week" → `last_week` across languages, but asking them to emit "1717372800" or `^.*\.(pdf|doc)$` fails ~60% of the time. Separating classification (model) from computation (`mappings/`) makes the pipeline robust regardless of model size or quantization.

**Decision**: Single LLM pass, no refine step.
**Why**: Deterministic mapping leaves nothing to refine. A second pass on previous designs regressed ~15% of queries (over-narrowing, flag dropping) and doubled latency.

**Decision**: The prompt asks for a `label:` field even though it's display-only.
**Why**: Snapshot panes need a short breadcrumb title ("Big PDFs from this week"), and the model is already summarizing intent for the other fields — one more line is free. Rust truncates and strips trailing punctuation; the frontend falls back to the raw prompt when `build_label` returns `None`.

## Gotchas

- **`scope` values are NOT enum-validated.** The prompt lists `downloads|documents|desktop|dotfiles|PATH`, but `parse_llm_response` accepts any non-empty `scope:` line verbatim. `scope_to_paths` in `mappings/size_scope_mapping.rs` is what actually decides what's a known name vs. a literal path. Don't add a `validate_scope` thinking it tightens the contract — it would block the `PATH` case.
- **`type: none` is a valid value, not a missing field.** It survives `validate_type` and reaches `build_search_query`, where `type_to_filter("none")` returns `None`. Treat parser-`Some("none")` and parser-`None` as the same downstream; don't branch on the string.
- **`folders` ≠ the `type` enum.** `folders: yes|no` is the file-vs-folder dimension (→ `is_directory`); the `type:` enum is the file-CATEGORY (photos, code, …). The dialog passes the user's current file-vs-folder choice as `current_type` and applies the returned `is_directory` leave-alone-if-null on the frontend (`applyTypeFromAi`). The "keep the user's choice when omitted" semantics live on the FRONTEND; Rust just returns `is_directory: Option<bool>` (already does, unchanged). Don't add a Rust-side default that forces `yes`/`no`.
- **`iso_date_to_timestamp` is re-exported for the MCP executor.** It looks like an internal helper of the assembler but `crate::mcp::executor` calls it for date arguments coming from MCP clients. Don't move it into `query_builder` private scope.
- **`KNOWN_EXTENSIONS` lives in `mappings/mod.rs`, not `keyword_mapping.rs`.** `keywords_to_pattern` reads it to detect exact-extension keywords (`.heic`, `package.json`). Adding a new extension means editing the constant in the parent `mod.rs` of `mappings/`, not the keyword file.

Full details: [DETAILS.md](DETAILS.md).
