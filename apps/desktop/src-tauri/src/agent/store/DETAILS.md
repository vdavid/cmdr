# Agent store details

Pull-tier docs for `agent/store/`. Must-knows live in [CLAUDE.md](CLAUDE.md). This store is the app's second durable DB,
built on the operation log's proven template (`operation_log/store/`); this file records only what's specific to
`main.db`, and points at the template for the shared discipline.

## Why a second durable DB, mirroring the operation log

`main.db` holds agent state (conversations, messages, cost) and lives for years, so it can't be a
delete-and-recreate cache like the drive index or `importance.db`. The operation-log effort built the forward-migration
ladder as a reusable template (agent-spec D3: a second consumer proves it generalizes). `store/migrations.rs` and
`store/connection.rs` mirror the operation log's structure closely: a `meta` anchor table outside the ladder, one
transaction per step, refuse-downgrade, delete-and-recreate only on the typed corrupt-DB sqlite code, and WAL +
incremental auto-vacuum pragmas with NO custom collation (so the file stays `sqlite3`-inspectable). The two ladders are
deliberately separate copies of the same tiny mechanism (each store self-contained, no cross-subsystem coupling); the
`token_enum!` macro is duplicated in `agent/types.rs` for the same reason.

## v1 DDL rationale (`migrate_v1_initial`)

The exact schema is in `migrations.rs`. The non-obvious choices:

- **`conversations.origin` is a nullable token column.** NULL means user-started (the only v1 case). It exists as cheap
  insurance so a future notification-spawned thread (the full agent's proactive surfaces) is a column value, not a
  migration (spec §3). The typed `ConversationOrigin` (`agent/types.rs`) carries the one anticipated `Notification`
  token; v1 never writes a non-null origin.
- **`messages.content_blocks` is typed JSON**, the serialized `Vec<AgentPart>` from the `AgentLlm` seam. The opaque provider
  reasoning blob rides inside it and is backend-only — it must never reach the frontend. `text_for_search` is the plain
  user+assistant prose extracted at insert (never tool blobs), the only thing the FTS index sees.
- **`prompt_tokens` / `completion_tokens` are nullable** (assistant turns only).
- **`messages_fts` is external-content FTS5** (`content='messages'`, `content_rowid='id'`): the index stores the term
  data but not a copy of the text, pointing back at `messages.id`. Three triggers keep it in sync — insert indexes,
  delete de-indexes (the `'delete'` command), update does both. There is no in-tree trigger-based FTS pattern to copy
  (the operation log folds a column in Rust instead), so these were authored fresh and are the area most prone to a
  subtle desync.

## FTS5 comes from `bundled`, not a feature

rusqlite 0.39 has no `fts5` feature; the FTS5 module is compiled into the `bundled` SQLite amalgamation by default. So
enabling FTS5 was a no-op on `Cargo.toml` (the plan assumed a feature flip). The guard against a future bundled build
dropping FTS5 is `fresh_open_builds_current_schema`, which runs a `MATCH` against the empty index.

## The search JOIN masks orphan FTS index rows

`search_conversations` resolves matches through `WHERE m.id IN (SELECT rowid FROM messages_fts WHERE … MATCH …)` and
JOINs to `messages` + `conversations`. Because the match is joined back to `messages`, a deleted message (whose row is
gone) can't contribute a hit even if its FTS index entry was never removed. That makes the search API insensitive to a
broken delete trigger — an orphan index row is invisible through it. Correctness of the delete/update triggers is
therefore tested by asserting on the FTS index directly (`SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH …`),
which the `fts_delete_trigger_deindexes_removed_messages` test does. Verified: with a delete trigger that fails to emit
the `'delete'` command, the direct-index assertion fails while the search-only assertion passes (2026-07-12, red→green).

## The cost meter and the NULL-in-PK trap

`cost_meter` is keyed `(day, conversation_id, provider, model)` and accumulates via `ON CONFLICT DO UPDATE SET col = col
+ excluded.col`. `conversation_id` is NOT NULL because SQLite treats NULLs as distinct in a PK/UNIQUE: a nullable column
in the PK would make every write a fresh insert (never an upsert), silently duplicating rows and double-counting. `priced`
ANDs on conflict, so a day/thread/model that ever took an unpriced contribution reads unpriced — its cost is then an
honest lower bound ("unknown"), never a silent $0 (spec §2.4). The per-day cross-thread rollup (`cost_summary`) sums with
`GROUP BY day` and reads `fully_priced` from `MIN(priced)`.

## No auto-retention in v1

Transcripts are small (spec §3), so there's no pruning yet. When real sizes exist, the operation log's
`operation_log/retention.rs` + `PruneRequest` scaffold (age + size prune, dir GC, vacuum on a startup + periodic timer)
is the template to follow — a follow-up, not built now.

## Wiring

`agent::start(app)` (in `agent/mod.rs`, modeled on `operation_log::start`) opens the DB through `AgentStore::open` (which
runs the schema lifecycle) and registers an `AgentDb` handle in managed state. `AgentDb` holds the DB path and hands out
read/write connections; the chat runtime owns the write-connection lifetime and single-writer discipline (the store
itself does not add a writer thread).
