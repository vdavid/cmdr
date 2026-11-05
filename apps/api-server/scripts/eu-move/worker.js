/**
 * The EU move tool (vdavid/cmdr#805): copies the error-report bucket and reconciles the telemetry
 * database between the old (unlocked) Cloudflare resources and their `eu`-jurisdiction twins.
 *
 * ❗ It runs ONLY under `wrangler dev` with remote bindings (`wrangler.jsonc` beside this file),
 * listening on 127.0.0.1. It is never deployed: a public Worker bound to both production databases
 * (buyer emails) and both buckets would be exposure for no gain. The runbook that drives it is
 * `docs/notes/eu-data-moves-2026-11.md`.
 *
 * Every call names a direction (`from` / `to`, each `old` or `eu`), so the same tool serves the
 * cutover and a rollback. Nothing here deletes or overwrites: a copy skips keys and ids the target
 * already has, and differences it can't settle on its own are reported for a human.
 *
 * Routes (all GET, JSON out):
 * - `/r2/diff?from=old&to=eu`: keys missing in `to`, keys whose etag or size differ, keys only in `to`.
 * - `/r2/meta-diff?from=old&to=eu`: keys both hold whose HTTP or custom metadata differ.
 * - `/r2/copy?from=old&to=eu&limit=50`: copy up to `limit` missing keys, with their HTTP and custom
 *   metadata. Repeat until `remaining` is 0.
 * - `/d1/stats?db=old`: per table, row count and max id.
 * - `/d1/compare?from=old&to=eu&table=t`: per-table fingerprint compare (count, max id, and a SHA-256
 *   over every row in primary-key order), for one table or all of them.
 * - `/d1/marks?db=old`: the per-table max ids to record at the export (the reconcile's starting point).
 * - `/d1/bump?db=eu&by=1000000`: raise each AUTOINCREMENT sequence in `db` by `by`, so rows the new
 *   Worker writes can never take an id the old database handed out during the cutover window.
 * - `/d1/reconcile?from=old&to=eu&marks=<json>&apply=0|1`: rows in `from` with id above their mark go
 *   to `to` WITH their ids (`INSERT OR IGNORE`, so a rerun is harmless), keyed rows only `from` has
 *   are copied too, and forward-only stamps (`STAMP_COLUMNS`) `from` set and `to` lacks are carried
 *   over. `apply=0` (the default) only counts. What's left (rows both hold that differ in another
 *   in-place column) is listed under `differing` for a human to settle.
 */

// A Worker, not a Node script like its neighbors in `scripts/`, so it runs on the web globals.
/* global URL, Response, crypto, TextEncoder */

/** Tables with an INTEGER PRIMARY KEY AUTOINCREMENT `id`. Rows only ever get appended, plus the in-place columns below. */
const ID_TABLES = [
  'analytics_event',
  'crash_reports',
  'daily_active_users',
  'downloads',
  'downloads_daily_unique',
  'error_report_amendments',
  'error_reports',
  'feedback',
  'heartbeat',
  'ops_events',
  'update_checks',
]

/** Tables keyed by something other than an id. Small, so the reconcile diffs them whole. */
const KEYED_TABLES = {
  card_ratings: 'comment_id',
  cron_job_streaks: 'job',
  db_size_samples: 'day',
  license_adjustments: 'adjustment_id',
  license_issuance: 'transaction_id',
}

/**
 * Columns changed in place after the row is written, by a request handler or the cron. A row with
 * id at or below its mark can still differ in these, so the reconcile reports such rows.
 */
const MUTABLE_COLUMNS = {
  crash_reports: ['notified_at', 'issue_number', 'email', 'diag_id'],
  downloads: ['hashed_ip', 'user_agent'],
  error_report_amendments: ['board_done_at', 'notified_at', 'note', 'email'],
  error_reports: ['issue_number', 'notified_at', 'user_note', 'email'],
  feedback: ['notified_at', 'issue_number', 'answered_at', 'email'],
  ops_events: ['notified_at', 'occurrences'],
  daily_active_users: ['unique_users'],
  downloads_daily_unique: ['unique_downloaders'],
}

/**
 * The `MUTABLE_COLUMNS` that only ever go from NULL to a value (the outbox's card number, the
 * digest's and board's stamps). The reconcile carries these forward on its own; the email and
 * diagnostics scrubs go the other way and the next retention sweep repeats them.
 */
const STAMP_COLUMNS = {
  crash_reports: ['notified_at', 'issue_number'],
  error_report_amendments: ['board_done_at', 'notified_at'],
  error_reports: ['issue_number', 'notified_at'],
  feedback: ['notified_at', 'issue_number'],
  ops_events: ['notified_at'],
}

