//! The forward-migration ladder for `main.db` — the second in this codebase, mirroring
//! the operation log's (`crates/cmdr-operation-log/src/store/migrations.rs`), which proved the pattern.
//!
//! `main.db` lives for years (agent decisions D1/D3), so it never delete-and-recreates on a
//! schema change: it migrates forward. [`run_migrations`] compares the stored
//! `meta.schema_version` against the ladder and, for each step newer than the stored
//! version, runs that step's `up` inside a transaction and bumps the version — stepwise,
//! so a crash between steps leaves a consistent intermediate version the next open
//! resumes from.
//!
//! Rules the ladder enforces:
//! - **Never destroy on a version gap.** A stored version *older* than the ladder
//!   migrates up. A stored version *newer* (a downgrade) is refused with
//!   [`AgentStoreError::SchemaDowngrade`], never wiped — the newer DB may hold data this
//!   build can't represent. Delete-and-recreate is reserved for a genuinely unparseable
//!   file (see `AgentStore::open`).
//! - **Each step is one transaction.** Table/index/trigger creation for version N, then
//!   the version stamp to N, commit together — so a reader never sees version N with N's
//!   schema half-applied.
//!
//! Adding a migration: append a [`Migration`] with the next version and an `up` that
//! transforms the previous schema in place. Never edit or renumber a shipped step
//! (users' DBs already ran it); the `up` runs against whatever the prior steps produced.

use rusqlite::{Connection, Transaction};

use super::AgentStoreError;

/// One forward step in the ladder: bring the schema from `version - 1` to `version`.
/// `up` runs inside a transaction the runner owns; the runner gates on the stored
/// version so it never runs twice for one DB.
pub struct Migration {
    /// The schema version this step produces. Strictly increasing across the ladder;
    /// the highest is the current version.
    pub version: u32,
    /// A short human note for logs. Not load-bearing.
    pub description: &'static str,
    /// Transform the schema from the prior version to [`version`](Self::version).
    pub up: fn(&Transaction<'_>) -> rusqlite::Result<()>,
}

/// The production ladder. Version 1 creates the whole initial schema; later schema
/// changes append steps here.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "initial schema: conversations, messages, messages_fts, cost_meter",
        up: migrate_v1_initial,
    },
    Migration {
        version: 2,
        description: "conversations.last_model for model-change events",
        up: migrate_v2_last_model,
    },
    Migration {
        version: 3,
        description: "conversations.last_prompt_tokens/last_prompt_budget for the context gauge",
        up: migrate_v3_context_usage,
    },
    Migration {
        version: 4,
        description: "the proposal spine: proposal_sets, proposals, proposal_ops, proposal_acceptances",
        up: migrate_v4_proposal_spine,
    },
    Migration {
        version: 5,
        description: "proposal_rename_evidence: why each proposed name is believable",
        up: migrate_v5_rename_evidence,
    },
    Migration {
        version: 6,
        description: "agent_inbox: folder-window bundles waiting with deliver-by deadlines",
        up: migrate_v6_agent_inbox,
    },
    Migration {
        version: 7,
        description: "agent_inbox.deliver_by is nullable, so a cold row rides along without a deadline",
        up: migrate_v7_nullable_deliver_by,
    },
    Migration {
        version: 8,
        description: "the reserved quiet-wakes thread, so a deleted noop wake still leaves its cost behind",
        up: migrate_v8_quiet_wakes_thread,
    },
    Migration {
        version: 9,
        description: "conversations.last_chat_memory for chat-memory-size events",
        up: migrate_v9_last_chat_memory,
    },
    Migration {
        version: 10,
        description: "proposals.thread_line_id: the one thread line a decided group rewrites in place",
        up: migrate_v10_proposal_thread_line,
    },
    Migration {
        version: 11,
        description: "proposals.revision: bumped by every amend, so an approval binds the version the user read",
        up: migrate_v11_proposal_revision,
    },
    Migration {
        version: 12,
        description: "proposals.source_folder: the folder a group's ops share, backfilled once",
        up: migrate_v12_proposal_source_folder,
    },
    Migration {
        version: 13,
        description: "proposal_server_drafts: the server a zero-op server group proposes",
        up: migrate_v13_proposal_server_drafts,
    },
    Migration {
        version: 14,
        description: "cost_meter.cache_read_tokens/cache_write_tokens: the prompt-cache share of each prompt",
        up: migrate_v14_cost_meter_cache_tokens,
    },
];

