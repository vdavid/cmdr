# Search details

Depth for the search backend. `CLAUDE.md` holds the must-knows; this file holds the design rationale.

## Decisions

- **In-memory `Vec` + rayon instead of SQLite queries**: the index has ~5M entries. SQLite `LIKE '%query%'` takes 1–3s
  (full table scan). Loading entries into a `Vec` and scanning with rayon gives sub-second results. The index loads
  lazily on dialog open and drops after idle (5 min timer + 10 min backstop), ~600 MB resident while active.
- **Structured `SearchQuery` model, not free-text SQL**: safe (no injection), composable (the AI mode fills the same
  struct), and simple to execute (single pass over the in-memory `Vec`). The frontend owns query building; the backend
  is a pure filter engine.
- **Path reconstruction at search time, not stored**: storing full paths would double memory. Reconstructing by walking
  the parent chain is O(depth) per result (for 30 results at average depth 8, ~240 HashMap lookups, microseconds).
- **`engine.rs` is pure (no I/O, no DB)**: it takes `&SearchIndex` + `&SearchQuery`, scans in-memory with rayon, returns
  results. Trivially testable without mocks; the hot path is isolated from side effects.
- **`types.rs` (data) separate from `query.rs` (operations)**: `types.rs` is imported by everything, so keeping it
  logic-free prevents circular dependencies and makes the data model easy to find.
- **AI pipeline lives in `search::ai`, not `commands/`**: the parser, prompt, and query builder are search domain logic,
  not IPC concerns; `commands/search.rs` stays a thin wrapper. AI-internal decisions live in [`ai/CLAUDE.md`](ai/CLAUDE.md).
- **Add history only on "Open in pane"**: David's explicit call. The 1000-entry budget stays signal-rich when it tracks
  results worth acting on, not every keystroke-debounced filename search. The gate is a frontend convention, not
  Rust-enforced.
- **`_schemaVersion` mismatch quarantines instead of migrating in place**: there's only schema v1, so a migrator would
  be speculative. When v2 lands, replace the quarantine branch with a `match` on the version calling a
  `migrate_v1_to_v2` helper.
