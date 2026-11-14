# Agent store details

Pull-tier docs for `agent/store/`. Must-knows live in `CLAUDE.md`. This store is the app's second durable DB,
built on the operation log's proven template (`crates/cmdr-operation-log/src/store/`); this file records only what's specific to
`main.db`, and points at the template for the shared discipline.

## Why a second durable DB, mirroring the operation log

`main.db` holds agent state (conversations, messages, cost) and lives for years, so it can't be a
delete-and-recreate cache like the drive index or `importance.db`. The operation-log effort built the forward-migration
ladder as a reusable template (agent decision D3: a second consumer proves it generalizes). `store/migrations.rs` and
`store/connection.rs` mirror the operation log's structure closely: a `meta` anchor table outside the ladder, one
transaction per step, refuse-downgrade, set aside and recreate only on the typed corrupt-DB sqlite code, and WAL +
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
  user+assistant prose extracted at insert (never tool blobs), the only thing the FTS index sees. A user row opens with
  an `Envelope` part (the context it was sent with, rendered; `../chat/DETAILS.md` § Anatomy of one call), never
  indexed or shown. No migration: the column is schemaless JSON. **The downgrade cost** (accepted, as for the event
  kinds below): `AgentPart` has no tolerant parse, so a build from before the `Envelope` part can't load, or send into,
  a thread holding one.
- **`prompt_tokens` / `completion_tokens` are nullable** (assistant turns only).
- **`messages_fts` is external-content FTS5** (`content='messages'`, `content_rowid='id'`): the index stores the term
  data but not a copy of the text, pointing back at `messages.id`. Three triggers keep it in sync — insert indexes,
  delete de-indexes (the `'delete'` command), update does both. There is no in-tree trigger-based FTS pattern to copy
  (the operation log folds a column in Rust instead), so these were authored fresh and are the area most prone to a
  subtle desync.

## v2: `last_model` + event rows

`conversations.last_model` (nullable; NULL = no completed turn yet) records the model a
thread's most recent completed turn (or recorded slot-change event) used. The chat runtime
and the `ask_cmdr_record_slot_change` command compare against it to decide when to log a
"switched to X" timeline event; `conversations.last_chat_memory` (v9) is its twin for the
chat memory size. The full flow is `agent/chat/DETAILS.md` § Slot-change events.

Event rows reuse the `messages` table with `role = 'event'` and `content_blocks` holding a
typed `ConversationEvent` (not `Vec<AgentPart>`): they share the per-conversation `seq`,
so ordering against real messages is free, and paging/history need no second table or
merge. The reader (`map_message_row`) branches on the role token into `StoredContent`
(`Message { role, parts }` vs `Event`); the token deliberately lives outside `AgentRole`
so the transcript loader can't feed an event to a provider. `text_for_search` stays empty
— a model name is not conversation content and never matches search.

### An event kind this build doesn't know

A new event kind is a forward-compat hazard with no schema bump to refuse it: the row lands in a `main.db` an older
build may open after a downgrade. `list_messages` feeds BOTH the history page and the transcript loader, so before the
tolerant parse, one unknown row made the thread impossible to open AND to send into.

- **Decision/Why**: `ConversationEvent` carries `#[serde(other)] Unknown`. An unknown `kind` parses to it and the ROW
  IS KEPT (paging counts rows, so dropping it would shift every offset); the views project it to an event view with no
  blocks, and the transcript loader skips it like any event. Only an unknown `kind` is tolerated: a known kind with a
  body that doesn't parse is still `ContentBlocks` corruption.
- `Unknown` is read-only: `append_event` debug-asserts it never gets written.
- **The accepted downgrade gap**: a build from BEFORE this tolerant parse still refuses such a thread whole (it can't
  load it or send into it). Open beta, auto-update, and rare downgrades make that acceptable; every build from this one
  on reads a newer kind as `Unknown`.

### What a turn spent (`TurnFinished`)