/// The meta key holding the integer schema version (as text). Absent ⇒ 0 (a fresh DB
/// that hasn't run any step). The migration anchor, outside the ladder.
pub const SCHEMA_VERSION_KEY: &str = "schema_version";

/// Run the ladder against `conn`, bringing the stored schema version up to the highest
/// in `migrations`. Bootstraps the `meta` table first. Refuses a downgrade; never
/// destroys. Parameterized over `migrations` so the ladder mechanism is tested with
/// synthetic steps independent of the production schema.
pub fn run_migrations(conn: &Connection, migrations: &[Migration]) -> Result<(), AgentStoreError> {
    bootstrap_meta(conn)?;
    let current = read_schema_version(conn)?;
    let target = migrations.iter().map(|m| m.version).max().unwrap_or(0);

    if current > target {
        // A downgrade: the DB was written by a newer build. Refuse — never destroy a
        // newer DB (it may hold data we can't represent). The caller surfaces this; the
        // file stays untouched.
        return Err(AgentStoreError::SchemaDowngrade {
            found: current,
            expected: target,
        });
    }

    // Apply each pending step, oldest first, each in its own transaction so a crash
    // between steps leaves a consistent intermediate version.
    let mut pending: Vec<&Migration> = migrations.iter().filter(|m| m.version > current).collect();
    pending.sort_by_key(|m| m.version);
    for migration in pending {
        let tx = conn.unchecked_transaction()?;
        (migration.up)(&tx)?;
        stamp_schema_version(&tx, migration.version)?;
        tx.commit()?;
        log::info!(
            target: "agent::store",
            "main.db migrated to schema v{} ({})",
            migration.version,
            migration.description
        );
    }
    Ok(())
}

/// Stamp the schema version into `meta`. Runs inside the migration step's transaction so
/// the version and the schema change commit atomically.
fn stamp_schema_version(conn: &Connection, version: u32) -> Result<(), AgentStoreError> {
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
        rusqlite::params![SCHEMA_VERSION_KEY, version.to_string()],
    )?;
    Ok(())
}

/// Create the `meta` key/value table if absent. Idempotent and safe on every open; it's
/// the anchor the ladder reads the version from.
fn bootstrap_meta(conn: &Connection) -> Result<(), AgentStoreError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        ) WITHOUT ROWID;",
    )?;
    Ok(())
}

/// Read the stored schema version (absent ⇒ 0).
pub(super) fn read_schema_version(conn: &Connection) -> Result<u32, AgentStoreError> {
    let mut stmt = conn.prepare_cached("SELECT value FROM meta WHERE key = ?1")?;
    let mut rows = stmt.query_map(rusqlite::params![SCHEMA_VERSION_KEY], |row| row.get::<_, String>(0))?;
    match rows.next() {
        Some(Ok(v)) => Ok(v.parse::<u32>().unwrap_or(0)),
        Some(Err(e)) => Err(e.into()),
        None => Ok(0),
    }
}

