# Agent store (`agent/store/`)

`main.db`: the agent's durable store, a peer to `operation-log.db` in the app data dir (agent-spec D1/D3).
Conversations, messages (typed `content_blocks` JSON), an FTS5 index over message text, a per-day cost meter, and the
proposal spine. Depth: `DETAILS.md`.

## Module map

- `migrations.rs` — the forward-migration ladder + the v1 DDL. Mirrors `operation_log/store/migrations.rs`.
- `connection.rs` — WAL/auto-vacuum pragmas; write connections run the ladder, no custom collation.
- `query.rs` — conversations, messages, the FTS5 search + its input sanitizer, the cost meter. `AgentStore` (in
  `mod.rs`) owns the schema lifecycle; `agent::start` opens the DB and registers `AgentDb` in state.
- `events.rs` — `ConversationEvent`, the timeline half of `messages`; `rows.rs` — the insert both writers share.
- `proposals/` — the sweep / group / op spine and the claim transaction, its own C+D pair:
  `proposals/CLAUDE.md`. A producer's own per-op sidecar table lives with that producer (`proposal_rename_evidence` is
  `agent/tools/propose/rename/`'s), never as columns on the shared `proposal_ops`.

## Must-knows

- **The ladder is DURABLE and MIGRATES; it never delete-and-recreates on a version bump.** Same discipline as the
  operation log: append a `Migration`, NEVER edit or renumber a shipped step; refuse a downgrade; delete-and-recreate
  ONLY a genuinely unparseable file (the typed `NotADatabase`/`DatabaseCorrupt` code, never a string).
- **FTS5 needs NO rusqlite feature** — the `bundled` SQLite compiles it in. `fresh_open_builds_current_schema` is the
  runtime guard: it MATCHes the empty index, so a bundled build without FTS5 fails there.
- **NEVER feed raw user input into `... MATCH ?`.** Filename fragments (`report(v2)`, `foo:bar`, a bareword
  `AND`/`OR`/`NOT`, an unbalanced `"`) throw an fts5 syntax error, and parameter binding does NOT help: the string is
  parsed as query syntax. Route through `sanitize_fts_query`.
- **`search_conversations` JOINs the FTS match back to `messages`, which MASKS orphan index rows**, so a broken delete
  trigger looks fine through search. External-content FTS5 desyncs easily, so test de-indexing against `messages_fts`
  directly, never only through the search API.
- **`cost_meter.conversation_id` is NOT NULL by necessity.** SQLite treats NULLs as distinct in a PK, so a nullable PK
  column breaks `ON CONFLICT DO UPDATE` (every write duplicates instead of upserting). The per-day cross-thread rollup
  is computed at query time.
- **One reserved conversation row nothing lists** (origin `quiet_wakes`, v8) keeps what quiet wakes spent once their
  threads are deleted. ❌ Never count threads with a bare `COUNT(*) FROM conversations`. `delete_conversation` is the
  store's one delete; a thread that spent anything goes through `discard_conversation_keeping_cost`, which folds its
  cost onto that row first. `DETAILS.md` § v8.
- **`content_blocks` is a backend-only column**: it carries the opaque provider reasoning blob, which must NEVER cross
  to the frontend. `StoredMessage` is not a wire type; the IPC layer derives a display `MessageView`.
- **`role = 'event'` rows are UI timeline entries (typed `ConversationEvent`), NEVER transcript content.** The token
  lives outside `AgentRole` so the transcript loader can't feed one to a provider; a new reader of `messages` branches
  on `StoredContent` and decides what an event means for it. They carry no `text_for_search`. ⚠️ And their limit: an
  outcome recorded only as an event teaches the agent nothing (`../outcomes.rs`).
  `conversations.last_model` (v2) records the last turn's model, powering model-change events.
- **Consent lives in the `meta` table, not a settings preference.** `get_consent`/`set_consent`/`clear_consent` own the
  `ask_cmdr_consent_version` + `ask_cmdr_consent_at` rows; a partial or absent record reads as no consent, so the gate
  fails CLOSED. The copy version belongs to `agent::consent::CONSENT_COPY_VERSION`, not here.
- **`conversation_cost` sums a thread's cost meter and ANDs `priced`** (any unpriced turn ⇒ `fully_priced = false`), so
  the footer stays honest: local ⇒ free, unpriced ⇒ unknown, never a silent $0. Pricing itself is
  `crate::agent::pricing`.

Depth: `DETAILS.md`.
