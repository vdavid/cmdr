# Agent store (`agent/store/`)

`main.db`: the agent's durable store, a peer to `operation-log.db` in the app data dir (agent decisions D1/D3).
Conversations, messages (typed `content_blocks` JSON), an FTS5 index over message text, a per-day cost meter, and the
proposal spine. Depth: `DETAILS.md`.

## Module map

- `migrations.rs` — the forward-migration ladder + the v1 DDL. Mirrors `crates/cmdr-operation-log/src/store/migrations.rs`.
- `connection.rs` — WAL/auto-vacuum pragmas; write connections run the ladder, no custom collation.
- `query.rs` — conversations, messages, the cost meter; `search.rs` — thread search + the FTS sanitizer;
  `wake_place.rs` — a wake row's title. `AgentStore` (`mod.rs`) owns the schema lifecycle.
- `events.rs` — `ConversationEvent`, the timeline half of `messages`; `rows.rs` — the shared insert; `turns.rs` —
  turn-scoped reads.
- `consent.rs` — `meta` consent records: "Allow cloud AI" (`ai/cloud_consent.rs`'s) and Ask Cmdr's read-only legacy one.
- `proposals/` — the sweep / group / op spine and the claim transaction: `proposals/CLAUDE.md`. A producer's per-op
  sidecar lives with that producer (`proposal_rename_evidence`), never as `proposal_ops` columns; only a server group's
  draft (v13) is the spine's own.

## Must-knows

- **The ladder is DURABLE and MIGRATES; it never delete-and-recreates on a version bump.** Same discipline as the
  operation log: append a `Migration`, NEVER edit or renumber a shipped step; refuse a downgrade; set aside and
  recreate ONLY an unparseable file (the typed `NotADatabase`/`DatabaseCorrupt` code, never a string).
- **FTS5 needs NO rusqlite feature** — the `bundled` SQLite compiles it in. `fresh_open_builds_current_schema` is the
  runtime guard: it MATCHes the empty index, so a bundled build without FTS5 fails there.
- **NEVER feed raw user input into `... MATCH ?`.** Filename fragments (`report(v2)`, `foo:bar`, a bareword
  `AND`/`OR`/`NOT`, an unbalanced `"`) throw an fts5 syntax error, and parameter binding does NOT help: the string is
  parsed as query syntax. Route through `sanitize_fts_query`.
- **`search_conversations` JOINs the FTS match back to `messages`, which MASKS orphan index rows**, so a broken delete
  trigger looks fine through search. External-content FTS5 desyncs easily, so test de-indexing against `messages_fts`
  directly, never only through the search API.
- **`cost_meter.conversation_id` is NOT NULL by necessity.** A nullable PK column breaks `ON CONFLICT DO UPDATE` (SQLite
  treats NULLs as distinct, so every write duplicates). The cross-thread rollup is computed at query time.
- **One reserved conversation row nothing lists** (origin `quiet_wakes`, v8) keeps what quiet wakes spent once their
  threads are deleted. ❌ Never count threads with a bare `COUNT(*) FROM conversations`. `delete_conversation` is the
  store's one delete; a thread that spent anything goes through `discard_conversation_keeping_cost`, which folds its
  cost onto that row first. `DETAILS.md` § v8.
- **`content_blocks` is a backend-only column**: it carries the opaque provider reasoning blob, which must NEVER cross
  to the frontend. `StoredMessage` is not a wire type; `../chat/message_view.rs` derives a display `MessageView`.
- **`role = 'event'` rows are UI timeline entries (typed `ConversationEvent`), NEVER transcript content.** The token
  lives outside `AgentRole` so the transcript loader can't feed one to a provider; a new reader of `messages` branches
  on `StoredContent` and decides what an event means. No `text_for_search`. ⚠️ Their limit: an outcome recorded only as
  an event teaches the agent nothing (`../outcomes.rs`). `last_model` (v2) + `last_chat_memory` (v9) power slot-change
  events; ❌ never stamp v3's `last_prompt_budget`, it's the gauge's (`DETAILS.md` § v9).
- **Consent lives in `meta`, not a preference.** A partial or absent record reads as no consent: gates fail CLOSED.
- **`conversation_cost` sums a thread's cost meter and ANDs `priced`** (any unpriced turn ⇒ `fully_priced = false`), so
  the footer stays honest: local ⇒ free, unpriced ⇒ unknown, never a silent $0. Pricing itself is
  `crate::agent::pricing`.

Depth: `DETAILS.md`.
