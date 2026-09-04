# Agent store details

Pull-tier docs for `agent/store/`. Must-knows live in `CLAUDE.md`. This store is the app's second durable DB,
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

## v2: `last_model` + event rows

`conversations.last_model` (nullable; NULL = no completed turn yet) records the model a
thread's most recent completed turn (or recorded model-change event) used. The chat
runtime and the `ask_cmdr_record_model_change` command compare against it to decide when
to log a "switched to X" timeline event; the full flow is `agent/chat/DETAILS.md` § Model-change events.

Event rows reuse the `messages` table with `role = 'event'` and `content_blocks` holding a
typed `ConversationEvent` (not `Vec<AgentPart>`): they share the per-conversation `seq`,
so ordering against real messages is free, and paging/history need no second table or
merge. The reader (`map_message_row`) branches on the role token into `StoredContent`
(`Message { role, parts }` vs `Event`); the token deliberately lives outside `AgentRole`
so the transcript loader can't feed an event to a provider. `text_for_search` stays empty
— a model name is not conversation content and never matches search.

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

`proactive_tokens_for_day` is the third reader: the wake loop's daily ceiling
(`agent/wake/DETAILS.md` § The three seatbelts) asks what the agent has spent on its own initiative today, prompt plus
completion. ⚠️ **It JOINs `conversations` and counts only the `notification` and `quiet_wakes` origins**, which is the
whole point of it existing beside `cost_summary`. Widening it to the whole meter would put two different budgets behind
one number: a chatty afternoon on the rail would starve the wake loop, and a runaway wake loop would eat the user's own
budget. A user-started thread has a NULL origin and never counts.

## v8: the reserved quiet-wakes thread

A wake that finds nothing leaves no thread (`agent/wake/`), which means DELETING the conversation it opened to think in.
`cost_meter.conversation_id` cascades, so the plain delete would take the spend with it — out of the one place the user
can see what the proactive agent costs them. Over a week of quiet wakes that is the whole proactive bill, silently zero.

**Why not `ON DELETE SET NULL`.** Because of the trap above: the column is NOT NULL so the upsert works at all. Making
it nullable to hold an orphaned total would break `ON CONFLICT DO UPDATE` for every ordinary turn, which is a far worse
bug than the one it fixes.

**What v8 does instead.** It inserts exactly one conversation row with origin `quiet_wakes` and an empty title. Before
deleting a quiet wake's thread, `discard_conversation_keeping_cost` folds that thread's `cost_meter` rows onto the
reserved id, through the same `ON CONFLICT (day, conversation_id, provider, model) DO UPDATE` shape `record_cost` uses,
with `priced = priced AND excluded.priced` so one unpriced quiet wake makes the reserved total an honest lower bound.
Fold and delete share one transaction, so a crash between them cannot drop the spend.

**Why an origin token rather than a fixed id.** `list_conversations` hides the row by `origin IS NOT 'quiet_wakes'`
(`IS NOT`, because `<>` answers NULL for the common NULL origin and would empty the list), and M2's thread icon reads
the same token set, so one vocabulary covers both. A fixed id would also have to be negative to avoid colliding with
`INTEGER PRIMARY KEY` allocation, which then hands the next real thread id `0`.

The title is empty on purpose: nothing renders this row, so an English string would be untranslated copy frozen in the
database with no reader.

## v4: the proposal spine

`proposal_sets` / `proposals` / `proposal_ops` / `proposal_acceptances`, the durable half of the suggested-ops feature.
The tables, the lifecycle machine, the claim transaction, and their DDL rationale live with the code:
`proposals/DETAILS.md`. The one thing worth knowing from up here is that `proposal_sets.conversation_id` breaks this
DB's usual pattern — nullable and `ON DELETE SET NULL`, where `messages` and `cost_meter` cascade — because a decision
record has to outlive the chat thread that produced it.

## No auto-retention in v1

Transcripts are small (spec §3), so there's no pruning yet. When real sizes exist, the operation log's
`operation_log/retention.rs` + `PruneRequest` scaffold (age + size prune, dir GC, vacuum on a startup + periodic timer)
is the template to follow — a follow-up, not built now.

## Wiring

`agent::start(app)` (in `agent/mod.rs`, modeled on `operation_log::start`) opens the DB through `AgentStore::open` (which
runs the schema lifecycle) and registers an `AgentDb` handle in managed state. `AgentDb` holds the DB path and hands out
read/write connections; the chat runtime owns the write-connection lifetime and single-writer discipline (the store
itself does not add a writer thread).