`run_turn` appends `ConversationEvent::TurnFinished { stats: TurnStats }` after the last row of every turn that ran at
least one tool, answered, failed, or stopped alike. `TurnStats` holds the tool-call count, the wall time, and the turn's
summed per-row `prompt_tokens` / `completion_tokens` (the cost meter's figures, so per-turn footers add up to the
thread's cost), plus `stopped` (the turn ended on a cancel, so the footer can say "Stopped" after a reload).

- **Decision/Why an event row, not a column**: the wall time can't be derived on reload (every row of a turn carries
  the same `now_secs`), so it must be stored, and an event row needs no migration. A missing row (an older turn, or a
  crash before it) loses the wall time and `stopped` (`#[serde(default)]`: an older row reads as not stopped); the
  rest is derivable from the rows.
- **The downgrade cost** (accepted): a build from before the `Unknown` tolerant parse can't load, or send into, a thread
  holding this row. A tolerant build reads it as `Unknown` and shows nothing for it.
- A persist problem writing it is logged and dropped: a footer is worth no turn.
- It projects to an empty event view: the history page reads its stats onto the turn's last assistant view instead
  (`../chat/DETAILS.md` § What a reloaded page says).

### Turn-scoped reads (`turns.rs`)

A turn is every row from one user-role row (its opener) to the next. What a tool line says is turn-scoped while the
history pages by rows, so `turn_context` reads the rows completing a page's first and last turns, and `turn_opener_at`
a row's opener. `row_and_rest_of_turn` reads one assistant row plus the rest of its turn (the rows holding its results)
for the expanded line, and it is the ownership check: a `message_id` of another thread reads as `None`. They return
rows; what the rows mean is `agent::chat::message_view`'s.

## Search: matching, ranking, excerpts

`search.rs`'s `search_conversations` serves the threads panel. **Decision**: a thread matches on its message text (FTS5)
OR on its title (every query word a word prefix, the FTS rule applied in Rust), and ranks title match first, then the
best `bm25` among its messages, then `updated_at`, then id. **Why**: the title is the line the user wrote to find the
thread again, and a renamed title is in no index; `bm25` puts a thread ABOUT the term above a newer one mentioning it in
passing, which pure recency (the earlier order) got backwards. The ordering is a pure `rank` over one `Candidate` per
thread, so it's unit-tested without SQL tricks; it reads every thread header plus one best row per matching thread,
which stays sub-millisecond at hundreds of threads (and runs on a blocking thread via `with_read_connection`).

**Excerpts are typed runs, never markup.** `snippet()` marks matches with two private-use characters that
`parse_snippet` splits into `SnippetPart { text, matched }`, so the frontend renders plain text with no `{@html}` (thread
text is untrusted). Snippets are fetched for the returned page only, one `rowid`-pinned FTS read per hit, from the
thread's best-scoring message.

**A wake thread's display title** comes from `wake_place.rs`: a wake titles its thread with the busiest folder's
basename (`WakeDigest::title_for`), and while the stored title still equals that, list and search rows carry a
`WakePlace` (the first folder, `~`-relative, plus how many others the digest covers) that the frontend words as
"Activity in ~/tmp and 2 other folders". A renamed thread no longer matches, so it shows the user's title. It reads the
thread's first user message (the digest) per wake row, a page at most.

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

## v9: `last_chat_memory`

`conversations.last_chat_memory` (nullable; NULL = no completed turn yet) is `last_model`'s twin for the other half of
the slot: the prompt-token budget the thread's most recent completed turn — or recorded chat-memory event — was
assembled against.

**Why a second column when v3's `last_prompt_budget` holds the same number.** That one belongs to the usage gauge and
is written only as a PAIR with `last_prompt_tokens`, which is what makes a percentage meaningful. A live settings
change has no prompt size to record beside it, so stamping the budget there would leave the gauge showing an old size
against a new budget — a wrong number, in the one place the user goes to judge how full their chat is. The comparison
that decides a timeline line and the measurement that fills a bar have different lifecycles; one column can't serve
both.

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