/// Version 1: the initial schema. Conversations, per-message rows with typed
/// `content_blocks` JSON, an external-content FTS5 index over message text kept in sync
/// by triggers, and a per-day/per-thread/per-model cost meter. All classification
/// columns are TEXT tokens ([`super::super::types`] / the LLM seam) so the DB stays
/// `sqlite3`-inspectable. Rationale for each table: `DETAILS.md`.
fn migrate_v1_initial(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch(
        "
        CREATE TABLE conversations (
            id         INTEGER PRIMARY KEY,
            title      TEXT    NOT NULL,          -- generated from first message; user-renamable
            created_at INTEGER NOT NULL,          -- unix secs
            updated_at INTEGER NOT NULL,
            archived   INTEGER NOT NULL DEFAULT 0,-- 0/1 flag + filter; no delete in v1
            origin     TEXT                       -- nullable snake_case token; NULL = user-started
        );
        CREATE INDEX conversations_updated ON conversations (archived, updated_at DESC, id DESC);

        CREATE TABLE messages (
            id                INTEGER PRIMARY KEY,
            conversation_id   INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
            seq               INTEGER NOT NULL,        -- per-conversation ordinal
            role              TEXT    NOT NULL,        -- token: system|user|assistant|tool
            content_blocks    TEXT    NOT NULL,        -- JSON: ordered typed parts; opaque provider
                                                       -- state rides here and NEVER crosses to the frontend
            text_for_search   TEXT    NOT NULL DEFAULT '', -- plain user+assistant text, extracted at insert
            prompt_tokens     INTEGER,                 -- nullable; assistant turns only
            completion_tokens INTEGER,
            created_at        INTEGER NOT NULL
        );
        CREATE UNIQUE INDEX messages_conv_seq ON messages (conversation_id, seq);

        -- External-content FTS5 over message text. `content='messages'` means the index
        -- stores no copy of the text; it points at `messages.id` (content_rowid). The
        -- three triggers keep it synced: an edit re-indexes, a delete de-indexes.
        CREATE VIRTUAL TABLE messages_fts USING fts5 (
            text_for_search,
            content='messages',
            content_rowid='id'
        );
        CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
            INSERT INTO messages_fts(rowid, text_for_search) VALUES (new.id, new.text_for_search);
        END;
        CREATE TRIGGER messages_ad AFTER DELETE ON messages BEGIN
            INSERT INTO messages_fts(messages_fts, rowid, text_for_search) VALUES('delete', old.id, old.text_for_search);
        END;
        CREATE TRIGGER messages_au AFTER UPDATE ON messages BEGIN
            INSERT INTO messages_fts(messages_fts, rowid, text_for_search) VALUES('delete', old.id, old.text_for_search);
            INSERT INTO messages_fts(rowid, text_for_search) VALUES (new.id, new.text_for_search);
        END;

        -- Per-day, per-thread, per-model token + cost rollup. `conversation_id` is NOT
        -- NULL: SQLite treats NULLs as distinct in a PK/UNIQUE, so a nullable column
        -- inside the PK breaks ON CONFLICT DO UPDATE (every write inserts a duplicate
        -- instead of upserting). One row per real thread; the per-day cross-thread
        -- rollup is computed at query time (SUM ... GROUP BY day).
        CREATE TABLE cost_meter (
            day               TEXT    NOT NULL,        -- YYYY-MM-DD, local day
            conversation_id   INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
            provider          TEXT    NOT NULL,        -- ProviderTag token
            model             TEXT    NOT NULL,
            prompt_tokens     INTEGER NOT NULL DEFAULT 0,
            completion_tokens INTEGER NOT NULL DEFAULT 0,
            cost_micros       INTEGER NOT NULL DEFAULT 0,  -- integer micro-USD; honest estimate
            priced            INTEGER NOT NULL DEFAULT 1,  -- 0 when the model wasn't in the price table
            PRIMARY KEY (day, conversation_id, provider, model)
        );
        ",
    )
}

/// Version 2: `conversations.last_model` — the model name the conversation's most recent
/// completed turn (or recorded model-change event) used. NULL means no turn has run yet.
/// The chat runtime compares against it to insert honest "switched to X" event rows when
/// the effective model changes between turns.
fn migrate_v2_last_model(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("ALTER TABLE conversations ADD COLUMN last_model TEXT;")
}

/// Version 3: what the conversation's last completed turn spent, and the budget it spent it
/// against, so reopening a thread shows its real usage instead of an empty gauge.
///
/// Nullable with no backfill, and read as a PAIR: a thread that predates this, or one whose
/// first turn hasn't finished, reads as "not measured yet" rather than as zero usage, which
/// would render as a reassuring empty bar for a thread that may be nearly full.
fn migrate_v3_context_usage(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch(
        "ALTER TABLE conversations ADD COLUMN last_prompt_tokens INTEGER;
         ALTER TABLE conversations ADD COLUMN last_prompt_budget INTEGER;",
    )
}

