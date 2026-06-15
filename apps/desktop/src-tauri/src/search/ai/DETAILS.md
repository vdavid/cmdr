# Search AI details

Depth and rationale for the NL → `SearchQuery` pipeline. `CLAUDE.md` holds the must-knows.

## The `folders` / `current_type` dimension

`folders` is the file-vs-folder dimension and is OPTIONAL in the prompt. `build_classification_prompt(current_type)`
surfaces the user's current choice (`Some(true)` folders / `Some(false)` files / `None` both) as a context line so the
model knows that omitting `folders` keeps it. The dialog passes the user's current file-vs-folder choice as
`current_type` and applies the returned `is_directory` leave-alone-if-null on the frontend (`applyTypeFromAi`). Rust
just returns `is_directory: Option<bool>`.

## Decisions

- **Key-value line output, not JSON.** JSON generation is the #1 failure mode for small LLMs (≈13% parse failure on 2B
  models). Lines like `keywords: rymd\ntype: documents\ntime: recent` are trivial for the model to produce and for
  `split_once(':')` to parse. A malformed line drops one field, not the whole response.
- **LLM picks enum tokens, Rust computes values.** Even 2B models reliably map "last week" → `last_week` across
  languages, but asking them to emit "1717372800" or `^.*\.(pdf|doc)$` fails ~60% of the time. Separating
  classification (model) from computation (`mappings/`) makes the pipeline robust regardless of model size or
  quantization.
- **Single LLM pass, no refine step.** Deterministic mapping leaves nothing to refine. A second pass on previous designs
  regressed ~15% of queries (over-narrowing, flag dropping) and doubled latency.
- **The prompt asks for a display-only `label:` field.** Snapshot panes need a short breadcrumb title ("Big PDFs from
  this week"), and the model is already summarizing intent for the other fields, so one more line is free. Rust
  truncates and strips trailing punctuation; the frontend falls back to the raw prompt when `build_label` returns
  `None`.