const ALL_TABLES = [...ID_TABLES, ...Object.keys(KEYED_TABLES), 'd1_migrations']

const PAGE_ROWS = 2000
const INSERT_BATCH = 50

export default {
  /** @param {Request} request @param {Record<string, any>} env */
  async fetch(request, env) {
    const url = new URL(request.url)
    const q = url.searchParams
    try {
      switch (url.pathname) {
        case '/r2/diff':
          return json(await r2Diff(bucket(env, q.get('from')), bucket(env, q.get('to'))))
        case '/r2/meta-diff':
          return json(await r2MetaDiff(bucket(env, q.get('from')), bucket(env, q.get('to'))))
        case '/r2/copy':
          return json(await r2Copy(bucket(env, q.get('from')), bucket(env, q.get('to')), intParam(q, 'limit', 50)))
        case '/d1/stats':
          return json(await d1Stats(db(env, q.get('db'))))
        case '/d1/marks':
          return json(await d1Marks(db(env, q.get('db'))))
        case '/d1/compare':
          return json(await d1Compare(db(env, q.get('from')), db(env, q.get('to')), q.get('table')))
        case '/d1/bump':
          return json(await d1Bump(db(env, q.get('db')), intParam(q, 'by', 1_000_000)))
        case '/d1/reconcile':
          return json(
            await d1Reconcile(
              db(env, q.get('from')),
              db(env, q.get('to')),
              JSON.parse(q.get('marks') ?? 'null'),
              q.get('apply') === '1',
            ),
          )
        default:
          return json({ error: 'unknown route' }, 404)
      }
    } catch (error) {
      return json({ error: error instanceof Error ? error.message : String(error) }, 500)
    }
  },
}

/** @param {unknown} body @param {number} [status] */
function json(body, status = 200) {
  return new Response(JSON.stringify(body, null, 2), { status, headers: { 'content-type': 'application/json' } })
}

/** @param {URLSearchParams} q @param {string} name @param {number} fallback */
function intParam(q, name, fallback) {
  const raw = q.get(name)
  const n = raw === null ? fallback : Number(raw)
  if (!Number.isSafeInteger(n) || n <= 0) throw new Error(`${name} must be a positive integer`)
  return n
}

/** @param {Record<string, any>} env @param {string | null} side */
function bucket(env, side) {
  if (side === 'old') return env.OLD_BUCKET
  if (side === 'eu') return env.EU_BUCKET
  throw new Error('from/to must be "old" or "eu"')
}

/** @param {Record<string, any>} env @param {string | null} side */
function db(env, side) {
  if (side === 'old') return env.OLD_DB
  if (side === 'eu') return env.EU_DB
  throw new Error('db/from/to must be "old" or "eu"')
}

// ---------- R2 ----------

/** @param {any} b */
async function listAll(b) {
  /** @type {Map<string, {size: number, etag: string}>} */
  const out = new Map()
  let cursor
  do {
    const page = await b.list({ cursor, limit: 1000 })
    for (const o of page.objects) out.set(o.key, { size: o.size, etag: o.etag })
    cursor = page.truncated ? page.cursor : undefined
  } while (cursor)
  return out
}

/** @param {any} from @param {any} to */
async function r2Diff(from, to) {
  const [a, b] = await Promise.all([listAll(from), listAll(to)])
  const missing = []
  const differing = []
  for (const [key, meta] of a) {
    const other = b.get(key)
    if (!other) missing.push(key)
    else if (other.size !== meta.size || other.etag !== meta.etag) differing.push({ key, from: meta, to: other })
  }
  const onlyInTarget = [...b.keys()].filter((k) => !a.has(k))
  return { fromCount: a.size, toCount: b.size, missing, differing, onlyInTarget }
}

/** Every key both buckets hold whose HTTP or custom metadata differ. @param {any} from @param {any} to */
async function r2MetaDiff(from, to) {
  const [a, b] = await Promise.all([listAll(from), listAll(to)])
  const differing = []
  let checked = 0
  for (const key of a.keys()) {
    if (!b.has(key)) continue
    const [x, y] = await Promise.all([from.head(key), to.head(key)])
    if (!x || !y) continue
    checked++
    const left = JSON.stringify([x.httpMetadata ?? {}, sortKeys(x.customMetadata ?? {})])
    const right = JSON.stringify([y.httpMetadata ?? {}, sortKeys(y.customMetadata ?? {})])
    if (left !== right) differing.push({ key, from: left, to: right })
  }
  return { checked, differing }
}

/** @param {Record<string, string>} o */
function sortKeys(o) {
  return Object.fromEntries(Object.entries(o).sort(([k1], [k2]) => k1.localeCompare(k2)))
}