/// Version 4: the proposal spine — sweeps (`proposal_sets`), reviewable groups
/// (`proposals`), their ops (`proposal_ops`), and the server-owned acceptance record
/// (`proposal_acceptances`) the claim transaction binds against.
///
/// Two shapes here differ from the tables around them, both on purpose:
///
/// - **`proposal_sets.conversation_id` is nullable and `ON DELETE SET NULL`**, where every
///   other conversation-linked table cascades. A sweep is a DECISION record: what the user
///   was asked and what they answered outlives the chat thread that produced it (and a
///   sweep from a background wake has no thread at all). Cascading would delete the
///   evidence of an approval alongside a tidied-up transcript.
/// - **No expiry column.** A suggestion waits until the user acts on it; a two-week-old
///   proposal is still the user's to decide.
///
/// Every classification column is a TEXT token (`super::proposals::ProposalVerb`,
/// `ProposalStatus`, `OpStatus`, `Reversibility`), so the file stays `sqlite3`-inspectable
/// and nothing branches on a message string. Column rationale: `proposals/DETAILS.md`.
fn migrate_v4_proposal_spine(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch(
        "
        -- One agent wake's output: display and provenance only. The reviewable unit is a
        -- group, one level down.
        CREATE TABLE proposal_sets (
            id               INTEGER PRIMARY KEY,
            conversation_id  INTEGER REFERENCES conversations(id) ON DELETE SET NULL,
            created_at       INTEGER NOT NULL,          -- unix secs
            created_by_model TEXT,                      -- provenance only; no logic reads it
            rationale        TEXT                       -- the agent's words for the sweep
        );
        CREATE INDEX proposal_sets_created ON proposal_sets (created_at DESC, id DESC);

        -- A group: the reviewable, approvable, executable unit, and exactly one call to one
        -- executor. `source_volume_id` lives here rather than on the sweep because a sweep
        -- may span volumes and a group may not.
        CREATE TABLE proposals (
            id                    INTEGER PRIMARY KEY,
            set_id                INTEGER NOT NULL REFERENCES proposal_sets(id) ON DELETE CASCADE,
            seq                   INTEGER NOT NULL,     -- ordinal within the sweep
            verb                  TEXT    NOT NULL,     -- ProposalVerb token
            status                TEXT    NOT NULL,     -- ProposalStatus token
            source_volume_id      TEXT    NOT NULL,
            destination           TEXT,                 -- shared dest dir / rename parent / archive path
            destination_volume_id TEXT,                 -- where `destination` lives; NULL when it has none
            reversible            TEXT    NOT NULL,     -- Reversibility token; disclosed, never a blocker
            display_name          TEXT    NOT NULL,     -- friendly name, may carry the selector's pattern
            rationale             TEXT,                 -- the agent's words, labelled as such in review
            selector              TEXT,                 -- JSON of the selector this group froze, if any
            created_at            INTEGER NOT NULL,
            decided_at            INTEGER               -- when it left `pending`
        );
        CREATE UNIQUE INDEX proposals_set_seq ON proposals (set_id, seq);
        CREATE INDEX proposals_status ON proposals (status, created_at DESC, id DESC);

        -- One op: one path, which may be a file or a whole directory. Paged and counted by
        -- (group_id, seq); a group of 60 000 is legitimate, so nothing loads these to count
        -- them. `destination` is per-op for rename and NULL for every other verb, which
        -- binds a shared one on the group.
        CREATE TABLE proposal_ops (
            id             INTEGER PRIMARY KEY,
            group_id       INTEGER NOT NULL REFERENCES proposals(id) ON DELETE CASCADE,
            seq            INTEGER NOT NULL,
            source_path    TEXT    NOT NULL,
            destination    TEXT,                        -- rename only: the new name
            status         TEXT    NOT NULL,            -- OpStatus token; per-op partial apply
            snapshot_size  INTEGER,                     -- creation snapshot, nullable: drift detection
            snapshot_mtime INTEGER,
            snapshot_inode INTEGER,
            created_at     INTEGER NOT NULL
        );
        CREATE UNIQUE INDEX proposal_ops_group_seq ON proposal_ops (group_id, seq);

        -- What preflight accepted, owned by the server. The client presents a group id and
        -- deselected op ids, NEVER values, so this record is the only thing the claim
        -- transaction trusts about what the user saw.
        CREATE TABLE proposal_acceptances (
            group_id   INTEGER PRIMARY KEY REFERENCES proposals(id) ON DELETE CASCADE,
            op_count   INTEGER NOT NULL,                -- how many ops were accepted
            op_digest  TEXT    NOT NULL,                -- hash of the values those ops carried
            created_at INTEGER NOT NULL
        );
        ",
    )
}

