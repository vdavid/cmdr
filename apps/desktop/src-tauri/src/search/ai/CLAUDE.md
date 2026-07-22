# Search AI

Natural-language → `SearchQuery` translation: a classification prompt, a key-value line parser, deterministic
enum-to-value mappings, and an assembler. The LLM only classifies intent; Rust computes every value. Parent:
`../CLAUDE.md`.

## File map

- **`mod.rs`**: re-exports the public surface (`build_classification_prompt`, `parse_llm_response`, `build_search_query`,
  `build_translate_display`, `build_translated_query`, `generate_caveat`, `iso_date_to_timestamp`, `fallback_keywords`,
  `ParsedLlmResponse`).
- **`prompt.rs`**: `CLASSIFICATION_PROMPT` const + `build_classification_prompt(current_type)`; substitutes `{TODAY}` +
  `{CURRENT_TYPE}` at runtime. Instructs the LLM to emit `keywords / type / time / size / scope / exclude / folders /
  note / label` one per line.
- **`parser.rs`**: `ParsedLlmResponse` + `parse_llm_response()`. First-colon split, enum validation, unknown keys
  silently skipped. `fallback_keywords()` for total LLM failure: top-3 longest tokens > 2 chars.
- **`query_builder.rs`**: assembles `SearchQuery` by invoking `mappings/`. Also `generate_caveat`,
  `build_translate_display`, `build_translated_query`, `build_label`, and `iso_date_to_timestamp`.
- **`mappings/`**: pure LLM-enum → value conversions split by domain (`type`, `time`, `size_scope`, `keyword`) plus
  shared `KB/MB/GB` and `KNOWN_EXTENSIONS`. Re-export hub in `mappings/mod.rs`.

## Must-knows

- **LLM classifies, Rust computes.** The LLM never produces regex, ISO dates, or byte counts; it picks a token from a
  closed enum (`last_week`, `huge`, `documents`, …) and Rust maps it deterministically in `mappings/`. A new filter
  dimension extends both the prompt enum and a matching mapping function in lockstep.
- **Key-value lines, not JSON.** One `key: value` line per dimension; the parser splits on the FIRST `:` (values may
  contain colons). Missing lines stay `None` (no filter); a malformed line drops one field, not the whole response.
- **Validation lives in the parser, not the assembler.** `validate_type` / `_time` / `_size` / `_folders` discard
  unknown enum values at parse time, so `build_search_query` only sees vocabulary it recognizes. `keywords`, `scope`,
  `exclude`, `note`, `label` pass through unvalidated (free text by design).
- **`scope` values are NOT enum-validated.** The prompt lists `downloads|documents|desktop|dotfiles|PATH`, but the
  parser accepts any non-empty `scope:` line verbatim; `scope_to_paths` (`mappings/size_scope_mapping.rs`) decides known
  name vs. literal path. Don't add a `validate_scope`: it would block the `PATH` case.
- **`type: none` is a valid value, not a missing field.** It survives `validate_type` and reaches `build_search_query`,
  where `type_to_filter("none")` returns `None`. Treat parser-`Some("none")` and parser-`None` as the same downstream;
  don't branch on the string.
- **`folders` ≠ the `type` enum.** `folders: yes|no` is the file-vs-folder dimension (→ `is_directory`); `type:` is the
  file CATEGORY (photos, code, …). Rust returns `is_directory: Option<bool>`; the "keep the user's choice when omitted"
  semantics live on the FRONTEND (`applyTypeFromAi`). Don't add a Rust-side default that forces `yes`/`no`.
- **`iso_date_to_timestamp` is re-exported for the MCP executor** (`crate::mcp::executor` calls it for date arguments
  from MCP clients). It looks like an internal assembler helper but isn't; don't move it into `query_builder` private
  scope.
- **`KNOWN_EXTENSIONS` lives in `mappings/mod.rs`, not `keyword_mapping.rs`.** `keywords_to_pattern` reads it to detect
  exact-extension keywords (`.heic`, `package.json`). Add a new extension in the parent `mod.rs` of `mappings/`.
- **`LABEL_MAX_CHARS = 40` must stay equal to `AI_LABEL_MAX_CHARS` in `lib/search/snapshot-label.ts`.** Truncation is
  Unicode-scalar based (emoji-safe), reserving one char for the `…`. A blank/missing label → `None` → the frontend
  falls back to the original prompt.
- **Caveat priority**: an LLM-provided `note` (sanitized: strip `<>`, truncate 200 chars) wins over any Rust-inferred
  caveat. The "no filename filter" fallback fires only when both `name_pattern` is `None` and the LLM omitted `note`.

Full details (decision rationale: key-value vs JSON, classify-vs-compute, single-pass, the label field):
`DETAILS.md`.