/** @param {any} from @param {any} to @param {number} limit */
async function r2Copy(from, to, limit) {
  const { missing } = await r2Diff(from, to)
  const copied = []
  for (const key of missing.slice(0, limit)) {
    const obj = await from.get(key)
    if (!obj) continue // Evicted or expired since the listing.
    // The bodies are at most 10 MB, so buffering keeps the put's length known.
    await to.put(key, await obj.arrayBuffer(), {
      httpMetadata: obj.httpMetadata,
      customMetadata: obj.customMetadata,
      // Refuse to overwrite: a key that appeared in the target meanwhile stays as it is.
      onlyIf: { etagDoesNotMatch: '*' },
    })
    copied.push(key)
  }
  return { copied: copied.length, remaining: Math.max(0, missing.length - copied.length) }
}

// ---------- D1 ----------

/** @param {any} d */
async function d1Stats(d) {
  const out = {}
  for (const t of ALL_TABLES) {
    const hasId = ID_TABLES.includes(t) || t === 'd1_migrations'
    const row = await d.prepare(`SELECT count(*) AS n${hasId ? ', max(id) AS m' : ''} FROM ${t}`).first()
    out[t] = { rows: row.n, maxId: hasId ? (row.m ?? 0) : null }
  }
  const seq = await d.prepare('SELECT name, seq FROM sqlite_sequence ORDER BY name').all()
  return { tables: out, sqliteSequence: seq.results }
}

/** @param {any} d */
async function d1Marks(d) {
  const marks = {}
  for (const t of ID_TABLES) {
    const row = await d.prepare(`SELECT max(id) AS m FROM ${t}`).first()
    marks[t] = row.m ?? 0
  }
  return marks
}

/** SHA-256 over every row of `table`, in primary-key order, read in pages. @param {any} d @param {string} table */
async function fingerprint(d, table) {
  const key = KEYED_TABLES[table] ?? 'id'
  let hash = ''
  let rows = 0
  let after = null
  for (;;) {
    const stmt =
      after === null
        ? d.prepare(`SELECT * FROM ${table} ORDER BY ${key} LIMIT ?`).bind(PAGE_ROWS)
        : d.prepare(`SELECT * FROM ${table} WHERE ${key} > ? ORDER BY ${key} LIMIT ?`).bind(after, PAGE_ROWS)
    const { results } = await stmt.all()
    if (results.length === 0) break
    // Chain the page hashes, so the result covers every row and its order without holding them all.
    hash = await sha256(hash + JSON.stringify(results))
    rows += results.length
    after = results[results.length - 1][key]
  }
  return { rows, hash }
}

/** @param {string} text */
async function sha256(text) {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text))
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('')
}

/** @param {any} from @param {any} to @param {string | null} table */
async function d1Compare(from, to, table) {
  const tables = table ? [table] : ALL_TABLES
  const out = {}
  for (const t of tables) {
    if (!ALL_TABLES.includes(t)) throw new Error(`unknown table ${t}`)
    const [a, b] = await Promise.all([fingerprint(from, t), fingerprint(to, t)])
    out[t] = { same: a.rows === b.rows && a.hash === b.hash, from: a, to: b }
  }
  return { allSame: Object.values(out).every((r) => r.same), tables: out }
}

/**
 * Set every `ID_TABLES` sequence to `max(seq, max(id)) + by`. A table that never had a row has no
 * `sqlite_sequence` entry and would start at id 1, colliding with a row the old database hands out
 * in the window (which `INSERT OR IGNORE` would then drop), so a missing entry gets inserted.
 * @param {any} d @param {number} by
 */
async function d1Bump(d, by) {
  const before = (await d.prepare('SELECT name, seq FROM sqlite_sequence ORDER BY name').all()).results
  const seqOf = new Map(before.map((r) => [r.name, r.seq]))
  const stmts = []
  for (const t of ID_TABLES) {
    const row = await d.prepare(`SELECT max(id) AS m FROM ${t}`).first()
    const target = Math.max(seqOf.get(t) ?? 0, row.m ?? 0) + by
    stmts.push(
      seqOf.has(t)
        ? d.prepare('UPDATE sqlite_sequence SET seq = ? WHERE name = ?').bind(target, t)
        : d.prepare('INSERT INTO sqlite_sequence (name, seq) VALUES (?, ?)').bind(t, target),
    )
  }
  await d.batch(stmts)
  const after = (await d.prepare('SELECT name, seq FROM sqlite_sequence ORDER BY name').all()).results
  const short = ID_TABLES.filter((t) => !after.some((r) => r.name === t && r.seq >= by))
  return { before, after, ok: short.length === 0, short }
}