/// Version 5: what each proposed rename NAME is based on, one row per rename op.
///
/// A sidecar rather than columns on `proposal_ops`, because evidence is a rename-producer
/// concern and the spine stays verb-agnostic: only `tools/propose/rename/` writes or
/// reads this table. It cascades with its op, so a re-proposed or deleted group takes its
/// evidence with it.
///
/// The coverage of an accepted `imageText` match is stored as its own columns rather than a
/// JSON blob, for two reasons: `EvidenceCoverage` is deliberately Serialize-only (a plan may
/// never send one), and a column list makes adding a field a compile error here rather than a
/// silently-dropped value at read time.
fn migrate_v5_rename_evidence(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch(
        "
        -- Why one proposed name is believable. Written with the group at staging time and
        -- rewritten by a revise; a rename op with no row here can never be reviewed (the
        -- loader refuses the whole proposal rather than showing a name with invented
        -- backing).
        CREATE TABLE proposal_rename_evidence (
            op_id  INTEGER PRIMARY KEY REFERENCES proposal_ops(id) ON DELETE CASCADE,
            source TEXT NOT NULL,      -- EvidenceSource token
            detail TEXT NOT NULL,      -- model-authored quote or note, bounded at the tool boundary

            -- How thin the match behind the name is, for an accepted `imageText` claim only.
            -- All NULL together for every other source.
            coverage_match_offset    INTEGER,
            coverage_matched_chars   INTEGER,
            coverage_delivered_chars INTEGER,
            coverage_context_before  TEXT,
            coverage_matched_text    TEXT,
            coverage_context_after   TEXT,
            coverage_trimmed_before  INTEGER,   -- 0/1
            coverage_trimmed_after   INTEGER    -- 0/1
        );
        ",
    )
}

/// Version 6: `agent_inbox` — the folder-window bundles waiting for a deliver-by deadline
/// (agent decision D17).
///
/// Three shapes here are deliberate:
///
/// - **`(folder, window_start)` IS the primary key, because it is the merge key.** The
///   in-memory inbox merges a new bundle into the row already waiting for that folder-window,
///   so making the same pair the PK means this table cannot hold two rows the inbox would have
///   merged. The invariant is structural rather than a rule somebody must remember.
/// - **No `conversation_id` and no foreign key.** The inbox is pre-proposal SIGNAL, not a
///   decision record: nobody has been asked anything yet. It belongs to no chat thread, and a
///   sweep that eventually comes out of it gets its own link.
/// - **Counters are four columns, not a JSON blob**, so `main.db` stays inspectable in any
///   stock `sqlite3` browser — the same reason every classification column here is a token.
///
/// `interest` is a REAL because it is a score rather than a classification. The wake tiers it
/// feeds are derived, never stored: tier boundaries are still being tuned (spec §18), and a
/// stored tier would freeze the guess of one run into the DB.
fn migrate_v6_agent_inbox(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch(
        "
        -- The changes in one folder in one window, waiting for a deadline. Rows live only
        -- until the next wake drains them, and a restart reconciles whatever it finds here.
        CREATE TABLE agent_inbox (
            folder        TEXT    NOT NULL,
            window_start  INTEGER NOT NULL,   -- unix secs, epoch-anchored
            created       INTEGER NOT NULL,   -- per-kind counters; never file names
            modified      INTEGER NOT NULL,
            removed       INTEGER NOT NULL,
            renamed       INTEGER NOT NULL,
            last_event_at INTEGER NOT NULL,   -- newest change; the staleness horizon reads this
            interest      REAL    NOT NULL,   -- strongest claim any contribution made, 0..=1
            deliver_by    INTEGER NOT NULL,   -- unix secs
            PRIMARY KEY (folder, window_start)
        );
        CREATE INDEX agent_inbox_deliver_by ON agent_inbox (deliver_by);
        ",
    )
}

