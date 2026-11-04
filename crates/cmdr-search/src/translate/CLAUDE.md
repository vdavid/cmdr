# Search translation (`translate`)

Natural-language → `SearchQuery` translation: a classification prompt, a key-value line parser, deterministic
enum-to-value mappings, and an assembler. The LLM only classifies intent; Rust computes every value. No model, no I/O:
the desktop host (`apps/desktop/src-tauri/src/search/ai.rs`) picks the provider, sends the prompt, and passes its
calendar (`LocalDays`) and home folder in. Parent: `../../CLAUDE.md`.

## File map

- **`../translate.rs`**: the public surface, flat re-exports from private modules.
- **`prompt.rs`**: `CLASSIFICATION_PROMPT`, `{TODAY}` and `{CURRENT_TYPE}` substituted per call.
- **`parser.rs`**: `ParsedLlmResponse`, `parse_llm_response` (first-colon split, enum validation), `fallback_keywords`.
- **`query_builder.rs`**: assembles the `SearchQuery` from `mappings/`, plus caveat, display, label, and
  `iso_date_to_timestamp`.
- **`types.rs`**: the DTOs `TranslatedQuery` and `TranslateDisplay`, re-exported by the desktop's `commands/search.rs`.
- **`mappings/`** (hub `mappings.rs`, with `KIB/MIB/GIB` and `KNOWN_EXTENSIONS`): type, time, size and scope, keyword.
  `local_days.rs`: `LocalDays`, the host's calendar (`FixedZone` for tests).

## Must-knows

- **LLM classifies, Rust computes.** The model picks a token from a closed enum (`last_week`, `huge`, `documents`) and
  `mappings/` maps it. A new filter dimension extends the prompt enum and a mapping function in lockstep.
- **Key-value lines, not JSON.** The parser splits on the FIRST `:`; a missing line is `None`, a malformed one drops one
  field, not the whole reply.
- **Validation lives in the parser.** `validate_type` / `_time` / `_size` / `_folders` drop unknown values, so the
  assembler only sees known vocabulary. `scope` is NOT validated: `scope_to_paths` tells a known name from a `PATH`, so
  ❌ no `validate_scope`.
- **Every time boundary and `{TODAY}` are LOCAL** (`LocalDays`), ❌ never `now_utc()`; the crate reads no clock.
- **`iso_date_to_timestamp` is public for the MCP executor** (the desktop's `search::ai::iso_date_to_timestamp`). ❌
  Don't make it private.
- **`KNOWN_EXTENSIONS` lives in `mappings.rs`**, ❌ not `keyword_mapping.rs`.
- **`LABEL_MAX_CHARS = 40` equals `AI_LABEL_MAX_CHARS`** in `apps/desktop/src/lib/search/snapshot-label.ts`.
- **The `(?is)` prefix rule lives in `merge_keyword_and_type` alone**: single-sourced, ❌ never re-derived.

`type: none`, `folders` vs `type`, caveat priority, and the decisions (key-value vs JSON, classify-vs-compute,
single-pass, the label field): `DETAILS.md`.