/**
 * @param {any} from @param {any} to @param {Record<string, number>} marks @param {boolean} apply
 */
async function d1Reconcile(from, to, marks, apply) {
  if (!marks || typeof marks !== 'object') throw new Error('marks (the JSON from /d1/marks at the export) is required')
  // Rows `from` gained after the export. `appended` counts what reached `to`; `ignored` what
  // `INSERT OR IGNORE` dropped (a UNIQUE clash or an id `to` already used), for a human to read.
  const appended = {}
  const ignored = {}
  for (const t of ID_TABLES) {
    const mark = marks[t]
    if (!Number.isSafeInteger(mark)) throw new Error(`no mark for ${t}`)
    const { results } = await from.prepare(`SELECT * FROM ${t} WHERE id > ? ORDER BY id`).bind(mark).all()
    appended[t] = results.length
    if (!apply || results.length === 0) continue
    const written = await insertOrIgnore(to, t, results)
    appended[t] = written
    if (written !== results.length) ignored[t] = results.length - written
  }

  // Keyed rows only `from` has (a license minted, refunded, or claimed in the window) are copied,
  // or `/validate` would miss them and a Paddle redelivery would mint a second set of licenses.
  // Rows both hold that differ stay a human's call: either side may be the newer one.
  const keyedCopied = {}
  const differing = {}
  for (const [t, key] of Object.entries(KEYED_TABLES)) {
    const [a, b] = await Promise.all([
      from.prepare(`SELECT * FROM ${t} ORDER BY ${key}`).all(),
      to.prepare(`SELECT * FROM ${t} ORDER BY ${key}`).all(),
    ])
    const byKey = new Map(b.results.map((r) => [r[key], JSON.stringify(r)]))
    const missing = a.results.filter((r) => !byKey.has(r[key]))
    keyedCopied[t] = apply && missing.length ? await insertOrIgnore(to, t, missing) : missing.length
    const rows = a.results.filter((r) => byKey.has(r[key]) && byKey.get(r[key]) !== JSON.stringify(r))
    if (rows.length) differing[t] = rows.map((r) => ({ key: r[key], from: r, to: byKey.get(r[key]) }))
  }

  // In-place changes on rows both sides have. A forward-only stamp `from` set in the window and `to`
  // lacks is carried over, or `to`'s next cron tick would file the card or send the digest again.
  // Every other difference is reported.
  const stamped = {}
  for (const [t, cols] of Object.entries(MUTABLE_COLUMNS)) {
    const sel = `SELECT id, ${cols.join(', ')} FROM ${t} WHERE id <= ? ORDER BY id`
    const [a, b] = await Promise.all([from.prepare(sel).bind(marks[t]).all(), to.prepare(sel).bind(marks[t]).all()])
    const byId = new Map(b.results.map((r) => [r.id, r]))
    const stamps = STAMP_COLUMNS[t] ?? []
    const updates = []
    const rows = []
    for (const r of a.results) {
      const other = byId.get(r.id)
      if (!other) continue
      const carry = stamps.filter((c) => r[c] !== null && other[c] === null)
      for (const c of carry) {
        updates.push(to.prepare(`UPDATE ${t} SET ${c} = ? WHERE id = ? AND ${c} IS NULL`).bind(r[c], r.id))
      }
      if (cols.some((c) => !carry.includes(c) && r[c] !== other[c])) rows.push({ id: r.id, from: r, to: other })
    }
    stamped[t] = updates.length
    if (apply) {
      for (let i = 0; i < updates.length; i += INSERT_BATCH) await to.batch(updates.slice(i, i + INSERT_BATCH))
    }
    if (rows.length) differing[t] = rows
  }
  return { applied: apply, appended, ignored, keyedCopied, stamped, differing }
}

/**
 * `INSERT OR IGNORE` `rows` (one table's, all with the same columns) into `d`; returns how many landed.
 * @param {any} d @param {string} table @param {Record<string, unknown>[]} rows
 */
async function insertOrIgnore(d, table, rows) {
  const cols = Object.keys(rows[0])
  const sql = `INSERT OR IGNORE INTO ${table} (${cols.join(', ')}) VALUES (${cols.map(() => '?').join(', ')})`
  let written = 0
  for (let i = 0; i < rows.length; i += INSERT_BATCH) {
    const chunk = rows.slice(i, i + INSERT_BATCH)
    const results = await d.batch(chunk.map((r) => d.prepare(sql).bind(...cols.map((c) => r[c]))))
    for (const res of results) written += res.meta?.changes ?? 0
  }
  return written
}