/// Make `deliver_by` nullable, because a COLD row has no deadline.
///
/// A cold bundle rides along on the next wake and never causes one of its own. With a `NOT NULL`
/// column it had to be given a real time like any other row, so a trickle in a barely-scored
/// folder came due on its own and spent a model turn saying that a cache directory changed.
///
/// SQLite cannot drop a `NOT NULL` constraint in place, so this is the standard
/// create-insert-drop-rename rebuild. The index goes with the dropped table and is recreated
/// here; the primary key is re-declared identically, because it is the merge key the in-memory
/// inbox relies on (see [`migrate_v6_agent_inbox`]).
fn migrate_v7_nullable_deliver_by(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch(
        "
        CREATE TABLE agent_inbox_v7 (
            folder        TEXT    NOT NULL,
            window_start  INTEGER NOT NULL,   -- unix secs, epoch-anchored
            created       INTEGER NOT NULL,   -- per-kind counters; never file names
            modified      INTEGER NOT NULL,
            removed       INTEGER NOT NULL,
            renamed       INTEGER NOT NULL,
            last_event_at INTEGER NOT NULL,   -- newest change; the staleness horizon reads this
            interest      REAL    NOT NULL,   -- strongest claim any contribution made, 0..=1
            deliver_by    INTEGER,            -- unix secs; NULL ⇒ rides along, never wakes alone
            PRIMARY KEY (folder, window_start)
        );
        INSERT INTO agent_inbox_v7
            SELECT folder, window_start, created, modified, removed, renamed, last_event_at, interest, deliver_by
            FROM agent_inbox;
        DROP TABLE agent_inbox;
        ALTER TABLE agent_inbox_v7 RENAME TO agent_inbox;
        CREATE INDEX agent_inbox_deliver_by ON agent_inbox (deliver_by);
        ",
    )
}

/// One reserved conversation row that holds what QUIET wakes spent, so deleting their threads
/// doesn't erase the proactive agent's cost.
///
/// A wake that finds nothing leaves no thread (`agent/wake/`), and `cost_meter.conversation_id`
/// is `ON DELETE CASCADE`, so the plain delete would take the spend with it — out of the one
/// place the user can see what the proactive agent costs.
///
/// ⚠️ **`ON DELETE SET NULL` is not available here.** The column is `NOT NULL` on purpose:
/// SQLite treats NULLs as distinct in a PK, so a nullable column inside `cost_meter`'s PK breaks
/// `ON CONFLICT DO UPDATE` and every write inserts a duplicate instead of upserting (see
/// [`migrate_v1_initial`]). A real row for the totals to point at is what keeps the upsert
/// working.
///
/// The title is empty on purpose: nothing renders this row, so an English string here would be
/// untranslated copy frozen in the database with no reader. Its identity is the `quiet_wakes`
/// origin token, which is also what `list_conversations` hides it by.
fn migrate_v8_quiet_wakes_thread(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0);
    tx.execute(
        "INSERT INTO conversations (title, created_at, updated_at, archived, origin)
         VALUES ('', ?1, ?1, 0, 'quiet_wakes')",
        rusqlite::params![now],
    )?;
    Ok(())
}

/// Version 9: `conversations.last_chat_memory` — the prompt-token budget the conversation's
/// most recent completed turn (or recorded chat-memory event) was assembled against. NULL
/// means no turn has run yet. The chat runtime compares against it to write an honest
/// "chat memory now holds N tokens" event row when the budget changes between turns.
///
/// ⚠️ **A separate column from `last_prompt_budget` (v3), which holds the same NUMBER.** That
/// one is half of the gauge's pair, written only with the `last_prompt_tokens` it must be read
/// beside; a live settings change has no prompt size to record, so stamping it there would
/// leave the gauge showing an old size against a new budget, which is a wrong percentage.
fn migrate_v9_last_chat_memory(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("ALTER TABLE conversations ADD COLUMN last_chat_memory INTEGER;")
}

/// Version 10: `proposals.thread_line_id`, the `messages` row that says in the sweep's thread
/// what became of the group. An approval writes it as "running" at the start and REWRITES it
/// when the operation settles, so the thread keeps one line per decision; this column is how
/// the settle finds it. NULL until a line exists, and NULLed with its thread.
fn migrate_v10_proposal_thread_line(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch(
        "ALTER TABLE proposals ADD COLUMN thread_line_id INTEGER REFERENCES messages(id) ON DELETE SET NULL;",
    )
}

