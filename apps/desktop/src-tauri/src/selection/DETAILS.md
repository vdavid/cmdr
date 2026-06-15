# Selection details

Depth for the Selection backend. `CLAUDE.md` holds the must-knows; this file holds the IPC signatures, AI pipeline, and
decision rationale.

## IPC surface (`crate::commands::selection`)

- **`translate_selection_query(prompt, sample_names, current_type)`**: AI translation, cloud-only. `current_type` (the
  dialog's type toggle as `Option<bool>`) is passed as prompt context. Returns `SelectionTranslateResult` (carrying
  optional `is_directory`) or a typed `AiTranslateError { kind, message }`. The cloud-only gate maps to
  `kind = notConfigured`.
- **`get_recent_selections(limit)`**: persisted entries, newest first.
- **`add_recent_selection(entry, max_count)`**: adds + dedupes + caps.
- **`remove_recent_selection(id)`**: removes by id; no-op when missing.
- **`clear_recent_selections()`**: drops every entry.
- **`apply_recent_selections_max_count(max_count)`**: live-applies a freshly tuned cap.

## AI translation steps

`translate_selection_query(prompt, sample_names, current_type)`:

1. Verifies the AI provider is `cloud`; hard-errors otherwise.
2. `ai::build_classification_prompt(&sample_names, current_type)` assembles the system prompt with today's date, the
   folder sample, and the type context (`Some(true)` folders / `Some(false)` files / `None` both). An omitted `type`
   from the model keeps the user's choice.
3. `chat_completion` via `crate::ai::client` with `temperature: 0.2`, `max_tokens: 300`, `top_p: 0.9`.
4. `ai::parse_selection_response` → `ParsedSelectionLlmResponse`.
5. `ai::build_selection_translate_result` → the wire result.

See [`ai/CLAUDE.md`](ai/CLAUDE.md) for prompt design, parser tolerances, caveat/kind defaulting, and the real-LLM eval.

## Decisions

- **Separate `selection-history.json` from `search-history.json`**: one file with a `kind` discriminator was rejected.
  The schemas already diverge (`scope` and `exclude_system_dirs` are irrelevant for Selection), and coupling two
  unrelated migrations forever didn't earn its keep. Two files cost nothing at runtime.
- **Re-export `HistoryMode` and `HistoryFilters` from `search::history`**: the two pure data types are identical in
  intent across consumers. The `SelectionHistoryEntry` struct stays separate so the on-disk schema doesn't bind
  Selection to Search's canonical-key shape. If Search's mode set or filter shape ever diverges, the re-export drops out
  and the types fork; the wiring is isolated enough that the change is mechanical.
- **Cloud-only AI**: folder samples weigh 1–3k tokens; the prompt plus completion lives at ~4–5k tokens. Local 4–8K
  context models often can't fit the payload, and quality on small models is unreliable for pattern inference. The
  frontend tooltip ("AI selection needs a cloud provider. Set one in Settings > AI.") matches the backend hard-error
  message for non-cloud callers.
- **`pattern` + `kind` instead of structured filter types**: the matcher runs on the frontend in JS, so there's no
  benefit to round-tripping a typed glob through Rust; the parsed string is the contract.
