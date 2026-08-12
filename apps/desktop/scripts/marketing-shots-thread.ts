/**
 * The Ask Cmdr conversation the `chat` masters photograph, and the SQL that puts it in
 * an instance's `main.db`.
 *
 * Why seeded rather than asked live: a marketing shot has to be reproducible, and a
 * live provider call is neither (different words every run, variable latency, real
 * spend, and a key to manage). The rail renders a stored thread with no provider
 * involved — `ask_cmdr_get_conversation` is a plain read — so the honest version of
 * "make the chat shot repeatable" is to store one.
 *
 * ❗ The copy is a DRAFT for David, like every user-facing string, and it carries one
 * rule beyond style: the question, the tool call, and the answer must describe what
 * Cmdr actually does. A shot showing a capability that doesn't exist is the one failure
 * here that no assertion catches.
 */

/** The consent version the rail requires; below it, the rail renders the consent screen instead. */
export const CONSENT_COPY_VERSION = 2

/** The model the thread is attributed to. A `claude-sonnet` id so the cost footer prices it. */
const THREAD_MODEL = 'claude-sonnet-5'

/** Roughly how full the context gauge reads. Both are needed, or it says "not measured". */
const PROMPT_TOKENS = 9_400
const PROMPT_BUDGET = 32_000

/** What the cost footer shows: one turn's completion tokens and its price in millionths. */
const COMPLETION_TOKENS = 420
const COST_MICROS = 31_000

export interface SeedMessage {
  role: 'user' | 'assistant' | 'tool'
  /** A JSON array of externally-tagged `AgentPart`s, exactly as the app stores them. */
  contentBlocks: string
  /** The prose the FTS index gets. Empty is legitimate only for a tool row. */
  textForSearch: string
}

export interface SeedThread {
  title: string
  messages: SeedMessage[]
}

const CALL_ID = 'shots-1'

const QUESTION = 'Which folders on this drive have I actually been working in this week?'

const ANSWER =
  'Three stand out: `~/projects-git/vdavid/cmdr` (touched daily, 41 GB), `~/Downloads` (28 new files since Monday, ' +
  '6.2 GB), and `~/Documents/Rymdskottkärra` (a handful of edits, 340 MB). Everything else on the drive has been ' +
  'sitting still for a month or more.'

/** The conversation, in the order the rail renders it. */
export const SHOTS_THREAD: SeedThread = {
  title: 'What have I been working on?',
  messages: [
    {
      role: 'user',
      contentBlocks: JSON.stringify([{ text: QUESTION }]),
      textForSearch: QUESTION,
    },
    {
      role: 'assistant',
      contentBlocks: JSON.stringify([
        { tool_call: { call_id: CALL_ID, tool: 'important_folders', arguments: { volume: 'root', limit: 5 } } },
      ]),
      textForSearch: 'Looking at which folders have seen recent activity.',
    },
    {
      role: 'tool',
      contentBlocks: JSON.stringify([
        {
          tool_result: {
            call_id: CALL_ID,
            content: {
              folders: [
                { path: '~/projects-git/vdavid/cmdr', bytes: 44_023_414_784, lastTouched: '2026-08-12' },
                { path: '~/Downloads', bytes: 6_657_199_308, lastTouched: '2026-08-11' },
                { path: '~/Documents/Rymdskottkärra', bytes: 356_515_840, lastTouched: '2026-08-09' },
              ],
            },
            elided: false,
          },
        },
      ]),
      textForSearch: '',
    },
    {
      role: 'assistant',
      contentBlocks: JSON.stringify([{ text: ANSWER }]),
      textForSearch: ANSWER,
    },
  ],
}

/** Doubles single quotes, the only escaping SQLite string literals need. */
function quote(value: string): string {
  return `'${value.replace(/'/g, "''")}'`
}

/**
 * The SQL that installs `thread` as the instance's newest conversation, plus the
 * consent rows the rail checks before rendering anything.
 *
 * Idempotent by construction: it deletes its own previous thread by title first, so
 * running it on every launch leaves one conversation rather than a week of duplicates.
 *
 * `at` is a unix timestamp passed in rather than read here, so the same inputs always
 * produce the same SQL and the tests can assert on it.
 */
export function buildThreadSql(at: number, thread: SeedThread = SHOTS_THREAD): string {
  const title = quote(thread.title)
  const statements: string[] = [
    `INSERT OR REPLACE INTO meta (key, value) VALUES ('ask_cmdr_consent_version','${String(CONSENT_COPY_VERSION)}');`,
    `INSERT OR REPLACE INTO meta (key, value) VALUES ('ask_cmdr_consent_at','${String(at)}');`,
    // ❗ Delete the messages EXPLICITLY, not via `ON DELETE CASCADE`. The cascade needs
    // `foreign_keys=ON`, which the `sqlite3` CLI leaves OFF by default (the app turns it
    // on for its own connections), so a plain conversation delete orphans its messages —
    // and SQLite then hands the replacement conversation the same rowid, where the
    // orphans collide with it on `(conversation_id, seq)`. The PRAGMA below is set too,
    // but outside the transaction, where it actually takes effect.
    `DELETE FROM messages WHERE conversation_id IN (SELECT id FROM conversations WHERE title = ${title});`,
    `DELETE FROM conversations WHERE title = ${title};`,
    `INSERT INTO conversations (title, created_at, updated_at, archived, origin, last_model, last_prompt_tokens, last_prompt_budget)
       VALUES (${title}, ${String(at)}, ${String(at)}, 0, NULL, ${quote(THREAD_MODEL)}, ${String(PROMPT_TOKENS)}, ${String(PROMPT_BUDGET)});`,
  ]

  for (const [index, message] of thread.messages.entries()) {
    statements.push(
      `INSERT INTO messages (conversation_id, seq, role, content_blocks, text_for_search, created_at)
         SELECT id, ${String(index)}, ${quote(message.role)}, ${quote(message.contentBlocks)}, ${quote(message.textForSearch)}, ${String(at)}
         FROM conversations WHERE title = ${title};`,
    )
  }

  statements.push(
    `INSERT OR REPLACE INTO cost_meter (day, conversation_id, provider, model, prompt_tokens, completion_tokens, cost_micros, priced)
       SELECT date(${String(at)}, 'unixepoch'), id, 'anthropic', ${quote(THREAD_MODEL)}, ${String(PROMPT_TOKENS)}, ${String(COMPLETION_TOKENS)}, ${String(COST_MICROS)}, 1
       FROM conversations WHERE title = ${title};`,
  )

  return `PRAGMA foreign_keys=ON;\nBEGIN;\n${statements.join('\n')}\nCOMMIT;\n`
}
