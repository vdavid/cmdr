# Agent store (`agent/store/`)

`main.db`: the agent's durable store, a peer to `operation-log.db` in the app data dir (agent-spec D1/D3). Conversations,
messages (typed `content_blocks` JSON), an FTS5 search index over message text, and a per-day cost meter. Depth (DDL
rationale, the FTS design, the search-JOIN gotcha, no-retention-in-v1): [DETAILS.md](DETAILS.md).

## Module map

- `migrations.rs` — the forward-migration ladder + the v1 DDL. Mirrors `operation_log/store/migrations.rs`.
- `connection.rs` — WAL/auto-vacuum pragmas; write connections run the ladder, no custom collation.
- `query.rs` — conversations, messages, the FTS5 search + its input sanitizer, the cost meter. `AgentStore` (in
  `mod.rs`) owns the schema lifecycle; `agent::start` opens the DB and registers `AgentDb` in state.

## Must-knows

- **The ladder is DURABLE and MIGRATES; it never delete-and-recreates on a version bump.** Same discipline as the
  operation log (which it mirrors): append a `Migration`, NEVER edit or renumber a shipped step; refuse a downgrade;
  delete-and-recreate ONLY a genuinely unparseable file (matched on the typed `NotADatabase`/`DatabaseCorrupt` sqlite
  code, never a string).
- **FTS5 is net-new here, but needs NO rusqlite feature** — the `bundled` SQLite already compiles it in. (rusqlite 0.39
  has no `fts5` feature; the plan's "flip the feature" premise was stale.) `fresh_open_builds_current_schema` is the
  runtime guard: it MATCHes the empty index, so a bundled build without FTS5 fails there.
- **NEVER feed raw user input into `... MATCH ?`.** Ordinary filename fragments (`report(v2)`, `foo:bar`, a bareword
  `AND`/`OR`/`NOT`, an unbalanced `"`) throw an fts5 syntax error, and parameter binding does NOT help (the string is
  parsed as query syntax). Always route through `sanitize_fts_query`.
- **`search_conversations` JOINs the FTS match back to `messages`, which MASKS orphan index rows.** A deleted message
  can't join, so a broken delete trigger looks fine through search. To test FTS de-index correctness, assert on the FTS
  index directly (`SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH …`), never only through the search API.
  External-content FTS5 is easy to desync; this is the top DB risk here.
- **`cost_meter.conversation_id` is NOT NULL by necessity.** SQLite treats NULLs as distinct in a PK, so a nullable
  column inside the PK breaks `ON CONFLICT DO UPDATE` (every write inserts a duplicate instead of upserting). Keep it NOT
  NULL; the per-day cross-thread rollup is computed at query time (`SUM … GROUP BY day`).
- **`content_blocks` is a backend-only column.** It carries the opaque provider reasoning blob, which must NEVER cross to
  the frontend. `StoredMessage` is deliberately not a wire type; the IPC layer derives a display `MessageView`.
- **Consent lives in the `meta` table, not a settings preference.** `get_consent`/`set_consent`/`clear_consent`
  read/write the `ask_cmdr_consent_version` + `ask_cmdr_consent_at` meta rows (agent state, `sqlite3`-inspectable). A
  partial/absent record reads as no consent, so the gate fails CLOSED. The copy version is owned by
  `commands/agent.rs::CONSENT_COPY_VERSION`, not here.
- **`conversation_cost` sums a thread's whole cost meter and ANDs `priced`** (any unpriced turn ⇒ `fully_priced = false`)
  and lists the distinct providers, so the per-thread footer can render the honest miss-path (local ⇒ free; unpriced ⇒
  unknown; never a silent $0). Pricing itself is `crate::agent::pricing`, not the store.

Depth: [DETAILS.md](DETAILS.md).