/// Version 11: `proposals.revision`, which every amend (`repropose_group`) bumps. The review sends
/// the revision it showed with an approval and preflight refuses a different one, so an agent
/// amending a group while the user reads it can't get its new verb, target, or list approved by a
/// click on the old one. Existing groups start at 0, the same as a fresh one.
fn migrate_v11_proposal_revision(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("ALTER TABLE proposals ADD COLUMN revision INTEGER NOT NULL DEFAULT 0;")
}

/// Version 12: `proposals.source_folder`, the folder every op of a group sits directly in (NULL
/// when they span folders), written with the op list from then on. The review names it once per
/// group, and reading it from a column keeps a refresh from streaming every op path.
///
/// Backfilled here, once, by streaming each existing group's paths in `seq` order and stopping
/// at the first one in another folder. The rule is copied rather than called
/// (`proposals::shared_parent` today) because a shipped step must keep doing what it did when
/// it shipped, whatever the live helper later becomes.
fn migrate_v12_proposal_source_folder(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch("ALTER TABLE proposals ADD COLUMN source_folder TEXT;")?;
    let group_ids: Vec<i64> = tx
        .prepare("SELECT id FROM proposals")?
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut paths = tx.prepare("SELECT source_path FROM proposal_ops WHERE group_id = ?1 ORDER BY seq")?;
    let mut write = tx.prepare("UPDATE proposals SET source_folder = ?2 WHERE id = ?1")?;
    for group_id in group_ids {
        let mut rows = paths.query([group_id])?;
        let mut shared: Option<String> = None;
        let mut spans = false;
        while let Some(row) = rows.next()? {
            let path: String = row.get(0)?;
            let parent = std::path::Path::new(&path).parent().and_then(|p| p.to_str());
            match (parent, &shared) {
                (Some(parent), None) => shared = Some(parent.to_string()),
                (Some(parent), Some(seen)) if seen == parent => {}
                _ => {
                    spans = true;
                    break;
                }
            }
        }
        if !spans && let Some(folder) = shared {
            write.execute(rusqlite::params![group_id, folder])?;
        }
    }
    Ok(())
}

/// Version 13: `proposal_server_drafts`, the server an `add_server` / `edit_server` group
/// proposes. Such a group has no ops: this row IS what it asks for, so the spine writes it in
/// the group's own transaction (unlike rename's evidence sidecar, which its producer writes
/// after). One row per group, cascading with it.
///
/// `draft` and `previous` are the spine's own `ServerDraft` as JSON, a type with no field that
/// could hold a secret. `editing_id` and `previous` are set together, for an edit only, which
/// the CHECK holds. `completion_note` is a `ServerSaveNote` token, set when an approved save
/// wrote something but not everything. Rationale: `proposals/DETAILS.md` § A server group.
fn migrate_v13_proposal_server_drafts(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch(
        "
        CREATE TABLE proposal_server_drafts (
            group_id        INTEGER PRIMARY KEY REFERENCES proposals(id) ON DELETE CASCADE,
            draft           TEXT NOT NULL,   -- ServerDraft JSON: what the click saves
            editing_id      TEXT,            -- the saved place an edit changes; NULL for an add
            previous        TEXT,            -- ServerDraft JSON: that place as the proposal saw it
            completion_note TEXT,            -- ServerSaveNote token; NULL unless a save was partial
            CHECK ((editing_id IS NULL) = (previous IS NULL))
        );
        ",
    )
}

/// Version 14: how much of each prompt the provider's cache served (`cache_read_tokens`) and
/// stored for the next call (`cache_write_tokens`), both shares of `prompt_tokens`, which stays
/// the whole input. Cmdr asks every provider to cache (`agent/llm/DETAILS.md` § Prompt
/// caching), and `cost_micros` already prices the two at their own rates; these say why a
/// cached thread costs what it does. Rows metered before this read 0 for both.
fn migrate_v14_cost_meter_cache_tokens(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute_batch(
        "ALTER TABLE cost_meter ADD COLUMN cache_read_tokens INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE cost_meter ADD COLUMN cache_write_tokens INTEGER NOT NULL DEFAULT 0;",
    )
}