**Why an origin token rather than a fixed id.** `list_conversations` and search hide the row by its token (with the
other ledger rows, § v15), and the rail's thread icon reads origins too (`origin === 'notification'` only), so one
vocabulary covers both. A fixed id would also have to be negative to avoid colliding with
`INTEGER PRIMARY KEY` allocation, which then hands the next real thread id `0`.

The title is empty on purpose: nothing renders this row, so an English string would be untranslated copy frozen in the
database with no reader.

## v4: the proposal spine

`proposal_sets` / `proposals` / `proposal_ops` / `proposal_acceptances`, the durable half of the suggested-ops feature.
The tables, the lifecycle machine, the claim transaction, and their DDL rationale live with the code:
`proposals/DETAILS.md`. The one thing worth knowing from up here is that `proposal_sets.conversation_id` breaks this
DB's usual pattern — nullable and `ON DELETE SET NULL`, where `messages` and `cost_meter` cascade — because a decision
record has to outlive the chat thread that produced it.

## v10: `proposals.thread_line_id`

The `messages` row a decided group's thread line lives in, so an approval's "running" line can be rewritten in place
when it settles. `ON DELETE SET NULL`. Its rationale and the writes that use it: `proposals/DETAILS.md` § A decided
group's thread line.

## v11: `proposals.revision`

Bumped by every amend, so an approval binds the version of a group the user read. Rationale and the check:
`proposals/DETAILS.md` § What preflight accepts.

## v12: `proposals.source_folder`

The folder every op of a group sits directly in, NULL when they span folders, written with the op list. The step backfills existing groups once by streaming their paths, with the rule COPIED into the step so it keeps doing what it shipped doing. Why a column: `../suggested_ops/DETAILS.md` § Header facts.

## v13: `proposal_server_drafts`

The server a zero-op `add_server` / `edit_server` group proposes, one row per group, cascading with it and written in
the group's own transaction. Rationale, the claim it feeds, and why the spine writes it when rename's producer writes its
own sidecar: `proposals/DETAILS.md` § A server group.

## v14: `cost_meter.cache_read_tokens` / `cache_write_tokens`

How much of each prompt the provider's cache served and stored, both SHARES of `prompt_tokens` (which stays the whole
input, so every existing reader keeps its meaning). `record_cost` and `discard_conversation_keeping_cost` accumulate them
like the other counts. `cost_micros` already prices the two at their own rates (`agent::pricing`); the columns say why a
cached thread costs what it does. Rows metered before v14 read 0. `conversation_cost` and `cost_summary` carry the read
share (`cache_read_tokens`) to the footer tooltip and the settings spend list; writes stay backend-only (they only
explain a surcharge, which `cost_micros` already holds).

## v15: the reserved about-this thread

A second ledger row, origin `about_this`, empty title, for what the "Tell me about this" card's verdicts spend
(`about/DETAILS.md` § The verdict). Same reason as v8: a verdict is never stored, so it has no thread to meter onto, and
`cost_meter.conversation_id` can't be NULL. Every ledger row is named in `ConversationOrigin::LEDGERS`, and both hide
filters (`list_conversations`, `search_conversations`) exclude the whole list, written `origin IS NULL OR origin NOT IN
(…)` (a bare `NOT IN` answers NULL for the common NULL origin and would empty the list). A new ledger joins both filters
by joining `LEDGERS`. The wake loop's daily ceiling counts the two proactive origins only, so a verdict's spend never
holds a wake back; it still shows in the day's total (`cost_summary`).

## No auto-retention in v1

Transcripts are small (spec §3), so there's no pruning yet. When real sizes exist, the operation log's
`crates/cmdr-operation-log/src/retention.rs` + `PruneRequest` scaffold (age + size prune, dir GC, vacuum on a startup + periodic timer)
is the template to follow — a follow-up, not built now.

## Wiring

`agent::start(app)` (in `agent/mod.rs`, modeled on `operation_log_host::start`) opens the DB through `AgentStore::open` (which
runs the schema lifecycle) and registers an `AgentDb` handle in managed state. `AgentDb` holds the DB path and hands out
read/write connections; the chat runtime owns the write-connection lifetime and single-writer discipline (the store
itself does not add a writer thread).
